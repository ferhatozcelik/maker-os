// Package system: sistem özeti (/proc, /sys, statfs) ve hostnamed/timedated/
// localed/logind üzerinden ayarlar. Parola ve gerekirse ad/güç makerd-priv'e gider.
package system

import (
	"context"
	"math"
	"regexp"
	"strconv"
	"strings"
)

// Info, GET /system gövdesidir.
type Info struct {
	Hostname  string  `json:"hostname"`
	OS        OSInfo  `json:"os"`
	Kernel    string  `json:"kernel"`
	Model     string  `json:"model"`
	UptimeS   int64   `json:"uptime_s"`
	CPU       CPUInfo `json:"cpu"`
	Mem       MemInfo `json:"mem"`
	Disk      Disk    `json:"disk"`
	Throttled bool    `json:"throttled"`
}

// OSInfo işletim sistemi adı ve sürümü.
type OSInfo struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

// CPUInfo işlemci özeti. TempC ölçülemezse null.
type CPUInfo struct {
	Cores   int        `json:"cores"`
	FreqMHz int        `json:"freq_mhz"`
	TempC   *float64   `json:"temp_c"`
	Load    [3]float64 `json:"load"`
	// UsagePct anlık toplam CPU kullanımı (%), iki /proc/stat örneği arasındaki farktan.
	UsagePct float64 `json:"usage_pct"`
}

// MemInfo bellek (bayt).
type MemInfo struct {
	Total     int64 `json:"total"`
	Used      int64 `json:"used"`
	Available int64 `json:"available"`
	ZramUsed  int64 `json:"zram_used"`
}

// Disk kullanıcı verisi bölümü (/data, yoksa /).
type Disk struct {
	Total int64 `json:"total"`
	Used  int64 `json:"used"`
}

// Time, GET/PUT /system/time gövdesi.
type Time struct {
	Now      string `json:"now"`
	Timezone string `json:"timezone"`
	NTP      bool   `json:"ntp"`
}

// TimeUpdate, PUT /system/time isteği; boş alanlar değiştirilmez.
type TimeUpdate struct {
	Now      *string `json:"now,omitempty"`
	Timezone *string `json:"timezone,omitempty"`
	NTP      *bool   `json:"ntp,omitempty"`
}

// Locale, GET/PUT /system/locale gövdesi.
type Locale struct {
	Lang   string `json:"lang"`
	Keymap string `json:"keymap"`
}

// Provider sistem arka ucudur (gerçek veya mock).
type Provider interface {
	Info(ctx context.Context) (Info, error)
	SetHostname(ctx context.Context, name string) error
	Time(ctx context.Context) (Time, error)
	SetTime(ctx context.Context, u TimeUpdate) error
	Timezones(ctx context.Context) ([]string, error)
	Locale(ctx context.Context) (Locale, error)
	SetLocale(ctx context.Context, l Locale) error
	SetPassword(ctx context.Context, user, old, new string) error
	Power(ctx context.Context, action string) error
}

var (
	langRe   = regexp.MustCompile(`^(C|POSIX|[a-z]{2,3}(_[A-Z]{2})?)(\.(UTF-8|utf8))?(@[a-z]+)?$`)
	keymapRe = regexp.MustCompile(`^[A-Za-z0-9_.-]{1,32}$`)
	tzRe     = regexp.MustCompile(`^[A-Za-z0-9_+-]+(/[A-Za-z0-9_+-]+){0,2}$`)
)

// ValidLang yerel ayar adı biçimini denetler.
func ValidLang(s string) bool { return langRe.MatchString(s) }

// ValidKeymap klavye düzeni adını denetler.
func ValidKeymap(s string) bool { return keymapRe.MatchString(s) }

// ValidTimezone saat dilimi adının biçimini denetler.
func ValidTimezone(s string) bool { return tzRe.MatchString(s) && len(s) < 64 }

