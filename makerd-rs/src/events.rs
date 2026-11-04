//! Konu (topic) abonelikli olay dağıtıcısı. Üreticiler (ör. 2 sn'de bir sistem
//! özeti) yalnızca o konuya en az bir abone varken çalışır.

use crate::apierr::ApiResult;
use futures_util::future::BoxFuture;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Bir konunun verisini üreten işlev.
pub type Fetch = Arc<dyn Fn() -> BoxFuture<'static, ApiResult<Value>> + Send + Sync>;

#[derive(Clone)]
pub enum Producer {
    /// Hemen ve sonra her aralıkta yayımlar.
    Every(Duration, Fetch),
    /// Aralıkla yoklar; yalnız JSON gösterimi değişince yayımlar.
    OnChange(Duration, Fetch),
}

struct Client {
    tx: mpsc::Sender<Arc<str>>,
    topics: HashSet<String>,
}

#[derive(Default)]
struct Inner {
    allowed: HashSet<String>,
    clients: HashMap<u64, Client>,
    counts: HashMap<String, usize>,
    producers: HashMap<String, Producer>,
    running: HashMap<String, JoinHandle<()>>,
    last: HashMap<String, Arc<str>>,
}

#[derive(Clone)]
pub struct Hub {
    inner: Arc<Mutex<Inner>>,
    next: Arc<AtomicU64>,
}

/// İstemci kaydı: `rx` çerçeveleri alır; düşürülünce kayıt silinmez, `remove` çağrılmalı.
pub struct ClientHandle {
    pub id: u64,
    pub rx: mpsc::Receiver<Arc<str>>,
}

#[derive(Serialize)]
struct Message<'a> {
    topic: &'a str,
    data: &'a Value,
}

