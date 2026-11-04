//! Kullanıcı adına yapılan işler: dosyalar, Python ve projeler, AI ajanları,
//! kamera, GPIO ve eklentiler. Dosya işlemleri kullanıcının kendi hesabıyla
//! (makerd-user) yapılır; makerd kullanıcının ev dizinini doğrudan okumaz.
//! Uzun işler terminal oturumunda çalışır: yanıt {"term":{"id","ticket"}} döner
//! ve kabuk Terminal penceresini o oturuma bağlar.

use super::*;
use crate::termproc::Open;
use crate::{agents, plugins, pyman};
use axum::extract::{Path, Query, State};
use bytes::Bytes;
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub const MAX_UPLOAD: usize = 512 << 20;

fn agent(s: &Server) -> ApiResult<Arc<dyn uagent::Agent>> {
    s.d.agent.clone().ok_or_else(|| ApiError::unavailable("Kullanıcı hizmeti yok."))
}

fn files_agent(s: &Server) -> ApiResult<Arc<dyn uagent::Agent>> {
    s.d.agent.clone().ok_or_else(|| ApiError::unavailable("Dosya hizmeti bu sistemde yok."))
}

#[derive(Serialize)]
pub struct TermResp {
    pub id: String,
    pub ticket: String,
}

/// Komutu yeni terminal oturumunda başlatır ve bağlanma biletini döner.
pub async fn term_run(s: &Server, user: &str, cmd: String, title: String) -> R {
    let sess = s.d.term.create_with(user, 100, 30, term::Opts { cmd, title }).await?;
    let t = s.d.term.ticket(&sess.id, user)?;
    Ok(json(&HashMap::from([("term", TermResp { id: sess.id.clone(), ticket: t })])))
}

// --- dosyalar ----------------------------------------------------------------

type Q = Query<HashMap<String, String>>;

fn q<'a>(q: &'a Q, k: &str) -> &'a str {
    q.0.get(k).map(String::as_str).unwrap_or("")
}

pub async fn files_list(State(s): State<S>, h: HeaderMap, qs: Q) -> R {
    let sess = session_of(&s, &h)?;
    let a = files_agent(&s)?;
    Ok(raw_json(a.fs(&sess.user, Open { mode: "list".into(), path: q(&qs, "path").into(), ..Default::default() }).await?))
}

/// RFC 6266 filename* (UTF-8) ve ASCII yedekli Content-Disposition.
fn attachment(name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    let enc: String = name.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{enc}")
}

pub async fn files_raw(State(s): State<S>, h: HeaderMap, qs: Q) -> R {
    let sess = session_of(&s, &h)?;
    let a = files_agent(&s)?;
    let p = q(&qs, "path").to_string();
    let (tx, mut rx) = mpsc::channel::<Bytes>(8);
    let user = sess.user.clone();
    let path = p.clone();
    let task = tokio::spawn(async move { a.read(&user, &path, tx).await });
    // İlk parça (veya hata) gelmeden yanıt başlamaz: hata JSON olarak dönebilsin.
    let first = rx.recv().await;
    if first.is_none() {
        task.await.map_err(|_| ApiError::internal("İstek tamamlanamadı."))??;
    }
    let base = p.rsplit('/').next().unwrap_or(&p).to_string();
    let mut b = Response::builder()
        .header(header::CONTENT_TYPE, static_files::content_type(&p))
        // Etkin içerik (HTML/SVG) kabuk kökenine gömülmesin.
        .header(header::CONTENT_SECURITY_POLICY, "sandbox");
    if q(&qs, "download") == "1" {
        b = b.header(header::CONTENT_DISPOSITION, attachment(&base));
    }
    let head = futures_util::stream::iter(first.map(Ok::<_, std::io::Error>));
    let rest = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|b| (Ok::<_, std::io::Error>(b), rx)) });
    b.body(Body::from_stream(head.chain(rest))).map_err(|e| ApiError::internal(e.to_string()))
}

