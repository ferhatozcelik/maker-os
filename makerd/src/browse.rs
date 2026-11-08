//! Tarayıcı ve web uygulamaları için Raspberry Pi üzerinden tam internet aktarımı
//! (`GET | POST /api/v1/browse?t=<bilet>&url=<adres>`).
//!
//! MakerOS içindeki Tarayıcı ve dış web adresli uygulamalar host bilgisayarın internetini KULLANMAZ;
//! tüm HTML, CSS, JS, görsel, font, fetch/XHR ve form (GET/POST) istekleri Raspberry Pi üzerindeki
//! `makerd` üzerinden indirilir ve sunulur. Yanıt sıkı `Content-Security-Policy` (`default-src 'self'`)
//! ile döner; böylece istemci tarayıcının doğrudan dış internete bağlanması motor düzeyinde de engellenir.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const TICKET_TTL: Duration = Duration::from_secs(12 * 3600);
const MAX_BODY: u64 = 16 << 20;
const MAX_REDIRECTS: usize = 6;
const UA: &str = "Mozilla/5.0 (X11; Linux aarch64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

fn tickets() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static T: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

/// Oturum sahibi için bilet (aynı kullanıcıya aynı bilet, süresi dolana dek).
pub fn ticket(user: &str) -> String {
    let mut m = tickets().lock().unwrap();
    let now = Instant::now();
    m.retain(|_, (_, exp)| *exp > now);
    if let Some((k, _)) = m.iter().find(|(_, (u, _))| u == user) {
        return k.clone();
    }
    let k = crate::util::random_hex(16);
    m.insert(k.clone(), (user.to_string(), now + TICKET_TTL));
    k
}

/// Bilet geçerliyse kullanıcı adı.
pub fn check(t: &str) -> Option<String> {
    let m = tickets().lock().unwrap();
    m.get(t).filter(|(_, exp)| *exp > Instant::now()).map(|(u, _)| u.clone())
}

/// Dışarıdan erişilmemesi gereken adresler (Pi'nin kendisi, yerel ağ, bağlantı-yerel, CGNAT, çok-noktaya…).
pub fn forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            a.is_loopback() || a.is_private() || a.is_link_local() || a.is_unspecified() || a.is_broadcast() || a.is_multicast() || a.is_documentation()
                || o[0] == 0 || (o[0] == 100 && (o[1] & 0xc0) == 64) || (o[0] == 192 && o[1] == 0 && o[2] == 0) || o[0] >= 240
        }
        IpAddr::V6(a) => {
            if let Some(v4) = a.to_ipv4_mapped() {
                return forbidden_ip(IpAddr::V4(v4));
            }
            let s = a.segments();
            a.is_loopback() || a.is_unspecified() || a.is_multicast() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

pub struct Page {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    /// Yönlendirmelerden sonraki son adres.
    pub url: String,
}

/// Adresi doğrular, yerel ağa gidiyorsa reddeder.
async fn vet(u: &url::Url) -> Result<(), String> {
    if !matches!(u.scheme(), "http" | "https") {
        return Err("yalnız http/https adresleri açılabilir".into());
    }
    let host = u.host_str().ok_or("adreste sunucu yok")?.trim_start_matches('[').trim_end_matches(']').to_string();
    let port = u.port_or_known_default().unwrap_or(80);
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if forbidden_ip(ip) { Err("yerel ağ adresleri bu pencerede açılmaz".into()) } else { Ok(()) };
    }
    let addrs: Vec<_> = tokio::net::lookup_host((host.as_str(), port)).await.map_err(|_| not_found(&host))?.collect();
    if addrs.is_empty() {
        return Err(format!("{host} bulunamadı"));
    }
    if addrs.iter().any(|a| forbidden_ip(a.ip())) {
        return Err("yerel ağ adresleri bu pencerede açılmaz".into());
    }
    Ok(())
}

/// Ad çözülemedi: çoğu zaman Raspberry Pi'nin internet bağlantısı yoktur (yalnız USB ya da kurulum ağı).
fn not_found(host: &str) -> String {
    if has_default_route() { format!("{host} bulunamadı") } else { "Raspberry Pi internete bağlı değil. Ayarlar › Wi-Fi'den Pi'yi bir ağa bağlayın.".into() }
}

