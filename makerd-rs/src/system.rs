//! Sistem özeti (/proc, /sys, statfs) ve hostnamed/timedated/localed/logind
//! üzerinden ayarlar. Parola ve gerekirse ad/güç makerd-priv'e gider.

use crate::apierr::{ApiError, ApiResult};
use crate::dbus;
use crate::privsep::{self, client::Client};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use zbus::zvariant::OwnedValue;

/// GET /system gövdesi.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Info {
    pub hostname: String,
    pub os: OsInfo,
    pub kernel: String,
    pub model: String,
    pub uptime_s: i64,
    pub cpu: CpuInfo,
    pub mem: MemInfo,
    pub disk: Disk,
    pub throttled: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
}

/// İşlemci özeti. temp_c ölçülemezse null.
#[derive(Serialize, Clone, Debug, Default)]
pub struct CpuInfo {
    pub cores: usize,
    pub freq_mhz: i64,
    pub temp_c: Option<f64>,
    pub load: [f64; 3],
    /// Anlık toplam CPU kullanımı (%), iki /proc/stat örneği arasındaki farktan.
    pub usage_pct: f64,
}

/// Bellek (bayt).
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total: i64,
    pub used: i64,
    pub available: i64,
    pub zram_used: i64,
}

/// Kullanıcı verisi bölümü (/data, yoksa /).
#[derive(Serialize, Clone, Debug, Default)]
pub struct Disk {
    pub total: i64,
    pub used: i64,
}

/// GET/PUT /system/time gövdesi.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Time {
    pub now: String,
    pub timezone: String,
    pub ntp: bool,
}

/// PUT /system/time isteği; boş alanlar değiştirilmez.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct TimeUpdate {
    #[serde(default)]
    pub now: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub ntp: Option<bool>,
}

/// GET/PUT /system/locale gövdesi.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Locale {
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub keymap: String,
}

/// Sistem arka ucu (gerçek veya mock).
pub trait Provider: Send + Sync {
    fn info(&self) -> BoxFuture<'_, ApiResult<Info>>;
    fn set_hostname<'a>(&'a self, name: &'a str) -> BoxFuture<'a, ApiResult<()>>;
    fn time(&self) -> BoxFuture<'_, ApiResult<Time>>;
    fn set_time(&self, u: TimeUpdate) -> BoxFuture<'_, ApiResult<()>>;
    fn timezones(&self) -> BoxFuture<'_, ApiResult<Vec<String>>>;
    fn locale(&self) -> BoxFuture<'_, ApiResult<Locale>>;
    fn set_locale(&self, l: Locale) -> BoxFuture<'_, ApiResult<()>>;
    fn set_password<'a>(&'a self, user: &'a str, old: &'a str, new: &'a str) -> BoxFuture<'a, ApiResult<()>>;
    fn power<'a>(&'a self, action: &'a str) -> BoxFuture<'a, ApiResult<()>>;
}

fn re(p: &'static str, cell: &'static OnceLock<regex::Regex>) -> &'static regex::Regex {
    cell.get_or_init(|| regex::Regex::new(p).unwrap())
}

/// Yerel ayar adı biçimi.
pub fn valid_lang(s: &str) -> bool {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    re(r"^(C|POSIX|[a-z]{2,3}(_[A-Z]{2})?)(\.(UTF-8|utf8))?(@[a-z]+)?$", &R).is_match(s)
}

/// Klavye düzeni adı.
pub fn valid_keymap(s: &str) -> bool {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    re(r"^[A-Za-z0-9_.-]{1,32}$", &R).is_match(s)
}

/// Saat dilimi adı biçimi.
pub fn valid_timezone(s: &str) -> bool {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    s.len() < 64 && re(r"^[A-Za-z0-9_+-]+(/[A-Za-z0-9_+-]+){0,2}$", &R).is_match(s)
}

/// Sistemde liste bulunamazsa kullanılan saat dilimleri.
pub const FALLBACK_TIMEZONES: &[&str] = &[
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
];

/// Kurulum sihirbazındaki seçenek (dil, klavye).
#[derive(Serialize, Clone, Copy, Debug)]
pub struct Opt {
    pub id: &'static str,
    pub name: &'static str,
}

const fn o(id: &'static str, name: &'static str) -> Opt {
    Opt { id, name }
}

