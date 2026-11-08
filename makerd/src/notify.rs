//! Bildirimleri bellekte tutar ve "notifications" konusuna yayımlar.
//!
//! Otomatik temizlik: okunmuş bildirimler 1 saat, tümü 24 saat sonra atılır.
//! AI kancası: her bildirim olay dosyasına (varsayılan /run/makeros/olaylar.log) tek kısa satır olarak yazılır
//! (`05:40 kamera: Fotoğraf kaydedildi — ~/Resimler/…`, en çok 120 karakter, son 20 satır). Yerel AI asistanı
//! (makeros-ai) bu dosyayı okuyup "son olaylar" sorularında bağlama ekler.

use crate::events::Hub;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_ITEMS: usize = 100;
const READ_TTL_S: i64 = 3600;
const ALL_TTL_S: i64 = 24 * 3600;
const HOOK_LINES: usize = 20;
const HOOK_LINE_MAX: usize = 120;
pub const HOOK_PATH: &str = "/run/makeros/olaylar.log";

#[derive(Serialize, Clone, Debug)]
pub struct Notification {
    pub id: String,
    pub app: String,
    pub title: String,
    pub body: String,
    pub time: jiff::Timestamp,
    pub read: bool,
}

#[derive(Default)]
struct Inner {
    items: Vec<Notification>, // eskiden yeniye
    next: u64,
    hook: Vec<String>,
}

#[derive(Clone)]
pub struct Store {
    inner: Arc<Mutex<Inner>>,
    hub: Option<Hub>,
    hook: Option<PathBuf>,
}

/// AI kancası satırı: saat, uygulama, başlık, gövde; tek satır, kısa.
pub fn hook_line(n: &Notification) -> String {
    let t = n.time.to_zoned(jiff::tz::TimeZone::system()).strftime("%H:%M").to_string();
    let app = n.app.rsplit('.').next().unwrap_or(&n.app);
    let mut s = format!("{t} {app}: {}", n.title.trim());
    let body = n.body.split_whitespace().collect::<Vec<_>>().join(" ");
    if !body.is_empty() {
        s.push_str(" — ");
        s.push_str(&body);
    }
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    if s.chars().count() > HOOK_LINE_MAX { s.chars().take(HOOK_LINE_MAX - 1).collect::<String>() + "…" } else { s }
}

fn prune(items: &mut Vec<Notification>, now: jiff::Timestamp) {
    items.retain(|n| {
        let age = now.as_second() - n.time.as_second();
        age < ALL_TTL_S && !(n.read && age >= READ_TTL_S)
    });
}

impl Store {
    pub fn new(hub: Option<Hub>) -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())), hub, hook: None }
    }

    /// AI olay dosyası (bkz. modül belgesi).
    pub fn with_hook(mut self, path: impl Into<PathBuf>) -> Self {
        let p = path.into();
        // Önceki çalıştırmadan kalan satırlar korunur.
        if let Ok(s) = std::fs::read_to_string(&p) {
            self.inner.lock().unwrap().hook = s.lines().map(String::from).rev().take(HOOK_LINES).collect::<Vec<_>>().into_iter().rev().collect();
        }
        self.hook = Some(p);
        self
    }

    /// Süresi dolanları atar (periyodik çağrılır; ekleme ve listelemede de).
    pub fn prune(&self) {
        let removed = {
            let mut g = self.inner.lock().unwrap();
            let before = g.items.len();
            prune(&mut g.items, crate::util::now_secs());
            before != g.items.len()
        };
        if removed {
            if let Some(h) = &self.hub {
                h.publish("notifications", serde_json::json!({"pruned": true}));
            }
        }
    }

    pub fn add(&self, app: &str, title: &str, body: &str) -> Notification {
        let n = {
            let mut g = self.inner.lock().unwrap();
            g.next += 1;
            let n = Notification {
                id: format!("n{}", g.next),
                app: app.into(),
                title: title.into(),
                body: body.into(),
                time: crate::util::now_secs(),
                read: false,
            };
            g.items.push(n.clone());
            let now = n.time;
            prune(&mut g.items, now);
            if g.items.len() > MAX_ITEMS {
                let cut = g.items.len() - MAX_ITEMS;
                g.items.drain(..cut);
            }
            if let Some(p) = &self.hook {
                g.hook.push(hook_line(&n));
                if g.hook.len() > HOOK_LINES {
                    let cut = g.hook.len() - HOOK_LINES;
                    g.hook.drain(..cut);
                }
                let mut body = g.hook.join("\n");
                body.push('\n');
                if let Err(e) = crate::util::write_atomic(p, body.as_bytes(), 0o644) {
                    tracing::debug!("olay dosyası yazılamadı: {e}");
                }
            }
            n
        };
        if let Some(h) = &self.hub {
            h.publish("notifications", &n);
        }
        n
    }

    /// Yeniden eskiye.
    pub fn list(&self) -> Vec<Notification> {
        let mut g = self.inner.lock().unwrap();
        prune(&mut g.items, crate::util::now_secs());
        g.items.iter().rev().cloned().collect()
    }

    pub fn mark_read(&self, id: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.items.iter_mut().find(|n| n.id == id) {
            Some(n) => {
                n.read = true;
                true
            }
            None => false,
        }
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().items.clear();
        if let Some(h) = &self.hub {
            h.publish("notifications", serde_json::json!({"cleared": true}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_and_capped() {
        let s = Store::new(None);
        for i in 0..105 {
            s.add("a", &format!("t{i}"), "");
        }
        let l = s.list();
        assert_eq!(l.len(), 100);
        assert_eq!(l[0].title, "t104");
        assert!(s.mark_read(&l[0].id));
        assert!(!s.mark_read("yok"));
        s.clear();
        assert!(s.list().is_empty());
    }

    #[test]
    fn auto_prune_and_hook() {
        let d = std::env::temp_dir().join(format!("mk-notify-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&d).unwrap();
        let s = Store::new(None).with_hook(d.join("olaylar.log"));
        let n = s.add("makeros.camera", "Fotoğraf kaydedildi", "~/Resimler/Kamera/a.jpg\nikinci satır");
        let line = std::fs::read_to_string(d.join("olaylar.log")).unwrap();
        assert!(line.ends_with("camera: Fotoğraf kaydedildi — ~/Resimler/Kamera/a.jpg ikinci satır\n"), "{line}");
        for i in 0..25 { s.add("a", &format!("t{i}"), &"x".repeat(300)); }
        let l = std::fs::read_to_string(d.join("olaylar.log")).unwrap();
        assert_eq!(l.lines().count(), 20);
        assert!(l.lines().all(|x| x.chars().count() <= 120));
        // eski ve okunmuş olanlar atılır
        let now = crate::util::now_secs();
        let mut v = vec![
            Notification { id: "a".into(), app: "x".into(), title: "eski".into(), body: String::new(), time: now - jiff::SignedDuration::from_hours(25), read: false },
            Notification { id: "b".into(), app: "x".into(), title: "okunmuş".into(), body: String::new(), time: now - jiff::SignedDuration::from_hours(2), read: true },
            Notification { id: "c".into(), app: "x".into(), title: "yeni okunmuş".into(), body: String::new(), time: now, read: true },
            n,
        ];
        prune(&mut v, now);
        assert_eq!(v.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["c", "n1"]);
        let _ = std::fs::remove_dir_all(d);
    }
}
