//! Kullanıcı başına kabuk tercihleri (serbest JSON nesnesi, en fazla 16 KB):
//! `<data-dir>/prefs/<kullanıcı>.json`.

use crate::apierr::{ApiError, ApiResult};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const MAX_SIZE: usize = 16 << 10;

pub struct Store {
    pub dir: PathBuf,
    mu: Mutex<()>,
}

fn name_ok(user: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[a-z_][a-z0-9_-]{0,31}$").unwrap()).is_match(user)
}

fn is_object(b: &[u8]) -> bool {
    let t = String::from_utf8_lossy(b);
    let t = t.trim();
    t.starts_with('{') && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(t).is_ok()
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, mu: Mutex::new(()) }
    }

    fn path(&self, user: &str) -> ApiResult<PathBuf> {
        if !name_ok(user) {
            return Err(ApiError::invalid("Geçersiz kullanıcı adı."));
        }
        Ok(self.dir.join(format!("{user}.json")))
    }

    /// Kullanıcının tercihleri; yoksa `{}`.
    pub fn get(&self, user: &str) -> ApiResult<Vec<u8>> {
        let p = self.path(user)?;
        let _g = self.mu.lock().unwrap();
        match std::fs::read(&p) {
            Ok(b) if is_object(&b) => Ok(b),
            Ok(_) => Ok(b"{}".to_vec()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(b"{}".to_vec()),
            Err(e) => Err(e.into()),
        }
    }

    /// Tercihleri sıkıştırıp atomik yazar. Gövde JSON nesnesi olmalı.
    pub fn put(&self, user: &str, raw: &[u8]) -> ApiResult<Vec<u8>> {
        let p = self.path(user)?;
        if raw.len() > MAX_SIZE {
            return Err(ApiError::new(413, "invalid", "Tercihler en fazla 16 KB olabilir."));
        }
        if !is_object(raw) {
            return Err(ApiError::invalid("Tercihler bir JSON nesnesi olmalıdır."));
        }
        let v: serde_json::Value = serde_json::from_slice(raw).map_err(|_| ApiError::invalid("Geçersiz JSON."))?;
        let compact = serde_json::to_vec(&v)?;
        let _g = self.mu.lock().unwrap();
        std::fs::create_dir_all(&self.dir)?;
        crate::util::replace_file(&p, &compact, 0o640)?;
        Ok(compact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let d = std::env::temp_dir().join(format!("mk-prefs-{}", crate::util::random_hex(4)));
        let s = Store::new(d.clone());
        assert_eq!(s.get("maker").unwrap(), b"{}");
        assert_eq!(s.put("maker", br#"{ "a" : 1 }"#).unwrap(), br#"{"a":1}"#);
        assert_eq!(s.get("maker").unwrap(), br#"{"a":1}"#);
        assert_eq!(s.put("maker", b"[1]").unwrap_err().status, 400);
        assert_eq!(s.put("../x", b"{}").unwrap_err().status, 400);
        assert_eq!(s.put("maker", &vec![b' '; MAX_SIZE + 1]).unwrap_err().status, 413);
        let _ = std::fs::remove_dir_all(d);
    }
}
