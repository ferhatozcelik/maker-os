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

// --- pencereli programlar (X köprüsü) ----------------------------------------

type Upgrade = Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>;

/// Denetim akışı: pencere listesi ve olaylar (açıldı/kapandı/başlık/boyut).
pub async fn xwin_ctl(State(s): State<S>, h: HeaderMap, up: Upgrade) -> R {
    xwin_open(s, h, up, String::new()).await
}

/// Tek pencerenin görüntü akışı ve girdisi.
pub async fn xwin_win(State(s): State<S>, h: HeaderMap, Path(id): Path<String>, up: Upgrade) -> R {
    if id.is_empty() || id.len() > 12 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::invalid("Geçersiz pencere."));
    }
    xwin_open(s, h, up, id).await
}

/// Tarayıcı hoparlörü ve mikrofonu: kullanıcı programlarının sesi (bkz. crate::audio::stream).
pub async fn audio(State(s): State<S>, h: HeaderMap, up: Upgrade) -> R {
    stream_open(s, h, up, "audio", String::new()).await
}

async fn xwin_open(s: S, h: HeaderMap, up: Upgrade, path: String) -> R {
    stream_open(s, h, up, "xwin", path).await
}

async fn stream_open(s: S, h: HeaderMap, up: Upgrade, mode: &str, path: String) -> R {
    let sess = session_of(&s, &h)?;
    if !origin_ok(&h) {
        return Err(ApiError::forbidden("Geçersiz Origin."));
    }
    let up = upgrade_of(up.ok())?;
    let agent = s.d.agent.clone().ok_or_else(|| ApiError::unavailable("Kullanıcı hizmeti yok."))?;
    let (r, w) = agent.stream(&sess.user, crate::termproc::Open { mode: mode.into(), path, ..Default::default() }).await?;
    Ok(up.max_message_size(1 << 20).on_upgrade(move |ws| xwin_pump(s, ws, r, w)))
}

/// WebSocket ↔ makerd-user çerçeveleri ('d': bir ileti; '{' ile başlayan metin).
async fn xwin_pump(s: S, ws: WebSocket, mut r: Box<dyn tokio::io::AsyncRead + Send + Unpin>, mut w: Box<dyn tokio::io::AsyncWrite + Send + Unpin>) {
    use crate::termproc::{read_frame, write_frame, FRAME_DATA, FRAME_HANGUP};
    let (mut wtx, mut wrx) = ws.split();
    let up = async {
        while let Some(Ok(m)) = wrx.next().await {
            let b: Bytes = match m {
                Message::Text(t) => Bytes::copy_from_slice(t.as_bytes()),
                Message::Binary(b) => b,
                Message::Close(_) => break,
                _ => continue,
            };
            if b.len() > 64 << 10 || write_frame(&mut w, FRAME_DATA, &b).await.is_err() {
                break;
            }
        }
        let _ = write_frame(&mut w, FRAME_HANGUP, b"").await;
    };
    // read_frame iptale dayanıklı değil: okuma ayrı görevde, çerçeveler kanaldan gelir.
    let (ftx, mut frx) = tokio::sync::mpsc::channel(32);
    let reader = tokio::spawn(async move {
        loop {
            let f = read_frame(&mut r).await;
            let end = !matches!(f, Ok((FRAME_DATA, _)));
            if ftx.send(f).await.is_err() || end {
                break;
            }
        }
    });
    let down = async {
        let mut ping = tokio::time::interval(Duration::from_secs(25));
        ping.tick().await;
        loop {
            tokio::select! {
                f = frx.recv() => {
                    let Some(f) = f else { break };
                    let m = match f {
                        Ok((FRAME_DATA, p)) if p.first() == Some(&b'{') => match String::from_utf8(p) {
                            Ok(t) => Message::Text(t.into()),
                            Err(_) => continue,
                        },
                        Ok((FRAME_DATA, p)) => Message::Binary(p.into()),
                        Ok((crate::termproc::FRAME_ERROR, p)) => {
                            let msg = serde_json::json!({"t": "error", "message": String::from_utf8_lossy(&p)}).to_string();
                            let _ = wtx.send(Message::Text(msg.into())).await;
                            break;
                        }
                        Ok(_) | Err(_) => break,
                    };
                    if !matches!(tokio::time::timeout(Duration::from_secs(20), wtx.send(m)).await, Ok(Ok(()))) {
                        break;
                    }
                }
                _ = ping.tick() => {
                    if !matches!(tokio::time::timeout(Duration::from_secs(10), wtx.send(Message::Ping(Bytes::new()))).await, Ok(Ok(()))) {
                        break;
                    }
                }
                _ = s.stop.cancelled() => break,
            }
        }
        let _ = wtx.send(Message::Close(None)).await;
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
    reader.abort();
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