pub async fn files_upload(State(s): State<S>, h: HeaderMap, qs: Q, body: Body) -> R {
    let sess = session_of(&s, &h)?;
    let a = files_agent(&s)?;
    let p = q(&qs, "path").to_string();
    if p.is_empty() {
        return Err(ApiError::invalid("Hedef yol gerekli."));
    }
    let too_big = || ApiError::invalid("Dosya en fazla 512 MB olabilir.");
    if h.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<usize>().ok()).is_some_and(|n| n > MAX_UPLOAD) {
        return Err(too_big());
    }
    let (tx, rx) = mpsc::channel::<std::io::Result<Bytes>>(8);
    let feed = async move {
        let mut st = body.into_data_stream();
        let mut total = 0usize;
        while let Some(c) = st.next().await {
            let c = match c {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                    return Ok(());
                }
            };
            total += c.len();
            if total > MAX_UPLOAD {
                let _ = tx.send(Err(std::io::Error::other("çok büyük"))).await;
                return Err(too_big());
            }
            if tx.send(Ok(c)).await.is_err() {
                return Ok(());
            }
        }
        Ok(())
    };
    let (w, f) = tokio::join!(a.write(&sess.user, &p, rx), feed);
    f?;
    w?;
    Ok(no_content())
}

pub async fn files_op(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    let a = files_agent(&s)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        op: String,
        #[serde(default)]
        path: String,
        #[serde(default)]
        to: String,
    }
    let req: Req = decode(&body)?;
    if !matches!(req.op.as_str(), "mkdir" | "rename" | "remove" | "stat") {
        return Err(ApiError::invalid("Geçersiz dosya işlemi."));
    }
    Ok(raw_json(a.fs(&sess.user, Open { mode: req.op, path: req.path, to: req.to, ..Default::default() }).await?))
}

/// Python betiğini çalıştıran komut: klasöründe pyproject.toml varsa "uv run"
/// (projenin sanal ortamı), yoksa python3.
pub fn run_file_cmd(p: &str) -> ApiResult<String> {
    if !p.to_lowercase().ends_with(".py") || p.contains(['\0', '\n', '\r']) {
        return Err(ApiError::invalid("Yalnız Python (.py) dosyaları çalıştırılabilir."));
    }
    let (home, rel) = match p.strip_prefix("~/") {
        Some(r) => (true, r),
        None => (!p.starts_with('/'), p),
    };
    let cleaned = clean(rel);
    let (dir, file) = match cleaned.rfind('/') {
        Some(i) => (&cleaned[..=i], &cleaned[i + 1..]),
        None => ("", cleaned.as_str()),
    };
    let dir = if dir.is_empty() { "." } else { dir };
    let mut cd = format!("cd {}", crate::util::shq(dir));
    if home {
        cd = format!("cd ~ && {cd}");
    }
    let f = crate::util::shq(file);
    Ok(format!("{cd} && if [ -f pyproject.toml ]; then uv run {f}; else python3 {f}; fi"))
}

/// path.Clean
fn clean(p: &str) -> String {
    let rooted = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    let s = out.join("/");
    if rooted {
        format!("/{s}")
    } else if s.is_empty() {
        ".".into()
    } else {
        s
    }
}

/// Bir Python betiğini terminal oturumunda çalıştırır (Dosyalar'da .py'ye tıklama).
pub async fn files_run(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    agent(&s)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        path: String,
    }
    let req: Req = decode(&body)?;
    let cmd = run_file_cmd(&req.path)?;
    let title = req.path.rsplit('/').next().unwrap_or(&req.path).to_string();
    term_run(&s, &sess.user, cmd, title).await
}

// --- Python, projeler, ajanlar ---------------------------------------------------

pub async fn python_list(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    Ok(json(&pyman::versions(agent(&s)?.as_ref(), &sess.user).await?))
}

pub async fn python_op(State(s): State<S>, h: HeaderMap, Path(op): Path<String>, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    agent(&s)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        version: String,
    }
    let req: Req = decode(&body)?;
    let cmd = pyman::python_cmd(&op, &req.version)?;
    term_run(&s, &sess.user, cmd, format!("Python {}", req.version)).await
}

pub async fn project_templates(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&pyman::templates()))
}

pub async fn project_list(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    Ok(json(&pyman::projects(agent(&s)?.as_ref(), &sess.user).await?))
}

