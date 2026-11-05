//! MakerOS yazılım güncellemesi: GitHub Releases'te yayınlanan imzalı RAUC
//! paketini (makeros-<sürüm>-rpi4.raucb) bulur, indirir (SHA-256 doğrulaması),
//! RAUC ile boştaki A/B slotuna kurar ve Pi'yi tek seferlik deneme açılışıyla
//! (tryboot) yeni slottan başlatır. Yeni slot sağlıklı açılırsa kalıcı olur
//! (makeros-mark-good); açılmazsa bir sonraki açılışta eski slota dönülür.
//! İmza doğrulamasını RAUC yapar (/etc/rauc/keyring.pem); makerd yalnız taşıyıcıdır.

use crate::privsep::{self, client::Client};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Paket adındaki kart son eki.
pub const BOARD: &str = "rpi4";

/// Kurulabilir sürüm.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub published: Option<jiff::Timestamp>,
    /// GitHub sürüm sayfası.
    pub url: String,
    pub size: i64,
    #[serde(skip)]
    bundle_url: String,
    #[serde(skip)]
    sum_url: String,
}

/// GET /api/v1/system/update yanıtı.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Status {
    pub current: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<jiff::Timestamp>,
    /// Yalnız mevcut sürümden yeniyse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<Release>,
    /// Son denetim hatası.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// idle | downloading | verifying | installing | rebooting | error
    pub phase: String,
    pub progress: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub job: String,
    /// Bu sistem kurulum yapabilir (gerçek cihaz; mock'ta benzetim).
    pub can_apply: bool,
}

/// Kurulum aracının (RAUC) anlık durumu.
#[derive(Clone, Debug, Default)]
pub struct InstallState {
    /// idle | installing
    pub operation: String,
    pub progress: i32,
    pub message: String,
    pub last_error: String,
}

/// Paketi kuran ve cihazı deneme açılışıyla başlatan arka uç (gerçek: makerd-priv → RAUC).
pub trait Installer: Send + Sync {
    fn install<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<(), String>>;
    fn state(&self) -> BoxFuture<'_, Result<InstallState, String>>;
    fn reboot(&self) -> BoxFuture<'_, Result<(), String>>;
}

/// Gerçek cihaz: makerd-priv üzerinden RAUC (kök) ve tryboot yeniden başlatması.
pub struct PrivInstaller(pub Client);

impl Installer for PrivInstaller {
    fn install<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.0.call::<_, serde_json::Value>(privsep::CMD_OTA_INSTALL, &privsep::OtaInstallArgs { path: path.into() }).await.map(|_| ()).map_err(|e| e.to_string())
        })
    }
    fn state(&self) -> BoxFuture<'_, Result<InstallState, String>> {
        Box::pin(async {
            let s: privsep::OtaStatus = self.0.call(privsep::CMD_OTA_STATUS, &serde_json::Value::Null).await.map_err(|e| e.to_string())?;
            Ok(InstallState { operation: s.operation, progress: s.progress, message: s.message, last_error: s.last_error })
        })
    }
    fn reboot(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async { self.0.call::<_, serde_json::Value>(privsep::CMD_POWER, &privsep::PowerArgs { action: "tryboot".into() }).await.map(|_| ()).map_err(|e| e.to_string()) })
    }
}

/// Docker önizlemesi (mock): cihaza dokunmaz; kurulum adımlarını ilerlemeyle benzetir.
#[derive(Default)]
pub struct SimInstaller {
    start: Mutex<Option<Instant>>,
}

impl Installer for SimInstaller {
    fn install<'a>(&'a self, _path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        *self.start.lock().unwrap() = Some(Instant::now());
        Box::pin(async { Ok(()) })
    }
    fn state(&self) -> BoxFuture<'_, Result<InstallState, String>> {
        let el = self.start.lock().unwrap().map(|s| s.elapsed()).unwrap_or(Duration::from_secs(60));
        Box::pin(async move {
            Ok(if el < Duration::from_secs(6) {
                InstallState { operation: "installing".into(), progress: (el.as_millis() * 100 / 6000) as i32, message: "Yazılıyor (benzetim)".into(), last_error: String::new() }
            } else {
                InstallState { operation: "idle".into(), progress: 100, ..Default::default() }
            })
        })
    }
    fn reboot(&self) -> BoxFuture<'_, Result<(), String>> {
        tracing::warn!("MOCK: güncelleme benzetimi tamamlandı; yeniden başlatma yok sayıldı");
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
struct State {
    st: Status,
    latest: Option<Release>,
    running: bool,
}

