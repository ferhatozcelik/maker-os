// Package screen, Ekran Modu'nu (Pi'nin ekranını tarayıcıda gösterme; arka planda wayvnc) yönetir.
// Varsayılan KAPALI: wayvnc hiç çalışmaz, /api/v1/vnc bağlantı kabul etmez. Yönetici Ayarlar'dan açar;
// seçim <data-dir>/screen-mode dosyasıyla kalıcıdır (labwc autostart da bu dosyaya bakar).
// Bağlantı ayrı bir port açmaz: noVNC → makerd (aynı HTTP portu, /api/v1/vnc) → 127.0.0.1:5900.
package screen

import (
	"context"
	"errors"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Unit, wayvnc'nin systemd kullanıcı birimidir (os/board/.../systemd/user/makeros-vnc.service).
const Unit = "makeros-vnc.service"

// State, GET /api/v1/screen yanıtıdır.
type State struct {
	Enabled bool `json:"enabled"`
	// Headless: Pi'ye monitör bağlı değil → sanal ekran tarayıcı penceresinin boyutuna uyar.
	Headless bool `json:"headless"`
	// Managed: makerd wayvnc'yi kendisi başlatıp durdurabiliyor (yerel adres).
	Managed bool `json:"managed"`
}

// Manager Ekran Modu durumunu tutar ve wayvnc'yi başlatır/durdurur.
type Manager struct {
	Path    string // kalıcı işaret dosyası: varsa açık
	Addr    string // wayvnc adresi (127.0.0.1:5900); yerel değilse yönetilmez (ör. compose "desktop" kabı)
	Log     *slog.Logger
	DRMRoot string // /sys/class/drm (testte değiştirilebilir)

	mu   sync.Mutex
	proc *exec.Cmd
	stop context.CancelFunc
}

// Enabled Ekran Modu açık mı.
func (m *Manager) Enabled() bool {
	if m == nil || m.Path == "" {
		return false
	}
	_, err := os.Stat(m.Path)
	return err == nil
}

// State güncel durumu döndürür.
func (m *Manager) State() State {
	return State{Enabled: m.Enabled(), Headless: m.headless(), Managed: m.managed()}
}

func (m *Manager) managed() bool {
	host, _, _ := strings.Cut(m.Addr, ":")
	return host == "127.0.0.1" || host == "localhost" || host == "::1"
}

// headless: hiçbir DRM bağlayıcısı (HDMI/DSI) "connected" değil. DRM yoksa (Docker) monitörsüzdür.
func (m *Manager) headless() bool {
	root := m.DRMRoot
	if root == "" {
		root = "/sys/class/drm"
	}
	ms, _ := filepath.Glob(filepath.Join(root, "card*-*", "status"))
	for _, p := range ms {
		if b, err := os.ReadFile(p); err == nil && strings.TrimSpace(string(b)) == "connected" {
			return false
		}
	}
	return true
}

// Set Ekran Modu'nu açar/kapatır, kalıcı yapar ve wayvnc'yi buna göre yönetir.
func (m *Manager) Set(on bool) error {
	if on {
		if err := os.WriteFile(m.Path, []byte("on\n"), 0o640); err != nil {
			return err
		}
	} else if err := os.Remove(m.Path); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	m.Apply()
	return nil
}

// Apply kayıtlı duruma uyar (açılışta ve her Set'te çağrılır).
func (m *Manager) Apply() {
	if !m.managed() {
		return
	}
	if m.Enabled() {
		m.start()
	} else {
		m.halt()
	}
}

// Close makerd kapanırken kendi başlattığı wayvnc'yi durdurur.
func (m *Manager) Close() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.killLocked()
}

// RuntimeDir kullanıcının XDG_RUNTIME_DIR dizini.
func RuntimeDir() string { return runtimeDir() }

func runtimeDir() string {
	if d := os.Getenv("XDG_RUNTIME_DIR"); d != "" {
		return d
	}
	return "/run/user/" + strconv.Itoa(os.Getuid())
}

// systemctl --user: gerçek sistemde wayvnc kullanıcı oturumunun (labwc) birimidir.
func systemctl(action string) error { return SystemctlUser(action, Unit) }

