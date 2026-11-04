// Package browser, MakerOS'taki "Chromium" penceresinin arkasındaki uzak tarayıcı oturumunu yönetir:
// ayrı, monitörsüz bir labwc + Chromium + wayvnc (yalnız 127.0.0.1:5901; /usr/lib/makeros/browser-session).
// Pi'nin kendi ekranı etkilenmez; Ekran Modu ayarından bağımsızdır. İlk pencere açılınca başlar,
// son pencere kapandıktan Grace süre sonra durur (Pi 4'te belleği boşa tutmasın).
package browser

import (
	"context"
	"errors"
	"log/slog"
	"net"
	"os"
	"os/exec"
	"sync"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/screen"
)

// Unit systemd kullanıcı birimi (os/board/.../systemd/user/makeros-browser.service).
const Unit = "makeros-browser.service"

// Manager oturumun yaşam döngüsünü tutar.
type Manager struct {
	Addr   string        // 127.0.0.1:5901
	Script string        // /usr/lib/makeros/browser-session
	Grace  time.Duration // son pencere kapandıktan sonra bekleme (varsayılan 45 sn)
	Log    *slog.Logger

	mu      sync.Mutex
	refs    int
	timer   *time.Timer
	proc    *exec.Cmd
	systemd bool
}

func (m *Manager) listening() bool {
	c, err := net.DialTimeout("tcp", m.Addr, 300*time.Millisecond)
	if err != nil {
		return false
	}
	c.Close()
	return true
}

// Acquire bir pencere için oturumu hazırlar (gerekirse başlatır) ve VNC portu açılana dek bekler.
func (m *Manager) Acquire(ctx context.Context) error {
	m.mu.Lock()
	m.refs++
	if m.timer != nil {
		m.timer.Stop()
		m.timer = nil
	}
	if !m.listening() {
		if err := m.startLocked(); err != nil {
			m.refs--
			m.mu.Unlock()
			return err
		}
	}
	m.mu.Unlock()
	deadline := time.Now().Add(25 * time.Second)
	for !m.listening() {
		if time.Now().After(deadline) {
			m.Release()
			return errors.New("tarayıcı oturumu zamanında açılmadı")
		}
		select {
		case <-ctx.Done():
			m.Release()
			return ctx.Err()
		case <-time.After(250 * time.Millisecond):
		}
	}
	return nil
}

// Release bir pencere kapandı; sonuncusuysa Grace sonra oturum durur.
func (m *Manager) Release() {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.refs > 0 {
		m.refs--
	}
	if m.refs > 0 {
		return
	}
	g := m.Grace
	if g == 0 {
		g = 45 * time.Second
	}
	if m.timer != nil {
		m.timer.Stop()
	}
	m.timer = time.AfterFunc(g, func() {
		m.mu.Lock()
		defer m.mu.Unlock()
		if m.refs == 0 {
			m.stopLocked()
		}
	})
}

// Close makerd kapanırken oturumu durdurur.
func (m *Manager) Close() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.stopLocked()
}

func (m *Manager) startLocked() error {
	// Önceki (kendiliğinden bitmiş) oturumdan kalan süreç grubu temizlenir.
	m.killLocked()
	if err := screen.SystemctlUser("start", Unit); err == nil {
		m.systemd = true
		m.Log.Info("tarayıcı oturumu başladı", "via", "systemd")
		return nil
	}
	if _, err := os.Stat(m.Script); err != nil {
		return errors.New("tarayıcı oturumu bu sistemde yok (" + m.Script + ")")
	}
	cmd := exec.Command(m.Script)
	home, _ := os.UserHomeDir()
	cmd.Dir = home
	cmd.Env = append(os.Environ(), "XDG_RUNTIME_DIR="+screen.RuntimeDir(), "HOME="+home)
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	if err := cmd.Start(); err != nil {
		return err
	}
	m.proc = cmd
	go func() { _ = cmd.Wait() }()
	m.Log.Info("tarayıcı oturumu başladı", "via", "doğrudan", "pid", cmd.Process.Pid)
	return nil
}

func (m *Manager) stopLocked() {
	if m.systemd {
		_ = screen.SystemctlUser("stop", Unit)
		m.systemd = false
		m.Log.Info("tarayıcı oturumu durdu", "via", "systemd")
	}
	if m.proc != nil {
		m.killLocked()
		m.Log.Info("tarayıcı oturumu durdu", "via", "doğrudan")
	}
}

func (m *Manager) killLocked() {
	if m.proc != nil && m.proc.Process != nil {
		// Süreç grubunun tamamı: labwc, wayvnc, Chromium.
		_ = syscall.Kill(-m.proc.Process.Pid, syscall.SIGTERM)
	}
	m.proc = nil
}