pub async fn project_create(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    let a = agent(&s)?;
    // Go sürümüyle uyum: alan adları büyük/küçük harf duyarsız.
    let v: serde_json::Value = decode::<Option<serde_json::Value>>(&body)?.unwrap_or_default();
    let field = |k: &str| -> String {
        v.as_object().and_then(|o| o.iter().find(|(n, _)| n.eq_ignore_ascii_case(k))).and_then(|(_, x)| x.as_str()).unwrap_or("").to_string()
    };
    let (name, tmpl, py) = (field("name"), field("template"), field("python"));
    let cmd = pyman::create(a.as_ref(), &sess.user, &name, &tmpl, &py).await?;
    term_run(&s, &sess.user, cmd, format!("{name} · kurulum")).await
}

pub async fn project_run(State(s): State<S>, h: HeaderMap, Path(name): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    let cmd = pyman::run_command(agent(&s)?.as_ref(), &sess.user, &name).await?;
    term_run(&s, &sess.user, cmd, name).await
}

pub async fn project_delete(State(s): State<S>, h: HeaderMap, Path(name): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    pyman::delete(agent(&s)?.as_ref(), &sess.user, &name).await?;
    Ok(no_content())
}

pub async fn agent_list(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    Ok(json(&agents::list(agent(&s)?.as_ref(), &sess.user).await?))
}

pub async fn agent_op(State(s): State<S>, h: HeaderMap, Path((id, op)): Path<(String, String)>) -> R {
    let sess = session_of(&s, &h)?;
    agent(&s)?;
    let (cmd, title) = agents::cmd(&id, &op)?;
    term_run(&s, &sess.user, cmd, title).await
}

// --- kamera ----------------------------------------------------------------

fn cam(s: &Server) -> ApiResult<Arc<camera::Hub>> {
    s.d.camera.clone().ok_or_else(|| ApiError::unavailable("Kamera hizmeti yok."))
}

pub async fn camera_status(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&cam(&s)?.status().await))
}

/// Çok parçalı MJPEG (img etiketinde doğrudan gösterilir).
pub async fn camera_stream(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    let sub = cam(&s)?.subscribe();
    let stop = s.stop.clone();
    let st = futures_util::stream::unfold((sub, stop), |(mut sub, stop)| async move {
        let f = tokio::select! {
            _ = stop.cancelled() => None,
            f = sub.frames.recv() => f,
        }?;
        let mut part = format!("--mkframe\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", f.len()).into_bytes();
        part.extend_from_slice(&f);
        part.extend_from_slice(b"\r\n");
        Some((Ok::<_, std::io::Error>(Bytes::from(part)), (sub, stop)))
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "multipart/x-mixed-replace; boundary=mkframe")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(st))
        .map_err(|e| ApiError::internal(e.to_string()))
}

/// Fotoğraf çeker ve ~/Resimler/Kamera altına kaydeder.
pub async fn camera_photo(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    let img = cam(&s)?.photo().await?;
    let name = format!("Resimler/Kamera/{}.jpg", jiff::Zoned::now().strftime("%Y-%m-%d_%H-%M-%S"));
    if let Some(a) = &s.d.agent {
        uagent::write_all(a.as_ref(), &sess.user, &name, img.clone()).await?;
    }
    Ok(json(&serde_json::json!({"path": format!("~/{name}"), "size": img.len()})))
}

// --- GPIO --------------------------------------------------------------------

fn gpio_p(s: &Server) -> ApiResult<Arc<dyn gpio::Provider>> {
    s.d.gpio.clone().ok_or_else(|| ApiError::unavailable("GPIO hizmeti yok."))
}

pub async fn gpio_board(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    let g = gpio_p(&s)?;
    Ok(json(&tokio::task::spawn_blocking(move || g.board()).await.map_err(|e| ApiError::internal(e.to_string()))??))
}