/// Varsayılan rota var mı (/proc/net/route, /proc/net/ipv6_route).
fn has_default_route() -> bool {
    let v4 = std::fs::read_to_string("/proc/net/route").unwrap_or_default().lines().skip(1).any(|l| l.split_whitespace().nth(1) == Some("00000000"));
    let v6 = std::fs::read_to_string("/proc/net/ipv6_route").unwrap_or_default().lines().any(|l| {
        let f: Vec<_> = l.split_whitespace().collect();
        f.len() > 9 && f[0] == "00000000000000000000000000000000" && f[1] == "00" && f[9] != "lo"
    });
    v4 || v6
}

/// Sunucu çerez deposu (alan adı → ad=değer çiftleri): yönlendirme ve oturum çerezleri korunsun.
fn cookies() -> &'static Mutex<HashMap<String, HashMap<String, String>>> {
    static C: OnceLock<Mutex<HashMap<String, HashMap<String, String>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn save_cookies(host: &str, set_cookies: &[String]) {
    if set_cookies.is_empty() {
        return;
    }
    let key = host.trim_start_matches("www.").to_lowercase();
    let mut jar = cookies().lock().unwrap();
    let entry = jar.entry(key).or_default();
    for raw in set_cookies {
        if let Some(pair) = raw.split(';').next() {
            if let Some((k, v)) = pair.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if !k.is_empty() {
                    entry.insert(k.to_string(), v.to_string());
                }
            }
        }
    }
}

fn cookie_header(host: &str) -> Option<String> {
    let key = host.trim_start_matches("www.").to_lowercase();
    let jar = cookies().lock().unwrap();
    let map = jar.get(&key)?;
    if map.is_empty() {
        return None;
    }
    Some(map.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; "))
}

/// Sayfayı veya alt kaynağı Raspberry Pi üzerinden indirir (GET).
pub async fn fetch(start: &str, lang: &str) -> Result<Page, String> {
    fetch_req("GET", start, lang, None, None).await
}

/// Sayfayı veya alt kaynağı Raspberry Pi üzerinden indirir (GET / POST destekli; yönlendirmeler elle izlenir).
pub async fn fetch_req(method: &str, start: &str, lang: &str, req_ct: Option<&str>, req_body: Option<&[u8]>) -> Result<Page, String> {
    let mut u = url::Url::parse(start).map_err(|_| "geçersiz adres".to_string())?;
    let lang = if lang.is_empty() { "tr-TR,tr;q=0.9,en-US;q=0.8,en;q=0.7".to_string() } else { lang.chars().take(200).collect() };
    let mut cur_method = method.to_uppercase();
    let mut cur_body: Option<Vec<u8>> = req_body.map(|b| b.to_vec());
    let mut cur_ct: Option<String> = req_ct.map(String::from);

    for _ in 0..=MAX_REDIRECTS {
        vet(&u).await?;
        let host = u.host_str().unwrap_or("").to_string();
        let ck = cookie_header(&host);
        let (cur, lang2, m2, ct2, b2) = (u.to_string(), lang.clone(), cur_method.clone(), cur_ct.clone(), cur_body.clone());
        let r = tokio::task::spawn_blocking(move || -> Result<(u16, Option<String>, String, Vec<String>, Vec<u8>), String> {
            let ag: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(20)))
                .max_redirects(0)
                .http_status_as_error(false)
                .build()
                .into();
            let mut res = if m2 == "POST" {
                let mut req = ag
                    .post(&cur)
                    .header("User-Agent", UA)
                    .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
                    .header("Accept-Language", &lang2);
                if let Some(ref c) = ck {
                    req = req.header("Cookie", c);
                }
                if let Some(ref ctype) = ct2 {
                    req = req.header("Content-Type", ctype);
                }
                req.send(b2.as_deref().unwrap_or(&[])).map_err(|e| format!("bağlanılamadı: {e}"))?
            } else {
                let mut req = ag
                    .get(&cur)
                    .header("User-Agent", UA)
                    .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8")
                    .header("Accept-Language", &lang2)
                    .header("Upgrade-Insecure-Requests", "1")
                    .header("Sec-Fetch-Dest", "document")
                    .header("Sec-Fetch-Mode", "navigate")
                    .header("Sec-Fetch-Site", "none")
                    .header("Sec-Fetch-User", "?1");
                if let Some(ref c) = ck {
                    req = req.header("Cookie", c);
                }
                req.call().map_err(|e| format!("bağlanılamadı: {e}"))?
            };
            let status = res.status().as_u16();
            let loc = res.headers().get("location").and_then(|v| v.to_str().ok()).map(String::from);
            let ct = res.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
            let set_ck: Vec<String> = res.headers().get_all("set-cookie").iter().filter_map(|v| v.to_str().ok().map(String::from)).collect();
            let body = if (300..400).contains(&status) { Vec::new() } else { res.body_mut().with_config().limit(MAX_BODY).read_to_vec().map_err(|e| format!("sayfa okunamadı: {e}"))? };
            Ok((status, loc, ct, set_ck, body))
        }).await.map_err(|e| e.to_string())??;
        let (status, loc, ct, set_ck, body) = r;
        save_cookies(&host, &set_ck);
        if (300..400).contains(&status) {
            if let Some(l) = loc {
                u = u.join(&l).map_err(|_| "geçersiz yönlendirme".to_string())?;
                if matches!(status, 301 | 302 | 303) {
                    cur_method = "GET".into();
                    cur_body = None;
                    cur_ct = None;
                }
                continue;
            }
        }
        return Ok(Page { status, content_type: ct, body, url: u.to_string() });
    }
    Err("çok fazla yönlendirme".into())
}

