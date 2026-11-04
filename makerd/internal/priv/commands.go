package priv

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"os/user"
	"regexp"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/sysbus"
	"github.com/godbus/dbus/v5"
)

var hostnameRe = regexp.MustCompile(`^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`)

// ValidHostname, küçük harf/rakam/'-' ile en fazla 63 karakterlik ad mı.
func ValidHostname(name string) bool { return hostnameRe.MatchString(name) }

// MinPasswordLen yeni parolanın en kısa uzunluğudur.
const MinPasswordLen = 6

// Commands, makerd-priv komutlarının gerçek (kök) uygulamasıdır.
type Commands struct {
	Shadow   string // /etc/shadow
	Hostname string // /etc/hostname
	Hosts    string // /etc/hosts
	// MinUID altındaki (sistem) hesapların parolası değiştirilemez ve doğrulanamaz.
	MinUID int
	// FailDelay başarısız doğrulamadan sonra bekleme (kaba kuvvete karşı).
	FailDelay time.Duration
	// Users user.create için hesap dosyalarıdır.
	Users UserFiles
}

// DefaultCommands sistem yollarıyla komutları döndürür.
func DefaultCommands() *Commands {
	return &Commands{Shadow: "/etc/shadow", Hostname: "/etc/hostname", Hosts: "/etc/hosts", MinUID: 1000, FailDelay: 300 * time.Millisecond, Users: DefaultUserFiles()}
}

// Handlers komut izin listesini döndürür.
func (c *Commands) Handlers() map[string]Handler {
	return map[string]Handler{
		CmdAuthCheck:      c.authCheck,
		CmdSystemPassword: c.password,
		CmdSystemHostname: c.hostname,
		CmdPower:          c.power,
		CmdUserCreate:     c.userCreate,
		CmdOTAInstall:     c.otaInstall,
		CmdOTAStatus:      c.otaStatus,
	}
}

func perr(code, msg string) error { return &Error{Code: code, Message: msg} }

func decode(raw json.RawMessage, v any) error {
	if len(raw) == 0 {
		return perr("invalid", "Argüman eksik.")
	}
	if err := json.Unmarshal(raw, v); err != nil {
		return perr("invalid", "Geçersiz argüman.")
	}
	return nil
}

func (c *Commands) loginUser(name string) bool {
	u, err := user.Lookup(name)
	if err != nil {
		return false
	}
	uid, err := strconv.Atoi(u.Uid)
	return err == nil && uid >= c.MinUID && uid < 60000
}

func (c *Commands) authCheck(_ context.Context, _ Peer, raw json.RawMessage) (any, error) {
	var a AuthCheckArgs
	if err := decode(raw, &a); err != nil {
		return nil, err
	}
	ok := false
	if a.User != "" && c.loginUser(a.User) {
		var err error
		ok, err = CheckShadow(c.Shadow, a.User, a.Password)
		if errors.Is(err, ErrUnsupportedHash) {
			return nil, perr("internal", "Parola özeti biçimi desteklenmiyor (SHA-512 gerekli).")
		}
		if err != nil {
			return nil, err
		}
	}
	if !ok && c.FailDelay > 0 {
		time.Sleep(c.FailDelay)
	}
	return AuthCheckResult{OK: ok}, nil
}

func (c *Commands) password(_ context.Context, peer Peer, raw json.RawMessage) (any, error) {
	var a PasswordArgs
	if err := decode(raw, &a); err != nil {
		return nil, err
	}
	target := peer.User
	if peer.UID == 0 && a.User != "" {
		target = a.User
	}
	if a.User != "" && a.User != target {
		return nil, perr("forbidden", "Yalnızca kendi parolanızı değiştirebilirsiniz.")
	}
	if target == "" || !c.loginUser(target) {
		return nil, perr("forbidden", "Bu hesabın parolası değiştirilemez.")
	}
	if !ValidNewPassword(a.New) {
		return nil, perr("invalid", "Yeni parola en az 6 karakter olmalıdır.")
	}
	ok, err := CheckShadow(c.Shadow, target, a.Old)
	if err != nil && !errors.Is(err, ErrUnsupportedHash) {
		return nil, err
	}
	if !ok {
		if c.FailDelay > 0 {
			time.Sleep(c.FailDelay)
		}
		return nil, perr("forbidden", "Mevcut parola hatalı.")
	}
	h, err := HashPassword(a.New)
	if err != nil {
		return nil, err
	}
	if err := SetShadowHash(c.Shadow, target, h, time.Now()); err != nil {
		return nil, err
	}
	return struct{}{}, nil
}

func (c *Commands) hostname(ctx context.Context, _ Peer, raw json.RawMessage) (any, error) {
	var a HostnameArgs
	if err := decode(raw, &a); err != nil {
		return nil, err
	}
	if !ValidHostname(a.Hostname) {
		return nil, perr("invalid", "Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir.")
	}
	if conn, err := sysbus.Get(); err == nil {
		obj := conn.Object("org.freedesktop.hostname1", "/org/freedesktop/hostname1")
		if obj.CallWithContext(ctx, "org.freedesktop.hostname1.SetStaticHostname", 0, a.Hostname, false).Err == nil {
			_ = obj.CallWithContext(ctx, "org.freedesktop.hostname1.SetHostname", 0, a.Hostname, false).Err
			c.updateHosts(a.Hostname)
			return struct{}{}, nil
		}
	}
	if err := os.WriteFile(c.Hostname, []byte(a.Hostname+"\n"), 0o644); err != nil {
		return nil, err
	}
	if err := syscall.Sethostname([]byte(a.Hostname)); err != nil {
		return nil, err
	}
	c.updateHosts(a.Hostname)
	return struct{}{}, nil
}

