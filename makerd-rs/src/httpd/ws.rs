//! Terminal oturumları, events ve VNC WebSocket'leri, Ekran Modu.

use super::user::TermResp;
use super::*;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

// --- VNC ------------------------------------------------------------------

/// noVNC'yi yerel VNC sunucusuna (wayvnc, yalnız 127.0.0.1) bağlar. Uzak
/// masaüstü sistemi tam denetler, bu yüzden yönetici oturumu ister.
pub async fn vnc_ws(State(s): State<S>, h: HeaderMap, up: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>) -> R {
    let sess = admin_of(&s, &h)?;
    if s.d.vnc_addr.is_empty() {
        return Err(ApiError::unavailable("Uzak masaüstü bu sistemde kapalı."));
    }
    if s.d.screen.as_ref().is_some_and(|sc| !sc.enabled()) {
        return Err(ApiError::forbidden("Ekran Modu kapalı. Ayarlar › Ekran Modu'ndan açabilirsiniz."));
    }
    if !origin_ok(&h) {
        return Err(ApiError::forbidden("Geçersiz Origin."));
    }
    let addr = s.d.vnc_addr.clone();
    bridge_rfb(s, up.ok(), addr, "uzak masaüstü", sess.user, None).await
}

/// MakerOS'taki "Chromium" penceresini uzak tarayıcı oturumuna bağlar: oturum
/// ilk pencerede başlar, son pencere kapanınca bir süre sonra durur.
pub async fn browser_ws(State(s): State<S>, h: HeaderMap, up: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>) -> R {
    let sess = admin_of(&s, &h)?;
    let b = s.d.browser.clone().ok_or_else(|| ApiError::unavailable("Tarayıcı bu sistemde yok."))?;
    if !origin_ok(&h) {
        return Err(ApiError::forbidden("Geçersiz Origin."));
    }
    let up = upgrade_of(up.ok())?;
    b.acquire().await.map_err(|e| ApiError::unavailable(format!("Tarayıcı başlatılamadı: {e}")))?;
    let addr = b.addr.clone();
    let res = bridge_rfb(s, Some(up), addr, "tarayıcı", sess.user, Some(b.clone())).await;
    if res.is_err() {
        b.release().await;
    }
    res
}

/// WebSocket ikili çerçeveler <-> yerel RFB TCP akışı (websockify'ın yaptığı iş).
async fn bridge_rfb(s: S, up: Option<WebSocketUpgrade>, addr: String, what: &'static str, user: String, release: Option<Arc<browser::Manager>>) -> R {
    let up = upgrade_of(up)?;
    let tc = match tokio::time::timeout(Duration::from_secs(3), tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(c)) => c,
        _ => return Err(ApiError::unavailable("Uzak masaüstü sunucusu çalışmıyor (ekran oturumu açık mı?).")),
    };
    let _ = tc.set_nodelay(true);
    // noVNC "binary" alt protokolünü ister; Origin yukarıda doğrulandı.
    Ok(up.protocols(["binary"]).max_message_size(4 << 20).on_upgrade(move |ws| async move {
        tracing::info!(user = %user, "{what} bağlandı");
        let (mut wtx, mut wrx) = ws.split();
        let (mut tr, mut tw) = tc.into_split();
        let tcp_to_ws = async {
            let mut buf = vec![0u8; 64 << 10];
            loop {
                let n = match tr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let send = wtx.send(Message::Binary(Bytes::copy_from_slice(&buf[..n])));
                if !matches!(tokio::time::timeout(Duration::from_secs(15), send).await, Ok(Ok(()))) {
                    break;
                }
            }
            let _ = wtx.send(Message::Close(None)).await;
        };
        let ws_to_tcp = async {
            while let Some(Ok(m)) = wrx.next().await {
                let data = match m {
                    Message::Binary(b) => b,
                    Message::Text(t) => Bytes::copy_from_slice(t.as_bytes()),
                    Message::Close(_) => break,
                    _ => continue,
                };
                if tw.write_all(&data).await.is_err() {
                    break;
                }
            }
        };
        tokio::select! {
            _ = tcp_to_ws => {}
            _ = ws_to_tcp => {}
            _ = s.stop.cancelled() => {}
        }
        if let Some(b) = release {
            b.release().await;
        }
        tracing::info!(user = %user, "{what} kapandı");
    }))
}

// --- Ekran Modu --------------------------------------------------------------

/// Ekran Modu açık mı, Pi'ye monitör bağlı mı.
pub async fn screen_state(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(match &s.d.screen {
        None => json(&screen::State { enabled: !s.d.vnc_addr.is_empty(), headless: true, managed: false }),
        Some(sc) => json(&sc.state()),
    })
}

pub async fn screen_set(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = admin_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        enabled: bool,
    }
    let req: Req = decode(&body)?;
    let sc = s.d.screen.clone().ok_or_else(|| ApiError::unavailable("Ekran Modu bu sistemde yönetilemiyor."))?;
    sc.set(req.enabled).await.map_err(|e| ApiError::internal(format!("Ekran Modu ayarı kaydedilemedi: {e}")))?;
    tracing::info!(enabled = req.enabled, user = %sess.user, "Ekran Modu");
    Ok(json(&sc.state()))
}