// SystemctlUser makerd'nin kendi kullanıcı oturumunda (maker) "systemctl --user <action> <unit>" çalıştırır.
// Kullanıcı oturumu yoksa (Docker önizlemesi) hata döner; çağıran doğrudan başlatmaya düşer.
// Birimler makerd'nin kum havuzunun (MemoryDenyWriteExecute, MemoryMax…) dışında çalışır.
func SystemctlUser(action, unit string) error {
	path, err := exec.LookPath("systemctl")
	if err != nil {
		return err
	}
	rd := runtimeDir()
	if _, err := os.Stat(filepath.Join(rd, "bus")); err != nil {
		return errors.New("kullanıcı oturum veri yolu yok")
	}
	cmd := exec.Command(path, "--user", action, unit)
	cmd.Env = append(os.Environ(), "XDG_RUNTIME_DIR="+rd, "DBUS_SESSION_BUS_ADDRESS=unix:path="+filepath.Join(rd, "bus"))
	out, err := cmd.CombinedOutput()
	if err != nil {
		return errors.New(strings.TrimSpace(string(out)))
	}
	return nil
}

func (m *Manager) start() {
	if err := systemctl("start"); err == nil {
		m.Log.Info("Ekran Modu açık", "via", "systemd")
		return
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.stop != nil {
		return // zaten izleniyor
	}
	// systemd kullanıcı oturumu yok (Docker önizlemesi): wayvnc doğrudan, ekran (Wayland) hazır olunca.
	ctx, cancel := context.WithCancel(context.Background())
	m.stop = cancel
	go m.supervise(ctx)
}

func (m *Manager) halt() {
	if err := systemctl("stop"); err == nil {
		m.Log.Info("Ekran Modu kapalı", "via", "systemd")
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	m.killLocked()
}

func (m *Manager) killLocked() {
	if m.stop != nil {
		m.stop()
		m.stop = nil
	}
	if m.proc != nil && m.proc.Process != nil {
		_ = m.proc.Process.Signal(syscall.SIGTERM)
		m.proc = nil
	}
}

// supervise wayvnc'yi Wayland soketi hazır olunca başlatır, düşerse yeniden başlatır.
func (m *Manager) supervise(ctx context.Context) {
	host, port, _ := strings.Cut(m.Addr, ":")
	if port == "" {
		port = "5900"
	}
	sock := filepath.Join(runtimeDir(), envOr("WAYLAND_DISPLAY", "wayland-0"))
	for ctx.Err() == nil {
		if fi, err := os.Stat(sock); err != nil || fi.Mode()&os.ModeSocket == 0 {
			sleep(ctx, time.Second)
			continue
		}
		path, err := exec.LookPath("wayvnc")
		if err != nil {
			m.Log.Warn("Ekran Modu: wayvnc bulunamadı")
			return
		}
		args := []string{"--render-cursor", "--max-fps=30", host, port}
		if g, _ := filepath.Glob("/dev/dri/renderD*"); len(g) > 0 {
			args = append([]string{"--gpu"}, args...)
		}
		cmd := exec.Command(path, args...)
		cmd.Env = append(os.Environ(), "XDG_RUNTIME_DIR="+runtimeDir(), "WAYLAND_DISPLAY="+filepath.Base(sock))
		cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
		if err := cmd.Start(); err != nil {
			m.Log.Warn("Ekran Modu: wayvnc başlatılamadı", "err", err)
			sleep(ctx, 3*time.Second)
			continue
		}
		m.mu.Lock()
		m.proc = cmd
		m.mu.Unlock()
		m.Log.Info("Ekran Modu açık", "via", "wayvnc", "pid", cmd.Process.Pid)
		_ = cmd.Wait()
		if ctx.Err() != nil {
			m.Log.Info("Ekran Modu kapalı", "via", "wayvnc")
			return
		}
		m.Log.Warn("Ekran Modu: wayvnc durdu, yeniden başlatılıyor")
		sleep(ctx, 2*time.Second)
	}
}

func envOr(k, d string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return d
}

func sleep(ctx context.Context, d time.Duration) {
	select {
	case <-ctx.Done():
	case <-time.After(d):
	}
}