// updateHosts /etc/hosts içindeki 127.0.1.1 satırını günceller (yoksa ekler).
func (c *Commands) updateHosts(name string) {
	data, err := os.ReadFile(c.Hosts)
	if err != nil {
		return
	}
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	found := false
	for i, l := range lines {
		if strings.HasPrefix(l, "127.0.1.1") {
			lines[i] = "127.0.1.1\t" + name
			found = true
		}
	}
	if !found {
		lines = append(lines, "127.0.1.1\t"+name)
	}
	_ = os.WriteFile(c.Hosts, []byte(strings.Join(lines, "\n")+"\n"), 0o644)
}

func (c *Commands) power(ctx context.Context, _ Peer, raw json.RawMessage) (any, error) {
	var a PowerArgs
	if err := decode(raw, &a); err != nil {
		return nil, err
	}
	if a.Action == "tryboot" {
		// Pi firmware'e tek seferlik deneme açılışı bayrağı: reboot argümanı "0 tryboot".
		syscall.Sync()
		if err := exec.CommandContext(ctx, "/usr/bin/systemctl", "reboot", "0 tryboot").Run(); err != nil {
			return nil, err
		}
		return struct{}{}, nil
	}
	method := map[string]string{"reboot": "Reboot", "poweroff": "PowerOff"}[a.Action]
	if method == "" {
		return nil, perr("invalid", "Geçersiz güç işlemi.")
	}
	syscall.Sync()
	if conn, err := sysbus.Get(); err == nil {
		obj := conn.Object("org.freedesktop.login1", "/org/freedesktop/login1")
		if obj.CallWithContext(ctx, "org.freedesktop.login1.Manager."+method, 0, false).Err == nil {
			return struct{}{}, nil
		}
	}
	if err := exec.CommandContext(ctx, "/usr/bin/systemctl", a.Action).Run(); err != nil {
		return nil, err
	}
	return struct{}{}, nil
}

// --- OTA (RAUC) ---------------------------------------------------------------
// Kurulumu RAUC hizmeti (de.pengutronix.rauc, kök) yapar; makerd-priv yalnız başlatır ve durumu okur
// (kısa ömürlü; boşta kapanır). Paket imzası RAUC'ta /etc/rauc/keyring.pem ile doğrulanır.

const (
	raucService = "de.pengutronix.rauc"
	raucIface   = "de.pengutronix.rauc.Installer"
)

// ValidOTAPath yalnız OTADir altındaki düz .raucb dosyalarını kabul eder.
func ValidOTAPath(p string) bool {
	if !strings.HasPrefix(p, OTADir+"/") || !strings.HasSuffix(p, ".raucb") {
		return false
	}
	rest := strings.TrimPrefix(p, OTADir+"/")
	return rest != "" && !strings.ContainsAny(rest, "/\\\x00") && !strings.HasPrefix(rest, ".")
}

func (c *Commands) otaInstall(ctx context.Context, _ Peer, raw json.RawMessage) (any, error) {
	var a OTAInstallArgs
	if err := decode(raw, &a); err != nil {
		return nil, err
	}
	if !ValidOTAPath(a.Path) {
		return nil, perr("invalid", "Geçersiz güncelleme paketi yolu.")
	}
	if fi, err := os.Lstat(a.Path); err != nil || !fi.Mode().IsRegular() {
		return nil, perr("not_found", "Güncelleme paketi bulunamadı.")
	}
	conn, err := sysbus.Get()
	if err != nil {
		return nil, perr("internal", "Sistem veriyolu yok: "+err.Error())
	}
	obj := conn.Object(raucService, "/")
	if call := obj.CallWithContext(ctx, raucIface+".InstallBundle", 0, a.Path, map[string]dbus.Variant{}); call.Err != nil {
		return nil, perr("internal", "Kurulum başlatılamadı: "+call.Err.Error())
	}
	return struct{}{}, nil
}

func (c *Commands) otaStatus(ctx context.Context, _ Peer, _ json.RawMessage) (any, error) {
	conn, err := sysbus.Get()
	if err != nil {
		return nil, perr("internal", "Sistem veriyolu yok: "+err.Error())
	}
	obj := conn.Object(raucService, "/")
	var st OTAStatus
	if v, err := obj.GetProperty(raucIface + ".Operation"); err == nil {
		st.Operation, _ = v.Value().(string)
	} else {
		return nil, perr("internal", "RAUC durumu okunamadı: "+err.Error())
	}
	if v, err := obj.GetProperty(raucIface + ".LastError"); err == nil {
		st.LastError, _ = v.Value().(string)
	}
	if v, err := obj.GetProperty(raucIface + ".Progress"); err == nil {
		// (isi): yüzde, ileti, iç içelik derinliği
		if s, ok := v.Value().([]any); ok && len(s) >= 2 {
			if p, ok := s[0].(int32); ok {
				st.Progress = int(p)
			}
			st.Message, _ = s[1].(string)
		}
	}
	return st, nil
}
