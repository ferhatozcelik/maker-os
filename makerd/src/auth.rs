//! Bellek içi oturumlar (HMAC imzalı çerez), giriş hız sınırı, yerel kabuk
//! belirteci ve parola doğrulayıcı arayüzü.

use crate::apierr::ApiResult;
use crate::util::random_token;
use base64::Engine;
use futures_util::future::BoxFuture;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const COOKIE_NAME: &str = "mk_session";
pub const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
const MAX_SESSIONS: usize = 256;

/// Parola denetiminin sonucu.
#[derive(Debug, Clone, Copy, Default)]
pub struct CheckResult {
    pub ok: bool,
    pub admin: bool,
}

/// Kullanıcı/parola doğrular. Gerçek modda makerd-priv, mock'ta bellek içi.
pub trait Checker: Send + Sync {
    fn check<'a>(&'a self, user: &'a str, password: &'a str) -> BoxFuture<'a, ApiResult<CheckResult>>;
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub user: String,
    pub admin: bool,
    expires: Instant,
}

pub struct Manager {
    key: [u8; 32],
    ttl: Duration,
    sessions: Mutex<HashMap<String, Session>>,
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Manager {
    pub fn new() -> Self {
        let mut key = [0u8; 32];
        rand::fill(&mut key);
        Self { key, ttl: SESSION_TTL, sessions: Mutex::new(HashMap::new()) }
    }

    fn sign(&self, id: &str) -> String {
        let mut m = Hmac::<Sha256>::new_from_slice(&self.key).expect("hmac anahtarı");
        m.update(id.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())
    }

    /// Yeni oturum açar; çerez değerini döndürür.
    pub fn create(&self, user: &str, admin: bool) -> (Session, String) {
        let id = random_token(18);
        let now = Instant::now();
        let s = Session { id: id.clone(), user: user.into(), admin, expires: now + self.ttl };
        let mut map = self.sessions.lock().unwrap();
        map.retain(|_, v| v.expires > now);
        if map.len() >= MAX_SESSIONS {
            if let Some(oldest) = map.values().min_by_key(|v| v.expires).map(|v| v.id.clone()) {
                map.remove(&oldest);
            }
        }
        map.insert(id.clone(), s.clone());
        let cookie = format!("{id}.{}", self.sign(&id));
        (s, cookie)
    }

    /// Çerez değerini doğrular. İmza bozuksa harita bile taranmaz.
    pub fn lookup(&self, value: &str) -> Option<Session> {
        let (id, sig) = value.split_once('.')?;
        if id.is_empty() || !constant_eq(sig.as_bytes(), self.sign(id).as_bytes()) {
            return None;
        }
        let mut map = self.sessions.lock().unwrap();
        let s = map.get(id)?;
        if Instant::now() > s.expires {
            map.remove(id);
            return None;
        }
        Some(s.clone())
    }

    pub fn delete(&self, value: &str) {
        if let Some(s) = self.lookup(value) {
            self.sessions.lock().unwrap().remove(&s.id);
        }
    }

    pub fn set_cookie(&self, value: &str) -> String {
        format!("{COOKIE_NAME}={value}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict", self.ttl.as_secs())
    }

    pub fn clear_cookie(&self) -> String {
        format!("{COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict")
    }
}

/// Sabit süreli karşılaştırma.
pub fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Cookie başlığından bir çerezin değerini çıkarır.
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').map(str::trim).find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
}

/// IP başına kayan pencerede başarısız giriş denemelerini sınırlar.
pub struct Limiter {
    max: usize,
    window: Duration,
    fails: Mutex<HashMap<String, Vec<Instant>>>,
}

impl Limiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self { max, window, fails: Mutex::new(HashMap::new()) }
    }

    fn prune(map: &mut HashMap<String, Vec<Instant>>, window: Duration, ip: &str, now: Instant) {
        if let Some(list) = map.get_mut(ip) {
            list.retain(|t| now.duration_since(*t) < window);
            if list.is_empty() {
                map.remove(ip);
            }
        }
    }

    /// Yeni deneme yapılabiliyorsa None; değilse beklenmesi gereken süre.
    pub fn allow(&self, ip: &str) -> Option<Duration> {
        let mut map = self.fails.lock().unwrap();
        let now = Instant::now();
        Self::prune(&mut map, self.window, ip, now);
        let list = map.get(ip)?;
        if list.len() < self.max {
            return None;
        }
        Some(self.window.saturating_sub(now.duration_since(list[0])))
    }

    pub fn fail(&self, ip: &str) {
        let mut map = self.fails.lock().unwrap();
        let now = Instant::now();
        Self::prune(&mut map, self.window, ip, now);
        if map.len() > 4096 {
            let w = self.window;
            map.retain(|_, l| {
                l.retain(|t| now.duration_since(*t) < w);
                !l.is_empty()
            });
        }
        map.entry(ip.to_string()).or_default().push(now);
    }

    pub fn reset(&self, ip: &str) {
        self.fails.lock().unwrap().remove(ip);
    }
}

/// Yerel kabuk el sıkışması için yeni belirteç üretir ve dosyaya atomik (0640) yazar.
pub fn write_token(path: &Path) -> std::io::Result<String> {
    let token = random_token(24);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    crate::util::write_atomic(path, token.as_bytes(), 0o640)?;
    Ok(token)
}

/// Üyeleri yönetici sayılan gruplar.
pub const ADMIN_GROUPS: [&str; 3] = ["wheel", "sudo", "admin"];

/// Kullanıcı /etc/group'ta yönetici gruplarından birinde mi.
pub fn is_admin(group_file: &str, user: &str) -> bool {
    let Ok(s) = std::fs::read_to_string(group_file) else { return false };
    s.lines().any(|l| {
        let p: Vec<&str> = l.split(':').collect();
        p.len() >= 4 && ADMIN_GROUPS.contains(&p[0]) && p[3].split(',').any(|m| m.trim() == user)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrip() {
        let m = Manager::new();
        let (s, c) = m.create("maker", true);
        let got = m.lookup(&c).unwrap();
        assert_eq!(got.user, "maker");
        assert!(got.admin);
        assert!(m.lookup(&format!("{}.bozuk", s.id)).is_none());
        assert!(m.lookup("yok").is_none());
        m.delete(&c);
        assert!(m.lookup(&c).is_none());
    }

    #[test]
    fn limiter_blocks_after_max() {
        let l = Limiter::new(2, Duration::from_secs(60));
        assert!(l.allow("1.2.3.4").is_none());
        l.fail("1.2.3.4");
        l.fail("1.2.3.4");
        assert!(l.allow("1.2.3.4").is_some());
        assert!(l.allow("5.6.7.8").is_none());
        l.reset("1.2.3.4");
        assert!(l.allow("1.2.3.4").is_none());
    }

    #[test]
    fn cookies() {
        assert_eq!(cookie_value("a=1; mk_session=x.y; b=2", "mk_session"), Some("x.y"));
        assert_eq!(cookie_value("mk_sessionX=1", "mk_session"), None);
    }

    #[test]
    fn admin_groups() {
        let d = std::env::temp_dir().join(format!("mk-grp-{}", crate::util::random_hex(4)));
        std::fs::write(&d, "wheel:x:10:root,ali\nvideo:x:44:veli\n").unwrap();
        assert!(is_admin(d.to_str().unwrap(), "ali"));
        assert!(!is_admin(d.to_str().unwrap(), "veli"));
        let _ = std::fs::remove_file(d);
    }
}