impl Hub {
    pub fn new(topics: &[&str]) -> Self {
        let inner = Inner { allowed: topics.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        Self { inner: Arc::new(Mutex::new(inner)), next: Arc::new(AtomicU64::new(1)) }
    }

    /// Bir konu için üretici kaydeder.
    pub fn register(&self, topic: &str, p: Producer) {
        let mut g = self.inner.lock().unwrap();
        g.allowed.insert(topic.to_string());
        g.producers.insert(topic.to_string(), p);
        if g.counts.get(topic).copied().unwrap_or(0) > 0 {
            self.start_locked(&mut g, topic);
        }
    }

    pub fn new_client(&self) -> ClientHandle {
        let (tx, rx) = mpsc::channel(32);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.inner.lock().unwrap().clients.insert(id, Client { tx, topics: HashSet::new() });
        ClientHandle { id, rx }
    }

    /// İstemciyi konulara abone eder; bilinmeyen konular yok sayılır.
    pub fn subscribe(&self, id: u64, topics: &[String]) {
        let mut g = self.inner.lock().unwrap();
        for t in topics {
            if !g.allowed.contains(t) {
                continue;
            }
            let Some(c) = g.clients.get_mut(&id) else { return };
            if !c.topics.insert(t.clone()) {
                continue;
            }
            let n = {
                let e = g.counts.entry(t.clone()).or_insert(0);
                *e += 1;
                *e
            };
            if n == 1 {
                self.start_locked(&mut g, t);
            } else if let Some(b) = g.last.get(t).cloned() {
                if let Some(c) = g.clients.get(&id) {
                    let _ = c.tx.try_send(b);
                }
            }
        }
    }

    pub fn unsubscribe(&self, id: u64, topics: &[String]) {
        let mut g = self.inner.lock().unwrap();
        for t in topics {
            let removed = g.clients.get_mut(&id).map(|c| c.topics.remove(t)).unwrap_or(false);
            if removed {
                Self::dec_locked(&mut g, t);
            }
        }
    }

    /// İstemciyi tüm aboneliklerinden çıkarır.
    pub fn remove(&self, id: u64) {
        let mut g = self.inner.lock().unwrap();
        let Some(c) = g.clients.remove(&id) else { return };
        for t in c.topics {
            Self::dec_locked(&mut g, &t);
        }
    }

    pub fn subscribers(&self, topic: &str) -> usize {
        self.inner.lock().unwrap().counts.get(topic).copied().unwrap_or(0)
    }

    pub fn running(&self, topic: &str) -> bool {
        self.inner.lock().unwrap().running.contains_key(topic)
    }

    /// Konuya abone tüm istemcilere gönderir. Abone yoksa serileştirme bile yapılmaz;
    /// yavaş istemcinin kuyruğu doluysa çerçeve atlanır.
    pub fn publish(&self, topic: &str, data: impl Serialize) {
        if self.subscribers(topic) == 0 {
            return;
        }
        let Ok(v) = serde_json::to_value(data) else { return };
        self.publish_value(topic, &v);
    }

    pub fn publish_value(&self, topic: &str, data: &Value) {
        let Ok(s) = serde_json::to_string(&Message { topic, data }) else {
            tracing::error!(topic, "olay serileştirilemedi");
            return;
        };
        let b: Arc<str> = Arc::from(s);
        let mut g = self.inner.lock().unwrap();
        if g.producers.contains_key(topic) {
            g.last.insert(topic.to_string(), b.clone());
        }
        for c in g.clients.values() {
            if c.topics.contains(topic) {
                let _ = c.tx.try_send(b.clone());
            }
        }
    }

    fn start_locked(&self, g: &mut Inner, topic: &str) {
        let Some(p) = g.producers.get(topic).cloned() else { return };
        if g.running.contains_key(topic) {
            return;
        }
        let hub = self.clone();
        let t = topic.to_string();
        tracing::debug!(topic, "üretici başladı");
        let h = tokio::spawn(async move {
            match p {
                Producer::Every(iv, f) => {
                    let mut tick = tokio::time::interval(iv);
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    loop {
                        tick.tick().await;
                        if let Ok(v) = f().await {
                            hub.publish_value(&t, &v);
                        }
                    }
                }
                Producer::OnChange(iv, f) => {
                    let mut tick = tokio::time::interval(iv);
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    let mut prev = String::new();
                    loop {
                        tick.tick().await;
                        if let Ok(v) = f().await {
                            let s = v.to_string();
                            if s != prev {
                                prev = s;
                                hub.publish_value(&t, &v);
                            }
                        }
                    }
                }
            }
        });
        g.running.insert(topic.to_string(), h);
    }

    fn dec_locked(g: &mut Inner, topic: &str) {
        let n = g.counts.get_mut(topic).map(|n| {
            *n = n.saturating_sub(1);
            *n
        });
        if n.unwrap_or(0) > 0 {
            return;
        }
        g.counts.remove(topic);
        if let Some(h) = g.running.remove(topic) {
            h.abort();
            g.last.remove(topic);
            tracing::debug!(topic, "üretici durdu");
        }
    }
}

/// Bir sonuç döndüren async işlevi `Fetch`'e çevirir.
pub fn fetch<F, Fut, T>(f: F) -> Fetch
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ApiResult<T>> + Send + 'static,
    T: Serialize,
{
    let f = Arc::new(f);
    Arc::new(move || {
        let f = f.clone();
        Box::pin(async move { f().await.and_then(|v| serde_json::to_value(v).map_err(Into::into)) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[tokio::test]
    async fn producer_runs_only_with_subscribers() {
        let hub = Hub::new(&["system"]);
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        hub.register(
            "system",
            Producer::Every(
                Duration::from_millis(20),
                fetch(move || {
                    let c = c2.clone();
                    async move { Ok(c.fetch_add(1, Ordering::SeqCst)) }
                }),
            ),
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let mut cl = hub.new_client();
        hub.subscribe(cl.id, &["system".into(), "nope".into()]);
        assert!(hub.running("system"));
        let msg = tokio::time::timeout(Duration::from_secs(1), cl.rx.recv()).await.unwrap().unwrap();
        assert!(msg.contains("\"topic\":\"system\""));
        hub.remove(cl.id);
        assert!(!hub.running("system"));
        assert_eq!(hub.subscribers("system"), 0);
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_noop() {
        let hub = Hub::new(&["jobs"]);
        hub.publish("jobs", serde_json::json!({"id": "x"}));
        let mut cl = hub.new_client();
        hub.subscribe(cl.id, &["jobs".into()]);
        hub.publish("jobs", serde_json::json!({"id": "y"}));
        let m = cl.rx.recv().await.unwrap();
        assert_eq!(&*m, r#"{"topic":"jobs","data":{"id":"y"}}"#);
    }
}
