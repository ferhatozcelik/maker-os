package system

import (
	"bufio"
	"context"
	"errors"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
	"github.com/ferhatozcelik/maker-os/makerd/internal/sysbus"
	"github.com/godbus/dbus/v5"
)

// PrivCaller makerd-priv istemcisidir.
type PrivCaller interface {
	Call(ctx context.Context, cmd string, args, out any) error
}

// Real gerçek sistemden okur. Root, testlerde fikstür kökü olarak kullanılır.
type Real struct {
	Root    string // "/" (testte testdata dizini)
	Version string // MakerOS sürümü (os-release yoksa)
	Priv    PrivCaller

	cpuMu   sync.Mutex
	cpuPrev CPUSample
	cpuAt   time.Time
	cpuLast float64
}

// cpuUsage önceki örnekle farkı hesaplar. Önceki örnek yoksa veya bir
// dakikadan eskiyse 100 ms arayla iki örnek alır; 200 ms'den yeniyse son
// değeri döndürür.
func (r *Real) cpuUsage() float64 {
	r.cpuMu.Lock()
	defer r.cpuMu.Unlock()
	age := time.Since(r.cpuAt)
	if !r.cpuAt.IsZero() && age < 200*time.Millisecond {
		return r.cpuLast
	}
	cur, ok := ParseCPUStat(r.read("/proc/stat"))
	if !ok {
		return 0
	}
	prev := r.cpuPrev
	if r.cpuAt.IsZero() || age > time.Minute {
		prev = cur
		time.Sleep(100 * time.Millisecond)
		cur, _ = ParseCPUStat(r.read("/proc/stat"))
	}
	r.cpuPrev, r.cpuAt = cur, time.Now()
	r.cpuLast = CPUUsage(prev, cur)
	return r.cpuLast
}

func (r *Real) path(p string) string { return filepath.Join(r.Root, p) }

func (r *Real) read(p string) string {
	b, err := os.ReadFile(r.path(p))
	if err != nil {
		return ""
	}
	return strings.TrimSpace(strings.TrimRight(string(b), "\x00"))
}

// Info sistem özetini toplar.
func (r *Real) Info(_ context.Context) (Info, error) {
	var in Info
	in.Hostname, _ = os.Hostname()
	in.OS = OSInfo{Name: "MakerOS", Version: r.osVersion()}
	var u syscall.Utsname
	if syscall.Uname(&u) == nil {
		in.Kernel = cstr(u.Release[:])
	}
	in.Model = r.read("/proc/device-tree/model")
	if f := strings.Fields(r.read("/proc/uptime")); len(f) > 0 {
		v, _ := strconv.ParseFloat(f[0], 64)
		in.UptimeS = int64(v)
	}
	in.CPU.Cores = runtime.NumCPU()
	if khz, err := strconv.Atoi(r.read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")); err == nil {
		in.CPU.FreqMHz = khz / 1000
	}
	if mc, err := strconv.Atoi(r.read("/sys/class/thermal/thermal_zone0/temp")); err == nil {
		t := math.Round(float64(mc)/100) / 10
		in.CPU.TempC = &t
	}
	if f := strings.Fields(r.read("/proc/loadavg")); len(f) >= 3 {
		for i := 0; i < 3; i++ {
			in.CPU.Load[i], _ = strconv.ParseFloat(f[i], 64)
		}
	}
	in.CPU.UsagePct = r.cpuUsage()
	in.Mem = r.mem()
	in.Disk = r.disk()
	if v, err := strconv.ParseUint(strings.TrimPrefix(r.read("/sys/devices/platform/soc/soc:firmware/get_throttled"), "0x"), 16, 32); err == nil {
		in.Throttled = v&0xF != 0 // anlık: düşük gerilim, frekans sınırı, kısma, sıcaklık sınırı
	}
	return in, nil
}

func cstr(b []int8) string {
	out := make([]byte, 0, len(b))
	for _, c := range b {
		if c == 0 {
			break
		}
		out = append(out, byte(c))
	}
	return string(out)
}

func (r *Real) osVersion() string {
	for _, l := range strings.Split(r.read("/etc/os-release"), "\n") {
		if v, ok := strings.CutPrefix(l, "VERSION_ID="); ok {
			return strings.Trim(v, `"'`)
		}
	}
	return r.Version
}

// ParseMeminfo /proc/meminfo içeriğini bayt cinsinden haritaya çevirir.
func ParseMeminfo(s string) map[string]int64 {
	m := map[string]int64{}
	sc := bufio.NewScanner(strings.NewReader(s))
	for sc.Scan() {
		k, v, ok := strings.Cut(sc.Text(), ":")
		if !ok {
			continue
		}
		f := strings.Fields(v)
		if len(f) == 0 {
			continue
		}
		n, _ := strconv.ParseInt(f[0], 10, 64)
		if len(f) > 1 && f[1] == "kB" {
			n *= 1024
		}
		m[k] = n
	}
	return m
}

