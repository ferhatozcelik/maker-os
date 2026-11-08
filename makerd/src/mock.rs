//! --mock modu için sahte arka uçlar: kök yetkisi, NetworkManager veya D-Bus
//! gerektirmez. Önizleme (docker compose) ve testlerde kullanılır.
//! Kullanıcı: maker / parola: maker.

use crate::activity::{self, Process};
use crate::apierr::{ApiError, ApiResult};
use crate::apps::{self, App, Launcher};
use crate::auth::{CheckResult, Checker as AuthChecker};
use crate::network::{self, Ap, ConnectRequest, Iface, Progress};
use crate::privsep::{self, UserCreateArgs};
use crate::system::{self, CpuInfo, Disk, Info, Locale, MemInfo, OsInfo, Time, TimeUpdate};
use futures_util::future::BoxFuture;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const USER: &str = "maker";
pub const PASSWORD: &str = "maker";

/// Mock arka uçlarının paylaşılan durumu: kullanıcı → parola (hepsi yönetici).
#[derive(Clone, Default)]
pub struct State {
    users: Arc<Mutex<HashMap<String, String>>>,
}

impl State {
    /// Kurulumu tamamlanmış durum (maker/maker).
    pub fn new() -> Self {
        Self { users: Arc::new(Mutex::new(HashMap::from([(USER.to_string(), PASSWORD.to_string())]))) }
    }

    /// Hiç kullanıcısı olmayan (ilk açılış) durum.
    pub fn fresh() -> Self {
        Self::default()
    }

    /// Kurulum sihirbazının yönetici hesabı (bellekte).
    pub fn create_user(&self, a: &UserCreateArgs) -> ApiResult<()> {
        if !privsep::users::valid_username(&a.user) {
            return Err(ApiError::invalid("Geçersiz kullanıcı adı."));
        }
        if !privsep::users::valid_new_password(&a.password) {
            return Err(ApiError::invalid("Parola en az 6 karakter olmalıdır."));
        }
        if !privsep::users::valid_fullname(&a.fullname) {
            return Err(ApiError::invalid("Ad soyad geçersiz karakter içeriyor."));
        }
        self.users.lock().unwrap().insert(a.user.clone(), a.password.clone());
        Ok(())
    }
}

impl crate::httpd::Accounts for State {
    fn create_user(&self, a: UserCreateArgs) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move { State::create_user(self, &a) })
    }
}

// --- auth ------------------------------------------------------------------

/// Mock parola doğrulayıcı.
pub struct Checker(pub State);

impl AuthChecker for Checker {
    fn check<'a>(&'a self, user: &'a str, password: &'a str) -> BoxFuture<'a, ApiResult<CheckResult>> {
        let ok = self.0.users.lock().unwrap().get(user).is_some_and(|pw| crate::auth::constant_eq(password.as_bytes(), pw.as_bytes()));
        Box::pin(async move { Ok(CheckResult { ok, admin: ok }) })
    }
}

fn jitter(base: f64, amp: f64) -> f64 {
    ((base + (rand::random::<f64>() * 2.0 - 1.0) * amp) * 10.0).round() / 10.0
}

// --- system ----------------------------------------------------------------

struct SysState {
    hostname: String,
    tz: String,
    ntp: bool,
    /// Elle ayarlanan saatin gerçek saatten farkı (sn).
    offset: f64,
    locale: Locale,
}

/// Mock sistem sağlayıcısı (sahte Raspberry Pi 4).
pub struct System {
    pub state: State,
    pub version: String,
    started: Instant,
    st: Mutex<SysState>,
}

impl System {
    pub fn new(state: State, version: &str) -> Self {
        Self {
            state,
            version: version.into(),
            started: Instant::now(),
            st: Mutex::new(SysState { hostname: "makeros".into(), tz: "Europe/Istanbul".into(), ntp: true, offset: 0.0, locale: Locale { lang: "tr_TR.UTF-8".into(), keymap: "trq".into() } }),
        }
    }
}

