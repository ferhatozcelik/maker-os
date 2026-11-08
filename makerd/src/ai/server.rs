//! OpenAI uyumlu yerel API (varsayılan 127.0.0.1:8091) — llama-server'ın önünde ince bir katman.
//!
//!   POST /v1/chat/completions   OpenAI Chat Completions (stream dahil). Sistem iletisi yoksa Türkçe Linux asistanı
//!                               istemi eklenir; geçmiş bağlama (ctx) sığacak şekilde eskiden kırpılır. Yanıta
//!                               `x_makeros.commands` eklenir: yanıttaki komutlar ve kurallı risk düzeyleri.
//!   GET  /v1/models             tek model
//!   GET  /health                hizmet ayakta mı
//!   GET  /v1/makeros/durum      model yüklü mü, RAM (RSS), yükleme süresi
//!   POST /v1/makeros/risk       {"command": "..."} ya da {"text": "...yanıt..."} → risk değerlendirmesi
//!   POST /v1/makeros/bosalt     modeli hemen bellekten at
//!   GET  /v1/makeros/olaylar    son MakerOS bildirimleri (makerd'in olay dosyası, kısa satırlar)
//!
//! Hiçbir uç komut çalıştırmaz.

use super::{llama::Llama, prompt, risk};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::sync::Arc;

pub const MODEL_ID: &str = "qwen2.5-0.5b-instruct-q4_k_m";
type HttpClient = Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>>;

pub struct App {
    pub llama: Arc<Llama>,
    http: HttpClient,
}

pub fn router(llama: Arc<Llama>) -> Router {
    let app = Arc::new(App { llama, http: Client::builder(TokioExecutor::new()).build_http() });
    Router::new()
        .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/makeros/durum", get(status))
        .route("/v1/makeros/risk", post(risk_ep))
        .route("/v1/makeros/bosalt", post(unload))
        .route("/v1/makeros/olaylar", get(|| async { Json(json!({"olaylar": read_events()})) }))
        .with_state(app)
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    let t = if code.is_server_error() { "server_error" } else { "invalid_request_error" };
    (code, Json(json!({"error": {"message": msg.into(), "type": t, "code": code.as_u16()}}))).into_response()
}

async fn models() -> Json<Value> {
    Json(json!({"object": "list", "data": [{"id": MODEL_ID, "object": "model", "created": 0, "owned_by": "makeros"}]}))
}

async fn status(State(a): State<Arc<App>>) -> Json<Value> {
    let c = &a.llama.cfg;
    Json(json!({"model": MODEL_ID, "model_path": c.model, "ctx": c.ctx, "threads": c.threads, "idle_unload_s": c.idle.as_secs(), "llama": a.llama.state().await}))
}

async fn unload(State(a): State<Arc<App>>) -> Json<Value> {
    a.llama.stop().await;
    Json(json!({"ok": true}))
}

async fn risk_ep(Json(v): Json<Value>) -> Response {
    if let Some(c) = v.get("command").and_then(Value::as_str) {
        return Json(json!(risk::classify(c))).into_response();
    }
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        return Json(json!({"commands": assess(t)})).into_response();
    }
    err(StatusCode::BAD_REQUEST, "\"command\" ya da \"text\" alanı gerekli")
}

/// makerd'in AI olay dosyası (bkz. notify.rs): her satır bir bildirim.
pub fn read_events() -> Vec<String> {
    let p = std::env::var("MAKEROS_AI_EVENTS").unwrap_or_else(|_| "/run/makeros/olaylar.log".into());
    std::fs::read_to_string(p).map(|s| s.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect()).unwrap_or_default()
}

pub fn assess(text: &str) -> Vec<risk::Assessment> {
    risk::extract_commands(text).iter().map(|c| risk::classify(c)).collect()
}

/// İçerik: düz metin ya da [{type:"text", text}] parçaları.
fn content_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => Some(parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

async fn post_json(a: &App, path: &str, body: &Value) -> Result<hyper::Response<hyper::body::Incoming>, String> {
    let req = hyper::Request::post(format!("{}{path}", a.llama.base()))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(serde_json::to_vec(body).unwrap_or_default())))
        .map_err(|e| e.to_string())?;
    a.http.request(req).await.map_err(|e| format!("llama-server'a bağlanılamadı: {e}"))
}