const WALLED_SEED: [&str; 0] = [];

fn walled() -> &'static Mutex<std::collections::BTreeSet<String>> {
    static W: OnceLock<Mutex<std::collections::BTreeSet<String>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(WALLED_SEED.iter().map(|s| s.to_string()).collect()))
}

pub fn mark_walled(url: &str) {
    if let Some(h) = url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string())) {
        walled().lock().unwrap().insert(h);
    }
}

pub fn walled_hosts() -> Vec<String> {
    walled().lock().unwrap().iter().cloned().collect()
}

pub fn is_bot_wall(p: &Page) -> bool {
    matches!(p.status, 403 | 429 | 503) && (find(&p.body, b"challenge-platform").is_some() || find(&p.body, b"<title>Just a moment").is_some() || find(&p.body, b"cf-chl").is_some())
}

/// JS dizgisi olarak güvenli (</script> kapanmasın).
fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()).replace('<', "\\u003c").replace('>', "\\u003e")
}

/// CSS içindeki `url(...)` adreslerini Raspberry Pi `/api/v1/browse` vekiline çevirir.
pub fn rewrite_css(css: &[u8], base_url: &str, ticket: &str) -> Vec<u8> {
    let Ok(base) = url::Url::parse(base_url) else { return css.to_vec() };
    let text = String::from_utf8_lossy(css);
    let mut out = String::with_capacity(text.len() + 256);
    let mut rest = text.as_ref();
    while let Some(pos) = rest.to_ascii_lowercase().find("url(") {
        out.push_str(&rest[..pos + 4]);
        rest = &rest[pos + 4..];
        let Some(end) = rest.find(')') else { break };
        let raw = rest[..end].trim().trim_matches(|c| c == '"' || c == '\'').trim();
        if raw.is_empty() || raw.starts_with("data:") || raw.starts_with("blob:") || raw.starts_with('#') || raw.starts_with("/api/v1/browse") {
            out.push_str(&rest[..=end]);
        } else if let Ok(abs) = base.join(raw) {
            if matches!(abs.scheme(), "http" | "https") {
                out.push_str(&format!("\"/api/v1/browse?t={ticket}&url={}\")", urlencoding_simple(abs.as_str())));
            } else {
                out.push_str(&rest[..=end]);
            }
        } else {
            out.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.into_bytes()
}

fn urlencoding_simple(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 16);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

/// HTML'e <base> + Raspberry Pi vekil betiği ekler, sayfanın kendi meta CSP'sini etkisizleştirir.
/// Tüm alt kaynaklar (img, script, link, iframe, fetch, XHR) ve bağlantılar/formlar
/// host PC tarayıcısının internetini kullanmadan doğrudan Raspberry Pi üzerinden geçer.
pub fn rewrite(html: &[u8], url: &str, ticket: &str) -> Vec<u8> {
    let attr = url.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
    let inject = format!(
        r##"<base href="{attr}"><script>(()=>{{const B=location.protocol+"//"+location.host+"/api/v1/browse?t={t}&url=",U={u};
const prox=h=>{{try{{if(!h||/^(data:|blob:|javascript:|mailto:|tel:|#)/i.test(h))return null;if(h.includes("/api/v1/browse?t="))return null;const x=new URL(h,U);if(!/^https?:$/.test(x.protocol))return null;if(x.origin===location.origin&&x.pathname.startsWith("/api/v1/browse"))return null;return B+encodeURIComponent(x.href)}}catch(e){{return null}}}};
const rwEl=el=>{{if(!el||el.nodeType!==1)return;if(el.hasAttribute&&el.hasAttribute("srcset"))el.removeAttribute("srcset");if(el.hasAttribute&&el.hasAttribute("integrity"))el.removeAttribute("integrity");for(const k of["src","href","action","poster"]){{if(!el.hasAttribute(k))continue;if(k==="href"&&el.tagName==="A")continue;const v=el.getAttribute(k),p=prox(v);if(p)el.setAttribute(k,p)}}}};
const rwTree=root=>{{rwEl(root);if(root.querySelectorAll)root.querySelectorAll("[src],[href],[action],[poster],[srcset]").forEach(rwEl)}};
new MutationObserver(ms=>{{for(const m of ms){{if(m.type==="attributes")rwEl(m.target);else m.addedNodes.forEach(rwTree)}}}}).observe(document.documentElement,{{childList:true,subtree:true,attributes:true,attributeFilter:["src","href","action","poster","srcset"]}});
addEventListener("DOMContentLoaded",()=>rwTree(document.documentElement));
const oFetch=window.fetch;if(oFetch)window.fetch=function(input,init){{try{{const raw=typeof input==="string"?input:(input&&input.url);const p=prox(raw);if(p)input=typeof input==="string"?p:new Request(p,input)}}catch(e){{}}return oFetch.call(this,input,init)}};
const oOpen=XMLHttpRequest.prototype.open;XMLHttpRequest.prototype.open=function(m,u,...rest){{const p=prox(String(u));return oOpen.call(this,m,p||u,...rest)}};
const go=h=>{{const p=prox(h);if(!p)return false;location.href=p;return true}};
window.open=u=>{{if(u)go(u);return null}};
document.addEventListener("click",e=>{{if(e.defaultPrevented||e.button!==0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;const a=e.target.closest&&e.target.closest("a[href]");if(!a)return;const h=a.getAttribute("href")||"";if(!h||h[0]==="#"||/^(javascript|mailto|tel):/i.test(h))return;if(go(a.href))e.preventDefault()}},true);
document.addEventListener("submit",e=>{{const f=e.target;const m=(f.getAttribute("method")||"get").toLowerCase();const act=f.getAttribute("action")||U;if(m==="get"){{const x=new URL(act,U);x.search=new URLSearchParams(new FormData(f)).toString();if(go(x.href))e.preventDefault()}}else{{const p=prox(act);if(p)f.setAttribute("action",p)}}}},true);
const tell=()=>parent.postMessage({{mkBrowse:1,url:U,title:document.title}},"*");tell();addEventListener("DOMContentLoaded",tell)}})()</script>"##,
        t = ticket,
        u = js_str(url),
    );
    let low: Vec<u8> = html.iter().map(|b| b.to_ascii_lowercase()).collect();
    // Sayfanın kendi <meta http-equiv="Content-Security-Policy"> etiketi eklenen betiği engellemesin.
    let mut out = html.to_vec();
    let needle = b"content-security-policy";
    let mut i = 0;
    while let Some(p) = find(&low[i..], needle) {
        let at = i + p;
        let pre = &low[at.saturating_sub(16)..at];
        if find(pre, b"http-equiv").is_some() {
            out[at..at + needle.len()].copy_from_slice(b"x-mos-csp-removed-xxxxx");
        }
        i = at + needle.len();
    }
    let pos = tag_end(&low, b"<head").or_else(|| tag_end(&low, b"<html")).unwrap_or(0);
    let mut res = Vec::with_capacity(out.len() + inject.len());
    res.extend_from_slice(&out[..pos]);
    res.extend_from_slice(inject.as_bytes());
    res.extend_from_slice(&out[pos..]);
    res
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// `<tag` ile başlayan ilk etiketin '>' sonrası (ör. "<header" değil "<head" ya da "<head ...>").
fn tag_end(low: &[u8], tag: &[u8]) -> Option<usize> {
    let mut i = 0;
    while let Some(p) = find(&low[i..], tag) {
        let at = i + p + tag.len();
        if matches!(low.get(at), Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')) {
            return find(&low[at..], b">").map(|q| at + q + 1);
        }
        i = at;
    }
    None
}

/// Bilet geçersiz (makerd yeniden başladı ya da süresi doldu): kabuk yeni bilet alıp yeniden yükler.
pub fn expired_page(url: &str) -> Vec<u8> {
    format!(
        r#"<!doctype html><meta charset="utf-8"><body style="margin:0;height:100vh;display:grid;place-items:center;font:14px system-ui;color:#555;background:#fafafa">Yeniden bağlanılıyor…<script>parent.postMessage({{mkBrowse:1,url:{u},expired:1}},"*")</script>"#,
        u = js_str(url),
    )
    .into_bytes()
}

/// Hata sayfası (MakerOS Tarayıcı içinde yeniden deneme sunar).
pub fn error_page(url: &str, msg: &str) -> Vec<u8> {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    format!(
        r#"<!doctype html><meta charset="utf-8"><body style="margin:0;height:100vh;display:grid;place-items:center;font:14px system-ui;color:#555;background:#fafafa;text-align:center;padding:24px"><div><p style="font-size:16px;color:#222;margin:0 0 6px">Sayfa yüklenemedi</p><p style="margin:0 0 16px">{m}</p><button onclick="location.reload()" style="display:inline-block;padding:9px 18px;border:0;border-radius:999px;background:#ff7a1a;color:#fff;font:inherit;font-weight:600;cursor:pointer">Yeniden dene ↻</button></div><script>parent.postMessage({{mkBrowse:1,url:{u},error:{e}}},"*")</script>"#,
        m = esc(msg),
        u = js_str(url),
        e = js_str(msg),
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ips() {
        for a in ["127.0.0.1", "10.55.0.1", "192.168.1.1", "172.16.0.1", "169.254.1.1", "100.64.0.1", "0.0.0.0", "::1", "fe80::1", "fd00::1", "::ffff:192.168.1.5"] {
            assert!(forbidden_ip(a.parse().unwrap()), "{a}");
        }
        for a in ["1.1.1.1", "151.101.1.1", "2606:4700:4700::1111"] {
            assert!(!forbidden_ip(a.parse().unwrap()), "{a}");
        }
    }

    #[test]
    fn rewrite_injects_and_kills_meta_csp() {
        let html = br#"<!DOCTYPE html><html lang="tr"><HEAD><meta http-equiv="Content-Security-Policy" content="default-src 'self'"><title>x</title></head><header>h</header></html>"#;
        let out = String::from_utf8(rewrite(html, "https://ex.com/a?b=1&c=</script>", "tkt")).unwrap();
        let head = out.find("<HEAD>").unwrap() + 6;
        assert!(out[head..].starts_with(r#"<base href="https://ex.com/a?b=1&amp;c=&lt;/script>">"#), "{out}");
        assert!(!out.to_lowercase().contains("http-equiv=\"content-security-policy\""));
        assert!(out.contains("t=tkt&url="));
        assert!(!out.contains("c=</script>\""), "url JS içinde kaçışlı olmalı");
        assert_eq!(tag_end(b"<header>", b"<head"), None);
    }

    #[test]
    fn css_rewrites_urls() {
        let css = br#"body { background: url('../img/bg.png'); } .a { background: url("data:image/png;base64,AAA"); }"#;
        let out = String::from_utf8(rewrite_css(css, "https://ex.com/css/main.css", "tkt1")).unwrap();
        assert!(out.contains("/api/v1/browse?t=tkt1&url=https%3A%2F%2Fex.com%2Fimg%2Fbg.png"), "{out}");
        assert!(out.contains("data:image/png;base64,AAA"), "{out}");
    }

    #[tokio::test]
    async fn vet_blocks_local() {
        for u in ["http://127.0.0.1/", "http://localhost:8080/", "http://[::1]/", "file:///etc/passwd", "http://10.0.0.1/"] {
            assert!(vet(&url::Url::parse(u).unwrap()).await.is_err(), "{u}");
        }
    }

    #[test]
    fn bot_wall() {
        let p = |status, body: &str| Page { status, content_type: "text/html".into(), body: body.as_bytes().to_vec(), url: "https://x/".into() };
        assert!(is_bot_wall(&p(403, "<title>Just a moment...</title>")));
        assert!(!is_bot_wall(&p(200, "<title>Just a moment...</title>")));
        assert!(!is_bot_wall(&p(404, "<title>Yok</title>")));
    }

    #[test]
    fn tickets_per_user() {
        let a = ticket("ayse");
        assert_eq!(ticket("ayse"), a);
        assert_ne!(ticket("ali"), a);
        assert_eq!(check(&a).as_deref(), Some("ayse"));
        assert!(check("yok").is_none());
    }
}