fn now_in(tz: &str, offset: f64) -> String {
    let t = jiff::Timestamp::now() + jiff::SignedDuration::from_secs_f64(offset);
    match t.in_tz(tz) {
        Ok(z) => z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        Err(_) => t.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
    }
}

impl system::Provider for System {
    fn info(&self) -> BoxFuture<'_, ApiResult<Info>> {
        let st = self.st.lock().unwrap();
        let total: i64 = 3_981_000_000;
        let used = jitter(820.0, 40.0) as i64 * 1_000_000;
        let i = Info {
            hostname: st.hostname.clone(),
            os: OsInfo { name: "MakerOS".into(), version: self.version.clone() },
            kernel: "6.18.12-v8+".into(),
            model: "Raspberry Pi 4 Model B Rev 1.5".into(),
            uptime_s: self.started.elapsed().as_secs() as i64 + 3600,
            cpu: CpuInfo {
                cores: 4,
                freq_mhz: [600, 1000, 1500, 1800][rand::random_range(0..4)],
                temp_c: Some(jitter(47.5, 2.5)),
                load: [jitter(0.4, 0.3), jitter(0.35, 0.1), jitter(0.3, 0.05)],
                usage_pct: jitter(9.0, 6.0).max(0.0),
            },
            mem: MemInfo { total, used, available: total - used, zram_used: 12_582_912 },
            disk: Disk { total: 59_000_000_000, used: 7_400_000_000 },
            throttled: false,
        };
        Box::pin(async move { Ok(i) })
    }

    fn set_hostname<'a>(&'a self, name: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            if !privsep::valid_hostname(name) {
                return Err(ApiError::invalid("Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir (en fazla 63)."));
            }
            self.st.lock().unwrap().hostname = name.into();
            Ok(())
        })
    }

    fn time(&self) -> BoxFuture<'_, ApiResult<Time>> {
        let st = self.st.lock().unwrap();
        let t = Time { now: now_in(&st.tz, st.offset), timezone: st.tz.clone(), ntp: st.ntp };
        Box::pin(async move { Ok(t) })
    }

    fn set_time(&self, u: TimeUpdate) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let mut st = self.st.lock().unwrap();
            if let Some(tz) = &u.timezone {
                if !system::valid_timezone(tz) {
                    return Err(ApiError::invalid("Geçersiz saat dilimi."));
                }
                st.tz = tz.clone();
            }
            if let Some(ntp) = u.ntp {
                st.ntp = ntp;
                if ntp {
                    st.offset = 0.0;
                }
            }
            if let Some(now) = &u.now {
                let t: jiff::Timestamp = now.parse().map_err(|_| ApiError::invalid("Saat RFC 3339 biçiminde olmalıdır."))?;
                if st.ntp {
                    return Err(ApiError::conflict("Otomatik saat açıkken saat elle ayarlanamaz."));
                }
                st.offset = t.duration_since(jiff::Timestamp::now()).as_secs_f64();
            }
            Ok(())
        })
    }

    fn timezones(&self) -> BoxFuture<'_, ApiResult<Vec<String>>> {
        Box::pin(async { Ok(system::FALLBACK_TIMEZONES.iter().map(|s| s.to_string()).collect()) })
    }

    fn locale(&self) -> BoxFuture<'_, ApiResult<Locale>> {
        let l = self.st.lock().unwrap().locale.clone();
        Box::pin(async move { Ok(l) })
    }

    fn set_locale(&self, l: Locale) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            if !l.lang.is_empty() && !system::valid_lang(&l.lang) {
                return Err(ApiError::invalid("Geçersiz dil."));
            }
            if !l.keymap.is_empty() && !system::valid_keymap(&l.keymap) {
                return Err(ApiError::invalid("Geçersiz klavye düzeni."));
            }
            let mut st = self.st.lock().unwrap();
            if !l.lang.is_empty() {
                st.locale.lang = l.lang;
            }
            if !l.keymap.is_empty() {
                st.locale.keymap = l.keymap;
            }
            Ok(())
        })
    }

    fn set_password<'a>(&'a self, user: &'a str, old: &'a str, new: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            if new.len() < privsep::MIN_PASSWORD_LEN {
                return Err(ApiError::invalid("Yeni parola en az 6 karakter olmalıdır."));
            }
            let mut u = self.state.users.lock().unwrap();
            match u.get(user) {
                Some(pw) if pw == old => {
                    u.insert(user.into(), new.into());
                    Ok(())
                }
                _ => Err(ApiError::forbidden("Mevcut parola hatalı.")),
            }
        })
    }

    fn power<'a>(&'a self, action: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            if action != "reboot" && action != "poweroff" {
                return Err(ApiError::invalid("Geçersiz güç işlemi."));
            }
            tracing::info!(action, "mock güç işlemi (yok sayıldı)");
            Ok(())
        })
    }
}

