//! llama-server süreci: istek gelince başlatılır, boşta kalınca kapatılır (model RAM'de gereksiz yere durmaz).

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Config {
    pub bin: String,
    pub model: String,
    pub ctx: u32,
    pub threads: u32,
    pub port: u16,
    /// Boşta kalma süresi; sonra model bellekten atılır (0: hiç atılmaz).
    pub idle: Duration,
    /// KV önbelleği (sabit istem öneki) dosyalarının dizini; boşsa kapalı.
    pub cache_dir: String,
}

struct Proc {
    child: Child,
    started: Instant,
    load_ms: u64,
}

pub struct Llama {
    pub cfg: Config,
    proc: Mutex<Option<Proc>>,
    last_use: std::sync::Mutex<Instant>,
    active: AtomicUsize,
}

/// Etkin istek sayacı: düşene dek model kapatılmaz.
pub struct Busy(Arc<Llama>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        *self.0.last_use.lock().unwrap() = Instant::now();
    }
}

#[derive(serde::Serialize, Default)]
pub struct State {
    pub loaded: bool,
    pub pid: Option<u32>,
    pub uptime_s: u64,
    pub load_ms: u64,
    pub rss_mb: f64,
    pub peak_rss_mb: f64,
    pub active: usize,
    pub idle_s: u64,
}

impl Llama {
    pub fn new(cfg: Config) -> Arc<Self> {
        let l = Arc::new(Self { cfg, proc: Mutex::new(None), last_use: std::sync::Mutex::new(Instant::now()), active: AtomicUsize::new(0) });
        let w = Arc::downgrade(&l);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let Some(l) = w.upgrade() else { return };
                l.reap().await;
            }
        });
        l
    }

    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.cfg.port)
    }

    pub fn busy(self: &Arc<Self>) -> Busy {
        self.active.fetch_add(1, Ordering::SeqCst);
        *self.last_use.lock().unwrap() = Instant::now();
        Busy(self.clone())
    }

    /// Model yüklü değilse llama-server'ı başlatır ve hazır olmasını bekler. Yeni başlatıldıysa `true`.
    pub async fn ensure(&self) -> Result<bool, String> {
        let mut g = self.proc.lock().await;
        if let Some(p) = g.as_mut() {
            match p.child.try_wait() {
                Ok(None) => return Ok(false),
                _ => {
                    tracing::warn!("llama-server beklenmedik şekilde kapanmış; yeniden başlatılıyor");
                    *g = None;
                }
            }
        }
        if !std::path::Path::new(&self.cfg.model).is_file() {
            return Err(format!("model dosyası yok: {} (kurulum: makeros-ai model-kur)", self.cfg.model));
        }
        let t0 = Instant::now();
        let mut cmd = Command::new(&self.cfg.bin);
        cmd.args(["-m", &self.cfg.model, "--host", "127.0.0.1", "--port", &self.cfg.port.to_string()])
            .args(["-c", &self.cfg.ctx.to_string(), "-t", &self.cfg.threads.to_string(), "-tb", &self.cfg.threads.to_string()])
            // Tek kullanıcı: tek yuva, tüm bağlam ona. Web arayüzü kapalı; ağ yalnız 127.0.0.1.
            .args(["--parallel", "1", "--no-webui", "--jinja", "--cache-reuse", "256", "--log-verbosity", "2"])
            .args(if self.cfg.cache_dir.is_empty() { vec![] } else { vec!["--slot-save-path".to_string(), format!("{}/", self.cfg.cache_dir.trim_end_matches('/'))] })
            .env("LLAMA_ARG_ALIAS", "qwen2.5-0.5b-instruct")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("{} başlatılamadı: {e}", self.cfg.bin))?;
        // /health 200 dönene dek bekle (model yükleniyor: 503).
        let url = format!("{}/health", self.base());
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Ok(Some(st)) = child.try_wait() {
                return Err(format!("llama-server açılışta kapandı ({st}); günlük: journalctl -u makeros-ai"));
            }
            if http_get_ok(&url).await {
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill().await;
                return Err("llama-server 120 sn içinde hazır olmadı".into());
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        let load_ms = t0.elapsed().as_millis() as u64;
        tracing::info!(model = %self.cfg.model, load_ms, pid = child.id(), "model yüklendi");
        *g = Some(Proc { child, started: Instant::now(), load_ms });
        *self.last_use.lock().unwrap() = Instant::now();
        Ok(true)
    }

    /// Tek yuvanın KV önbelleğini diske yazar / diskten yükler (llama-server /slots/0?action=save|restore).
    pub async fn slot(&self, action: &str, file: &str) -> Result<(), String> {
        if self.cfg.cache_dir.is_empty() {
            return Err("önbellek dizini yok".into());
        }
        let url = format!("{}/slots/0?action={action}", self.base());
        let body = serde_json::json!({"filename": file}).to_string();
        tokio::task::spawn_blocking(move || {
            let ag: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).build().into();
            let mut r = ag.post(&url).header("Content-Type", "application/json").send(body).map_err(|e| e.to_string())?;
            if r.status().is_success() { Ok(()) } else { Err(format!("{}: {}", r.status(), r.body_mut().read_to_string().unwrap_or_default())) }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn reap(&self) {
        if self.cfg.idle.is_zero() || self.active.load(Ordering::SeqCst) > 0 {
            return;
        }
        if self.last_use.lock().unwrap().elapsed() < self.cfg.idle {
            return;
        }
        let mut g = self.proc.lock().await;
        if let Some(mut p) = g.take() {
            let _ = p.child.kill().await;
            tracing::info!(idle_s = self.cfg.idle.as_secs(), "boşta: model bellekten atıldı");
        }
    }

    pub async fn stop(&self) {
        if let Some(mut p) = self.proc.lock().await.take() {
            let _ = p.child.kill().await;
        }
    }

    pub async fn state(&self) -> State {
        let g = self.proc.lock().await;
        let idle_s = self.last_use.lock().unwrap().elapsed().as_secs();
        let active = self.active.load(Ordering::SeqCst);
        let Some(p) = g.as_ref() else { return State { idle_s, active, ..Default::default() } };
        let pid = p.child.id();
        let (rss, hwm) = pid.map(proc_mem).unwrap_or((0, 0));
        State { loaded: true, pid, uptime_s: p.started.elapsed().as_secs(), load_ms: p.load_ms, rss_mb: rss as f64 / 1024.0, peak_rss_mb: hwm as f64 / 1024.0, active, idle_s }
    }
}

/// /proc/<pid>/status → (VmRSS, VmHWM) kB.
pub fn proc_mem(pid: u32) -> (u64, u64) {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let get = |k: &str| s.lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse().ok()).unwrap_or(0);
    (get("VmRSS:"), get("VmHWM:"))
}

async fn http_get_ok(url: &str) -> bool {
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        let ag: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(2))).http_status_as_error(false).build().into();
        ag.get(&url).call().map(|r| r.status().as_u16() == 200).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}
