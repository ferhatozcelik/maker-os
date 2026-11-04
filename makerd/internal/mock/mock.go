// Package mock, --mock modu için sahte arka uçlardır: kök yetkisi, NetworkManager
// veya D-Bus gerektirmez. Önizleme (docker compose) ve testlerde kullanılır.
// Kullanıcı: maker / parola: maker.
package mock

import (
	"context"
	"crypto/subtle"
	"errors"
	"log/slog"
	"math"
	"math/rand/v2"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/activity"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apps"
	"github.com/ferhatozcelik/maker-os/makerd/internal/auth"
	"github.com/ferhatozcelik/maker-os/makerd/internal/network"
	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
	"github.com/ferhatozcelik/maker-os/makerd/internal/system"
)

// User ve Password mock kimliğidir.
const (
	User     = "maker"
	Password = "maker"
)

// State, mock arka uçlarının paylaşılan durumudur.
type State struct {
	mu    sync.Mutex
	users map[string]string // kullanıcı → parola (hepsi yönetici)
}

// NewState kurulumu tamamlanmış durumla (maker/maker) oluşturur.
func NewState() *State { return &State{users: map[string]string{User: Password}} }

// NewFreshState hiç kullanıcısı olmayan (ilk açılış) durumu oluşturur.
func NewFreshState() *State { return &State{users: map[string]string{}} }

// CreateUser kurulum sihirbazının yönetici hesabını oluşturur (bellekte).
func (s *State) CreateUser(_ context.Context, a priv.UserCreateArgs) error {
	if !priv.ValidUsername(a.User) {
		return apierr.Invalid("Geçersiz kullanıcı adı.")
	}
	if !priv.ValidNewPassword(a.Password) {
		return apierr.Invalid("Parola en az 6 karakter olmalıdır.")
	}
	if !priv.ValidFullname(a.Fullname) {
		return apierr.Invalid("Ad soyad geçersiz karakter içeriyor.")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.users[a.User] = a.Password
	return nil
}

// --- auth ------------------------------------------------------------------

// Checker mock parola doğrulayıcı.
type Checker struct{ S *State }

// Check maker/<parola> kabul eder.
func (c Checker) Check(_ context.Context, user, password string) (auth.Result, error) {
	c.S.mu.Lock()
	defer c.S.mu.Unlock()
	pw, found := c.S.users[user]
	ok := found && subtle.ConstantTimeCompare([]byte(password), []byte(pw)) == 1
	return auth.Result{OK: ok, Admin: ok}, nil
}

// --- system ----------------------------------------------------------------

// System mock sistem sağlayıcısı.
type System struct {
	S       *State
	Version string
	Log     *slog.Logger

	mu       sync.Mutex
	started  time.Time
	hostname string
	tz       string
	ntp      bool
	offset   time.Duration
	locale   system.Locale
}

// NewSystem mock sistem oluşturur.
func NewSystem(s *State, version string, log *slog.Logger) *System {
	return &System{S: s, Version: version, Log: log, started: time.Now(), hostname: "makeros",
		tz: "Europe/Istanbul", ntp: true, locale: system.Locale{Lang: "tr_TR.UTF-8", Keymap: "trq"}}
}

func jitter(base, amp float64) float64 { return math.Round((base+(rand.Float64()*2-1)*amp)*10) / 10 }

// Info sahte Raspberry Pi 4 özeti.
func (m *System) Info(context.Context) (system.Info, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	temp := jitter(47.5, 2.5)
	total := int64(3_981_000_000)
	used := int64(jitter(820, 40)) * 1_000_000
	return system.Info{
		Hostname: m.hostname,
		OS:       system.OSInfo{Name: "MakerOS", Version: m.Version},
		Kernel:   "6.18.12-v8+",
		Model:    "Raspberry Pi 4 Model B Rev 1.5",
		UptimeS:  int64(time.Since(m.started).Seconds()) + 3600,
		CPU: system.CPUInfo{Cores: 4, FreqMHz: []int{600, 1000, 1500, 1800}[rand.IntN(4)], TempC: &temp,
			Load: [3]float64{jitter(0.4, 0.3), jitter(0.35, 0.1), jitter(0.3, 0.05)}, UsagePct: math.Max(0, jitter(9, 6))},
		Mem:  system.MemInfo{Total: total, Used: used, Available: total - used, ZramUsed: 12_582_912},
		Disk: system.Disk{Total: 59_000_000_000, Used: 7_400_000_000},
	}, nil
}

// SetHostname adı bellekte değiştirir.
func (m *System) SetHostname(_ context.Context, name string) error {
	if !priv.ValidHostname(name) {
		return apierr.Invalid("Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir (en fazla 63).")
	}
	m.mu.Lock()
	m.hostname = name
	m.mu.Unlock()
	return nil
}

// Time mock saat.
func (m *System) Time(context.Context) (system.Time, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	now := time.Now().Add(m.offset)
	if loc, err := time.LoadLocation(m.tz); err == nil {
		now = now.In(loc)
	}
	return system.Time{Now: now.Format(time.RFC3339), Timezone: m.tz, NTP: m.ntp}, nil
}

// SetTime mock saat ayarı.
func (m *System) SetTime(_ context.Context, u system.TimeUpdate) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if u.Timezone != nil {
		if !system.ValidTimezone(*u.Timezone) {
			return apierr.Invalid("Geçersiz saat dilimi.")
		}
		m.tz = *u.Timezone
	}
	if u.NTP != nil {
		m.ntp = *u.NTP
		if m.ntp {
			m.offset = 0
		}
	}
	if u.Now != nil {
		t, err := time.Parse(time.RFC3339, *u.Now)
		if err != nil {
			return apierr.Invalid("Saat RFC 3339 biçiminde olmalıdır.")
		}
		if m.ntp {
			return apierr.Conflict("Otomatik saat açıkken saat elle ayarlanamaz.")
		}
		m.offset = time.Until(t)
	}
	return nil
}