/// Güncelleme durum makinesi.
pub struct Updater {
    /// ör. ferhatozcelik/maker-os
    pub repo: String,
    /// Çalışan sürüm.
    pub current: String,
    /// İndirme dizini (makerd-priv bunu kabul eder: /var/lib/makerd/ota).
    pub dir: PathBuf,
    /// https://api.github.com (testte değiştirilir).
    pub api: String,
    pub installer: Option<Arc<dyn Installer>>,
    /// Kurulumdan sonra yeniden başlatmadan önce bekleme (arayüz "yeniden başlatılıyor" gösterir).
    pub reboot_delay: Duration,
    s: Mutex<State>,
}

#[derive(Deserialize)]
struct GhAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<jiff::Timestamp>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

impl Updater {
    pub fn new(repo: &str, current: &str, dir: impl Into<PathBuf>, installer: Option<Arc<dyn Installer>>) -> Self {
        Self { repo: repo.into(), current: current.into(), dir: dir.into(), api: "https://api.github.com".into(), installer, reboot_delay: Duration::from_secs(4), s: Mutex::new(State::default()) }
    }

    fn ua(&self) -> String {
        format!("makerd/{}", self.current)
    }

    /// Anlık durum.
    pub fn status(&self) -> Status {
        let g = self.s.lock().unwrap();
        let mut s = g.st.clone();
        s.current = self.current.clone();
        s.repo = self.repo.clone();
        s.can_apply = self.installer.is_some();
        if s.phase.is_empty() {
            s.phase = "idle".into();
        }
        s.available = g.latest.clone().filter(|r| newer(&r.version, &self.current));
        s
    }