pub async fn gpio_set(State(s): State<S>, h: HeaderMap, Path(n): Path<String>, body: Bytes) -> R {
    session_of(&s, &h)?;
    let g = gpio_p(&s)?;
    let n: i32 = n.parse().map_err(|_| ApiError::invalid("Geçersiz GPIO numarası."))?;
    #[derive(Deserialize, Default)]
    struct Req {
        value: Option<i32>,
        #[serde(default)]
        release: bool,
    }
    let req: Req = decode(&body)?;
    let board = tokio::task::spawn_blocking(move || {
        if req.release {
            g.release(n)?;
        } else if let Some(v) = req.value {
            g.set(n, v)?;
        } else {
            return Err(ApiError::invalid("value veya release gerekli."));
        }
        g.board()
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    Ok(json(&board))
}

// --- eklentiler --------------------------------------------------------------

pub async fn plugin_list(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    Ok(json(&plugins::list(agent(&s)?.as_ref(), &sess.user).await?))
}

fn zip_resp(b: Vec<u8>, name: &str) -> Response {
    ([(header::CONTENT_TYPE, "application/zip".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))], b).into_response()
}

pub async fn plugin_template(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(zip_resp(plugins::template_zip()?, "makeros-eklenti-sablonu.zip"))
}

pub async fn plugin_import(State(s): State<S>, h: HeaderMap, body: Body) -> R {
    let sess = session_of(&s, &h)?;
    let a = agent(&s)?;
    let data = axum::body::to_bytes(body, plugins::MAX_ZIP + 1).await.map_err(|_| ApiError::invalid("Zip en fazla 15 MB olabilir."))?;
    Ok(json(&plugins::import(a.as_ref(), &sess.user, &data).await?))
}

pub async fn plugin_export(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    let b = plugins::export(agent(&s)?.as_ref(), &sess.user, &id).await?;
    Ok(zip_resp(b, &format!("{id}.zip")))
}

pub async fn plugin_op(State(s): State<S>, h: HeaderMap, Path((id, op)): Path<(String, String)>) -> R {
    let sess = session_of(&s, &h)?;
    let a = agent(&s)?;
    let work = async {
        match op.as_str() {
            "start" => Ok(json(&plugins::start(a.as_ref(), &sess.user, &id).await?)),
            "stop" => Ok(json(&plugins::stop(a.as_ref(), &sess.user, &id).await?)),
            "logs" => Ok(json(&HashMap::from([("log", plugins::logs(a.as_ref(), &sess.user, &id).await?)]))),
            _ => Err(ApiError::invalid("Geçersiz işlem.")),
        }
    };
    tokio::time::timeout(Duration::from_secs(30), work).await.unwrap_or_else(|_| Err(ApiError::unavailable("İşlem zaman aşımına uğradı.")))
}

pub async fn plugin_delete(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    plugins::remove(agent(&s)?.as_ref(), &sess.user, &id).await?;
    Ok(no_content())
}

/// Eklenti içindeki statik görsel (tema duvar kâğıdı, logo, işaret).
pub async fn plugin_asset(State(s): State<S>, h: HeaderMap, Path((id, rel)): Path<(String, String)>) -> R {
    let sess = session_of(&s, &h)?;
    let a = agent(&s)?;
    let ext = rel.rsplit('.').next().unwrap_or("").to_lowercase();
    let ct = match ext.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return Err(ApiError::not_found("Yalnız görsel dosyalar sunulur.")),
    };
    let path = plugins::asset_path(&id, &rel)?;
    let b = uagent::read_all(a.as_ref(), &sess.user, &path).await?;
    Ok(([(header::CONTENT_TYPE, ct), (header::CONTENT_SECURITY_POLICY, "sandbox"), (header::CACHE_CONTROL, "private, max-age=300")], b).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_file() {
        assert_eq!(run_file_cmd("~/Projeler/a/main.py").unwrap(), "cd ~ && cd 'Projeler/a/' && if [ -f pyproject.toml ]; then uv run 'main.py'; else python3 'main.py'; fi");
        assert_eq!(run_file_cmd("/tmp/x y.py").unwrap(), "cd '/tmp/' && if [ -f pyproject.toml ]; then uv run 'x y.py'; else python3 'x y.py'; fi");
        assert_eq!(run_file_cmd("b.py").unwrap(), "cd ~ && cd '.' && if [ -f pyproject.toml ]; then uv run 'b.py'; else python3 'b.py'; fi");
        assert!(run_file_cmd("a.sh").is_err() && run_file_cmd("a\n.py").is_err());
        assert_eq!(attachment("ö a.txt"), "attachment; filename=\"_ a.txt\"; filename*=UTF-8''%C3%B6%20a.txt");
    }
}