// Timezones gömülü liste.
func (m *System) Timezones(context.Context) ([]string, error) { return system.FallbackTimezones, nil }

// Locale mock dil.
func (m *System) Locale(context.Context) (system.Locale, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.locale, nil
}

// SetLocale mock dil ayarı.
func (m *System) SetLocale(_ context.Context, l system.Locale) error {
	if l.Lang != "" && !system.ValidLang(l.Lang) {
		return apierr.Invalid("Geçersiz dil.")
	}
	if l.Keymap != "" && !system.ValidKeymap(l.Keymap) {
		return apierr.Invalid("Geçersiz klavye düzeni.")
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	if l.Lang != "" {
		m.locale.Lang = l.Lang
	}
	if l.Keymap != "" {
		m.locale.Keymap = l.Keymap
	}
	return nil
}

// SetPassword mock parolayı değiştirir.
func (m *System) SetPassword(_ context.Context, user, old, new string) error {
	if len(new) < priv.MinPasswordLen {
		return apierr.Invalid("Yeni parola en az 6 karakter olmalıdır.")
	}
	m.S.mu.Lock()
	defer m.S.mu.Unlock()
	if pw, ok := m.S.users[user]; !ok || old != pw {
		return apierr.Forbidden("Mevcut parola hatalı.")
	}
	m.S.users[user] = new
	return nil
}

// Power yalnız günlüğe yazar.
func (m *System) Power(_ context.Context, action string) error {
	if action != "reboot" && action != "poweroff" {
		return apierr.Invalid("Geçersiz güç işlemi.")
	}
	m.Log.Info("mock güç işlemi (yok sayıldı)", "action", action)
	return nil
}

// --- network ---------------------------------------------------------------

type mockNet struct {
	ssid     string
	signal   int
	security string
}

// Network mock ağ sağlayıcısı.
type Network struct {
	// Step bağlanma adımları arasındaki bekleme (test için kısaltılabilir).
	Step time.Duration

	mu     sync.Mutex
	nets   []mockNet
	known  map[string]bool
	active string
	busy   bool
	radio  bool
	last   string // radyo kapanınca etkin ağ (açılınca geri bağlanır)
}

// NewNetwork mock ağ oluşturur.
func NewNetwork() *Network {
	return &Network{Step: 700 * time.Millisecond, active: "MakerLab", radio: true, known: map[string]bool{"MakerLab": true, "Ev-5G": true},
		nets: []mockNet{
			{"MakerLab", 82, "wpa2"}, {"Kafe_WiFi", 61, "open"}, {"eduroam", 55, "enterprise"},
			{"Ev-5G", 40, "wpa3"}, {"TurkTelekom_ZT3K", 30, "wpa2"}, {"MakerOS", 20, "open"},
		}}
}

// Status mock durum.
func (n *Network) Status(context.Context) (network.Status, error) {
	n.mu.Lock()
	defer n.mu.Unlock()
	speed := 1000
	wifi := network.Iface{Name: "wlan0", Type: "wifi", State: "disconnected", IPv4: []string{}, MAC: "dc:a6:32:00:00:02"}
	if !n.radio {
		wifi.State = "unavailable"
	} else if n.active != "" {
		wifi.State, wifi.SSID, wifi.IPv4 = "connected", n.active, []string{"192.168.1.57/24"}
	}
	return network.Status{
		Online: true,
		Ifaces: []network.Iface{
			{Name: "eth0", Type: "ethernet", State: "connected", IPv4: []string{"192.168.1.42/24"}, MAC: "dc:a6:32:00:00:01", SpeedMbps: &speed},
			wifi,
		},
		WifiEnabled: n.radio,
		Hotspot:     network.Hotspot{Active: false, SSID: network.HotspotSSID},
	}, nil
}

// SetWifiEnabled mock radyoyu açar/kapatır.
func (n *Network) SetWifiEnabled(_ context.Context, on bool) error {
	n.mu.Lock()
	defer n.mu.Unlock()
	if on == n.radio {
		return nil
	}
	n.radio = on
	if on {
		n.active, n.last = n.last, ""
	} else {
		n.last, n.active = n.active, ""
	}
	return nil
}

// Scan mock tarama.
func (n *Network) Scan(ctx context.Context) ([]network.AP, error) {
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-time.After(n.Step):
	}
	n.mu.Lock()
	defer n.mu.Unlock()
	if !n.radio {
		return []network.AP{}, nil
	}
	list := make([]network.AP, 0, len(n.nets))
	for _, x := range n.nets {
		sig := min(100, max(0, x.signal+rand.IntN(7)-3))
		list = append(list, network.AP{SSID: x.ssid, Signal: sig, Security: x.security, Known: n.known[x.ssid], Active: x.ssid == n.active})
	}
	return network.DedupeAPs(list), nil
}

