//! httpd uçtan uca testleri: mock arka uçlarla gerçek sunucu (127.0.0.1:0).

use super::*;
use crate::termproc::login::CmdSpec;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use bytes::Bytes;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

const SETUP_BODY: &str = r#"{"user":"ayse","password":"gizli-parola","fullname":"Ayşe Yılmaz","hostname":"makeros-0001",
	"timezone":"Europe/Berlin","lang":"de_DE.UTF-8","keymap":"de"}"#;

struct Env {
    base: String,
    addr: SocketAddr,
    web: PathBuf,
    srv: S,
    sys: Arc<crate::mock::System>,
}

struct Resp {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Resp {
    fn code(&self) -> String {
        serde_json::from_slice::<serde_json::Value>(&self.body).ok().and_then(|v| v["error"]["code"].as_str().map(String::from)).unwrap_or_default()
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_default()
    }
    fn cookie(&self) -> String {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter_map(|c| c.split(';').next())
            .find(|kv| kv.starts_with("mk_session=") && kv.len() > "mk_session=".len())
            .unwrap_or("")
            .to_string()
    }
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mk-http-{tag}-{}", crate::util::random_hex(4)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sh() -> crate::termproc::login::Command {
    Arc::new(|| Ok(CmdSpec { path: "/bin/sh".into(), argv0: "sh".into(), args: vec![], dir: "/".into(), env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("TERM".into(), "dumb".into())] }))
}

async fn env(fresh: bool) -> Env {
    let web = tmp("web");
    let st = if fresh { crate::mock::State::fresh() } else { crate::mock::State::new() };
    let store = Arc::new(setup::Store::new(tmp("data")));
    if !fresh {
        store.mark_done("maker").unwrap();
    }
    let mut net = crate::mock::Network::new();
    net.step = Duration::from_millis(5);
    let hub = Hub::new(TOPICS);
    let sys = Arc::new(crate::mock::System::new(st.clone(), "test"));
    let d = Deps {
        web_dir: web.to_string_lossy().into_owned(),
        sessions: Arc::new(auth::Manager::new()),
        checker: Arc::new(crate::mock::Checker(st.clone())),
        limiter: Arc::new(auth::Limiter::new(5, Duration::from_secs(60))),
        is_admin: None,
        local_user: "maker".into(),
        local_token: "yerel-belirtec".into(),
        local_loopback_only: false,
        setup_done: None,
        setup: Some(store),
        accounts: Some(Arc::new(st.clone())),
        prefs: Some(Arc::new(prefs::Store::new(tmp("prefs")))),
        jobs: jobs::Manager::new(Some(hub.clone())),
        notify: notify::Store::new(Some(hub.clone())),
        hub,
        system: sys.clone(),
        network: Arc::new(net),
        apps: Arc::new(apps::Registry::new(vec![], "", crate::mock::native_apps(), crate::mock::launcher(None))),
        activity: Arc::new(crate::mock::Activity::new()),
        term: term::Manager::new(term::local(sh())),
        agent: Some(Arc::new(uagent::Local { command: sh(), home: tmp("home").to_string_lossy().into_owned() })),
        camera: Some(Arc::new(camera::Hub::new(Arc::new(camera::Synthetic)))),
        gpio: Some(Arc::new(gpio::Sim::new("test"))),
        update: None,
        catalog: None,
    };
    let srv = Server::new(d, CancellationToken::new());
    let ln = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = ln.local_addr().unwrap();
    tokio::spawn(srv.clone().serve(ln));
    Env { base: format!("http://{addr}"), addr, web, srv, sys }
}

impl Env {
    async fn req(&self, method: &str, path: &str, body: &str, hdr: &[(&str, &str)]) -> Resp {
        let c: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
        let mut b = axum::http::Request::builder().method(method).uri(format!("{}{path}", self.base));
        for (k, v) in hdr {
            b = b.header(*k, *v);
        }
        let res = c.request(b.body(Full::new(Bytes::from(body.to_string()))).unwrap()).await.unwrap();
        let status = res.status().as_u16();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.map(|b| b.to_bytes().to_vec()).unwrap_or_default();
        Resp { status, headers, body }
    }

