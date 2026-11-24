//! Pencereli kullanıcı programları (tkinter, PySide6/Qt, pygame, OpenCV imshow, GTK) için
//! "yerel pencere" köprüsü.
//!
//! Programlar kullanıcıya ait ekransız bir X sunucusunda (Xvfb, `:<10+uid%1000>`) çalışır.
//! makerd-user bu sunucunun pencere yöneticisidir (`wm`): her üst düzey pencereyi Composite
//! ile ayrı bir piksel tamponuna yönlendirir, Damage ile yalnız değişen dikdörtgenleri izler
//! ve bunları web kabuğuna aktarır (`stream`). Kabuk her pencereyi kendi `<mk-window>`'unda
//! çizer; fare ve klavye XTest ile programa geri iletilir. Bu VNC değildir: masaüstünün
//! tamamı değil, pencere başına yalnız değişen bölgeler gönderilir; HDMI takılı olsun
//! olmasın aynı çalışır (Pi'nin kendi ekranındaki kabuk da aynı yoldan çizer).
//!
//! makerd bağlantıyı `Open { mode: "xwin", path }` ile açar: `path` boşsa denetim akışı
//! (pencere listesi ve olaylar), doluysa o pencerenin görüntü akışıdır.

mod keys;
pub mod stream;
mod wm;

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use wm::Bridge;

/// Ekransız X sunucusunun yolu.
pub const XVFB: &str = "/usr/bin/Xvfb";
/// Sanal ekran boyutu: pencereler (ve açılır menüler) bunun içine sığmalı; fare yalnız
/// ekran içinde gezer. Tampon 2560×1600×4 ≈ 16 MB.
pub const SCREEN_W: u16 = 2560;
pub const SCREEN_H: u16 = 1600;

/// Kullanıcının X ekranı.
#[derive(Clone, Debug)]
pub struct Display {
    pub num: u32,
    /// MIT-MAGIC-COOKIE-1 dosyası (XAUTHORITY).
    pub auth: PathBuf,
    pub xvfb: PathBuf,
}

impl Display {
    /// Xvfb kuruluysa kullanıcının ekranını tanımlar (henüz başlatmaz).
    pub fn for_uid(uid: u32, runtime_dir: &Path) -> Option<Self> {
        let xvfb = std::env::var_os("MAKEROS_XVFB").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(XVFB));
        if !xvfb.is_file() {
            return None;
        }
        Some(Self { num: 10 + uid % 1000, auth: runtime_dir.join("makeros-x11.auth"), xvfb })
    }

    pub fn name(&self) -> String {
        format!(":{}", self.num)
    }

    /// Kullanıcı programlarına verilecek ortam: araç takımları X11'i seçer (Wayland'e değil).
    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("DISPLAY", self.name()),
            ("XAUTHORITY", self.auth.to_string_lossy().into_owned()),
            ("QT_QPA_PLATFORM", "xcb".into()),
            ("SDL_VIDEODRIVER", "x11".into()),
            ("SDL_VIDEO_DRIVER", "x11".into()),
            ("GDK_BACKEND", "x11".into()),
            ("XDG_SESSION_TYPE", "x11".into()),
            // Qt: Xvfb'de GLX yok; widget'lar yazılımla çizilir, QtQuick yazılım arka ucu.
            ("QT_XCB_GL_INTEGRATION", "none".into()),
            ("QT_QUICK_BACKEND", "software".into()),
            ("LIBGL_ALWAYS_SOFTWARE", "1".into()),
        ]
    }

    /// Çerezi okur; yoksa (ya da bozuksa) yenisini yazar. Var olan çerez korunur: makerd-user
    /// yeniden başladığında hâlâ çalışan Xvfb ve programlar aynı çerezle bağlanmaya devam eder.
    pub fn ensure_cookie(&self) -> std::io::Result<[u8; 16]> {
        if let Ok(b) = std::fs::read(&self.auth) {
            if let Some(c) = parse_cookie(&b) {
                return Ok(c);
            }
        }
        let mut c = [0u8; 16];
        rand::fill(&mut c[..]);
        let host = nix::unistd::gethostname().ok().and_then(|h| h.into_string().ok()).unwrap_or_default();
        let mut out = Vec::new();
        // FamilyLocal (256) + ana makine adı ve FamilyWild (65535): ad değişse de eşleşir.
        for (fam, addr) in [(256u16, host.as_bytes()), (65535u16, &b""[..])] {
            out.extend_from_slice(&fam.to_be_bytes());
            for f in [addr, self.num.to_string().as_bytes(), b"MIT-MAGIC-COOKIE-1", &c[..]] {
                out.extend_from_slice(&(f.len() as u16).to_be_bytes());
                out.extend_from_slice(f);
            }
        }
        let tmp = self.auth.with_extension("tmp");
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(&out)?;
        drop(f);
        std::fs::rename(&tmp, &self.auth)?;
        Ok(c)
    }
}

