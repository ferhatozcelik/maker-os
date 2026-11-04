//! makerd-priv sunucusu: SO_PEERCRED ile çağıranı doğrular, tek satır JSON
//! istek okur, izin listesindeki komutu çalıştırır.

use super::{PrivError, Request, Response};
use futures_util::future::BoxFuture;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// SO_PEERCRED ile öğrenilen çağıran.
#[derive(Clone, Debug, Default)]
pub struct Peer {
    pub uid: u32,
    pub gid: u32,
    pub pid: i32,
    pub user: String,
}

pub type Handler = Arc<dyn Fn(Peer, Value) -> BoxFuture<'static, Result<Value, PrivError>> + Send + Sync>;

pub struct Server {
    pub allowed: HashSet<u32>,
    pub handlers: HashMap<&'static str, Handler>,
    /// Sıfırdan büyükse bu süre istek gelmezse `serve` döner (soket etkinleştirmede çıkış).
    pub idle: Duration,
}

pub fn peer_of(s: &UnixStream) -> std::io::Result<Peer> {
    let c = s.peer_cred()?;
    let uid = c.uid();
    Ok(Peer { uid, gid: c.gid(), pid: c.pid().unwrap_or(0), user: crate::util::name_of(uid).unwrap_or_default() })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Server {
    pub async fn serve(self: Arc<Self>, ln: UnixListener, mut stop: tokio::sync::watch::Receiver<bool>) -> std::io::Result<()> {
        let last = Arc::new(AtomicI64::new(now_ms()));
        let active = Arc::new(AtomicUsize::new(0));
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                _ = tick.tick(), if !self.idle.is_zero() => {
                    if active.load(Ordering::SeqCst) == 0 && now_ms() - last.load(Ordering::SeqCst) > self.idle.as_millis() as i64 {
                        tracing::info!("boşta kalındı, çıkılıyor");
                        break;
                    }
                }
                r = ln.accept() => {
                    let (conn, _) = r?;
                    active.fetch_add(1, Ordering::SeqCst);
                    let (me, last, active) = (self.clone(), last.clone(), active.clone());
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(Duration::from_secs(30), me.handle(conn)).await;
                        last.store(now_ms(), Ordering::SeqCst);
                        active.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            }
        }
        // Süren komutların bitmesini kısa süre bekle.
        for _ in 0..300 {
            if active.load(Ordering::SeqCst) == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }

    async fn handle(&self, mut conn: UnixStream) {
        async fn reply(c: &mut UnixStream, r: Response) {
            let mut b = serde_json::to_vec(&r).unwrap_or_default();
            b.push(b'\n');
            let _ = c.write_all(&b).await;
        }
        let fail = |code: &str, msg: &str| Response { ok: false, data: Value::Null, error: Some(PrivError::new(code, msg)) };
        let peer = match peer_of(&conn) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(err = %e, "çağıran kimliği alınamadı");
                return;
            }
        };
        if !self.allowed.contains(&peer.uid) {
            tracing::warn!(uid = peer.uid, pid = peer.pid, "izinsiz çağıran reddedildi");
            reply(&mut conn, fail("forbidden", "Bu işlem için yetkiniz yok.")).await;
            return;
        }
        let mut line = Vec::new();
        let (rd, _) = conn.split();
        let r = BufReader::new(rd.take(64 << 10)).read_until(b'\n', &mut line).await;
        if r.is_err() || line.last() != Some(&b'\n') {
            reply(&mut conn, fail("invalid", "İstek okunamadı.")).await;
            return;
        }
        let req: Request = match serde_json::from_slice(&line) {
            Ok(r) => r,
            Err(_) => {
                reply(&mut conn, fail("invalid", "Geçersiz JSON.")).await;
                return;
            }
        };
        let Some(h) = self.handlers.get(req.cmd.as_str()) else {
            tracing::warn!(cmd = %req.cmd, uid = peer.uid, "bilinmeyen komut");
            reply(&mut conn, fail("invalid", "Bilinmeyen komut.")).await;
            return;
        };
        // Her çağrı günlüğe yazılır (argümanlar parola içerebildiği için yazılmaz).
        tracing::info!(cmd = %req.cmd, uid = peer.uid, user = %peer.user, pid = peer.pid, "komut");
        match h(peer, req.args).await {
            Ok(data) => reply(&mut conn, Response { ok: true, data, error: None }).await,
            Err(e) => {
                if e.code == "internal" {
                    tracing::error!(cmd = %req.cmd, err = %e.message, "komut başarısız");
                }
                reply(&mut conn, Response { ok: false, data: Value::Null, error: Some(e) }).await
            }
        }
    }
}
