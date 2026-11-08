//! Kabuk dizininin sunumu: dizin dışına (sembolik bağ dahil) çıkılamaz. Yalnız
//! *.woff2 bir yıl "immutable", diğerleri (vendor/ dahil) no-cache + ETag.
//! .br/.gz ön-sıkıştırılmış eşi varsa ve istemci kabul ediyorsa o sunulur.

use axum::body::Body;
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct Static {
    dir: PathBuf,
}

/// Dosya uzantısından içerik türü.
pub fn content_type(name: &str) -> &'static str {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "webmanifest" => "application/manifest+json",
        "png" => "image/png",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" | "log" | "md" | "py" | "toml" | "sh" | "ini" | "cfg" | "conf" | "csv" => "text/plain; charset=utf-8",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "xz" => "application/x-xz",
        "tar" => "application/x-tar",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "xml" => "text/xml; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Accept-Encoding başlığı kodlamayı q>0 ile kabul ediyor mu.
pub fn accepts(header: &str, enc: &str) -> bool {
    for part in header.split(',') {
        let mut it = part.trim().splitn(2, ';');
        let name = it.next().unwrap_or("").trim();
        if !name.eq_ignore_ascii_case(enc) {
            continue;
        }
        if let Some(q) = it.next().and_then(|p| p.trim().strip_prefix("q=")) {
            if q.parse::<f64>().is_ok_and(|v| v == 0.0) {
                return false;
            }
        }
        return true;
    }
    false
}

pub fn not_found() -> Response {
    (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], "Bulunamadı\n").into_response()
}

/// path.Clean("/"+rel) eşdeğeri, baştaki "/" olmadan.
fn clean_rel(rel: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for c in rel.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out.join("/")
}

impl Static {
    /// Dizin sonradan oluşabilir.
    pub fn new(dir: &str) -> Self {
        Self { dir: PathBuf::from(dir) }
    }

    /// Kökün içindeki düzenli dosya (sembolik bağ kök dışına çıkamaz).
    fn regular(&self, root: &Path, rel: &str) -> Option<(PathBuf, std::fs::Metadata)> {
        let p = std::fs::canonicalize(root.join(rel)).ok()?;
        if !p.starts_with(root) {
            return None;
        }
        let md = std::fs::metadata(&p).ok()?;
        Some((p, md))
    }

    /// Kabuk köküne göre `rel` yolundaki dosyayı sunar.
    pub async fn serve(&self, req: &Parts, rel: &str) -> Response {
        let Ok(root) = std::fs::canonicalize(&self.dir) else { return not_found() };
        let mut rel = clean_rel(rel);
        if rel.is_empty() {
            rel = "index.html".into();
        }
        if rel.split('/').any(|s| s.starts_with('.')) {
            return not_found();
        }
        let mut found = self.regular(&root, &rel);
        if found.as_ref().is_some_and(|(_, m)| m.is_dir()) {
            let path = req.uri.path();
            if !path.ends_with('/') {
                return (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, format!("{path}/"))]).into_response();
            }
            rel = format!("{rel}/index.html");
            found = self.regular(&root, &rel);
        }
        let Some((mut file, mut md)) = found.filter(|(_, m)| m.is_file()) else { return not_found() };
        let mut headers = vec![(header::CONTENT_TYPE, HeaderValue::from_static(content_type(&rel)))];
        let cc = if rel.ends_with(".woff2") {
            "public, max-age=31536000, immutable"
        } else if rel.ends_with(".html") || rel.ends_with(".js") || rel.ends_with(".json") || rel.ends_with(".webmanifest") {
            "no-cache, no-store, must-revalidate, max-age=0"
        } else {
            "no-cache, must-revalidate"
        };
        headers.push((header::CACHE_CONTROL, HeaderValue::from_static(cc)));
        let ae = req.headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).unwrap_or("");
        let mut suffix = "";
        for (ext, enc) in [(".br", "br"), (".gz", "gzip")] {
            if let Some((vp, vm)) = self.regular(&root, &format!("{rel}{ext}")).filter(|(_, m)| m.is_file()) {
                if !headers.iter().any(|(k, _)| k == header::VARY) {
                    headers.push((header::VARY, HeaderValue::from_static("Accept-Encoding")));
                }
                if suffix.is_empty() && accepts(ae, enc) {
                    file = vp;
                    md = vm;
                    suffix = if enc == "br" { "-br" } else { "-gzip" };
                    headers.push((header::CONTENT_ENCODING, HeaderValue::from_static(enc)));
                }
            }
        }
        let mtime = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).unwrap_or_default();
        let etag = format!("\"{:x}-{:x}{suffix}\"", mtime.as_nanos(), md.len());
        if let Ok(v) = HeaderValue::from_str(&etag) {
            headers.push((header::ETAG, v));
        }
        let lm = jiff::Timestamp::from_second(mtime.as_secs() as i64).map(|t| t.strftime("%a, %d %b %Y %H:%M:%S GMT").to_string()).unwrap_or_default();
        if let Ok(v) = HeaderValue::from_str(&lm) {
            headers.push((header::LAST_MODIFIED, v));
        }
        let inm = req.headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
        let fresh = match inm {
            Some(v) => v.split(',').any(|t| t.trim() == etag || t.trim() == "*"),
            None => req.headers.get(header::IF_MODIFIED_SINCE).and_then(|v| v.to_str().ok()) == Some(lm.as_str()),
        };
        let mut b = Response::builder();
        for (k, v) in headers {
            b = b.header(k, v);
        }
        if fresh {
            return b.status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap_or_else(|_| not_found());
        }
        b = b.header(header::CONTENT_LENGTH, md.len());
        if req.method == Method::HEAD {
            return b.body(Body::empty()).unwrap_or_else(|_| not_found());
        }
        match tokio::fs::File::open(&file).await {
            Ok(f) => b.body(Body::from_stream(tokio_util::io::ReaderStream::new(f))).unwrap_or_else(|_| not_found()),
            Err(_) => not_found(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings() {
        assert!(accepts("gzip, deflate, br", "br"));
        assert!(accepts("GZIP;q=0.5", "gzip"));
        assert!(!accepts("br;q=0, gzip", "br"));
        assert!(!accepts("deflate", "gzip"));
        assert_eq!(clean_rel("a/../../etc/passwd"), "etc/passwd");
        assert_eq!(content_type("x.JS"), "text/javascript; charset=utf-8");
    }
}
