//! Uzun süren işleri (ör. Wi-Fi bağlantısı) bellekte izler. Her güncelleme
//! "jobs" konusuna `{"id","state","progress","log"?}` olarak yayımlanır.

use crate::events::Hub;
use futures_util::FutureExt;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const STATE_RUNNING: &str = "running";
pub const STATE_DONE: &str = "done";
pub const STATE_FAILED: &str = "failed";

const MAX_LOG: usize = 200;
const KEEP_FOR: Duration = Duration::from_secs(15 * 60);
pub const JOB_MAX_DUR: Duration = Duration::from_secs(3 * 60);

#[derive(Serialize, Clone)]
pub struct Event {
    pub id: String,
    pub state: &'static str,
    pub progress: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub log: String,
}

#[derive(Serialize, Clone)]
pub struct Job {
    pub id: String,
    pub kind: String,
    #[serde(skip)]
    pub user: String,
    pub state: &'static str,
    pub progress: i32,
    pub log: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    pub created: jiff::Timestamp,
    pub updated: jiff::Timestamp,
    #[serde(skip)]
    touched: Option<Instant>,
}

#[derive(Clone)]
pub struct Manager {
    hub: Option<Hub>,
    jobs: Arc<Mutex<HashMap<String, Job>>>,
}

/// Çalışan işin ilerleme bildirme arayüzü (eşzamanlı; her iş parçacığından çağrılabilir).
#[derive(Clone)]
pub struct Handle {
    m: Manager,
    id: String,
}

impl Handle {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn progress(&self, p: i32) {
        self.m.update(&self.id, p, "");
    }
    pub fn log(&self, line: &str) {
        self.m.update(&self.id, -1, line);
    }
    pub fn step(&self, p: i32, line: &str) {
        self.m.update(&self.id, p, line);
    }
}

impl Manager {
    pub fn new(hub: Option<Hub>) -> Self {
        Self { hub, jobs: Arc::new(Mutex::new(HashMap::new())) }
    }

    fn publish(&self, ev: Event) {
        if let Some(h) = &self.hub {
            h.publish("jobs", ev);
        }
    }

    /// İşi arka planda çalıştırır (en çok 3 dk) ve kimliğini döndürür.
    pub fn start<F, Fut>(&self, kind: &str, user: &str, f: F) -> String
    where
        F: FnOnce(Handle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        self.start_for(kind, user, JOB_MAX_DUR, f)
    }

    /// `start` gibi; uzun işler için süre sınırı verilir.
    pub fn start_for<F, Fut>(&self, kind: &str, user: &str, max: Duration, f: F) -> String
    where
        F: FnOnce(Handle) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let id = crate::util::random_hex(8);
        let now = crate::util::now();
        {
            let mut g = self.jobs.lock().unwrap();
            let inst = Instant::now();
            g.retain(|_, j| j.state == STATE_RUNNING || j.touched.map(|t| inst.duration_since(t) <= KEEP_FOR).unwrap_or(true));
            g.insert(
                id.clone(),
                Job {
                    id: id.clone(),
                    kind: kind.into(),
                    user: user.into(),
                    state: STATE_RUNNING,
                    progress: 0,
                    log: Vec::new(),
                    error: String::new(),
                    created: now,
                    updated: now,
                    touched: Some(inst),
                },
            );
        }
        self.publish(Event { id: id.clone(), state: STATE_RUNNING, progress: 0, log: String::new() });
        let m = self.clone();
        let h = Handle { m: self.clone(), id: id.clone() };
        let jid = id.clone();
        tokio::spawn(async move {
            let fut = AssertUnwindSafe(f(h)).catch_unwind();
            let res = match tokio::time::timeout(max, fut).await {
                Ok(Ok(r)) => r,
                Ok(Err(p)) => Err(format!("iş çöktü: {}", panic_msg(&p))),
                Err(_) => Err("İş zaman aşımına uğradı.".into()),
            };
            m.finish(&jid, res);
        });
        id
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.jobs.lock().unwrap().get(id).cloned()
    }

    fn update(&self, id: &str, progress: i32, line: &str) {
        let ev = {
            let mut g = self.jobs.lock().unwrap();
            let Some(j) = g.get_mut(id) else { return };
            if j.state != STATE_RUNNING {
                return;
            }
            if progress >= 0 {
                j.progress = progress.clamp(0, 100);
            }
            if !line.is_empty() {
                j.log.push(line.to_string());
                if j.log.len() > MAX_LOG {
                    let cut = j.log.len() - MAX_LOG;
                    j.log.drain(..cut);
                }
            }
            j.updated = crate::util::now();
            j.touched = Some(Instant::now());
            Event { id: id.into(), state: j.state, progress: j.progress, log: line.into() }
        };
        self.publish(ev);
    }

    fn finish(&self, id: &str, res: Result<(), String>) {
        let (ev, kind) = {
            let mut g = self.jobs.lock().unwrap();
            let Some(j) = g.get_mut(id) else { return };
            let mut ev = Event { id: id.into(), state: STATE_DONE, progress: 100, log: String::new() };
            match &res {
                Err(e) => {
                    j.state = STATE_FAILED;
                    j.error = e.clone();
                    j.log.push(e.clone());
                    ev.log = e.clone();
                }
                Ok(()) => {
                    j.state = STATE_DONE;
                    j.progress = 100;
                }
            }
            j.updated = crate::util::now();
            j.touched = Some(Instant::now());
            ev.state = j.state;
            ev.progress = j.progress;
            (ev, j.kind.clone())
        };
        match &res {
            Err(e) => tracing::warn!(id, kind, err = %e, "iş başarısız"),
            Ok(()) => tracing::info!(id, kind, "iş tamamlandı"),
        }
        self.publish(ev);
    }
}

fn panic_msg(p: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "bilinmeyen".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn job_lifecycle() {
        let m = Manager::new(None);
        let id = m.start("t", "maker", |h| async move {
            h.step(40, "yarısı");
            Ok(())
        });
        for _ in 0..50 {
            if m.get(&id).unwrap().state != STATE_RUNNING {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let j = m.get(&id).unwrap();
        assert_eq!(j.state, STATE_DONE);
        assert_eq!(j.progress, 100);
        assert_eq!(j.log, vec!["yarısı".to_string()]);

        let id = m.start("t", "maker", |_| async move { Err("olmadı".into()) });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let j = m.get(&id).unwrap();
        assert_eq!(j.state, STATE_FAILED);
        assert_eq!(j.error, "olmadı");
    }
}