/// Kurulumda sunulan diller (LANG değeri, yerel ad).
pub const LANGUAGES: &[Opt] = &[
    o("tr_TR.UTF-8", "Türkçe"), o("en_US.UTF-8", "English (US)"), o("en_GB.UTF-8", "English (UK)"),
    o("de_DE.UTF-8", "Deutsch"), o("fr_FR.UTF-8", "Français"), o("es_ES.UTF-8", "Español"),
    o("it_IT.UTF-8", "Italiano"), o("nl_NL.UTF-8", "Nederlands"), o("pt_BR.UTF-8", "Português (Brasil)"),
    o("ru_RU.UTF-8", "Русский"), o("uk_UA.UTF-8", "Українська"), o("ar_SA.UTF-8", "العربية"),
    o("az_AZ.UTF-8", "Azərbaycan dili"), o("C.UTF-8", "C (POSIX)"),
];

/// Kurulumda sunulan konsol klavye düzenleri (localed VConsoleKeymap).
pub const KEYMAPS: &[Opt] = &[
    o("trq", "Türkçe Q"), o("trf", "Türkçe F"), o("us", "English (US)"), o("uk", "English (UK)"),
    o("de", "Deutsch"), o("fr", "Français"), o("es", "Español"), o("it", "Italiano"),
    o("nl", "Nederlands"), o("br-abnt2", "Português (Brasil)"), o("ru", "Русский"), o("ua", "Українська"),
    o("az", "Azərbaycan"),
];

/// /proc/stat "cpu" satırından toplam ve boşta jiffy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuSample {
    pub total: u64,
    pub idle: u64,
}

/// /proc/stat içeriğinin ilk "cpu " satırı.
pub fn parse_cpu_stat(s: &str) -> Option<CpuSample> {
    for l in s.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 5 || f[0] != "cpu" {
            continue;
        }
        let mut c = CpuSample::default();
        for (i, v) in f[1..].iter().enumerate() {
            let n: u64 = v.parse().ok()?;
            if i >= 8 {
                break; // guest, guest_nice user'a zaten dahil
            }
            c.total += n;
            if i == 3 || i == 4 {
                c.idle += n; // idle, iowait
            }
        }
        return Some(c);
    }
    None
}

/// İki örnek arasındaki kullanım yüzdesi (0-100, bir ondalık).
pub fn cpu_usage(prev: CpuSample, cur: CpuSample) -> f64 {
    if cur.total <= prev.total {
        return 0.0;
    }
    let dt = (cur.total - prev.total) as f64;
    let di = (cur.idle - cur.idle.min(prev.idle)) as f64;
    (((dt - di) / dt * 100.0).clamp(0.0, 100.0) * 10.0).round() / 10.0
}

/// /proc/meminfo → bayt.
pub fn parse_meminfo(s: &str) -> std::collections::HashMap<String, i64> {
    s.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let mut f = v.split_whitespace();
            let mut n: i64 = f.next()?.parse().unwrap_or(0);
            if f.next() == Some("kB") {
                n *= 1024;
            }
            Some((k.to_string(), n))
        })
        .collect()
}

#[derive(Default)]
struct CpuState {
    prev: CpuSample,
    at: Option<Instant>,
    last: f64,
}

/// Gerçek sistemden okur. `root` testlerde fikstür kökü.
pub struct Real {
    pub root: PathBuf,
    /// MakerOS sürümü (os-release yoksa).
    pub version: String,
    pub priv_client: Client,
    cpu: Mutex<CpuState>,
}

const HOSTNAMED: &str = "org.freedesktop.hostname1";
const TIMEDATED: &str = "org.freedesktop.timedate1";
const LOCALED: &str = "org.freedesktop.locale1";
const LOGIND: &str = "org.freedesktop.login1";

async fn bus_call<B>(dest: &str, iface: &str, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    dbus::call(dest, &dbus::path_of(dest), iface, method, body).await
}

async fn bus_prop<T: TryFrom<OwnedValue>>(dest: &str, name: &str) -> Option<T> {
    dbus::prop(dest, &dbus::path_of(dest), dest, name).await
}

fn dbus_err(e: zbus::Error, what: &str) -> ApiError {
    let n = dbus::err_name(&e);
    if n.ends_with("AccessDenied") || n.ends_with("InteractiveAuthorizationRequired") {
        ApiError::forbidden(format!("{what}: yetki yok."))
    } else if n.ends_with("InvalidArgs") {
        ApiError::invalid(format!("{what}: geçersiz değer."))
    } else {
        ApiError::unavailable(format!("{what}: {e}"))
    }
}