// --- network ---------------------------------------------------------------

struct MockNet {
    ssid: String,
    signal: i32,
    security: &'static str,
}

struct NetState {
    nets: Vec<MockNet>,
    known: HashSet<String>,
    active: String,
    busy: bool,
    radio: bool,
    /// Radyo kapanınca etkin ağ (açılınca geri bağlanır).
    last: String,
}

/// Mock ağ sağlayıcısı.
pub struct Network {
    /// Bağlanma adımları arasındaki bekleme (test için kısaltılabilir).
    pub step: Duration,
    st: Mutex<NetState>,
}

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

impl Network {
    pub fn new() -> Self {
        let n = |s: &str, sig: i32, sec: &'static str| MockNet { ssid: s.into(), signal: sig, security: sec };
        Self {
            step: Duration::from_millis(700),
            st: Mutex::new(NetState {
                nets: vec![n("MakerLab", 82, "wpa2"), n("Kafe_WiFi", 61, "open"), n("eduroam", 55, "enterprise"), n("Ev-5G", 40, "wpa3"), n("TurkTelekom_ZT3K", 30, "wpa2"), n("MakerOS", 20, "open")],
                known: HashSet::from(["MakerLab".to_string(), "Ev-5G".to_string()]),
                active: "MakerLab".into(),
                busy: false,
                radio: true,
                last: String::new(),
            }),
        }
    }
}

