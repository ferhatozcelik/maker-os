//! İlk açılış sihirbazının durumu: `<data-dir>/setup-done` (JSON `{"user","time"}`)
//! ve tek seferlik tamamlama kilidi (`<data-dir>/setup.lock` üzerinde flock + süreç içi kilit).

use std::fs::{File, OpenOptions};
use std::net::IpAddr;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, PartialEq, Eq)]
pub enum BeginError {
    Done,
    Busy,
    Io(String),
}

pub struct Store {
    pub dir: PathBuf,
    busy: Mutex<bool>,
}

/// Tamamlama kilidi; düşürülünce bırakılır.
pub struct Guard<'a> {
    store: &'a Store,
    file: File,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        // SAFETY: geçerli dosya tanıtıcısı.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        *self.store.busy.lock().unwrap() = false;
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DoneFile {
    user: String,
    time: jiff::Timestamp,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, busy: Mutex::new(false) }
    }

    /// setup-done dosyasının yolu (makerd-priv --setup-flag ile aynı olmalı).
    pub fn path(&self) -> PathBuf {
        self.dir.join("setup-done")
    }

    pub fn done(&self) -> bool {
        self.path().exists()
    }

    /// Kurulumda oluşturulan kullanıcı (yoksa boş).
    pub fn primary_user(&self) -> String {
        std::fs::read(self.path()).ok().and_then(|b| serde_json::from_slice::<DoneFile>(&b).ok()).map(|d| d.user).unwrap_or_default()
    }

    /// Tamamlama kilidini alır.
    pub fn begin(&self) -> Result<Guard<'_>, BeginError> {
        {
            let mut b = self.busy.lock().unwrap();
            if *b {
                return Err(BeginError::Busy);
            }
            *b = true;
        }
        let release = || *self.busy.lock().unwrap() = false;
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            release();
            return Err(BeginError::Io(e.to_string()));
        }
        let file = match OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(self.dir.join("setup.lock")) {
            Ok(f) => f,
            Err(e) => {
                release();
                return Err(BeginError::Io(e.to_string()));
            }
        };
        // SAFETY: geçerli dosya tanıtıcısı.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            release();
            return Err(BeginError::Busy);
        }
        let g = Guard { store: self, file };
        if self.done() {
            return Err(BeginError::Done);
        }
        Ok(g)
    }

    /// setup-done dosyasını atomik yazar.
    pub fn mark_done(&self, user: &str) -> std::io::Result<()> {
        let mut b = serde_json::to_vec(&DoneFile { user: user.into(), time: crate::util::now() }).unwrap_or_default();
        b.push(b'\n');
        crate::util::write_atomic(&self.path(), &b, 0o644)
    }

    /// Kurulum durumunu siler (yalnız mock/test).
    pub fn reset(&self) -> std::io::Result<()> {
        match std::fs::remove_file(self.path()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Kurulum isteği yerel ağdan mı: loopback, özel (RFC 1918 / ULA) veya bağlantı-yerel.
pub fn local_addr(ip: &str) -> bool {
    let Ok(a) = ip.parse::<IpAddr>() else { return false };
    match a.to_canonical() {
        IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local(),
        IpAddr::V6(v) => {
            let s = v.segments()[0];
            v.is_loopback() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
        }
    }
}

/// Varsayılan cihaz adı: web arayüzü her zaman http://maker-os.local/ adresinden açılır.
/// (Aynı ağda ikinci bir MakerOS varsa mDNS çakışmasını systemd-resolved çözer; kullanıcı adı değiştirebilir.)
pub const DEFAULT_HOSTNAME: &str = "maker-os";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        for ok in ["127.0.0.1", "10.42.0.5", "192.168.1.2", "172.16.0.1", "169.254.1.1", "::1", "fd00::1", "fe80::1", "::ffff:192.168.1.9"] {
            assert!(local_addr(ok), "{ok}");
        }
        for bad in ["8.8.8.8", "2001:db8::1", "x"] {
            assert!(!local_addr(bad), "{bad}");
        }
    }

    #[test]
    fn begin_lock() {
        let d = std::env::temp_dir().join(format!("mk-setup-{}", crate::util::random_hex(4)));
        let s = Store::new(d.clone());
        {
            let g = s.begin().unwrap();
            assert_eq!(s.begin().err(), Some(BeginError::Busy));
            s.mark_done("ali").unwrap();
            drop(g);
        }
        assert!(s.done());
        assert_eq!(s.primary_user(), "ali");
        assert_eq!(s.begin().err(), Some(BeginError::Done));
        s.reset().unwrap();
        assert!(!s.done());
        let _ = std::fs::remove_dir_all(d);
    }
}