impl Real {
    pub fn new(root: impl Into<PathBuf>, version: &str, priv_client: Client) -> Self {
        Self { root: root.into(), version: version.into(), priv_client, cpu: Mutex::new(CpuState::default()) }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p.trim_start_matches('/'))
    }

    fn read(&self, p: &str) -> String {
        std::fs::read_to_string(self.path(p)).map(|s| s.trim_end_matches('\0').trim().to_string()).unwrap_or_default()
    }

    /// Önceki örnek yoksa veya bir dakikadan eskiyse 100 ms arayla iki örnek
    /// alınır; son örnek 200 ms'den yeniyse son değer döner.
    async fn cpu_usage(&self) -> f64 {
        let (prev, fresh) = {
            let c = self.cpu.lock().unwrap();
            match c.at {
                Some(at) if at.elapsed() < Duration::from_millis(200) => return c.last,
                Some(at) if at.elapsed() <= Duration::from_secs(60) => (Some(c.prev), false),
                _ => (None, true),
            }
        };
        let Some(mut cur) = parse_cpu_stat(&self.read("/proc/stat")) else { return 0.0 };
        let prev = match prev {
            Some(p) if !fresh => p,
            _ => {
                let p = cur;
                tokio::time::sleep(Duration::from_millis(100)).await;
                cur = parse_cpu_stat(&self.read("/proc/stat")).unwrap_or(p);
                p
            }
        };
        let v = cpu_usage(prev, cur);
        let mut c = self.cpu.lock().unwrap();
        *c = CpuState { prev: cur, at: Some(Instant::now()), last: v };
        v
    }

    fn os_version(&self) -> String {
        self.read("/etc/os-release")
            .lines()
            .find_map(|l| l.strip_prefix("VERSION_ID="))
            .map(|v| v.trim_matches(['"', '\'']).to_string())
            .unwrap_or_else(|| self.version.clone())
    }

    fn mem(&self) -> MemInfo {
        let m = parse_meminfo(&self.read("/proc/meminfo"));
        let total = m.get("MemTotal").copied().unwrap_or(0);
        let avail = m.get("MemAvailable").or(m.get("MemFree")).copied().unwrap_or(0);
        let mut mi = MemInfo { total, available: avail, used: (total - avail).max(0), zram_used: 0 };
        if let Ok(rd) = std::fs::read_dir(self.path("/sys/block")) {
            for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("zram")) {
                let s = std::fs::read_to_string(e.path().join("mm_stat")).unwrap_or_default();
                // mem_used_total
                mi.zram_used += s.split_whitespace().nth(2).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            }
        }
        mi
    }

    fn disk(&self) -> Disk {
        let data = self.path("/data");
        let target = if data.is_dir() { data } else { self.root.clone() };
        match nix::sys::statfs::statfs(&target) {
            Ok(s) => {
                let bs = s.block_size() as i64;
                let total = s.blocks() as i64 * bs;
                Disk { total, used: total - s.blocks_free() as i64 * bs }
            }
            Err(_) => Disk::default(),
        }
    }

    pub async fn read_info(&self) -> Info {
        let mut i = Info {
            hostname: nix::unistd::gethostname().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default(),
            os: OsInfo { name: "MakerOS".into(), version: self.os_version() },
            kernel: nix::sys::utsname::uname().map(|u| u.release().to_string_lossy().into_owned()).unwrap_or_default(),
            model: self.read("/proc/device-tree/model"),
            ..Default::default()
        };
        i.uptime_s = self.read("/proc/uptime").split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as i64;
        i.cpu.cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        i.cpu.freq_mhz = self.read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").parse::<i64>().map(|k| k / 1000).unwrap_or(0);
        i.cpu.temp_c = self.read("/sys/class/thermal/thermal_zone0/temp").parse::<i64>().ok().map(|mc| (mc as f64 / 100.0).round() / 10.0);
        let la: Vec<f64> = self.read("/proc/loadavg").split_whitespace().take(3).filter_map(|v| v.parse().ok()).collect();
        if la.len() == 3 {
            i.cpu.load = [la[0], la[1], la[2]];
        }
        i.cpu.usage_pct = self.cpu_usage().await;
        i.mem = self.mem();
        i.disk = self.disk();
        // Anlık bitler: düşük gerilim, frekans sınırı, kısma, sıcaklık sınırı.
        i.throttled = u32::from_str_radix(self.read("/sys/devices/platform/soc/soc:firmware/get_throttled").trim_start_matches("0x"), 16).is_ok_and(|v| v & 0xF != 0);
        i
    }

    fn local_timezone(&self) -> String {
        std::fs::read_link(self.path("/etc/localtime"))
            .ok()
            .and_then(|l| l.to_string_lossy().split_once("zoneinfo/").map(|(_, tz)| tz.to_string()))
            .unwrap_or_else(|| "UTC".into())
    }

    fn tz_tables(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::from(["UTC".to_string()]);
        for f in ["/usr/share/zoneinfo/tzdata.zi", "/usr/share/zoneinfo/zone1970.tab", "/usr/share/zoneinfo/zone.tab"] {
            for l in self.read(f).lines().filter(|l| !l.starts_with('#')) {
                if let Some(z) = l.split('\t').nth(2) {
                    seen.insert(z.to_string());
                }
            }
        }
        if seen.len() == 1 {
            return FALLBACK_TIMEZONES.iter().map(|s| s.to_string()).collect();
        }
        seen.into_iter().collect()
    }

    async fn priv_call<A: Serialize>(&self, cmd: &str, args: &A) -> ApiResult<()> {
        self.priv_client.call::<_, serde_json::Value>(cmd, args).await.map(|_| ()).map_err(ApiError::from)
    }
}