func (r *Real) mem() MemInfo {
	m := ParseMeminfo(r.read("/proc/meminfo"))
	avail, ok := m["MemAvailable"]
	if !ok {
		avail = m["MemFree"]
	}
	mi := MemInfo{Total: m["MemTotal"], Available: avail, Used: max(0, m["MemTotal"]-avail)}
	matches, _ := filepath.Glob(r.path("/sys/block/zram*/mm_stat"))
	for _, p := range matches {
		b, _ := os.ReadFile(p)
		if f := strings.Fields(string(b)); len(f) >= 3 {
			n, _ := strconv.ParseInt(f[2], 10, 64) // mem_used_total
			mi.ZramUsed += n
		}
	}
	return mi
}

func (r *Real) disk() Disk {
	target := r.path("/data")
	if st, err := os.Stat(target); err != nil || !st.IsDir() {
		target = r.path("/")
	}
	var s syscall.Statfs_t
	if syscall.Statfs(target, &s) != nil {
		return Disk{}
	}
	bs := int64(s.Bsize)
	total := int64(s.Blocks) * bs
	return Disk{Total: total, Used: total - int64(s.Bfree)*bs}
}

// --- systemd D-Bus ---------------------------------------------------------

const (
	hostnamed = "org.freedesktop.hostname1"
	timedated = "org.freedesktop.timedate1"
	localed   = "org.freedesktop.locale1"
	logind    = "org.freedesktop.login1"
)

func busObj(dest string) (dbus.BusObject, error) {
	c, err := sysbus.Get()
	if err != nil {
		return nil, apierr.Unavailable("Sistem veriyoluna (D-Bus) bağlanılamadı.")
	}
	return c.Object(dest, dbus.ObjectPath("/"+strings.ReplaceAll(dest, ".", "/"))), nil
}

func dbusErr(err error, what string) error {
	if err == nil {
		return nil
	}
	var de dbus.Error
	if errors.As(err, &de) && (strings.HasSuffix(de.Name, "AccessDenied") || strings.HasSuffix(de.Name, "InteractiveAuthorizationRequired")) {
		return apierr.Forbidden(what + ": yetki yok.")
	}
	if errors.As(err, &de) && strings.HasSuffix(de.Name, "InvalidArgs") {
		return apierr.Invalid(what + ": geçersiz değer.")
	}
	return apierr.Unavailable(what + ": " + err.Error())
}

// SetHostname hostnamed ile; yetki yoksa makerd-priv ile ayarlar.
func (r *Real) SetHostname(ctx context.Context, name string) error {
	if !priv.ValidHostname(name) {
		return apierr.Invalid("Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir (en fazla 63).")
	}
	if obj, err := busObj(hostnamed); err == nil {
		if obj.CallWithContext(ctx, hostnamed+".SetStaticHostname", 0, name, false).Err == nil {
			_ = obj.CallWithContext(ctx, hostnamed+".SetHostname", 0, name, false).Err
			return nil
		}
	}
	return privErr(r.Priv.Call(ctx, priv.CmdSystemHostname, priv.HostnameArgs{Hostname: name}, nil))
}

// Time timedated'den saat dilimi ve NTP durumunu okur.
func (r *Real) Time(_ context.Context) (Time, error) {
	t := Time{Now: time.Now().Format(time.RFC3339), Timezone: r.localTimezone()}
	obj, err := busObj(timedated)
	if err != nil {
		return t, nil
	}
	if v, err := obj.GetProperty(timedated + ".Timezone"); err == nil {
		if s, ok := v.Value().(string); ok && s != "" {
			t.Timezone = s
		}
	}
	if v, err := obj.GetProperty(timedated + ".NTP"); err == nil {
		t.NTP, _ = v.Value().(bool)
	}
	return t, nil
}

func (r *Real) localTimezone() string {
	if link, err := os.Readlink(r.path("/etc/localtime")); err == nil {
		if _, tz, ok := strings.Cut(link, "zoneinfo/"); ok {
			return tz
		}
	}
	return "UTC"
}

// SetTime saat dilimi, NTP ve (NTP kapalıyken) saati ayarlar.
func (r *Real) SetTime(ctx context.Context, u TimeUpdate) error {
	obj, err := busObj(timedated)
	if err != nil {
		return err
	}
	if u.Timezone != nil {
		if !ValidTimezone(*u.Timezone) {
			return apierr.Invalid("Geçersiz saat dilimi.")
		}
		if err := dbusErr(obj.CallWithContext(ctx, timedated+".SetTimezone", 0, *u.Timezone, false).Err, "Saat dilimi"); err != nil {
			return err
		}
	}
	if u.NTP != nil {
		if err := dbusErr(obj.CallWithContext(ctx, timedated+".SetNTP", 0, *u.NTP, false).Err, "Otomatik saat"); err != nil {
			return err
		}
	}
	if u.Now != nil {
		when, err := time.Parse(time.RFC3339, *u.Now)
		if err != nil {
			return apierr.Invalid("Saat RFC 3339 biçiminde olmalıdır.")
		}
		if err := dbusErr(obj.CallWithContext(ctx, timedated+".SetTime", 0, when.UnixMicro(), false, false).Err, "Saat"); err != nil {
			return err
		}
	}
	return nil
}

