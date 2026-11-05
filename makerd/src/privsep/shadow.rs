//! /etc/shadow okuma ve atomik güncelleme.

use super::crypt::{self, CryptError};
use std::path::Path;

#[derive(Debug)]
pub enum ShadowError {
    NoUser,
    Locked,
    Unsupported,
    Io(std::io::Error),
}

impl std::fmt::Display for ShadowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadowError::NoUser => f.write_str("kullanıcı bulunamadı"),
            ShadowError::Locked => f.write_str("hesap kilitli"),
            ShadowError::Unsupported => f.write_str("desteklenmeyen parola özeti biçimi"),
            ShadowError::Io(e) => e.fmt(f),
        }
    }
}

/// Kullanıcının özeti.
pub fn shadow_hash(path: &str, user: &str) -> Result<String, ShadowError> {
    let s = std::fs::read_to_string(path).map_err(ShadowError::Io)?;
    for l in s.lines() {
        let f: Vec<&str> = l.split(':').collect();
        if f.len() >= 2 && f[0] == user {
            let h = f[1];
            if h.is_empty() || h.starts_with('!') || h.starts_with('*') {
                return Err(ShadowError::Locked);
            }
            return Ok(h.to_string());
        }
    }
    Err(ShadowError::NoUser)
}

/// Parolayı shadow dosyasına karşı doğrular. Kullanıcı yok/kilitliyse Ok(false).
pub fn check_shadow(path: &str, user: &str, password: &str) -> Result<bool, ShadowError> {
    match shadow_hash(path, user) {
        Err(ShadowError::NoUser | ShadowError::Locked) => Ok(false),
        Err(e) => Err(e),
        Ok(h) => crypt::verify(password, &h).map_err(|e| match e {
            CryptError::Unsupported => ShadowError::Unsupported,
            CryptError::BadRounds => ShadowError::Io(std::io::Error::other("geçersiz özet")),
        }),
    }
}

/// Kullanıcının özetini ve son değişiklik gününü atomik günceller (izin/sahiplik korunur).
pub fn set_shadow_hash(path: &str, user: &str, hash: &str, now_secs: i64) -> Result<(), ShadowError> {
    let data = std::fs::read_to_string(path).map_err(ShadowError::Io)?;
    let mut found = false;
    let lines: Vec<String> = data
        .split('\n')
        .map(|line| {
            let mut f: Vec<String> = line.split(':').map(String::from).collect();
            if f.len() >= 3 && f[0] == user {
                f[1] = hash.to_string();
                f[2] = (now_secs / 86400).to_string();
                found = true;
                f.join(":")
            } else {
                line.to_string()
            }
        })
        .collect();
    if !found {
        return Err(ShadowError::NoUser);
    }
    crate::util::replace_file(Path::new(path), lines.join("\n").as_bytes(), 0o600).map_err(ShadowError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_and_set() {
        let p = std::env::temp_dir().join(format!("mk-shadow-{}", crate::util::random_hex(4)));
        let h = crypt::sha512_crypt("maker123", "$6$abcdefgh").unwrap();
        std::fs::write(&p, format!("root:*:19000:0:99999:7:::\nmaker:{h}:19000:0:99999:7:::\nkilit:!:1::::::\n")).unwrap();
        let ps = p.to_str().unwrap();
        assert!(check_shadow(ps, "maker", "maker123").unwrap());
        assert!(!check_shadow(ps, "maker", "x").unwrap());
        assert!(!check_shadow(ps, "kilit", "x").unwrap());
        assert!(!check_shadow(ps, "yok", "x").unwrap());
        let nh = crypt::hash_password("yeni-parola");
        set_shadow_hash(ps, "maker", &nh, 86400 * 20000).unwrap();
        assert!(check_shadow(ps, "maker", "yeni-parola").unwrap());
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains(":20000:0:99999"));
        assert!(s.starts_with("root:*:19000"));
        let _ = std::fs::remove_file(p);
    }
}