impl Provider for Real {
    fn info(&self) -> BoxFuture<'_, ApiResult<Info>> {
        Box::pin(async { Ok(self.read_info().await) })
    }

    /// hostnamed ile; yetki yoksa makerd-priv ile.
    fn set_hostname<'a>(&'a self, name: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            if !privsep::valid_hostname(name) {
                return Err(ApiError::invalid("Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir (en fazla 63)."));
            }
            if bus_call(HOSTNAMED, HOSTNAMED, "SetStaticHostname", &(name, false)).await.is_ok() {
                let _ = bus_call(HOSTNAMED, HOSTNAMED, "SetHostname", &(name, false)).await;
                return Ok(());
            }
            self.priv_call(privsep::CMD_SYSTEM_HOSTNAME, &privsep::HostnameArgs { hostname: name.into() }).await
        })
    }

    fn time(&self) -> BoxFuture<'_, ApiResult<Time>> {
        Box::pin(async {
            let mut t = Time { now: crate::util::now_rfc3339_local(), timezone: self.local_timezone(), ntp: false };
            if let Some(tz) = bus_prop::<String>(TIMEDATED, "Timezone").await.filter(|s| !s.is_empty()) {
                t.timezone = tz;
            }
            t.ntp = bus_prop::<bool>(TIMEDATED, "NTP").await.unwrap_or(false);
            Ok(t)
        })
    }

    /// Saat dilimi, NTP ve (NTP kapalıyken) saat.
    fn set_time(&self, u: TimeUpdate) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            dbus::system().await.map(|_| ()).map_err(|_| ApiError::unavailable("Sistem veriyoluna (D-Bus) bağlanılamadı."))?;
            if let Some(tz) = &u.timezone {
                if !valid_timezone(tz) {
                    return Err(ApiError::invalid("Geçersiz saat dilimi."));
                }
                bus_call(TIMEDATED, TIMEDATED, "SetTimezone", &(tz.as_str(), false)).await.map_err(|e| dbus_err(e, "Saat dilimi"))?;
            }
            if let Some(ntp) = u.ntp {
                bus_call(TIMEDATED, TIMEDATED, "SetNTP", &(ntp, false)).await.map_err(|e| dbus_err(e, "Otomatik saat"))?;
            }
            if let Some(now) = &u.now {
                let when: jiff::Timestamp = now.parse().map_err(|_| ApiError::invalid("Saat RFC 3339 biçiminde olmalıdır."))?;
                bus_call(TIMEDATED, TIMEDATED, "SetTime", &(when.as_microsecond(), false, false)).await.map_err(|e| dbus_err(e, "Saat"))?;
            }
            Ok(())
        })
    }

    /// timedated ListTimezones, yoksa tzdata tabloları.
    fn timezones(&self) -> BoxFuture<'_, ApiResult<Vec<String>>> {
        Box::pin(async {
            if let Ok(m) = bus_call(TIMEDATED, TIMEDATED, "ListTimezones", &()).await {
                if let Ok(z) = m.body().deserialize::<Vec<String>>() {
                    if !z.is_empty() {
                        return Ok(z);
                    }
                }
            }
            Ok(self.tz_tables())
        })
    }

    fn locale(&self) -> BoxFuture<'_, ApiResult<Locale>> {
        Box::pin(async {
            let mut l = Locale { lang: "C.UTF-8".into(), keymap: "us".into() };
            if let Some(list) = bus_prop::<Vec<String>>(LOCALED, "Locale").await {
                if let Some(v) = list.iter().rev().find_map(|kv| kv.strip_prefix("LANG=")) {
                    l.lang = v.into();
                }
            }
            if let Some(k) = bus_prop::<String>(LOCALED, "VConsoleKeymap").await.filter(|s| !s.is_empty()) {
                l.keymap = k;
            }
            Ok(l)
        })
    }

    /// localed ile dil ve klavye düzeni (X11 eşlemesiyle).
    fn set_locale(&self, l: Locale) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            if !l.lang.is_empty() && !valid_lang(&l.lang) {
                return Err(ApiError::invalid("Geçersiz dil."));
            }
            if !l.keymap.is_empty() && !valid_keymap(&l.keymap) {
                return Err(ApiError::invalid("Geçersiz klavye düzeni."));
            }
            dbus::system().await.map(|_| ()).map_err(|_| ApiError::unavailable("Sistem veriyoluna (D-Bus) bağlanılamadı."))?;
            if !l.lang.is_empty() {
                bus_call(LOCALED, LOCALED, "SetLocale", &(vec![format!("LANG={}", l.lang)], false)).await.map_err(|e| dbus_err(e, "Dil"))?;
            }
            if !l.keymap.is_empty() {
                bus_call(LOCALED, LOCALED, "SetVConsoleKeyboard", &(l.keymap.as_str(), "", true, false)).await.map_err(|e| dbus_err(e, "Klavye"))?;
            }
            Ok(())
        })
    }

    fn set_password<'a>(&'a self, user: &'a str, old: &'a str, new: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            if new.len() < privsep::MIN_PASSWORD_LEN {
                return Err(ApiError::invalid("Yeni parola en az 6 karakter olmalıdır."));
            }
            self.priv_call(privsep::CMD_SYSTEM_PASSWORD, &privsep::PasswordArgs { user: user.into(), old: old.into(), new: new.into() }).await
        })
    }

    /// logind ile; yetki yoksa makerd-priv ile.
    fn power<'a>(&'a self, action: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let method = match action {
                "reboot" => "Reboot",
                "poweroff" => "PowerOff",
                _ => return Err(ApiError::invalid("Geçersiz güç işlemi.")),
            };
            if bus_call(LOGIND, &format!("{LOGIND}.Manager"), method, &(false,)).await.is_ok() {
                return Ok(());
            }
            self.priv_call(privsep::CMD_POWER, &privsep::PowerArgs { action: action.into() }).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn info_fixtures() {
        let r = Real::new("testdata/system", "", Client::new("/yok"));
        let i = r.read_info().await;
        assert_eq!((i.os.name.as_str(), i.os.version.as_str(), i.model.as_str(), i.uptime_s), ("MakerOS", "2.0.0", "Raspberry Pi 4 Model B Rev 1.5", 3600));
        assert_eq!(i.cpu.temp_c, Some(47.2));
        assert_eq!(i.cpu.load, [0.52, 0.40, 0.31]);
        assert_eq!(i.mem, MemInfo { total: 3981000 * 1024, available: 3000000 * 1024, used: 981000 * 1024, zram_used: 8192 });
        assert!(i.throttled);
        assert!(i.disk.total > 0);
    }

    #[test]
    fn validators() {
        for (s, w) in [("tr_TR.UTF-8", true), ("C.UTF-8", true), ("en_US", true), ("x;rm", false)] {
            assert_eq!(valid_lang(s), w, "{s}");
        }
        for (s, w) in [("Europe/Istanbul", true), ("America/Argentina/Buenos_Aires", true), ("UTC", true), ("../etc", false)] {
            assert_eq!(valid_timezone(s), w, "{s}");
        }
        assert!(valid_keymap("br-abnt2") && !valid_keymap("a b"));
    }

    #[test]
    fn cpu() {
        let a = parse_cpu_stat("cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3 4 5\n").unwrap();
        assert_eq!(a, CpuSample { total: 1000, idle: 800 });
        let b = parse_cpu_stat("cpu  150 0 150 750 150 0 0 0 5 5\n").unwrap();
        assert_eq!(cpu_usage(a, b), 50.0);
        assert_eq!(cpu_usage(b, a), 0.0);
        assert_eq!(cpu_usage(a, a), 0.0);
        assert!(parse_cpu_stat("intr 1\n").is_none());
    }

    #[tokio::test]
    async fn priv_fallback_errors() {
        let r = Real::new("/", "", Client::new("/yok/priv.sock"));
        assert_eq!(r.set_password("maker", "a", "kisa").await.unwrap_err().code, "invalid");
        assert_eq!(r.power("uyku").await.unwrap_err().code, "invalid");
        assert_eq!(r.set_hostname("Kötü Ad").await.unwrap_err().code, "invalid");
        // D-Bus yok, makerd-priv yok: erişilemez.
        assert_eq!(r.set_password("maker", "eski-parola", "yeni-parola").await.unwrap_err().code, "unavailable");
    }
}