/// Xauthority dosyasındaki ilk MIT-MAGIC-COOKIE-1 değeri.
fn parse_cookie(b: &[u8]) -> Option<[u8; 16]> {
    let mut i = 2usize;
    let field = |i: &mut usize| -> Option<&[u8]> {
        let n = u16::from_be_bytes([*b.get(*i)?, *b.get(*i + 1)?]) as usize;
        let s = b.get(*i + 2..*i + 2 + n)?;
        *i += 2 + n;
        Some(s)
    };
    let _addr = field(&mut i)?;
    let _num = field(&mut i)?;
    let name = field(&mut i)?;
    let data = field(&mut i)?;
    if name != b"MIT-MAGIC-COOKIE-1" || data.len() != 16 {
        return None;
    }
    data.try_into().ok()
}

static LOCAL: std::sync::OnceLock<Arc<Bridge>> = std::sync::OnceLock::new();

/// Süreç içi köprü (yalnız mock önizlemede makerd kendisi başlatır).
pub fn local() -> Option<Arc<Bridge>> {
    LOCAL.get().cloned()
}

pub fn set_local(b: Arc<Bridge>) {
    let _ = LOCAL.set(b);
}

/// Xvfb'yi çalıştırır ve köprüyü bağlı tutar (ayrı iş parçacığında; X istemcisi bloklayıcıdır).
pub fn start(d: Display) -> Arc<Bridge> {
    let bridge = Arc::new(Bridge::new(d.clone()));
    let b = bridge.clone();
    let child: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
    std::thread::Builder::new()
        .name("xwin".into())
        .spawn(move || {
            let mut wait = Duration::from_millis(500);
            loop {
                match ensure_server(&d, &child) {
                    Ok(()) => match b.run() {
                        Ok(()) => wait = Duration::from_millis(500),
                        Err(e) => tracing::warn!(err = %e, "x11 köprüsü durdu"),
                    },
                    Err(e) => tracing::warn!(err = %e, display = %d.name(), "Xvfb başlatılamadı"),
                }
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(10));
            }
        })
        .expect("xwin iş parçacığı");
    bridge
}

/// X sunucusu yanıt veriyorsa döner; yoksa Xvfb'yi başlatıp hazır olmasını bekler.
fn ensure_server(d: &Display, child: &Mutex<Option<std::process::Child>>) -> std::io::Result<()> {
    // XAUTHORITY süreç başında (Display::env) ayarlanır; RustConnection çerezi oradan okur.
    d.ensure_cookie()?;
    if x11rb::rust_connection::RustConnection::connect(Some(&d.name())).is_ok() {
        return Ok(());
    }
    let mut g = child.lock().unwrap();
    if let Some(mut c) = g.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(&d.xvfb);
    cmd.arg(d.name())
        .args(["-screen", "0", &format!("{SCREEN_W}x{SCREEN_H}x24")])
        .args(["-nolisten", "tcp", "-auth"])
        .arg(&d.auth)
        .args(["-dpi", "96", "+extension", "Composite", "+extension", "DAMAGE", "+extension", "XTEST", "+extension", "RANDR", "-noreset", "-nocursor"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    // SAFETY: fork sonrası yalnız async-signal-safe prctl çağrılır.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        })
    };
    let mut c = cmd.spawn()?;
    // Xvfb'nin uyarıları günlüğe (yalnız ilk satırlar; "Could not resolve keysym" gibi gürültüler bol).
    if let Some(err) = c.stderr.take() {
        std::thread::spawn(move || {
            use std::io::BufRead;
            for l in std::io::BufReader::new(err).lines().map_while(Result::ok).take(40) {
                if !l.contains("Could not resolve keysym") && !l.trim().is_empty() {
                    tracing::info!(line = %l, "Xvfb");
                }
            }
        });
    }
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(100));
        if let Ok(Some(st)) = c.try_wait() {
            return Err(std::io::Error::other(format!("Xvfb çıktı: {st}")));
        }
        if x11rb::rust_connection::RustConnection::connect(Some(&d.name())).is_ok() {
            tracing::info!(display = %d.name(), pid = c.id(), "Xvfb hazır");
            *g = Some(c);
            return Ok(());
        }
    }
    let _ = c.kill();
    let _ = c.wait();
    Err(std::io::Error::other("Xvfb zamanında hazır olmadı"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mk-xauth-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let d = Display { num: 42, auth: dir.join("a"), xvfb: PathBuf::from("/bin/true") };
        let c1 = d.ensure_cookie().unwrap();
        let c2 = d.ensure_cookie().unwrap();
        assert_eq!(c1, c2, "var olan çerez korunmalı");
        assert_eq!(parse_cookie(&std::fs::read(&d.auth).unwrap()), Some(c1));
        assert!(d.env().iter().any(|(k, v)| *k == "DISPLAY" && v == ":42"));
    }
}
