//! Terminal oturumları ve events WebSocket'leri.

use super::user::TermResp;
use super::*;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;

// --- terminal --------------------------------------------------------------

pub async fn term_create(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        cols: i64,
        #[serde(default)]
        rows: i64,
        /// Var olan (başka cihazda açılmış olabilir) oturuma bağlanmak için bilet.
        #[serde(default)]
        id: String,
        /// İsteğe bağlı: kabuk yerine bu komut (ör. "uv run main.py").
        #[serde(default)]
        cmd: String,
        /// Sekme başlığı.
        #[serde(default)]
        title: String,
    }
    let req: Req = decode(&body)?;
    let mut id = req.id;
    if id.is_empty() {
        if req.cmd.len() > 4096 || req.title.len() > 80 {
            return Err(ApiError::invalid("Komut veya başlık çok uzun."));
        }
        id = s.d.term.create_with(&sess.user, req.cols, req.rows, term::Opts { cmd: req.cmd, title: req.title }).await?.id.clone();
    }
    let t = s.d.term.ticket(&id, &sess.user)?;
    Ok(json(&TermResp { id, ticket: t }))
}

/// Kullanıcının açık terminal oturumları (web ve Pi ekranı ortak).
pub async fn term_list(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    Ok(json(&s.d.term.list(&sess.user)))
}

pub async fn term_close(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    if s.d.term.get(&id, &sess.user).is_none() {
        return Err(ApiError::not_found("Terminal oturumu bulunamadı."));
    }
    s.d.term.close(&id);
    Ok(no_content())
}

pub async fn term_ticket(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    let t = s.d.term.ticket(&id, &sess.user)?;
    Ok(json(&TermResp { id, ticket: t }))
}

fn upgrade_of(u: Option<WebSocketUpgrade>) -> ApiResult<WebSocketUpgrade> {
    u.ok_or_else(|| ApiError::invalid("WebSocket bağlantısı gerekli."))
}

pub async fn term_ws(State(s): State<S>, h: HeaderMap, Path(id): Path<String>, Query(q): Query<HashMap<String, String>>, up: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>) -> R {
    if !origin_ok(&h) {
        return Err(ApiError::forbidden("Geçersiz Origin."));
    }
    let sess = s.d.term.redeem(q.get("ticket").map(String::as_str).unwrap_or(""), &id).ok_or_else(|| ApiError::forbidden("Geçersiz veya süresi dolmuş bilet."))?;
    let up = upgrade_of(up.ok())?;
    let s2 = s.clone();
    Ok(up.max_message_size(1 << 20).on_upgrade(move |ws| async move {
        tokio::select! {
            _ = s2.d.term.attach(sess, ws) => {}
            _ = s2.stop.cancelled() => {}
        }
    }))
}

// --- events ----------------------------------------------------------------

pub async fn events_ws(State(s): State<S>, h: HeaderMap, up: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>) -> R {
    session_of(&s, &h)?;
    if !origin_ok(&h) {
        return Err(ApiError::forbidden("Geçersiz Origin."));
    }
    let up = upgrade_of(up.ok())?;
    Ok(up.max_message_size(64 << 10).on_upgrade(move |ws| events_loop(s, ws)))
}

async fn events_loop(s: S, ws: WebSocket) {
    #[derive(Deserialize)]
    struct Op {
        #[serde(default)]
        op: String,
        #[serde(default)]
        topics: Vec<String>,
    }
    let mut cl = s.d.hub.new_client();
    let id = cl.id;
    let (mut tx, mut rx) = ws.split();
    let hub = s.d.hub.clone();
    let reader = async move {
        while let Some(Ok(m)) = rx.next().await {
            let data: &[u8] = match &m {
                Message::Text(t) => t.as_bytes(),
                Message::Binary(b) => b,
                Message::Close(_) => break,
                _ => continue,
            };
            if let Ok(op) = serde_json::from_slice::<Op>(data) {
                match op.op.as_str() {
                    "sub" => hub.subscribe(id, &op.topics),
                    "unsub" => hub.unsubscribe(id, &op.topics),
                    _ => {}
                }
            }
        }
    };
    let writer = async {
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        ping.tick().await;
        loop {
            tokio::select! {
                b = cl.rx.recv() => {
                    let Some(b) = b else { break };
                    let send = tx.send(Message::Text(b.as_ref().into()));
                    if !matches!(tokio::time::timeout(Duration::from_secs(10), send).await, Ok(Ok(()))) {
                        break;
                    }
                }
                _ = ping.tick() => {
                    let send = tx.send(Message::Ping(Bytes::new()));
                    if !matches!(tokio::time::timeout(Duration::from_secs(10), send).await, Ok(Ok(()))) {
                        break;
                    }
                }
                _ = s.stop.cancelled() => {
                    let _ = tx.send(Message::Close(None)).await;
                    break;
                }
            }
        }
    };
    tokio::select! {
        _ = reader => {}
        _ = writer => {}
    }
    s.d.hub.remove(id);
}