    /// İmzalı RAUC paketi olan en yeni (taslak/ön sürüm olmayan) sürümü bulur.
    /// RAUC paketi olmayan sürümler hiç önerilmez.
    pub async fn check(&self) -> Result<Status, String> {
        let url = format!("{}/repos/{}/releases?per_page=20", self.api.trim_end_matches('/'), self.repo);
        let ua = self.ua();
        let res = tokio::task::spawn_blocking(move || -> Result<Vec<GhRelease>, String> {
            let mut r = agent(Duration::from_secs(20)).get(&url).header("Accept", "application/vnd.github+json").header("User-Agent", &ua).call().map_err(|e| match e {
                ureq::Error::StatusCode(c) => format!("GitHub yanıtı {c}"),
                e => e.to_string(),
            })?;
            let body = r.body_mut().with_config().limit(4 << 20).read_to_string().map_err(|e| e.to_string())?;
            serde_json::from_str(&body).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        let list = match res {
            Ok(l) => l,
            Err(e) => {
                let mut g = self.s.lock().unwrap();
                g.st.error = format!("Güncellemeler denetlenemedi: {e}");
                g.st.checked = Some(jiff::Timestamp::now());
                drop(g);
                return Err(e);
            }
        };
        let mut best: Option<Release> = None;
        for r in list.into_iter().filter(|r| !r.draft && !r.prerelease) {
            let v = r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name).to_string();
            let want = format!("makeros-{v}-{BOARD}.raucb");
            let mut rel = Release { version: v, tag: r.tag_name.clone(), name: r.name.unwrap_or_default(), notes: r.body.unwrap_or_default(), published: r.published_at, url: r.html_url, ..Default::default() };
            for a in r.assets {
                if a.name == want {
                    rel.bundle_url = a.browser_download_url;
                    rel.size = a.size;
                } else if a.name == format!("{want}.sha256") {
                    rel.sum_url = a.browser_download_url;
                }
            }
            if rel.bundle_url.is_empty() || rel.sum_url.is_empty() {
                continue;
            }
            if best.as_ref().is_none_or(|b| newer(&rel.version, &b.version)) {
                best = Some(rel);
            }
        }
        {
            let mut g = self.s.lock().unwrap();
            g.latest = best;
            g.st.error.clear();
            g.st.checked = Some(jiff::Timestamp::now());
        }
        Ok(self.status())
    }

    fn set(&self, phase: &str, pct: i32, msg: &str, step: &(dyn Fn(i32, &str) + Send + Sync)) {
        {
            let mut g = self.s.lock().unwrap();
            g.st.phase = phase.into();
            g.st.progress = pct;
            g.st.message = msg.into();
        }
        step(pct, msg);
    }

    fn fail(&self, e: String) -> String {
        let mut g = self.s.lock().unwrap();
        g.st.phase = "error".into();
        g.st.message = e.clone();
        e
    }

    /// En yeni sürümü indirir, doğrular, kurar ve cihazı yeniden başlatır (uzun iş).
    pub async fn apply(&self, job: &str, step: &(dyn Fn(i32, &str) + Send + Sync)) -> Result<(), String> {
        let (rel, inst) = {
            let mut g = self.s.lock().unwrap();
            if g.running {
                return Err("Bir güncelleme zaten sürüyor.".into());
            }
            let Some(inst) = self.installer.clone() else { return Err("Bu sistem güncelleme kuramıyor.".into()) };
            let Some(rel) = g.latest.clone().filter(|r| newer(&r.version, &self.current)) else { return Err("Kurulacak yeni bir sürüm yok.".into()) };
            g.running = true;
            g.st.job = job.into();
            (rel, inst)
        };
        let res = self.apply_inner(&rel, inst.as_ref(), step).await;
        self.s.lock().unwrap().running = false;
        res.map_err(|e| self.fail(e))
    }

    async fn apply_inner(&self, rel: &Release, inst: &dyn Installer, step: &(dyn Fn(i32, &str) + Send + Sync)) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        // Eski paketler (önceki denemeler) silinir: /data alanı boşa gitmesin.
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().contains(".raucb")) {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let path = self.dir.join(format!("makeros-{}-{BOARD}.raucb", rel.version));
        self.set("downloading", 1, &format!("MakerOS {} indiriliyor…", rel.version), step);
        let want = self.fetch_sum(&rel.sum_url).await.map_err(|e| format!("özet dosyası alınamadı: {e}"))?;
        let got = match self.download(rel, &path, step).await {
            Ok(g) => g,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(format!("indirme başarısız: {e}"));
            }
        };
        self.set("verifying", 61, "Paket doğrulanıyor…", step);
        if got != want {
            let _ = std::fs::remove_file(&path);
            return Err("paket özeti eşleşmiyor (bozuk ya da yarım indirme)".into());
        }
        self.set("installing", 62, "Yedek bölüme kuruluyor…", step);
        let ps = path.to_string_lossy().into_owned();
        inst.install(&ps).await.map_err(|e| format!("kurulum başlatılamadı: {e}"))?;
        let mut saw = false;
        let deadline = Instant::now() + Duration::from_secs(25 * 60);
        loop {
            if Instant::now() > deadline {
                return Err("kurulum zaman aşımına uğradı".into());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            let Ok(s) = inst.state().await else { continue };
            if s.operation == "installing" {
                saw = true;
                self.set("installing", 62 + s.progress * 36 / 100, format!("Kuruluyor: {}", s.message).trim(), step);
                continue;
            }
            if !saw && s.progress < 100 && s.last_error.is_empty() {
                continue; // henüz başlamadı
            }
            if !s.last_error.is_empty() {
                let _ = std::fs::remove_file(&path);
                return Err(format!("RAUC: {}", s.last_error));
            }
            break;
        }
        let _ = std::fs::remove_file(&path);
        self.set("rebooting", 100, "Kuruldu. Yeni sürümle yeniden başlatılıyor…", step);
        tracing::info!(version = %rel.version, "güncelleme kuruldu, deneme açılışı");
        tokio::time::sleep(self.reboot_delay).await;
        inst.reboot().await.map_err(|e| format!("yeniden başlatılamadı: {e}"))
    }