// Connect mock bağlantı: parola "wrong" veya 8 karakterden kısa (WPA) ise başarısız.
func (n *Network) Connect(ctx context.Context, req network.ConnectRequest, p network.Progress) error {
	if !network.ValidSSID(req.SSID) {
		return apierr.Invalid("Ağ adı 1–32 bayt olmalıdır.")
	}
	n.mu.Lock()
	if n.busy {
		n.mu.Unlock()
		return apierr.Conflict("Bir ağ işlemi devam ediyor.")
	}
	n.busy = true
	n.radio = true // NM gibi: bağlanma radyoyu açar
	sec := ""
	for _, x := range n.nets {
		if x.ssid == req.SSID {
			sec = x.security
		}
	}
	known := n.known[req.SSID]
	n.mu.Unlock()
	defer func() { n.mu.Lock(); n.busy = false; n.mu.Unlock() }()

	steps := []struct {
		pct  int
		line string
	}{{10, req.SSID + " ağına bağlanılıyor…"}, {40, "Kimlik doğrulanıyor…"}, {70, "Adres alınıyor (DHCP)…"}}
	for _, s := range steps {
		p(s.pct, s.line)
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(n.Step):
		}
	}
	needPass := sec != "" && sec != "open" && sec != "enterprise" && !known
	if req.Password == "wrong" || (req.Password != "" && len(req.Password) < 8) || (needPass && req.Password == "") ||
		(sec == "" && !req.Hidden) {
		return errors.New("Ağa bağlanılamadı: parola hatalı veya ağ bulunamadı.\nÖnceki bağlantı geri açıldı.")
	}
	n.mu.Lock()
	n.active = req.SSID
	n.known[req.SSID] = true
	if sec == "" {
		n.nets = append(n.nets, mockNet{req.SSID, 50, "wpa2"})
	}
	n.mu.Unlock()
	p(100, req.SSID+" ağına bağlanıldı (wlan0).")
	return nil
}

// Forget kayıtlı ağı siler.
func (n *Network) Forget(_ context.Context, ssid string) error {
	n.mu.Lock()
	defer n.mu.Unlock()
	if !n.known[ssid] {
		return apierr.NotFound("Kayıtlı Wi-Fi profili bulunamadı: " + ssid)
	}
	delete(n.known, ssid)
	if n.active == ssid {
		n.active = ""
	}
	return nil
}

// --- activity --------------------------------------------------------------

type fakeProc struct {
	activity.Process
	base float64
}

// Activity mock süreç listesi.
type Activity struct {
	mu    sync.Mutex
	procs []fakeProc
}