async fn tokens(a: &App, text: &str) -> usize {
    let fallback = text.chars().count() / 3 + 1; // Türkçe ≈ 3 karakter/token
    let Ok(r) = post_json(a, "/tokenize", &json!({"content": text})).await else { return fallback };
    let Ok(b) = r.into_body().collect().await else { return fallback };
    serde_json::from_slice::<Value>(&b.to_bytes()).ok().and_then(|v| v.get("tokens").and_then(Value::as_array).map(Vec::len)).unwrap_or(fallback)
}

/// Bağlam yönetimi: istem + yanıt payı ctx'e sığmalı. En eski (sistem dışı) iletiler atılır; son kullanıcı iletisi
/// tek başına sığmıyorsa ortası kısaltılır (uzun günlüklerin başı ve sonu korunur).
async fn fit(a: &App, msgs: &mut Vec<(String, String)>, budget: usize, protect: usize) -> (usize, usize) {
    const PER_MSG: usize = 6; // <|im_start|>role\n … <|im_end|>\n
    let mut counts = Vec::with_capacity(msgs.len());
    for (_, c) in msgs.iter() {
        counts.push(tokens(a, c).await + PER_MSG);
    }
    let mut dropped = 0;
    let total = |c: &Vec<usize>| c.iter().sum::<usize>() + 3;
    while total(&counts) > budget {
        // ilk sistem dışı ileti (son ileti hariç)
        // Önce gerçek geçmiş atılır; biçim örneği (sistemden sonraki ilk çift) en son.
        let Some(i) = (protect..msgs.len() - 1).find(|&i| msgs[i].0 != "system").or_else(|| (0..msgs.len() - 1).find(|&i| msgs[i].0 != "system")) else { break };
        msgs.remove(i);
        counts.remove(i);
        dropped += 1;
    }
    if total(&counts) > budget {
        let last = msgs.len() - 1;
        let over = total(&counts) - budget;
        let (_, c) = &mut msgs[last];
        let chars: Vec<char> = c.chars().collect();
        let ratio = chars.len() as f64 / counts[last].max(1) as f64;
        let cut = ((over as f64 + 24.0) * ratio * 1.15) as usize;
        if cut < chars.len() {
            let keep = chars.len() - cut;
            let head: String = chars[..keep * 2 / 3].iter().collect();
            let tail: String = chars[chars.len() - (keep - keep * 2 / 3)..].iter().collect();
            *c = format!("{head}\n…(uzun olduğu için kısaltıldı)…\n{tail}");
            counts[last] = tokens(a, c).await + PER_MSG;
        }
    }
    (dropped, total(&counts))
}