// FallbackTimezones, sistemde liste bulunamazsa kullanılan saat dilimleridir.
var FallbackTimezones = []string{
	"Africa/Cairo", "Africa/Johannesburg", "Africa/Lagos", "Africa/Nairobi",
	"America/Argentina/Buenos_Aires", "America/Bogota", "America/Chicago", "America/Denver",
	"America/Los_Angeles", "America/Mexico_City", "America/New_York", "America/Sao_Paulo",
	"America/Toronto", "Asia/Baku", "Asia/Bangkok", "Asia/Dubai", "Asia/Hong_Kong",
	"Asia/Jakarta", "Asia/Karachi", "Asia/Kolkata", "Asia/Riyadh", "Asia/Seoul",
	"Asia/Shanghai", "Asia/Singapore", "Asia/Tashkent", "Asia/Tehran", "Asia/Tokyo",
	"Atlantic/Reykjavik", "Australia/Melbourne", "Australia/Perth", "Australia/Sydney",
	"Europe/Amsterdam", "Europe/Athens", "Europe/Berlin", "Europe/Brussels", "Europe/Bucharest",
	"Europe/Budapest", "Europe/Dublin", "Europe/Helsinki", "Europe/Istanbul", "Europe/Kyiv",
	"Europe/Lisbon", "Europe/London", "Europe/Madrid", "Europe/Moscow", "Europe/Paris",
	"Europe/Prague", "Europe/Rome", "Europe/Sofia", "Europe/Stockholm", "Europe/Vienna",
	"Europe/Warsaw", "Europe/Zurich", "Pacific/Auckland", "Pacific/Honolulu", "UTC",
}

// Option, kurulum sihirbazındaki seçeneklerdir (dil, klavye).
type Option struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

// Languages kurulumda sunulan dillerdir (LANG değeri, yerel ad).
var Languages = []Option{
	{"tr_TR.UTF-8", "Türkçe"}, {"en_US.UTF-8", "English (US)"}, {"en_GB.UTF-8", "English (UK)"},
	{"de_DE.UTF-8", "Deutsch"}, {"fr_FR.UTF-8", "Français"}, {"es_ES.UTF-8", "Español"},
	{"it_IT.UTF-8", "Italiano"}, {"nl_NL.UTF-8", "Nederlands"}, {"pt_BR.UTF-8", "Português (Brasil)"},
	{"ru_RU.UTF-8", "Русский"}, {"uk_UA.UTF-8", "Українська"}, {"ar_SA.UTF-8", "العربية"},
	{"az_AZ.UTF-8", "Azərbaycan dili"}, {"C.UTF-8", "C (POSIX)"},
}

// Keymaps kurulumda sunulan konsol klavye düzenleridir (localed VConsoleKeymap).
var Keymaps = []Option{
	{"trq", "Türkçe Q"}, {"trf", "Türkçe F"}, {"us", "English (US)"}, {"uk", "English (UK)"},
	{"de", "Deutsch"}, {"fr", "Français"}, {"es", "Español"}, {"it", "Italiano"},
	{"nl", "Nederlands"}, {"br-abnt2", "Português (Brasil)"}, {"ru", "Русский"}, {"ua", "Українська"},
	{"az", "Azərbaycan"},
}

// CPUSample /proc/stat "cpu" satırından toplam ve boşta jiffy sayısıdır.
type CPUSample struct{ Total, Idle uint64 }

// ParseCPUStat /proc/stat içeriğinin ilk "cpu " satırını ayrıştırır.
func ParseCPUStat(s string) (CPUSample, bool) {
	for _, l := range strings.Split(s, "\n") {
		f := strings.Fields(l)
		if len(f) < 5 || f[0] != "cpu" {
			continue
		}
		var c CPUSample
		for i, v := range f[1:] {
			n, err := strconv.ParseUint(v, 10, 64)
			if err != nil {
				return CPUSample{}, false
			}
			if i >= 8 { // guest, guest_nice user'a zaten dahil
				break
			}
			c.Total += n
			if i == 3 || i == 4 { // idle, iowait
				c.Idle += n
			}
		}
		return c, true
	}
	return CPUSample{}, false
}

// CPUUsage iki örnek arasındaki kullanım yüzdesidir (0-100, bir ondalık).
func CPUUsage(prev, cur CPUSample) float64 {
	if cur.Total <= prev.Total {
		return 0
	}
	dt := float64(cur.Total - prev.Total)
	di := float64(cur.Idle - min(cur.Idle, prev.Idle))
	return math.Round(max(0, min(100, (dt-di)/dt*100))*10) / 10
}
