//! makerd tarafındaki makerd-priv istemcisi.

use super::{PrivError, Request, Response};
use crate::apierr::ApiError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug)]
pub enum CallError {
    /// makerd-priv'in döndürdüğü hata.
    Priv(PrivError),
    /// Bağlantı/zaman aşımı/protokol hatası.
    Transport(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Priv(e) => e.fmt(f),
            CallError::Transport(s) => f.write_str(s),
        }
    }
}

/// makerd-priv hatasını API hatasına çevirir.
impl From<CallError> for ApiError {
    fn from(e: CallError) -> Self {
        match e {
            CallError::Priv(pe) => match pe.code.as_str() {
                "invalid" => ApiError::invalid(pe.message),
                "forbidden" => ApiError::forbidden(pe.message),
                "not_found" => ApiError::not_found(pe.message),
                "conflict" => ApiError::conflict(pe.message),
                _ => ApiError::internal(pe.message),
            },
            CallError::Transport(_) => ApiError::unavailable("Yetkili yardımcıya (makerd-priv) ulaşılamadı."),
        }
    }
}

#[derive(Clone)]
pub struct Client {
    pub socket: String,
    pub timeout: Duration,
}

impl Client {
    pub fn new(socket: &str) -> Self {
        Self { socket: socket.into(), timeout: Duration::from_secs(15) }
    }

    /// Komutu çalıştırır; yanıt verisi `T`'ye çözülür.
    pub async fn call<A: Serialize, T: DeserializeOwned + Default>(&self, cmd: &str, args: &A) -> Result<T, CallError> {
        match tokio::time::timeout(self.timeout, self.call_inner(cmd, args)).await {
            Ok(r) => r,
            Err(_) => Err(CallError::Transport("makerd-priv zaman aşımı".into())),
        }
    }

    async fn call_inner<A: Serialize, T: DeserializeOwned + Default>(&self, cmd: &str, args: &A) -> Result<T, CallError> {
        let mut conn = UnixStream::connect(&self.socket).await.map_err(|e| CallError::Transport(format!("makerd-priv'e bağlanılamadı: {e}")))?;
        let args = serde_json::to_value(args).map_err(|e| CallError::Transport(e.to_string()))?;
        let mut req = serde_json::to_vec(&Request { cmd: cmd.into(), args }).map_err(|e| CallError::Transport(e.to_string()))?;
        req.push(b'\n');
        // Sunucu isteği okumadan yanıtlayıp kapatabilir (izinsiz çağıran): yazma hatası olsa da yanıt okunur.
        let werr = conn.write_all(&req).await.err();
        let mut line = Vec::new();
        let n = BufReader::new(&mut conn).read_until(b'\n', &mut line).await;
        match n {
            Ok(n) if n > 0 => {}
            _ => {
                return Err(CallError::Transport(match werr {
                    Some(e) => e.to_string(),
                    None => "makerd-priv yanıt vermedi".into(),
                }));
            }
        }
        let resp: Response = serde_json::from_slice(&line).map_err(|e| CallError::Transport(e.to_string()))?;
        if !resp.ok {
            return Err(CallError::Priv(resp.error.unwrap_or_else(|| PrivError::new("internal", "bilinmeyen hata"))));
        }
        if resp.data.is_null() {
            return Ok(T::default());
        }
        serde_json::from_value(resp.data).map_err(|e| CallError::Transport(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::server::{Handler, Server};
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    #[tokio::test]
    async fn roundtrip_and_forbidden() {
        let dir = std::env::temp_dir().join(format!("mk-priv-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("p.sock");
        let ln = tokio::net::UnixListener::bind(&sock).unwrap();
        let mut handlers: HashMap<&'static str, Handler> = HashMap::new();
        handlers.insert("echo", Arc::new(|p, v| Box::pin(async move { Ok(serde_json::json!({"uid": p.uid, "got": v})) })));
        let me = crate::util::getuid();
        let (tx, rx) = tokio::sync::watch::channel(false);
        let srv = Arc::new(Server { allowed: HashSet::from([me]), handlers, idle: Duration::ZERO });
        tokio::spawn(srv.serve(ln, rx));
        let c = Client::new(sock.to_str().unwrap());
        let v: serde_json::Value = c.call("echo", &serde_json::json!({"a": 1})).await.unwrap();
        assert_eq!(v["got"]["a"], 1);
        let e = c.call::<_, serde_json::Value>("yok", &()).await.unwrap_err();
        assert!(matches!(e, CallError::Priv(ref p) if p.code == "invalid"));
        let _ = tx.send(true);
        let _ = std::fs::remove_dir_all(dir);
    }
}