async fn chat(State(a): State<Arc<App>>, body: Bytes) -> Response {
    let Ok(mut req) = serde_json::from_slice::<Value>(&body) else { return err(StatusCode::BAD_REQUEST, "geçersiz JSON") };
    let Some(arr) = req.get("messages").and_then(Value::as_array) else { return err(StatusCode::BAD_REQUEST, "\"messages\" dizisi gerekli") };
    let mut msgs: Vec<(String, String)> = Vec::new();
    for m in arr {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("");
        if !matches!(role, "system" | "user" | "assistant" | "developer") {
            return err(StatusCode::BAD_REQUEST, format!("desteklenmeyen rol: {role:?}"));
        }
        let Some(c) = m.get("content").and_then(content_str) else { return err(StatusCode::BAD_REQUEST, "ileti içeriği metin olmalı") };
        msgs.push((if role == "developer" { "system".into() } else { role.into() }, c));
    }
    if !msgs.last().is_some_and(|(r, _)| r == "user") {
        return err(StatusCode::BAD_REQUEST, "son ileti kullanıcıdan (role: user) olmalı");
    }
    // Varsayılan Türkçe Linux asistanı; istemci kendi sistem iletisini verirse o kullanılır.
    let default_sys = !msgs.iter().any(|(r, _)| r == "system");
    let events_q = default_sys && msgs.last().is_some_and(|(_, q)| prompt::wants_events(q));
    if events_q {
        msgs.insert(0, ("system".into(), prompt::EVENTS_SYSTEM.into()));
    } else if default_sys {
        msgs.insert(0, ("system".into(), prompt::SYSTEM.into()));
        // Komut örneği en sonda: en sık istek odur ve küçük model en son örneğin biçimine yaslanır.
        msgs.insert(1, ("user".into(), prompt::EXAMPLE2_Q.into()));
        msgs.insert(2, ("assistant".into(), prompt::EXAMPLE2_A.into()));
        msgs.insert(3, ("user".into(), prompt::EXAMPLE_Q.into()));
        msgs.insert(4, ("assistant".into(), prompt::EXAMPLE_A.into()));
    }
    // Doğrulanmış komut kartları (kapatmak için: "makeros_bilgi": false).
    if req.get("makeros_bilgi").and_then(Value::as_bool) != Some(false) {
        let last = msgs.len() - 1;
        // Son olaylar (bildirimler) yalnız soru onlarla ilgiliyse: her istekte eklemek ilk yanıtı geciktirir.
        // Bu sorularda komut biçimi istenmez (küçük model biçime uyup komut uydurur).
        let k = if prompt::wants_events(&msgs[last].1) {
            prompt::events_context(&read_events(), 5)
        } else {
            prompt::knowledge(&msgs[last].1)
        };
        msgs[last].1.push_str(&k);
    }
    let ctx = a.llama.cfg.ctx as usize;
    let max_tokens = req.get("max_tokens").or_else(|| req.get("max_completion_tokens")).and_then(Value::as_u64).map(|v| v as usize).unwrap_or(512).clamp(16, ctx / 2);
    match a.llama.ensure().await {
        Ok(true) => prime(&a.llama).await,
        Ok(false) => {}
        Err(e) => return err(StatusCode::SERVICE_UNAVAILABLE, e),
    }
    let busy = a.llama.busy();
    let protect = if default_sys && !events_q { 5 } else { 0 };
    let (dropped, prompt_tokens) = fit(&a, &mut msgs, ctx - max_tokens - 8, protect).await;
    let stream = req.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let o = req.as_object_mut().unwrap();
    o.insert("messages".into(), Value::Array(msgs.iter().map(|(r, c)| json!({"role": r, "content": c})).collect()));
    o.insert("max_tokens".into(), json!(max_tokens));
    o.remove("max_completion_tokens");
    o.remove("makeros_bilgi");
    o.insert("model".into(), json!(MODEL_ID));
    o.entry("temperature").or_insert(json!(0.2));
    o.entry("top_p").or_insert(json!(0.9));
    o.entry("repeat_penalty").or_insert(json!(1.1));
    o.insert("cache_prompt".into(), json!(true));
    let meta = json!({"dropped_messages": dropped, "prompt_tokens_est": prompt_tokens, "ctx": ctx, "max_tokens": max_tokens});
    let res = match post_json(&a, "/v1/chat/completions", &req).await {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_GATEWAY, e),
    };
    let status = res.status();
    if !status.is_success() || !stream {
        let b = match res.into_body().collect().await {
            Ok(b) => b.to_bytes(),
            Err(e) => return err(StatusCode::BAD_GATEWAY, e.to_string()),
        };
        drop(busy);
        let Ok(mut v) = serde_json::from_slice::<Value>(&b) else { return (StatusCode::BAD_GATEWAY, b).into_response() };
        if status.is_success() {
            let text = v.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or("").to_string();
            v["x_makeros"] = json!({"commands": assess(&text), "context": meta});
        }
        return (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(v)).into_response();
    }
    // Akış: satırlar olduğu gibi aktarılır; içerik biriktirilir, [DONE]'dan önce x_makeros parçası eklenir.
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(64);
    let mut body = res.into_body();
    tokio::spawn(async move {
        let _busy = busy;
        let (mut buf, mut text) = (Vec::<u8>::new(), String::new());
        let mut sent_extra = false;
        let extra = |text: &str| Bytes::from(format!("data: {}\n\n", json!({"object": "chat.completion.chunk", "model": MODEL_ID, "choices": [], "x_makeros": {"commands": assess(text), "context": meta}})));
        while let Some(fr) = body.frame().await {
            let Ok(fr) = fr else { break };
            let Some(d) = fr.data_ref() else { continue };
            buf.extend_from_slice(d);
            while let Some(p) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=p).collect();
                let s = String::from_utf8_lossy(&line);
                if let Some(data) = s.trim_end().strip_prefix("data: ") {
                    if data == "[DONE]" {
                        if !sent_extra && tx.send(extra(&text)).await.is_err() { return }
                        sent_extra = true;
                    } else if let Ok(v) = serde_json::from_str::<Value>(data) {
                        if let Some(c) = v.pointer("/choices/0/delta/content").and_then(Value::as_str) { text.push_str(c) }
                    }
                }
                if tx.send(Bytes::from(line)).await.is_err() { return } // istemci ayrıldı
            }
        }
        if !sent_extra {
            let _ = tx.send(extra(&text)).await;
            let _ = tx.send(Bytes::from_static(b"data: [DONE]\n\n")).await;
        }
    });
    let s = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|b| (Ok::<_, Infallible>(b), rx)) });
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(s))
        .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "yanıt kurulamadı"))
}