impl network::Provider for Network {
    fn status(&self) -> BoxFuture<'_, ApiResult<network::Status>> {
        let st = self.st.lock().unwrap();
        let mut wifi = Iface { name: "wlan0".into(), kind: "wifi".into(), state: "disconnected".into(), mac: "dc:a6:32:00:00:02".into(), ..Default::default() };
        if !st.radio {
            wifi.state = "unavailable".into();
        } else if !st.active.is_empty() {
            wifi.state = "connected".into();
            wifi.ssid = st.active.clone();
            wifi.ipv4 = vec!["192.168.1.57/24".into()];
        }
        let s = network::Status {
            online: true,
            ifaces: vec![
                Iface { name: "eth0".into(), kind: "ethernet".into(), state: "connected".into(), ipv4: vec!["192.168.1.42/24".into()], mac: "dc:a6:32:00:00:01".into(), speed_mbps: Some(1000), ssid: String::new() },
                wifi,
            ],
            wifi_enabled: st.radio,
            hotspot: network::Hotspot { active: false, ssid: network::HOTSPOT_SSID.into() },
        };
        Box::pin(async move { Ok(s) })
    }

    fn set_wifi_enabled(&self, on: bool) -> BoxFuture<'_, ApiResult<()>> {
        let mut st = self.st.lock().unwrap();
        if on != st.radio {
            st.radio = on;
            if on {
                st.active = std::mem::take(&mut st.last);
            } else {
                st.last = std::mem::take(&mut st.active);
            }
        }
        Box::pin(async { Ok(()) })
    }

    fn scan(&self) -> BoxFuture<'_, ApiResult<Vec<Ap>>> {
        Box::pin(async move {
            tokio::time::sleep(self.step).await;
            let st = self.st.lock().unwrap();
            if !st.radio {
                return Ok(Vec::new());
            }
            let list = st
                .nets
                .iter()
                .map(|x| Ap { ssid: x.ssid.clone(), signal: (x.signal + rand::random_range(0..7) - 3).clamp(0, 100), security: x.security.into(), known: st.known.contains(&x.ssid), active: x.ssid == st.active })
                .collect();
            Ok(network::dedupe_aps(list))
        })
    }

    fn cached_scan(&self) -> BoxFuture<'_, Vec<Ap>> {
        let st = self.st.lock().unwrap();
        if !st.radio {
            return Box::pin(async { Vec::new() });
        }
        let list = st
            .nets
            .iter()
            .map(|x| Ap { ssid: x.ssid.clone(), signal: x.signal, security: x.security.into(), known: st.known.contains(&x.ssid), active: x.ssid == st.active })
            .collect();
        Box::pin(async move { network::dedupe_aps(list) })
    }

    /// Parola "wrong" veya 8 karakterden kısa (WPA) ise başarısız.
    fn connect<'a>(&'a self, req: ConnectRequest, p: Progress<'a>) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            if !network::valid_ssid(&req.ssid) {
                return Err(ApiError::invalid("Ağ adı 1–32 bayt olmalıdır."));
            }
            let (sec, known) = {
                let mut st = self.st.lock().unwrap();
                if st.busy {
                    return Err(ApiError::conflict("Bir ağ işlemi devam ediyor."));
                }
                st.busy = true;
                st.radio = true; // NM gibi: bağlanma radyoyu açar
                let sec = st.nets.iter().filter(|x| x.ssid == req.ssid).map(|x| x.security).last().unwrap_or("");
                (sec, st.known.contains(&req.ssid))
            };
            struct Busy<'a>(&'a Mutex<NetState>);
            impl Drop for Busy<'_> {
                fn drop(&mut self) {
                    self.0.lock().unwrap().busy = false;
                }
            }
            let _busy = Busy(&self.st);
            for (pct, line) in [(10, format!("{} ağına bağlanılıyor…", req.ssid)), (40, "Kimlik doğrulanıyor…".into()), (70, "Adres alınıyor (DHCP)…".into())] {
                p(pct, &line);
                tokio::time::sleep(self.step).await;
            }
            let need_pass = !sec.is_empty() && sec != "open" && sec != "enterprise" && !known;
            if req.password == "wrong" || (!req.password.is_empty() && req.password.len() < 8) || (need_pass && req.password.is_empty()) || (sec.is_empty() && !req.hidden) {
                return Err(ApiError::unavailable("Ağa bağlanılamadı: parola hatalı veya ağ bulunamadı.\nÖnceki bağlantı geri açıldı."));
            }
            {
                let mut st = self.st.lock().unwrap();
                st.active = req.ssid.clone();
                st.known.insert(req.ssid.clone());
                if sec.is_empty() {
                    st.nets.push(MockNet { ssid: req.ssid.clone(), signal: 50, security: "wpa2" });
                }
            }
            p(100, &format!("{} ağına bağlanıldı (wlan0).", req.ssid));
            Ok(())
        })
    }

    fn forget<'a>(&'a self, ssid: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        let mut st = self.st.lock().unwrap();
        let r = if !st.known.remove(ssid) {
            Err(ApiError::not_found(format!("Kayıtlı Wi-Fi profili bulunamadı: {ssid}")))
        } else {
            if st.active == ssid {
                st.active.clear();
            }
            Ok(())
        };
        Box::pin(async move { r })
    }
}

