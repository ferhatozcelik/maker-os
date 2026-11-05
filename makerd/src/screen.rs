//! Ekran Modu: Pi'nin ekranını tarayıcıda gösterme (arka planda wayvnc).
//! Varsayılan KAPALI: wayvnc hiç çalışmaz, /api/v1/vnc bağlantı kabul etmez.
//! Yönetici Ayarlar'dan açar; seçim <data-dir>/screen-mode dosyasıyla kalıcıdır
//! (labwc autostart da bu dosyaya bakar). Bağlantı ayrı bir port açmaz:
//! noVNC → makerd (aynı HTTP portu, /api/v1/vnc) → 127.0.0.1:5900.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// wayvnc'nin systemd kullanıcı birimi (os/board/.../systemd/user/makeros-vnc.service).
pub const UNIT: &str = "makeros-vnc.service";

/// GET /api/v1/screen yanıtı.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub enabled: bool,
    /// Pi'ye monitör bağlı değil → sanal ekran tarayıcı penceresinin boyutuna uyar.
    pub headless: bool,
    /// makerd wayvnc'yi kendisi başlatıp durdurabiliyor (yerel adres).
    pub managed: bool,
}

#[derive(Default)]
struct Run {
    stop: Option<CancellationToken>,
    pid: Option<i32>,
}

/// Ekran Modu durumunu tutar ve wayvnc'yi başlatır/durdurur.
pub struct Manager {
    /// Kalıcı işaret dosyası: varsa açık.
    pub path: PathBuf,
    /// wayvnc adresi (127.0.0.1:5900); yerel değilse yönetilmez (ör. compose "desktop" kabı).
    pub addr: String,
    /// /sys/class/drm (testte değiştirilebilir).
    pub drm_root: PathBuf,
    run: std::sync::Arc<Mutex<Run>>,
}

impl Manager {
    pub fn new(path: impl Into<PathBuf>, addr: &str) -> Self {
        Self { path: path.into(), addr: addr.into(), drm_root: "/sys/class/drm".into(), run: Default::default() }
    }

    pub fn enabled(&self) -> bool {
        !self.path.as_os_str().is_empty() && self.path.exists()
    }

    pub fn state(&self) -> State {
        State { enabled: self.enabled(), headless: self.headless(), managed: self.managed() }
    }

    fn managed(&self) -> bool {
        let host = self.addr.split(':').next().unwrap_or("");
        matches!(host, "127.0.0.1" | "localhost" | "::1")
    }

    /// Hiçbir DRM bağlayıcısı (HDMI/DSI) "connected" değil. DRM yoksa (Docker) monitörsüzdür.
    fn headless(&self) -> bool {
        let Ok(rd) = std::fs::read_dir(&self.drm_root) else { return true };
        !rd.flatten().any(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("card") && n.contains('-') && std::fs::read_to_string(e.path().join("status")).map(|s| s.trim() == "connected").unwrap_or(false)
        })
    }

    /// Ekran Modu'nu açar/kapatır, kalıcı yapar ve wayvnc'yi buna göre yönetir.
    pub async fn set(&self, on: bool) -> std::io::Result<()> {
        if on {
            use std::os::unix::fs::OpenOptionsExt;
            use std::io::Write;
            std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o640).open(&self.path)?.write_all(b"on\n")?;
        } else {
            match std::fs::remove_file(&self.path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        self.apply().await;
        Ok(())
    }

    /// Kayıtlı duruma uyar (açılışta ve her set'te çağrılır).
    pub async fn apply(&self) {
        if !self.managed() {
            return;
        }
        if self.enabled() { self.start().await } else { self.halt().await }
    }

    /// makerd kapanırken kendi başlattığı wayvnc'yi durdurur.
    pub fn close(&self) {
        kill(&mut self.run.lock().unwrap());
    }

    async fn start(&self) {
        if systemctl_user("start", UNIT).await.is_ok() {
            tracing::info!(via = "systemd", "Ekran Modu açık");
            return;
        }
        let mut r = self.run.lock().unwrap();
        if r.stop.is_some() {
            return; // zaten izleniyor
        }
        // systemd kullanıcı oturumu yok (Docker önizlemesi): wayvnc doğrudan, ekran (Wayland) hazır olunca.
        let tok = CancellationToken::new();
        r.stop = Some(tok.clone());
        tokio::spawn(supervise(self.addr.clone(), self.run.clone(), tok));
    }

    async fn halt(&self) {
        if systemctl_user("stop", UNIT).await.is_ok() {
            tracing::info!(via = "systemd", "Ekran Modu kapalı");
        }
        kill(&mut self.run.lock().unwrap());
    }
}

fn kill(r: &mut Run) {
    if let Some(t) = r.stop.take() {
        t.cancel();
    }
    if let Some(pid) = r.pid.take() {
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGTERM);
    }
}