    async fn fetch_sum(&self, url: &str) -> Result<String, String> {
        let (url, ua) = (url.to_string(), self.ua());
        tokio::task::spawn_blocking(move || {
            let mut r = agent(Duration::from_secs(60)).get(&url).header("User-Agent", &ua).call().map_err(|e| e.to_string())?;
            let s = r.body_mut().with_config().limit(4096).read_to_string().map_err(|e| e.to_string())?;
            match s.split_whitespace().next() {
                Some(h) if h.len() == 64 => Ok(h.to_lowercase()),
                _ => Err("geçersiz özet dosyası".to_string()),
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Paketi dosyaya indirir; SHA-256 özetini döner. İlerleme 1–60 arası.
    async fn download(&self, rel: &Release, path: &std::path::Path, step: &(dyn Fn(i32, &str) + Send + Sync)) -> Result<String, String> {
        let (url, ua, size, path) = (rel.bundle_url.clone(), self.ua(), rel.size, path.to_path_buf());
        let (ptx, mut prx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64)>();
        let task = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let mut r = agent(Duration::from_secs(30 * 60)).get(&url).header("User-Agent", &ua).call().map_err(|e| e.to_string())?;
            let total = r.body().content_length().filter(|n| *n > 0).unwrap_or(size.max(0) as u64);
            let mut rd = r.body_mut().with_config().limit(u64::MAX).reader();
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o640).open(&path).map_err(|e| e.to_string())?;
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 256 << 10];
            let mut done: u64 = 0;
            let mut last = Instant::now() - Duration::from_secs(1);
            loop {
                let n = rd.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                h.update(&buf[..n]);
                done += n as u64;
                if last.elapsed() > Duration::from_millis(500) {
                    let _ = ptx.send((done, total));
                    last = Instant::now();
                }
            }
            let _ = ptx.send((done, total));
            f.sync_all().map_err(|e| e.to_string())?;
            Ok(hex::encode(h.finalize()))
        });
        while let Some((done, total)) = prx.recv().await {
            if total > 0 {
                self.set("downloading", 1 + (done * 59 / total) as i32, &format!("İndiriliyor: {} / {} MB", done >> 20, total >> 20), step);
            }
        }
        task.await.map_err(|e| e.to_string())?
    }
}

/// "v?MAJ.MIN.YAMA(-ek)?" ayrıştırması.
fn parse_ver(s: &str) -> Option<([u64; 3], &str)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let (num, pre) = match s.split_once('-') {
        Some((n, p)) => {
            if p.is_empty() || !p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-') {
                return None;
            }
            (n, p)
        }
        None => (s, ""),
    };
    let p: Vec<&str> = num.split('.').collect();
    if p.len() != 3 || p.iter().any(|x| x.is_empty() || !x.bytes().all(|c| c.is_ascii_digit())) {
        return None;
    }
    Some(([p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?], pre))
}