// --- activity --------------------------------------------------------------

/// Mock süreç listesi.
pub struct Activity {
    procs: Mutex<Vec<(Process, f64)>>,
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

impl Activity {
    pub fn new() -> Self {
        let mk = |pid: i32, name: &str, user: &str, slice: &str, rss_mb: i64, cpu: f64| (Process { pid, name: name.into(), user: user.into(), cpu_pct: 0.0, rss: rss_mb << 20, slice: slice.into() }, cpu);
        Self {
            procs: Mutex::new(vec![
                mk(1, "systemd", "root", "", 11, 0.1),
                mk(212, "systemd-journal", "root", "system.slice", 9, 0.1),
                mk(240, "systemd-udevd", "root", "system.slice", 6, 0.0),
                mk(388, "NetworkManager", "root", "system.slice", 17, 0.3),
                mk(390, "wpa_supplicant", "root", "system.slice", 8, 0.1),
                mk(402, "systemd-resolve", "systemd-resolve", "system.slice", 10, 0.0),
                mk(415, "systemd-timesyn", "systemd-timesync", "system.slice", 7, 0.0),
                mk(431, "makerd", "maker", "system.slice", 14, 0.8),
                mk(612, "cage", "maker", "shell.slice", 38, 1.5),
                mk(640, "cog", "maker", "shell.slice", 96, 4.2),
                mk(655, "WPEWebProcess", "maker", "shell.slice", 142, 6.5),
                mk(661, "WPENetworkProce", "maker", "shell.slice", 31, 0.4),
                mk(1203, "bash", "maker", "user-1000.slice", 5, 0.0),
                mk(1240, "python3", "maker", "apps.slice", 64, 12.0),
                mk(1301, "node", "maker", "apps.slice", 88, 2.1),
                mk(1402, "rpicam-vid", "maker", "apps.slice", 42, 7.4),
                mk(1500, "zram0", "root", "", 0, 0.2),
                mk(1601, "kworker/u8:2", "root", "", 0, 0.1),
                mk(1702, "sshd", "root", "system.slice", 7, 0.0),
                mk(1803, "systemd-oomd", "systemd-oom", "system.slice", 6, 0.0),
            ]),
        }
    }
}

impl activity::Source for Activity {
    /// Sahte CPU dalgalanmasıyla liste.
    fn processes(&self) -> BoxFuture<'_, ApiResult<Vec<Process>>> {
        let mut out: Vec<Process> = self
            .procs
            .lock()
            .unwrap()
            .iter()
            .map(|(p, base)| {
                let mut p = p.clone();
                if *base > 0.0 {
                    p.cpu_pct = jitter(*base, base * 0.4).max(0.0);
                }
                p
            })
            .collect();
        out.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal).then(b.rss.cmp(&a.rss)));
        Box::pin(async move { Ok(out) })
    }

    /// maker'a ait sahte süreci listeden çıkarır.
    fn signal<'a>(&'a self, pid: i32, sig: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        let r = (|| {
            activity::parse_signal(sig).ok_or_else(|| ApiError::invalid("Sinyal TERM veya KILL olmalıdır."))?;
            let mut l = self.procs.lock().unwrap();
            let i = l.iter().position(|(p, _)| p.pid == pid).ok_or_else(|| ApiError::not_found("Süreç bulunamadı."))?;
            if l[i].0.user != USER || l[i].0.name == "makerd" {
                return Err(ApiError::forbidden("Yalnızca kendi süreçlerinizi sonlandırabilirsiniz."));
            }
            l.remove(i);
            Ok(())
        })();
        Box::pin(async move { r })
    }
}

// --- apps ------------------------------------------------------------------

/// Örnek yerel uygulamalar.
pub fn native_apps() -> Vec<App> {
    vec![App {
        id: "chromium".into(),
        name: "Chromium".into(),
        icon: "/shell/icons/chromium.svg".into(),
        kind: "native".into(),
        exec: "chromium %U".into(),
        categories: vec!["Network".into(), "WebBrowser".into()],
        ..Default::default()
    }]
}