// Timezones timedated ListTimezones, yoksa tzdata tablolarından.
func (r *Real) Timezones(ctx context.Context) ([]string, error) {
	if obj, err := busObj(timedated); err == nil {
		var zones []string
		if obj.CallWithContext(ctx, timedated+".ListTimezones", 0).Store(&zones) == nil && len(zones) > 0 {
			return zones, nil
		}
	}
	seen := map[string]bool{"UTC": true}
	for _, f := range []string{"/usr/share/zoneinfo/tzdata.zi", "/usr/share/zoneinfo/zone1970.tab", "/usr/share/zoneinfo/zone.tab"} {
		for _, l := range strings.Split(r.read(f), "\n") {
			if strings.HasPrefix(l, "#") {
				continue
			}
			if p := strings.Split(l, "\t"); len(p) >= 3 {
				seen[p[2]] = true
			}
		}
	}
	if len(seen) == 1 {
		return FallbackTimezones, nil
	}
	out := make([]string, 0, len(seen))
	for z := range seen {
		out = append(out, z)
	}
	sort.Strings(out)
	return out, nil
}

// Locale localed'den dil ve klavye düzenini okur.
func (r *Real) Locale(_ context.Context) (Locale, error) {
	l := Locale{Lang: "C.UTF-8", Keymap: "us"}
	obj, err := busObj(localed)
	if err != nil {
		return l, nil
	}
	if v, err := obj.GetProperty(localed + ".Locale"); err == nil {
		if list, ok := v.Value().([]string); ok {
			for _, kv := range list {
				if s, ok := strings.CutPrefix(kv, "LANG="); ok {
					l.Lang = s
				}
			}
		}
	}
	if v, err := obj.GetProperty(localed + ".VConsoleKeymap"); err == nil {
		if s, ok := v.Value().(string); ok && s != "" {
			l.Keymap = s
		}
	}
	return l, nil
}

// SetLocale localed ile dili ve klavye düzenini (X11 eşlemesiyle) ayarlar.
func (r *Real) SetLocale(ctx context.Context, l Locale) error {
	if l.Lang != "" && !ValidLang(l.Lang) {
		return apierr.Invalid("Geçersiz dil.")
	}
	if l.Keymap != "" && !ValidKeymap(l.Keymap) {
		return apierr.Invalid("Geçersiz klavye düzeni.")
	}
	obj, err := busObj(localed)
	if err != nil {
		return err
	}
	if l.Lang != "" {
		if err := dbusErr(obj.CallWithContext(ctx, localed+".SetLocale", 0, []string{"LANG=" + l.Lang}, false).Err, "Dil"); err != nil {
			return err
		}
	}
	if l.Keymap != "" {
		if err := dbusErr(obj.CallWithContext(ctx, localed+".SetVConsoleKeyboard", 0, l.Keymap, "", true, false).Err, "Klavye"); err != nil {
			return err
		}
	}
	return nil
}

// SetPassword makerd-priv üzerinden parolayı değiştirir.
func (r *Real) SetPassword(ctx context.Context, user, old, new string) error {
	if len(new) < priv.MinPasswordLen {
		return apierr.Invalid("Yeni parola en az 6 karakter olmalıdır.")
	}
	return privErr(r.Priv.Call(ctx, priv.CmdSystemPassword, priv.PasswordArgs{User: user, Old: old, New: new}, nil))
}

// Power logind ile; yetki yoksa makerd-priv ile yeniden başlatır/kapatır.
func (r *Real) Power(ctx context.Context, action string) error {
	method := map[string]string{"reboot": "Reboot", "poweroff": "PowerOff"}[action]
	if method == "" {
		return apierr.Invalid("Geçersiz güç işlemi.")
	}
	if obj, err := busObj(logind); err == nil {
		if obj.CallWithContext(ctx, logind+".Manager."+method, 0, false).Err == nil {
			return nil
		}
	}
	return privErr(r.Priv.Call(ctx, priv.CmdPower, priv.PowerArgs{Action: action}, nil))
}

// PrivError makerd-priv hatasını API hatasına çevirir.
func PrivError(err error) error { return privErr(err) }

// privErr makerd-priv hatasını API hatasına çevirir.
func privErr(err error) error {
	if err == nil {
		return nil
	}
	var pe *priv.Error
	if errors.As(err, &pe) {
		switch pe.Code {
		case "invalid":
			return apierr.Invalid(pe.Message)
		case "forbidden":
			return apierr.Forbidden(pe.Message)
		case "not_found":
			return apierr.NotFound(pe.Message)
		case "conflict":
			return apierr.Conflict(pe.Message)
		}
		return apierr.Internal(pe.Message)
	}
	return apierr.Unavailable("Yetkili yardımcıya (makerd-priv) ulaşılamadı.")
}