/// a, b'den yeni mi (anlamsal sürüm; ön sürüm eki olan aynı numaralı sürümden
/// eskidir: 2.0.0-dev < 2.0.0). Çözümlenemeyen sürüm hiçbir zaman yeni sayılmaz.
pub fn newer(a: &str, b: &str) -> bool {
    let Some((na, pa)) = parse_ver(a) else { return false };
    let Some((nb, pb)) = parse_ver(b) else { return true };
    if na != nb {
        return na > nb;
    }
    match (pa.is_empty(), pb.is_empty()) {
        (true, false) => true,
        (false, true) => false,
        _ => pa > pb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        for (a, b, w) in [
            ("2.0.1", "2.0.0", true), ("2.0.0", "2.0.1", false), ("2.0.0", "2.0.0", false),
            ("2.0.0", "2.0.0-dev", true), ("2.0.0-dev", "2.0.0", false), ("2.1.0", "2.0.9", true),
            ("10.0.0", "9.9.9", true), ("v2.0.1", "2.0.0", true), ("garip", "2.0.0", false), ("2.0.0", "garip", true),
            ("1.9.9", "2.0.0-dev", false),
        ] {
            assert_eq!(newer(a, b), w, "newer({a}, {b})");
        }
    }

    #[derive(Default)]
    struct Fake {
        installed: Mutex<String>,
        rebooted: Mutex<bool>,
        polls: Mutex<i32>,
        fail: String,
    }

    impl Installer for Fake {
        fn install<'a>(&'a self, p: &'a str) -> BoxFuture<'a, Result<(), String>> {
            *self.installed.lock().unwrap() = p.into();
            Box::pin(async { Ok(()) })
        }
        fn state(&self) -> BoxFuture<'_, Result<InstallState, String>> {
            let mut n = self.polls.lock().unwrap();
            *n += 1;
            let s = if *n < 2 {
                InstallState { operation: "installing".into(), progress: 50, ..Default::default() }
            } else {
                InstallState { operation: "idle".into(), progress: 100, last_error: self.fail.clone(), ..Default::default() }
            };
            Box::pin(async move { Ok(s) })
        }
        fn reboot(&self) -> BoxFuture<'_, Result<(), String>> {
            *self.rebooted.lock().unwrap() = true;
            Box::pin(async { Ok(()) })
        }
    }

    /// GitHub benzeri sunucu: sürüm listesi, paket (/b) ve özet (/s).
    async fn server(bundle: Vec<u8>, sum: String) -> String {
        use axum::routing::get;
        let ln = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", ln.local_addr().unwrap());
        let (b2, n) = (base.clone(), bundle.len());
        let list = format!(
            r#"[
             {{"tag_name":"v2.0.2","draft":true,"assets":[{{"name":"makeros-2.0.2-rpi4.raucb","browser_download_url":"{b2}/b"}},{{"name":"makeros-2.0.2-rpi4.raucb.sha256","browser_download_url":"{b2}/s"}}]}},
             {{"tag_name":"v2.0.1","name":"MakerOS 2.0.1","body":"notlar","published_at":"2026-11-01T10:00:00Z","assets":[{{"name":"makeros-2.0.1-rpi4.raucb","size":{n},"browser_download_url":"{b2}/b"}},{{"name":"makeros-2.0.1-rpi4.raucb.sha256","browser_download_url":"{b2}/s"}}]}},
             {{"tag_name":"v3.0.0","name":null,"body":null,"assets":[{{"name":"maker-os-3.0.0.img.xz","browser_download_url":"{b2}/x"}}]}},
             {{"tag_name":"v1.9.9","assets":[{{"name":"eski-ota.tar.xz","browser_download_url":"{b2}/x"}}]}}]"#
        );
        let app = axum::Router::new()
            .route("/repos/o/r/releases", get(move || async move { list }))
            .route("/b", get(move || async move { bundle }))
            .route("/s", get(move || async move { format!("{sum}  makeros-2.0.1-rpi4.raucb\n") }));
        tokio::spawn(async move { axum::serve(ln, app).await.unwrap() });
        base
    }

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("mk-ota-{}", crate::util::random_hex(4)))
    }

    fn updater(api: &str, current: &str, inst: Option<Arc<dyn Installer>>) -> Updater {
        let mut u = Updater::new("o/r", current, tmp(), inst);
        u.api = api.into();
        u.reboot_delay = Duration::from_millis(1);
        u
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn check_and_apply() {
        let bundle = "imzalı-paket".repeat(1000).into_bytes();
        let sum = hex::encode(Sha256::digest(&bundle));
        let api = server(bundle, sum).await;
        let inst = Arc::new(Fake::default());
        let u = updater(&api, "2.0.0", Some(inst.clone()));
        let st = u.check().await.unwrap();
        let a = st.available.unwrap();
        assert_eq!((a.version.as_str(), a.notes.as_str()), ("2.0.1", "notlar"));
        let last = Arc::new(Mutex::new(0));
        let l2 = last.clone();
        u.apply("j1", &move |p, _| *l2.lock().unwrap() = p).await.unwrap();
        assert!(inst.installed.lock().unwrap().ends_with("makeros-2.0.1-rpi4.raucb"));
        assert!(*inst.rebooted.lock().unwrap());
        assert_eq!(*last.lock().unwrap(), 100);
        assert_eq!(std::fs::read_dir(&u.dir).unwrap().count(), 0, "indirilen paket silinmeli");
        // Aynı sürümdeyken önerilmez.
        let u2 = updater(&api, "2.0.1", None);
        assert!(u2.check().await.unwrap().available.is_none());
        assert!(!u2.status().can_apply);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rejects_bad_sum() {
        let api = server(b"paket".to_vec(), "0".repeat(64)).await;
        let inst = Arc::new(Fake::default());
        let u = updater(&api, "2.0.0", Some(inst.clone()));
        u.check().await.unwrap();
        let e = u.apply("j", &|_, _| {}).await.unwrap_err();
        assert!(e.contains("özeti"), "{e}");
        assert!(inst.installed.lock().unwrap().is_empty());
        assert_eq!(u.status().phase, "error");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rauc_error_no_reboot() {
        let bundle = b"paket".to_vec();
        let api = server(bundle.clone(), hex::encode(Sha256::digest(&bundle))).await;
        let inst = Arc::new(Fake { fail: "signature verification failed".into(), ..Default::default() });
        let u = updater(&api, "2.0.0", Some(inst.clone()));
        u.check().await.unwrap();
        let e = u.apply("j", &|_, _| {}).await.unwrap_err();
        assert!(e.contains("signature"), "{e}");
        assert!(!*inst.rebooted.lock().unwrap());
        assert_eq!(u.apply("j", &|_, _| {}).await.is_err(), true);
    }

    #[tokio::test]
    async fn check_error_recorded() {
        let u = updater("http://127.0.0.1:1", "2.0.0", None);
        assert!(u.check().await.is_err());
        let s = u.status();
        assert!(s.error.starts_with("Güncellemeler denetlenemedi") && s.checked.is_some());
    }
}
