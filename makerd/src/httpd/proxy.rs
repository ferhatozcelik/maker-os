//! /plugin/<id>/... isteklerini eklentinin 127.0.0.1:<web_port> sunucusuna
//! iletir (WebSocket yükseltmeleri dahil). Kabuk oturum çerezi eklentiye gitmez.

use super::*;
use crate::plugins;
use axum::extract::State;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::sync::OnceLock;

type HttpClient = Client<hyper_util::client::legacy::connect::HttpConnector, Body>;

fn client() -> &'static HttpClient {
    static C: OnceLock<HttpClient> = OnceLock::new();
    C.get_or_init(|| Client::builder(TokioExecutor::new()).build_http())
}

/// Bağlantıya özel başlıklar (vekilde aktarılmaz).
const HOP: &[&str] = &["connection", "keep-alive", "proxy-connection", "transfer-encoding", "te", "trailer", "proxy-authorization", "proxy-authenticate"];

fn unavailable() -> ApiError {
    ApiError::unavailable("Eklenti çalışmıyor veya henüz hazır değil (Eklentiler → Başlat).")
}

pub async fn plugin_proxy(State(s): State<S>, mut req: Request) -> R {
    let sess = session_of(&s, req.headers())?;
    let a = s.d.agent.clone().ok_or_else(|| ApiError::unavailable("Kullanıcı hizmeti yok."))?;
    let path = req.uri().path().to_string();
    let id = path.trim_start_matches("/plugin/").split('/').next().unwrap_or("").to_string();
    let p = plugins::get(a.as_ref(), &sess.user, &id).await?;
    let port = p.manifest.web_port;
    if port == 0 {
        return Err(ApiError::not_found("Bu eklentinin web arayüzü yok."));
    }
    let prefix = format!("/plugin/{id}");
    let rest = path.strip_prefix(&prefix).unwrap_or("").trim_start_matches('/');
    let pq = match req.uri().query() {
        Some(q) => format!("/{rest}?{q}"),
        None => format!("/{rest}"),
    };
    let host = format!("127.0.0.1:{port}");
    *req.uri_mut() = format!("http://{host}{pq}").parse().map_err(|_| ApiError::invalid("Geçersiz adres."))?;
    let upgrade = req.headers().get(header::UPGRADE).cloned();
    let h = req.headers_mut();
    h.remove(header::COOKIE);
    if upgrade.is_none() {
        for k in HOP {
            h.remove(*k);
        }
    }
    h.insert(header::HOST, HeaderValue::from_str(&host).map_err(|_| unavailable())?);
    h.insert("x-forwarded-prefix", HeaderValue::from_str(&prefix).map_err(|_| unavailable())?);

    if upgrade.is_some() {
        return upgrade_proxy(req).await;
    }
    let res = client().request(req).await.map_err(|_| unavailable())?;
    let (mut parts, body) = res.into_parts();
    for k in HOP {
        parts.headers.remove(*k);
    }
    Ok(Response::from_parts(parts, Body::new(body)))
}

/// /api/v1/ai/<yol> → yerel Türkçe Linux asistanı (makeros-ai, 127.0.0.1:8091). Yalnız oturum sahibi; yalnız
/// bilinen uçlar. makeros-ai komut çalıştırmaz: yanıtlar yalnız öneri ve risk değerlendirmesidir.
pub async fn ai_proxy(State(s): State<S>, mut req: Request) -> R {
    let _sess = session_of(&s, req.headers())?;
    let rest = req.uri().path().trim_start_matches("/api/v1/ai/").to_string();
    if !matches!(rest.as_str(), "v1/chat/completions" | "v1/models" | "v1/makeros/durum" | "v1/makeros/risk" | "v1/makeros/olaylar" | "health") {
        return Err(ApiError::not_found("Bilinmeyen asistan ucu."));
    }
    let host = "127.0.0.1:8091";
    *req.uri_mut() = format!("http://{host}/{rest}").parse().map_err(|_| ApiError::invalid("Geçersiz adres."))?;
    let h = req.headers_mut();
    h.remove(header::COOKIE);
    for k in HOP {
        h.remove(*k);
    }
    h.insert(header::HOST, HeaderValue::from_static("127.0.0.1:8091"));
    let res = client().request(req).await.map_err(|_| ApiError::unavailable("Yerel AI asistanı çalışmıyor (sudo systemctl start makeros-ai)."))?;
    let (mut parts, body) = res.into_parts();
    for k in HOP {
        parts.headers.remove(*k);
    }
    // Akış (SSE) ara belleğe alınmasın.
    parts.headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(Response::from_parts(parts, Body::new(body)))
}

/// Yükseltme isteği (WebSocket): iki ucu ham bağlantı olarak birbirine bağlar.
async fn upgrade_proxy(mut req: Request) -> R {
    let client_up = hyper::upgrade::on(&mut req);
    let mut res = client().request(req).await.map_err(|_| unavailable())?;
    if res.status() != StatusCode::SWITCHING_PROTOCOLS {
        let (parts, body) = res.into_parts();
        return Ok(Response::from_parts(parts, Body::new(body)));
    }
    let server_up = hyper::upgrade::on(&mut res);
    tokio::spawn(async move {
        if let (Ok(c), Ok(s)) = (client_up.await, server_up.await) {
            let (mut c, mut s) = (TokioIo::new(c), TokioIo::new(s));
            let _ = tokio::io::copy_bidirectional(&mut c, &mut s).await;
        }
    });
    let (parts, _) = res.into_parts();
    Ok(Response::from_parts(parts, Body::empty()))
}
