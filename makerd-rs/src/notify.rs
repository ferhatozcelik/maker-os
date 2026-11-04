//! Bildirimleri bellekte tutar ve "notifications" konusuna yayımlar.

use crate::events::Hub;
use serde::Serialize;
use std::sync::{Arc, Mutex};

const MAX_ITEMS: usize = 100;

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
}

#[derive(Clone)]
pub struct Store {
    inner: Arc<Mutex<Inner>>,
    hub: Option<Hub>,
}

impl Store {
    pub fn new(hub: Option<Hub>) -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())), hub }
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
            if g.items.len() > MAX_ITEMS {
                let cut = g.items.len() - MAX_ITEMS;
                g.items.drain(..cut);
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
        self.inner.lock().unwrap().items.iter().rev().cloned().collect()
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
}