// NewActivity sahte süreçlerle oluşturur.
func NewActivity() *Activity {
	mk := func(pid int, name, user, slice string, rssMB int64, cpu float64) fakeProc {
		return fakeProc{activity.Process{PID: pid, Name: name, User: user, RSS: rssMB << 20, Slice: slice}, cpu}
	}
	return &Activity{procs: []fakeProc{
		mk(1, "systemd", "root", "", 11, 0.1),
		mk(212, "systemd-journal", "root", "system.slice", 9, 0.1),
		mk(240, "systemd-udevd", "root", "system.slice", 6, 0),
		mk(388, "NetworkManager", "root", "system.slice", 17, 0.3),
		mk(390, "wpa_supplicant", "root", "system.slice", 8, 0.1),
		mk(402, "systemd-resolve", "systemd-resolve", "system.slice", 10, 0),
		mk(415, "systemd-timesyn", "systemd-timesync", "system.slice", 7, 0),
		mk(431, "makerd", "maker", "system.slice", 14, 0.8),
		mk(612, "cage", "maker", "shell.slice", 38, 1.5),
		mk(640, "cog", "maker", "shell.slice", 96, 4.2),
		mk(655, "WPEWebProcess", "maker", "shell.slice", 142, 6.5),
		mk(661, "WPENetworkProce", "maker", "shell.slice", 31, 0.4),
		mk(1203, "bash", "maker", "user-1000.slice", 5, 0),
		mk(1240, "python3", "maker", "apps.slice", 64, 12.0),
		mk(1301, "node", "maker", "apps.slice", 88, 2.1),
		mk(1402, "rpicam-vid", "maker", "apps.slice", 42, 7.4),
		mk(1500, "zram0", "root", "", 0, 0.2),
		mk(1601, "kworker/u8:2", "root", "", 0, 0.1),
		mk(1702, "sshd", "root", "system.slice", 7, 0),
		mk(1803, "systemd-oomd", "systemd-oom", "system.slice", 6, 0),
	}}
}

// Processes sahte CPU dalgalanmasıyla listeyi döndürür.
func (a *Activity) Processes(context.Context) ([]activity.Process, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	out := make([]activity.Process, 0, len(a.procs))
	for _, p := range a.procs {
		pr := p.Process
		if p.base > 0 {
			pr.CPUPct = math.Max(0, jitter(p.base, p.base*0.4))
		}
		out = append(out, pr)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].CPUPct != out[j].CPUPct {
			return out[i].CPUPct > out[j].CPUPct
		}
		return out[i].RSS > out[j].RSS
	})
	return out, nil
}

// Signal maker'a ait sahte süreci listeden çıkarır.
func (a *Activity) Signal(_ context.Context, pid int, sig string) error {
	if _, ok := activity.ParseSignal(sig); !ok {
		return apierr.Invalid("Sinyal TERM veya KILL olmalıdır.")
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	for i, p := range a.procs {
		if p.PID != pid {
			continue
		}
		if p.User != User || p.Name == "makerd" {
			return apierr.Forbidden("Yalnızca kendi süreçlerinizi sonlandırabilirsiniz.")
		}
		a.procs = append(a.procs[:i], a.procs[i+1:]...)
		return nil
	}
	return apierr.NotFound("Süreç bulunamadı.")
}

// --- apps ------------------------------------------------------------------

// NativeApps örnek yerel uygulamalardır.
func NativeApps() []apps.App {
	return []apps.App{
		{ID: "chromium", Name: "Chromium", Icon: "/shell/icons/chromium.svg", Kind: "native", Exec: "chromium %U", Categories: []string{"Network", "WebBrowser"}},
	}
}

// Launcher, bir Wayland ekranı (labwc) ve uygulamanın kendisi varsa (MakerOS kök FS'i Docker'da)
// gerçek başlatıcıyı kullanır; yoksa (yalın önizleme) yalnız günlüğe yazar.
func Launcher(log *slog.Logger, real apps.Launcher) apps.Launcher {
	return func(ctx context.Context, a apps.App, argv []string) error {
		if real != nil && hasDisplay() {
			if _, err := exec.LookPath(argv[0]); err == nil {
				return real(ctx, a, argv)
			}
		}
		log.Info("mock uygulama başlatma (yok sayıldı)", "id", a.ID, "argv", strings.Join(argv, " "))
		return nil
	}
}

func hasDisplay() bool {
	rd := os.Getenv("XDG_RUNTIME_DIR")
	if rd == "" {
		rd = "/run/user/" + strconv.Itoa(os.Getuid())
	}
	d := os.Getenv("WAYLAND_DISPLAY")
	if d == "" {
		d = "wayland-0"
	}
	if !filepath.IsAbs(d) {
		d = filepath.Join(rd, d)
	}
	fi, err := os.Stat(d)
	return err == nil && fi.Mode()&os.ModeSocket != 0
}