    async fn login(&self) -> String {
        let r = self.req("POST", "/api/v1/auth/login", r#"{"user":"maker","password":"maker"}"#, &[("X-Maker", "1")]).await;
        assert_eq!(r.status, 200);
        let sc = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
        assert!(sc.contains("HttpOnly") && sc.contains("SameSite=Strict") && sc.contains("Path=/") && sc.contains(&format!("Max-Age={}", 7 * 24 * 3600)), "{sc}");
        r.cookie()
    }

    async fn ws(&self, path: &str, hdr: &[(&str, &str)]) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, u16> {
        let mut req = format!("ws://{}{path}", self.addr).into_client_request().unwrap();
        for (k, v) in hdr {
            req.headers_mut().insert(axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
        }
        match tokio_tungstenite::connect_async(req).await {
            Ok((ws, _)) => Ok(ws),
            Err(tungstenite::Error::Http(r)) => Err(r.status().as_u16()),
            Err(e) => panic!("ws: {e}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_and_csrf() {
    let e = env(false).await;
    let cookie = e.login().await;
    let authd: &[(&str, &str)] = &[("Cookie", &cookie), ("X-Maker", "1")];
    let no_csrf: &[(&str, &str)] = &[("Cookie", &cookie)];
    let x: &[(&str, &str)] = &[("X-Maker", "1")];
    let cases: Vec<(&str, &str, &str, &str, &[(&str, &str)], u16, &str)> = vec![
        ("session no cookie", "GET", "/api/v1/session", "", &[], 401, "unauthorized"),
        ("session bad cookie", "GET", "/api/v1/session", "", &[("Cookie", "mk_session=x.y")], 401, "unauthorized"),
        ("session ok", "GET", "/api/v1/session", "", no_csrf, 200, ""),
        ("system ok", "GET", "/api/v1/system", "", no_csrf, 200, ""),
        ("login without X-Maker", "POST", "/api/v1/auth/login", r#"{"user":"maker","password":"maker"}"#, &[], 403, "forbidden"),
        ("login wrong X-Maker", "POST", "/api/v1/auth/login", r#"{"user":"maker","password":"maker"}"#, &[("X-Maker", "0")], 403, "forbidden"),
        ("put without X-Maker", "PUT", "/api/v1/system/hostname", r#"{"hostname":"pi"}"#, no_csrf, 403, "forbidden"),
        ("put unauth", "PUT", "/api/v1/system/hostname", r#"{"hostname":"pi"}"#, x, 401, "unauthorized"),
        ("hostname invalid", "PUT", "/api/v1/system/hostname", r#"{"hostname":"Bad_Name"}"#, authd, 400, "invalid"),
        ("hostname ok", "PUT", "/api/v1/system/hostname", r#"{"hostname":"atolye-pi"}"#, authd, 204, ""),
        ("bad json", "PUT", "/api/v1/system/locale", "{", authd, 400, "invalid"),
        ("locale ok", "PUT", "/api/v1/system/locale", r#"{"lang":"en_US.UTF-8"}"#, authd, 200, ""),
        ("password wrong old", "PUT", "/api/v1/system/password", r#"{"old":"x","new":"yenisifre"}"#, authd, 403, "forbidden"),
        ("power invalid", "POST", "/api/v1/system/power", r#"{"action":"explode"}"#, authd, 400, "invalid"),
        ("power ok", "POST", "/api/v1/system/power", r#"{"action":"reboot"}"#, authd, 202, ""),
        ("unknown api", "GET", "/api/v1/nope", "", no_csrf, 404, "not_found"),
        ("app launch web", "POST", "/api/v1/apps/makeros.settings/launch", "", authd, 400, "invalid"),
        ("app launch native", "POST", "/api/v1/apps/chromium/launch", "", authd, 202, ""),
        ("app launch url", "POST", "/api/v1/apps/chromium/launch", r#"{"url":"https://example.com/a?b=1"}"#, authd, 202, ""),
        ("app launch bad url", "POST", "/api/v1/apps/chromium/launch", r#"{"url":"file:///etc/passwd"}"#, authd, 400, "invalid"),
        ("app launch flag url", "POST", "/api/v1/apps/chromium/launch", r#"{"url":"--gpu-launcher=sh"}"#, authd, 400, "invalid"),
        ("signal foreign", "POST", "/api/v1/processes/1/signal", r#"{"signal":"TERM"}"#, authd, 403, "forbidden"),
        ("signal own", "POST", "/api/v1/processes/1240/signal", r#"{"signal":"KILL"}"#, authd, 204, ""),
        ("notification missing", "POST", "/api/v1/notifications/zz/read", "", authd, 404, "not_found"),
        ("notifications clear", "DELETE", "/api/v1/notifications", "", authd, 204, ""),
        ("files run non-py", "POST", "/api/v1/files/run", r#"{"path":"a.sh"}"#, authd, 400, ""),
        ("forget unknown", "POST", "/api/v1/net/wifi/forget", r#"{"ssid":"Yok"}"#, authd, 404, "not_found"),
        ("forget known", "POST", "/api/v1/net/wifi/forget", r#"{"ssid":"Ev-5G"}"#, authd, 204, ""),
        ("logout no csrf", "POST", "/api/v1/auth/logout", "", no_csrf, 403, "forbidden"),
    ];
    for (name, m, p, b, h, status, code) in cases {
        let r = e.req(m, p, b, h).await;
        assert_eq!(r.status, status, "{name}: {}", String::from_utf8_lossy(&r.body));
        if !code.is_empty() {
            assert_eq!(r.code(), code, "{name}");
        }
        if p.starts_with("/api/") {
            assert_eq!(r.headers.get(header::CACHE_CONTROL).map(|v| v.to_str().unwrap()), Some("no-store"), "{name}");
        }
    }
    let s = e.req("GET", "/api/v1/session", "", no_csrf).await.json();
    assert_eq!((s["user"].as_str(), s["admin"].as_bool(), s["setup_done"].as_bool()), (Some("maker"), Some(true), Some(true)));
    assert_eq!(e.req("POST", "/api/v1/auth/logout", "", authd).await.status, 204);
    assert_eq!(e.req("GET", "/api/v1/session", "", no_csrf).await.status, 401, "çıkıştan sonra oturum geçersiz");
}

#[tokio::test(flavor = "multi_thread")]
async fn login_rate_limit_and_local() {
    let e = env(false).await;
    for i in 0..5 {
        assert_eq!(e.req("POST", "/api/v1/auth/login", r#"{"user":"maker","password":"bad"}"#, &[("X-Maker", "1")]).await.status, 401, "deneme {i}");
    }
    let r = e.req("POST", "/api/v1/auth/login", r#"{"user":"maker","password":"maker"}"#, &[("X-Maker", "1")]).await;
    assert_eq!((r.status, r.code().as_str()), (429, "rate_limited"));
    assert!(r.headers.get(header::RETRY_AFTER).is_some());

    let e = env(false).await;
    assert_eq!(e.req("GET", "/api/v1/auth/local?token=yanlis", "", &[]).await.status, 403);
    let r = e.req("GET", "/api/v1/auth/local?token=yerel-belirtec", "", &[]).await;
    assert_eq!((r.status, r.headers.get(header::LOCATION).unwrap().to_str().unwrap()), (302, "/?ui=desktop&local=1"));
    assert_eq!(e.req("GET", "/api/v1/session", "", &[("Cookie", &r.cookie())]).await.status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn static_caching() {
    let e = env(false).await;
    let w = |rel: &str, body: &str| {
        let p = e.web.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    w("index.html", "<!doctype html>kabuk");
    w("app.js", "console.log(1)");
    w("app.js.br", "BR");
    w("app.js.gz", "GZ");
    w("vendor/lib.js", "lib");
    w("fonts/inter.woff2", "font");
    w("apps/settings/index.html", "ayarlar");
    w(".secret", "x");
    std::os::unix::fs::symlink("/etc/passwd", e.web.join("kacis.txt")).unwrap();
    let cases: Vec<(&str, &str, Vec<(&str, &str)>, u16, &str, &str, &str, &str)> = vec![
        ("root", "/", vec![], 200, "<!doctype html>kabuk", "no-cache", "", "text/html; charset=utf-8"),
        ("js plain", "/shell/app.js", vec![], 200, "console.log(1)", "no-cache", "", "text/javascript; charset=utf-8"),
        ("js br", "/shell/app.js", vec![("Accept-Encoding", "gzip, br")], 200, "BR", "no-cache", "br", "text/javascript; charset=utf-8"),
        ("js gz", "/shell/app.js", vec![("Accept-Encoding", "gzip, br;q=0")], 200, "GZ", "no-cache", "gzip", ""),
        ("vendor", "/shell/vendor/lib.js", vec![], 200, "lib", "no-cache", "", ""),
        ("font", "/shell/fonts/inter.woff2", vec![], 200, "font", "public, max-age=31536000, immutable", "", "font/woff2"),
        ("dir index", "/shell/apps/settings/", vec![], 200, "ayarlar", "no-cache", "", ""),
        ("dir redirect", "/shell/apps/settings", vec![], 301, "", "", "", ""),
        ("dotfile", "/shell/.secret", vec![], 404, "", "", "", ""),
        ("symlink escape", "/shell/kacis.txt", vec![], 404, "", "", "", ""),
        ("traversal", "/shell/../../etc/passwd", vec![], 404, "", "", "", ""),
        ("missing", "/shell/nope.js", vec![], 404, "", "", "", ""),
        ("other root path", "/etc/passwd", vec![], 404, "", "", "", ""),
    ];
    for (name, path, hdr, status, body, cc, enc, ct) in cases {
        let r = e.req("GET", path, "", &hdr).await;
        assert_eq!(r.status, status, "{name}");
        if status != 200 {
            continue;
        }
        let g = |k| r.headers.get(k).map(|v: &HeaderValue| v.to_str().unwrap().to_string()).unwrap_or_default();
        assert_eq!((String::from_utf8_lossy(&r.body).as_ref(), g(header::CACHE_CONTROL).as_str(), g(header::CONTENT_ENCODING).as_str()), (body, cc, enc), "{name}");
        if !ct.is_empty() {
            assert_eq!(g(header::CONTENT_TYPE), ct, "{name}");
        }
        assert_eq!(g("x-content-type-options".parse().unwrap()), "nosniff");
        let etag = g(header::ETAG);
        assert!(!etag.is_empty(), "{name}: ETag yok");
        let mut h2 = hdr.clone();
        h2.push(("If-None-Match", &etag));
        assert_eq!(e.req("GET", path, "", &h2).await.status, 304, "{name}: yeniden doğrulama");
    }
    let plain = e.req("GET", "/shell/app.js", "", &[]).await.headers.get(header::ETAG).cloned();
    let br = e.req("GET", "/shell/app.js", "", &[("Accept-Encoding", "br")]).await.headers.get(header::ETAG).cloned();
    assert_ne!(plain, br, "ETag kodlamaya göre değişmeli");
}

#[tokio::test(flavor = "multi_thread")]
async fn setup_flow() {
    let e = env(true).await;
    let x: &[(&str, &str)] = &[("X-Maker", "1")];
    assert_eq!(e.req("POST", "/api/v1/auth/login", r#"{"user":"maker","password":"maker"}"#, x).await.status, 401, "kurulumdan önce giriş");
    let r = e.req("GET", "/api/v1/setup", "", &[]).await;
    let st = r.json();
    assert_eq!(r.status, 200);
    assert_eq!((st["setup_done"].as_bool(), st["hostname"].as_str(), st["defaults"]["lang"].as_str(), st["min_password"].as_u64()), (Some(false), Some("maker-os"), Some("tr_TR.UTF-8"), Some(6)));
    assert!(st["languages"].as_array().is_some_and(|l| !l.is_empty()) && st["timezones"].as_array().is_some_and(|l| !l.is_empty()));
    assert_eq!(e.req("POST", "/api/v1/setup/wifi/scan", "", x).await.status, 200);
    let r = e.req("GET", "/api/v1/auth/local?token=yerel-belirtec", "", &[]).await;
    assert_eq!((r.status, r.cookie().as_str()), (302, ""), "kurulumdan önce yerel oturum açılmaz");
    assert_eq!(e.req("POST", "/api/v1/setup/complete", SETUP_BODY, &[]).await.status, 403);
    for bad in [
        SETUP_BODY.replacen(r#""ayse""#, r#""root""#, 1),
        SETUP_BODY.replacen("gizli-parola", "abc", 1),
        SETUP_BODY.replacen("makeros-0001", "Bad_Name", 1),
        SETUP_BODY.replacen("Europe/Berlin", "../../etc", 1),
        r#"{"user":"ayse""#.to_string(),
    ] {
        assert_eq!(e.req("POST", "/api/v1/setup/complete", &bad, x).await.status, 400, "{bad}");
    }
    // Eşzamanlı tamamlama: tam olarak biri başarılı olur.
    let e = Arc::new(e);
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..6 {
        let e = e.clone();
        set.spawn(async move { e.req("POST", "/api/v1/setup/complete", SETUP_BODY, &[("X-Maker", "1")]).await });
    }
    let mut winner = None;
    let mut codes = HashMap::new();
    while let Some(r) = set.join_next().await {
        let r = r.unwrap();
        *codes.entry(r.status).or_insert(0) += 1;
        if r.status == 200 {
            winner = Some(r);
        }
    }
    assert_eq!((codes.get(&200), codes.get(&409)), (Some(&1), Some(&5)), "{codes:?}");
    let w = winner.unwrap();
    let sr = w.json();
    let cookie = w.cookie();
    assert_eq!((sr["user"].as_str(), sr["admin"].as_bool(), sr.get("warnings").is_none()), (Some("ayse"), Some(true), true), "{sr}");
    let s = e.req("GET", "/api/v1/session", "", &[("Cookie", &cookie)]).await.json();
    assert_eq!((s["user"].as_str(), s["setup_done"].as_bool()), (Some("ayse"), Some(true)));
    use crate::system::Provider as _;
    assert_eq!(e.sys.time().await.unwrap().timezone, "Europe/Berlin");
    let l = e.sys.locale().await.unwrap();
    assert_eq!((l.lang.as_str(), l.keymap.as_str()), ("de_DE.UTF-8", "de"));
    assert_eq!(e.sys.info().await.unwrap().hostname, "makeros-0001");
    assert_eq!(e.req("GET", "/api/v1/setup", "", &[]).await.status, 409);
    assert_eq!(e.req("POST", "/api/v1/setup/complete", SETUP_BODY, x).await.status, 409);
    assert_eq!(e.req("POST", "/api/v1/auth/login", r#"{"user":"ayse","password":"gizli-parola"}"#, x).await.status, 200);
    let r = e.req("GET", "/api/v1/auth/local?token=yerel-belirtec", "", &[]).await;
    assert_eq!(e.req("GET", "/api/v1/session", "", &[("Cookie", &r.cookie())]).await.json()["user"].as_str(), Some("ayse"), "yerel kabuk kurulum kullanıcısıyla açılır");
}

#[tokio::test(flavor = "multi_thread")]
async fn prefs_files_gpio_camera_jobs() {
    let e = env(false).await;
    let c = e.login().await;
    let a: &[(&str, &str)] = &[("Cookie", &c), ("X-Maker", "1")];
    assert_eq!(e.req("GET", "/api/v1/prefs", "", a).await.body, b"{}");
    assert_eq!(e.req("PUT", "/api/v1/prefs", r#"{"tema":"koyu"}"#, a).await.json()["tema"], "koyu");
    assert_eq!(e.req("PUT", "/api/v1/prefs", "[1]", a).await.status, 400);
    assert_eq!(e.req("PUT", "/api/v1/prefs", &format!(r#"{{"x":"{}"}}"#, "a".repeat(17 << 10)), a).await.status, 413);
    // Dosyalar: yükle, listele, indir, işlem.
    assert_eq!(e.req("PUT", "/api/v1/files/raw?path=Belgeler/not.txt", "merhaba", a).await.status, 204);
    let l = e.req("GET", "/api/v1/files?path=Belgeler", "", a).await.json();
    assert_eq!(l["entries"][0]["name"], "not.txt");
    let r = e.req("GET", "/api/v1/files/raw?path=Belgeler/not.txt&download=1", "", a).await;
    assert_eq!((r.status, r.body.as_slice()), (200, &b"merhaba"[..]));
    assert_eq!(r.headers.get(header::CONTENT_SECURITY_POLICY).unwrap(), "sandbox");
    assert!(r.headers.get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().contains("not.txt"));
    assert_eq!(e.req("GET", "/api/v1/files/raw?path=yok.txt", "", a).await.code(), "invalid");
    assert_eq!(e.req("POST", "/api/v1/files/op", r#"{"op":"chmod","path":"x"}"#, a).await.status, 400);
    assert_eq!(e.req("POST", "/api/v1/files/op", r#"{"op":"mkdir","path":"Yeni/Alt"}"#, a).await.status, 200);
    // GPIO benzetimi.
    let b = e.req("POST", "/api/v1/gpio/17", r#"{"value":1}"#, a).await.json();
    assert_eq!(b["pins"].as_array().unwrap().iter().find(|p| p["gpio"] == 17).unwrap()["held"], true);
    assert_eq!(e.req("POST", "/api/v1/gpio/0", r#"{"value":1}"#, a).await.status, 400);
    assert_eq!(e.req("POST", "/api/v1/gpio/x", "{}", a).await.status, 400);
    // Kamera: fotoğraf kaydedilir, akış çok parçalı.
    let p = e.req("POST", "/api/v1/camera/photo", "", a).await.json();
    assert!(p["path"].as_str().unwrap().starts_with("~/Resimler/Kamera/") && p["size"].as_u64().unwrap() > 1000);
    assert!(e.req("GET", "/api/v1/camera", "", a).await.json()["sim"].as_bool().unwrap());
    // Terminal: oluştur, listele, kapat.
    let t = e.req("POST", "/api/v1/term", r#"{"cols":90,"rows":20,"title":"deneme"}"#, a).await.json();
    let id = t["id"].as_str().unwrap().to_string();
    assert!(!t["ticket"].as_str().unwrap().is_empty());
    assert_eq!(e.req("GET", "/api/v1/term", "", a).await.json()[0]["title"], "deneme");
    assert_eq!(e.req("DELETE", &format!("/api/v1/term/{id}"), "", a).await.status, 204);
    assert_eq!(e.req("DELETE", "/api/v1/term/yok", "", a).await.status, 404);
    // Ajan işlemi terminal oturumu açar.
    let r = e.req("POST", "/api/v1/agents/yok/install", "", a).await;
    assert_eq!(r.status, 404);
    // Wi-Fi bağlanma işi.
    let j = e.req("POST", "/api/v1/net/wifi/connect", r#"{"ssid":"Kafe_WiFi"}"#, a).await;
    assert_eq!(j.status, 202);
    let jid = j.json()["job"].as_str().unwrap().to_string();
    let mut state = String::new();
    for _ in 0..100 {
        state = e.req("GET", &format!("/api/v1/jobs/{jid}"), "", a).await.json()["state"].as_str().unwrap_or("").to_string();
        if state != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(state, "done");
    assert_eq!(e.req("GET", "/api/v1/net", "", a).await.json()["ifaces"][1]["ssid"], "Kafe_WiFi");
    let st = e.req("PUT", "/api/v1/net/wifi", r#"{"enabled":false}"#, a).await.json();
    assert_eq!(st["wifi_enabled"], false);
    assert_eq!(e.req("PUT", "/api/v1/net/wifi", "{}", a).await.status, 400);
    // Eklenti şablonu zip olarak iner; içe aktarma ve dışa aktarma.
    let z = e.req("GET", "/api/v1/plugins/template.zip", "", a).await;
    assert_eq!(z.headers.get(header::CONTENT_TYPE).unwrap(), "application/zip");
    let imp = e.req_bytes("POST", "/api/v1/plugins/import", z.body.clone(), a).await.json();
    let pid = imp["id"].as_str().unwrap().to_string();
    let ex = e.req("GET", &format!("/api/v1/plugins/{pid}/export.zip"), "", a).await;
    assert_eq!(ex.status, 200);
    assert!(crate::plugins::unpack(&ex.body).is_ok());
    let r = e.req("GET", &format!("/api/v1/plugins/{pid}/asset/../plugin.json"), "", a).await;
    assert_eq!(r.status, 404);
    // Proje şablonları kimliksiz verilmez.
    assert_eq!(e.req("GET", "/api/v1/projects/templates", "", &[]).await.status, 401);
    assert!(e.req("GET", "/api/v1/projects/templates", "", a).await.json().as_array().unwrap().len() >= 8);
}

impl Env {
    async fn req_bytes(&self, method: &str, path: &str, body: Vec<u8>, hdr: &[(&str, &str)]) -> Resp {
        let c: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
        let mut b = axum::http::Request::builder().method(method).uri(format!("{}{path}", self.base));
        for (k, v) in hdr {
            b = b.header(*k, *v);
        }
        let res = c.request(b.body(Full::new(Bytes::from(body))).unwrap()).await.unwrap();
        let status = res.status().as_u16();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.map(|b| b.to_bytes().to_vec()).unwrap_or_default();
        Resp { status, headers, body }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn events_ws() {
    let e = env(false).await;
    let cookie = e.login().await;
    let origin = e.base.clone();
    assert_eq!(e.ws("/api/v1/events", &[("Cookie", &cookie)]).await.err(), Some(403), "Origin yoksa reddedilir");
    assert_eq!(e.ws("/api/v1/events", &[("Origin", &origin)]).await.err(), Some(401), "çerez yoksa 401");
    let mut c = e.ws("/api/v1/events", &[("Cookie", &cookie), ("Origin", &origin)]).await.unwrap();
    c.send(tungstenite::Message::text(r#"{"op":"sub","topics":["system","jobs","net"]}"#)).await.unwrap();
    let e = Arc::new(e);
    let (e2, ck) = (e.clone(), cookie.clone());
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        e2.req("POST", "/api/v1/net/wifi/connect", r#"{"ssid":"Kafe_WiFi"}"#, &[("Cookie", &ck), ("X-Maker", "1")]).await;
    });
    let mut seen = std::collections::HashSet::new();
    let mut job_done = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !(seen.contains("system") && seen.contains("net") && job_done) {
        let m = tokio::time::timeout_at(deadline, c.next()).await.expect("olay gelmedi").unwrap().unwrap();
        let tungstenite::Message::Text(t) = m else { continue };
        let v: serde_json::Value = serde_json::from_str(t.as_str()).unwrap();
        let topic = v["topic"].as_str().unwrap_or("").to_string();
        if topic == "system" {
            assert_eq!(v["data"]["os"]["name"], "MakerOS");
        }
        if topic == "jobs" && v["data"]["state"] == "done" {
            job_done = true;
        }
        seen.insert(topic);
    }
    assert!(e.srv.d.hub.running("system"));
    c.send(tungstenite::Message::text(r#"{"op":"unsub","topics":["system"]}"#)).await.unwrap();
    for _ in 0..50 {
        if !e.srv.d.hub.running("system") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!e.srv.d.hub.running("system"), "abonelik kalkınca üretici durmalı");
}

#[tokio::test(flavor = "multi_thread")]
async fn term_ws() {
    let e = env(false).await;
    let cookie = e.login().await;
    let h: &[(&str, &str)] = &[("Cookie", &cookie), ("X-Maker", "1")];
    let tr = e.req("POST", "/api/v1/term", r#"{"cols":100,"rows":30}"#, h).await.json();
    let (id, ticket) = (tr["id"].as_str().unwrap().to_string(), tr["ticket"].as_str().unwrap().to_string());
    let path = format!("/api/v1/term/{id}?ticket={ticket}");
    let origin = e.base.clone();
    assert_eq!(e.ws(&path, &[("Origin", "http://evil.example")]).await.err(), Some(403), "yanlış Origin");
    // Bilet hatalı Origin denemesinde harcanmaz.
    let mut c = e.ws(&path, &[("Origin", &origin)]).await.unwrap();
    c.send(tungstenite::Message::text(r#"{"type":"resize","cols":120,"rows":40}"#)).await.unwrap();
    c.send(tungstenite::Message::binary(&b"echo MERHABA_$((40+2))\n"[..])).await.unwrap();
    async fn read_until(c: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, want: &str) -> Option<tungstenite::protocol::CloseFrame> {
        let mut acc = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !String::from_utf8_lossy(&acc).contains(want) {
            match tokio::time::timeout_at(deadline, c.next()).await.expect("zaman aşımı") {
                Some(Ok(tungstenite::Message::Binary(b))) => acc.extend_from_slice(&b),
                Some(Ok(tungstenite::Message::Close(f))) => return f,
                Some(Ok(_)) => {}
                other => panic!("okuma: {other:?}; gelen {:?}", String::from_utf8_lossy(&acc)),
            }
        }
        None
    }
    read_until(&mut c, "MERHABA_42").await;
    assert_eq!(e.ws(&path, &[("Origin", &origin)]).await.err(), Some(403), "bilet tek kullanımlık");
    c.close(None).await.unwrap();
    let tr2 = e.req("POST", &format!("/api/v1/term/{id}/ticket"), "", h).await.json();
    assert_eq!(tr2["id"].as_str(), Some(id.as_str()));
    let mut c2 = e.ws(&format!("/api/v1/term/{id}?ticket={}", tr2["ticket"].as_str().unwrap()), &[("Origin", &origin)]).await.unwrap();
    read_until(&mut c2, "MERHABA_42").await;
    // Kabuk çıkınca WS normal kapanır.
    c2.send(tungstenite::Message::binary(&b"exit\n"[..])).await.unwrap();
    let f = read_until(&mut c2, "\u{0}YOK\u{0}").await.expect("kapanış çerçevesi");
    assert_eq!(u16::from(f.code), 1000);
}

#[tokio::test]
async fn wallpapers() {
    let e = env(false).await;
    let c = e.login().await;
    let a: &[(&str, &str)] = &[("Cookie", &c), ("X-Maker", "1")];
    let jpg = [vec![0xff, 0xd8, 0xff, 0xe0], vec![0u8; 64]].concat();
    assert_eq!(e.req("GET", "/api/v1/wallpaper/lock", "", &[]).await.status, 404);
    assert_eq!(e.req_bytes("PUT", "/api/v1/wallpaper/lock", jpg.clone(), &[("X-Maker", "1")]).await.status, 401);
    assert_eq!(e.req_bytes("PUT", "/api/v1/wallpaper/lock", b"<svg/>".to_vec(), a).await.status, 400, "resim değil");
    assert_eq!(e.req_bytes("PUT", "/api/v1/wallpaper/../prefs", jpg.clone(), a).await.status, 404);
    assert_eq!(e.req_bytes("PUT", "/api/v1/wallpaper/lock", jpg.clone(), a).await.status, 204);
    // Kilit ekranı girişten önce de görünür: oturumsuz GET.
    let r = e.req("GET", "/api/v1/wallpaper/lock", "", &[]).await;
    assert_eq!((r.status, r.headers.get("content-type").unwrap().to_str().unwrap(), r.body.len()), (200, "image/jpeg", jpg.len()));
    assert_eq!(e.req("DELETE", "/api/v1/wallpaper/lock", "", a).await.status, 204);
    assert_eq!(e.req("GET", "/api/v1/wallpaper/lock", "", &[]).await.status, 404);
}