/// Kullanıcının XDG_RUNTIME_DIR dizini.
pub fn runtime_dir() -> String {
    std::env::var("XDG_RUNTIME_DIR").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("/run/user/{}", crate::util::getuid()))
}

/// makerd'nin kendi kullanıcı oturumunda (maker) "systemctl --user <action> <unit>".
/// Kullanıcı oturumu yoksa (Docker önizlemesi) hata döner; çağıran doğrudan başlatmaya düşer.
/// Birimler makerd'nin kum havuzunun (MemoryDenyWriteExecute, MemoryMax…) dışında çalışır.
pub async fn systemctl_user(action: &str, unit: &str) -> Result<(), String> {
    let path = crate::util::look_path("systemctl").ok_or("systemctl yok")?;
    let rd = runtime_dir();
    let bus = Path::new(&rd).join("bus");
    if !bus.exists() {
        return Err("kullanıcı oturum veri yolu yok".into());
    }
    let out = tokio::process::Command::new(path)
        .args(["--user", action, unit])
        .env("XDG_RUNTIME_DIR", &rd)
        .env("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={}", bus.display()))
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let mut b = out.stdout;
        b.extend_from_slice(&out.stderr);
        Err(String::from_utf8_lossy(&b).trim().to_string())
    }
}

/// wayvnc'yi Wayland soketi hazır olunca başlatır, düşerse yeniden başlatır.
async fn supervise(addr: String, run: std::sync::Arc<Mutex<Run>>, tok: CancellationToken) {
    let (host, port) = match addr.split_once(':') {
        Some((h, p)) if !p.is_empty() => (h.to_string(), p.to_string()),
        Some((h, _)) => (h.to_string(), "5900".into()),
        None => (addr.clone(), "5900".into()),
    };
    let wl = std::env::var("WAYLAND_DISPLAY").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "wayland-0".into());
    let sock = Path::new(&runtime_dir()).join(&wl);
    let pause = |d: u64| {
        let tok = tok.clone();
        async move {
            tokio::select! {
                _ = tok.cancelled() => {}
                _ = tokio::time::sleep(Duration::from_secs(d)) => {}
            }
        }
    };
    while !tok.is_cancelled() {
        use std::os::unix::fs::FileTypeExt;
        if !std::fs::metadata(&sock).map(|m| m.file_type().is_socket()).unwrap_or(false) {
            pause(1).await;
            continue;
        }
        let Some(path) = crate::util::look_path("wayvnc") else {
            tracing::warn!("Ekran Modu: wayvnc bulunamadı");
            return;
        };
        let mut args: Vec<String> = vec!["--render-cursor".into(), "--max-fps=30".into(), host.clone(), port.clone()];
        let gpu = std::fs::read_dir("/dev/dri").map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with("renderD"))).unwrap_or(false);
        if gpu {
            args.insert(0, "--gpu".into());
        }
        let mut cmd = tokio::process::Command::new(path);
        cmd.args(&args).env("XDG_RUNTIME_DIR", runtime_dir()).env("WAYLAND_DISPLAY", &wl).process_group(0);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(err = %e, "Ekran Modu: wayvnc başlatılamadı");
                pause(3).await;
                continue;
            }
        };
        let pid = child.id().map(|p| p as i32);
        {
            let mut r = run.lock().unwrap();
            if tok.is_cancelled() {
                let _ = child.start_kill();
                return;
            }
            r.pid = pid;
        }
        tracing::info!(via = "wayvnc", pid, "Ekran Modu açık");
        let _ = child.wait().await;
        if tok.is_cancelled() {
            tracing::info!(via = "wayvnc", "Ekran Modu kapalı");
            return;
        }
        tracing::warn!("Ekran Modu: wayvnc durdu, yeniden başlatılıyor");
        pause(2).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("mk-scr-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn default_off_and_persist() {
        let dir = tmp();
        // Yönetilmeyen adres: wayvnc başlatılmaz, yalnız işaret dosyası değişir.
        let mut m = Manager::new(dir.join("screen-mode"), "desktop:5900");
        m.drm_root = dir.clone();
        assert!(!m.enabled());
        m.set(true).await.unwrap();
        assert!(m.enabled());
        m.set(false).await.unwrap();
        assert!(!m.enabled());
        m.set(false).await.unwrap();
        assert!(!m.state().managed);
        assert!(Manager::new(dir.join("x"), "127.0.0.1:5900").state().managed);
    }

    #[test]
    fn headless() {
        let dir = tmp();
        let mut m = Manager::new(dir.join("screen-mode"), "");
        m.drm_root = dir.clone();
        assert!(m.headless(), "DRM yoksa monitörsüz");
        let p = dir.join("card1-HDMI-A-1");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("status"), "disconnected\n").unwrap();
        assert!(m.headless());
        std::fs::write(p.join("status"), "connected\n").unwrap();
        assert!(!m.headless());
        m.drm_root = dir.join("yok");
        assert!(m.headless());
    }
}