/// Sabit istem önekinin (sistem + örnekler) KV önbellek dosyası: istem, model ya da bağlam değişince ad da değişir.
pub fn prefix_file(llama: &Llama) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for p in [prompt::SYSTEM, prompt::EXAMPLE_Q, prompt::EXAMPLE_A, prompt::EXAMPLE2_Q, prompt::EXAMPLE2_A, &llama.cfg.model, &llama.cfg.ctx.to_string()] {
        h.update(p.as_bytes());
        h.update([0]);
    }
    format!("onek-{}.bin", &hex::encode(h.finalize())[..16])
}

/// Model yeni yüklendi: önek önbelleği diskte varsa yükle (saniyeler), yoksa bir kez hesapla ve kaydet.
/// Pi 4'te 745 token'lık önek ~55 sn sürer; önbellekle soğuk başlangıç model yükleme süresine iner.
pub async fn prime(llama: &Arc<Llama>) {
    if llama.cfg.cache_dir.is_empty() {
        return;
    }
    let f = prefix_file(llama);
    let t0 = std::time::Instant::now();
    if std::path::Path::new(&llama.cfg.cache_dir).join(&f).is_file() {
        match llama.slot("restore", &f).await {
            Ok(()) => return tracing::info!(file = %f, ms = t0.elapsed().as_millis() as u64, "istem öneki önbellekten yüklendi"),
            Err(e) => tracing::warn!("önbellek yüklenemedi ({e}); yeniden hesaplanıyor"),
        }
    }
    warm(llama).await;
    match llama.slot("save", &f).await {
        Ok(()) => tracing::info!(file = %f, ms = t0.elapsed().as_millis() as u64, "istem öneki hesaplandı ve kaydedildi"),
        Err(e) => tracing::warn!("önbellek kaydedilemedi: {e}"),
    }
}

/// Sistem istemini önbelleğe al (model yüklenince bir kez): sonraki ilk yanıt gecikmesi kısalır.
pub async fn warm(llama: &Arc<Llama>) {
    let a = App { llama: llama.clone(), http: Client::builder(TokioExecutor::new()).build_http() };
    // Yalnız önek hesaplansın: son ileti kısa, tek token üretilir.
    let body = json!({"model": MODEL_ID, "messages": [{"role": "system", "content": prompt::SYSTEM}, {"role": "user", "content": prompt::EXAMPLE2_Q}, {"role": "assistant", "content": prompt::EXAMPLE2_A}, {"role": "user", "content": prompt::EXAMPLE_Q}, {"role": "assistant", "content": prompt::EXAMPLE_A}, {"role": "user", "content": "."}], "max_tokens": 1, "cache_prompt": true});
    if let Ok(r) = post_json(&a, "/v1/chat/completions", &body).await {
        let _ = r.into_body().collect().await;
    }
}