/// Wayland ekranı (labwc) ve uygulamanın kendisi varsa (MakerOS kök FS'i
/// Docker'da) gerçek başlatıcıyı kullanır; yoksa yalnız günlüğe yazar.
pub fn launcher(real: Option<Launcher>) -> Launcher {
    Arc::new(move |a: App, argv: Vec<String>| {
        let real = real.clone();
        Box::pin(async move {
            if let Some(r) = real {
                if has_display() && crate::util::look_path(&argv[0]).is_some() {
                    return r(a, argv).await;
                }
            }
            tracing::info!(id = %a.id, argv = %argv.join(" "), "mock uygulama başlatma (yok sayıldı)");
            Ok(())
        })
    })
}

fn has_display() -> bool {
    use std::os::unix::fs::FileTypeExt;
    let rd = crate::util::runtime_dir();
    let d = std::env::var("WAYLAND_DISPLAY").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "wayland-0".into());
    let p = if d.starts_with('/') { std::path::PathBuf::from(d) } else { std::path::Path::new(&rd).join(d) };
    std::fs::metadata(p).is_ok_and(|m| m.file_type().is_socket())
}

/// Gerçek başlatıcı (mock'ta görüntü varsa kullanılır).
pub fn system_launcher() -> Launcher {
    apps::system_launcher()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::Source;
    use crate::network::Provider as _;
    use crate::system::Provider as _;

    #[tokio::test]
    async fn mock_backends() {
        let st = State::new();
        assert!(Checker(st.clone()).check("maker", "maker").await.unwrap().ok);
        assert!(!Checker(st.clone()).check("maker", "x").await.unwrap().ok);
        let sys = System::new(st.clone(), "1.0.0");
        assert_eq!(sys.set_password("maker", "yanlis", "yeni-parola").await.unwrap_err().code, "forbidden");
        sys.set_password("maker", "maker", "yeni-parola").await.unwrap();
        assert!(Checker(st.clone()).check("maker", "yeni-parola").await.unwrap().ok);
        assert_eq!(sys.set_time(TimeUpdate { now: Some("2026-01-01T00:00:00Z".into()), ..Default::default() }).await.unwrap_err().code, "conflict");
        sys.set_time(TimeUpdate { ntp: Some(false), timezone: Some("UTC".into()), now: Some("2030-01-01T00:00:00Z".into()) }).await.unwrap();
        assert!(sys.time().await.unwrap().now.starts_with("2030-01-01T00:0"));

        let mut net = Network::new();
        net.step = Duration::from_millis(1);
        let aps = net.scan().await.unwrap();
        assert_eq!(aps[0].ssid, "MakerLab");
        assert!(!aps.iter().any(|a| a.ssid == "MakerOS"));
        let p = |_: i32, _: &str| {};
        assert!(net.connect(ConnectRequest { ssid: "Kafe_WiFi".into(), password: "wrong".into(), ..Default::default() }, &p).await.is_err());
        net.connect(ConnectRequest { ssid: "Kafe_WiFi".into(), ..Default::default() }, &p).await.unwrap();
        assert_eq!(net.status().await.unwrap().ifaces[1].ssid, "Kafe_WiFi");
        net.set_wifi_enabled(false).await.unwrap();
        assert_eq!(net.status().await.unwrap().ifaces[1].state, "unavailable");
        net.set_wifi_enabled(true).await.unwrap();
        assert_eq!(net.status().await.unwrap().ifaces[1].ssid, "Kafe_WiFi");

        let act = Activity::new();
        assert_eq!(act.signal(431, "TERM").await.unwrap_err().code, "forbidden");
        act.signal(1240, "KILL").await.unwrap();
        assert_eq!(act.processes().await.unwrap().len(), 19);
    }
}
