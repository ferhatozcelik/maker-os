//! makerd'nin HTTP katmanı: /api/v1 REST uçları, events/term
//! WebSocket'leri, CSRF/kimlik ara katmanları ve kabuk statik sunumu.

mod api;
mod proxy;
mod static_files;
mod user;
mod ws;

pub use static_files::Static;

use crate::apierr::{ApiError, ApiResult};
use crate::events::{fetch, Hub, Producer};
use crate::privsep::UserCreateArgs;
use crate::{activity, apps, auth, camera, gpio, jobs, network, notify, ota, prefs, setup, system, term, uagent};
use axum::body::Body;
use axum::extract::{ConnectInfo, Request};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get, post, put};
use axum::Router;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::net::SocketAddr;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// events WS konuları.
pub const TOPICS: &[&str] = &["system", "net", "jobs", "notifications", "processes"];

/// Kullanıcı hesabı oluşturucu (gerçek: makerd-priv user.create; mock: bellek).
pub trait Accounts: Send + Sync {
    fn create_user(&self, a: UserCreateArgs) -> BoxFuture<'_, ApiResult<()>>;
}

pub type IsAdmin = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Sunucunun bağımlılıkları.
pub struct Deps {
    pub web_dir: String,
    pub sessions: Arc<auth::Manager>,
    pub checker: Arc<dyn auth::Checker>,
    pub limiter: Arc<auth::Limiter>,
    /// None: Checker sonucu kullanılır.
    pub is_admin: Option<IsAdmin>,
    pub local_user: String,
    /// /auth/local belirteci; boşsa uç kapalıdır.
    pub local_token: String,
    /// /auth/local yalnız 127.0.0.1/::1'den kabul edilir.
    pub local_loopback_only: bool,
    /// None ise Setup'tan (o da None ise hep true) okunur.
    pub setup_done: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// İlk açılış sihirbazı; None ise /setup uçları 409 döner.
    pub setup: Option<Arc<setup::Store>>,
    pub accounts: Option<Arc<dyn Accounts>>,
    /// None ise /prefs 503 döner.
    pub prefs: Option<Arc<prefs::Store>>,
    pub hub: Hub,
    pub jobs: jobs::Manager,
    pub notify: notify::Store,
    pub system: Arc<dyn system::Provider>,
    pub network: Arc<dyn network::Provider>,
    pub apps: Arc<apps::Registry>,
    pub activity: Arc<dyn activity::Source>,
    pub term: term::Manager,
    /// Kullanıcı adına komut ve dosya işlemleri (makerd-user; mock: süreç içi).
    pub agent: Option<Arc<dyn uagent::Agent>>,
    pub camera: Option<Arc<camera::Hub>>,
    pub gpio: Option<Arc<dyn gpio::Provider>>,
    pub update: Option<Arc<ota::Updater>>,
    /// Eklenti kataloğu (mağaza); None ise /plugins/catalog 503.
    pub catalog: Option<Arc<crate::catalog::Catalog>>,
}

pub struct Server {
    pub d: Deps,
    /// Kapanışta iptal edilir; açık WebSocket'ler o zaman kapanır.
    pub stop: CancellationToken,
    stat: Static,
    /// Aynı anda tek ağ işlemi (tarama/bağlanma).
    net_busy: Arc<tokio::sync::Semaphore>,
}

type S = Arc<Server>;

impl Server {
    /// Sunucuyu kurar ve üreticileri events hub'ına kaydeder.
    pub fn new(d: Deps, stop: CancellationToken) -> S {
        let (sys, act, net) = (d.system.clone(), d.activity.clone(), d.network.clone());
        d.hub.register("system", Producer::Every(Duration::from_secs(2), fetch(move || {
            let s = sys.clone();
            async move { s.info().await }
        })));
        d.hub.register("processes", Producer::Every(Duration::from_secs(2), fetch(move || {
            let a = act.clone();
            async move { a.processes().await }
        })));
        d.hub.register("net", Producer::OnChange(Duration::from_secs(3), fetch(move || {
            let n = net.clone();
            async move { n.status().await }
        })));
        let stat = Static::new(&d.web_dir);
        Arc::new(Self { d, stop, stat, net_busy: Arc::new(tokio::sync::Semaphore::new(1)) })
    }

    pub fn setup_done(&self) -> bool {
        match (&self.d.setup_done, &self.d.setup) {
            (Some(f), _) => f(),
            (None, Some(s)) => s.done(),
            _ => true,
        }
    }

    fn admin_of(&self, user: &str, def: bool) -> bool {
        self.d.is_admin.as_ref().map(|f| f(user)).unwrap_or(def)
    }

    /// Yönlendirici (ara katmanlarla).
    pub fn router(self: &S) -> Router {
        let r = Router::new()
            .route("/api/v1/auth/login", post(api::login))
            .route("/api/v1/auth/logout", post(api::logout))
            .route("/api/v1/auth/local", get(api::local))
            .route("/api/v1/session", get(api::session))
            .route("/api/v1/system", get(api::system_info))
            .route("/api/v1/system/hostname", put(api::set_hostname))
            .route("/api/v1/system/time", get(api::get_time).put(api::set_time))
            .route("/api/v1/system/timezones", get(api::timezones))
            .route("/api/v1/system/locale", get(api::get_locale).put(api::set_locale))
            .route("/api/v1/system/password", put(api::set_password))
            .route("/api/v1/system/power", post(api::power))
            .route("/api/v1/net", get(api::net_status))
            .route("/api/v1/net/wifi/scan", post(api::wifi_scan))
            .route("/api/v1/net/wifi/connect", post(api::wifi_connect))
            .route("/api/v1/net/wifi/forget", post(api::wifi_forget))
            .route("/api/v1/net/wifi", put(api::wifi_enable))
            .route("/api/v1/prefs", get(api::get_prefs).put(api::put_prefs))
            .route("/api/v1/setup", get(api::setup_state))
            .route("/api/v1/setup/wifi/scan", post(api::setup_wifi_scan))
            .route("/api/v1/setup/complete", post(api::setup_complete))
            .route("/api/v1/apps", get(api::apps_list))
            .route("/api/v1/apps/{id}/launch", post(api::app_launch))
            .route("/api/v1/processes", get(api::processes))
            .route("/api/v1/processes/{pid}/signal", post(api::signal))
            .route("/api/v1/notifications", get(api::notifications).delete(api::notifications_clear))
            .route("/api/v1/notifications/{id}/read", post(api::notification_read))
            .route("/api/v1/jobs/{id}", get(api::job))
            .route("/api/v1/term", get(ws::term_list).post(ws::term_create))
            .route("/api/v1/term/{id}", get(ws::term_ws).delete(ws::term_close))
            .route("/api/v1/term/{id}/ticket", post(ws::term_ticket))
            .route("/api/v1/events", get(ws::events_ws))
            .route("/api/v1/browse", get(api::browse).post(api::browse).options(api::browse_options))
            .route("/api/v1/wallpaper/{name}", get(api::wallpaper_get).put(api::wallpaper_put).delete(api::wallpaper_delete))
            .route("/api/v1/browse/ticket", post(api::browse_ticket))
            .route("/api/v1/system/update", get(api::update_status))
            .route("/api/v1/system/update/check", post(api::update_check))
            .route("/api/v1/system/update/apply", post(api::update_apply))
            .route("/api/v1/files", get(user::files_list))
            .route("/api/v1/files/raw", get(user::files_raw).put(user::files_upload))
            .route("/api/v1/files/op", post(user::files_op))
            .route("/api/v1/files/run", post(user::files_run))
            .route("/api/v1/python", get(user::python_list))
            .route("/api/v1/python/{op}", post(user::python_op))
            .route("/api/v1/projects", get(user::project_list).post(user::project_create))
            .route("/api/v1/projects/templates", get(user::project_templates))
            .route("/api/v1/projects/{name}/run", post(user::project_run))
            .route("/api/v1/projects/{name}/status", get(user::project_status))
            .route("/api/v1/projects/{name}/start", post(user::project_start))
            .route("/api/v1/projects/{name}/stop", post(user::project_stop))
            .route("/api/v1/projects/{name}/log", get(user::project_log))
            .route("/api/v1/projects/{name}/packages", get(user::project_packages).post(user::project_package_op))
            .route("/api/v1/projects/{name}/python", put(user::project_python))
            .route("/api/v1/projects/{name}/autostart", put(user::project_autostart))
            .route("/api/v1/projects/{name}", delete(user::project_delete))
            .route("/api/v1/autostart", get(user::autostart_list).post(user::autostart_upsert))
            .route("/api/v1/autostart/{id}/toggle", post(user::autostart_toggle))
            .route("/api/v1/autostart/{id}/run", post(user::autostart_run))
            .route("/api/v1/autostart/{id}", delete(user::autostart_delete))
            .route("/api/v1/agents", get(user::agent_list))
            .route("/api/v1/agents/{id}/{op}", post(user::agent_op))
            .route("/api/v1/camera", get(user::camera_status))
            .route("/api/v1/camera/stream", get(user::camera_stream))
            .route("/api/v1/camera/photo", post(user::camera_photo))
            .route("/api/v1/gpio", get(user::gpio_board))
            .route("/api/v1/gpio/{n}", post(user::gpio_set))
            .route("/api/v1/plugins", get(user::plugin_list))
            .route("/api/v1/plugins/template.zip", get(user::plugin_template))
            .route("/api/v1/plugins/catalog", get(user::catalog_list))
            .route("/api/v1/plugins/catalog/{id}/install", post(user::catalog_install))
            .route("/api/v1/plugins/import", post(user::plugin_import))
            .route("/api/v1/plugins/{id}/export.zip", get(user::plugin_export))
            .route("/api/v1/plugins/{id}/asset/{*path}", get(user::plugin_asset))
            .route("/api/v1/plugins/{id}/{op}", post(user::plugin_op))
            .route("/api/v1/plugins/{id}", delete(user::plugin_delete))
            .route("/api/v1/asistan/calistir", post(user::ai_run))
            .route("/api/v1/ai/{*rest}", any(proxy::ai_proxy))
            .route("/plugin/{id}/", any(proxy::plugin_proxy))
            .route("/plugin/{id}/{*rest}", any(proxy::plugin_proxy))
            .route("/", get(|axum::extract::State(s): axum::extract::State<S>, req: Request| async move { let (p, _) = req.into_parts(); s.stat.serve(&p, "index.html").await }))
            // Tarayıcıların kendiliğinden istediği simge: MakerOS logosu.
            .route("/favicon.ico", get(|| async { redirect(StatusCode::MOVED_PERMANENTLY, "/shell/icons/logo.svg") }))
            // PWA: manifest ve service worker kökten sunulur (SW kapsamı "/" olabilsin).
            .route("/manifest.webmanifest", get(|axum::extract::State(s): axum::extract::State<S>, req: Request| async move { let (p, _) = req.into_parts(); s.stat.serve(&p, "manifest.webmanifest").await }))
            .route("/sw.js", get(|axum::extract::State(s): axum::extract::State<S>, req: Request| async move { let (p, _) = req.into_parts(); s.stat.serve(&p, "sw.js").await }))
            // Web arayüzü (uzak tarayıcı varsayılanı): kısa adres /web.
            .route("/web", get(|axum::extract::State(s): axum::extract::State<S>, req: Request| async move { let (p, _) = req.into_parts(); s.stat.serve(&p, "web/index.html").await }))
            .route("/web/", get(|| async { redirect(StatusCode::MOVED_PERMANENTLY, "/web") }))
            .route("/shell/{*rest}", get(|axum::extract::State(s): axum::extract::State<S>, req: Request| async move {
                let (p, _) = req.into_parts();
                let rel = p.uri.path().trim_start_matches("/shell/").to_string();
                s.stat.serve(&p, &rel).await
            }))
            .fallback(|req: Request| async move {
                if req.uri().path().starts_with("/api/") {
                    ApiError::not_found("Uç nokta bulunamadı.").into_response()
                } else {
                    static_files::not_found()
                }
            })
            .with_state(self.clone());
        r.layer(axum::middleware::from_fn(guard))
            // Gövde sınırları uçlarda ayrıca denetlenir (yükleme 512 MB).
            .layer(axum::extract::DefaultBodyLimit::max(user::MAX_UPLOAD + 1))
    }

    /// Dinleyicide sunar; `stop` iptal edilince kapanır.
    pub async fn serve(self: S, ln: tokio::net::TcpListener) -> std::io::Result<()> {
        let stop = self.stop.clone();
        let app = self.router().into_make_service_with_connect_info::<SocketAddr>();
        axum::serve(ln, app).with_graceful_shutdown(async move { stop.cancelled().await }).await
    }
}

/// Kurtarma → güvenlik başlıkları → CSRF → yönlendirici.
async fn guard(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let api = path.starts_with("/api/");
    let csrf_bad = api && path != "/api/v1/browse" && matches!(method, Method::POST | Method::PUT | Method::DELETE | Method::PATCH) && req.headers().get("x-maker").map(|v| v.as_bytes()) != Some(b"1");
    let mut res = if csrf_bad {
        ApiError::forbidden("Geçersiz istek (X-Maker başlığı eksik).").into_response()
    } else {
        match AssertUnwindSafe(next.run(req)).catch_unwind().await {
            Ok(r) => r,
            Err(p) => {
                let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
                tracing::error!(path = %path, panic = %msg, "işleyici çöktü");
                ApiError::internal("İstek tamamlanamadı.").into_response()
            }
        }
    };
    let h = res.headers_mut();
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    h.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    if api {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    tracing::debug!(method = %method, path = %path, status = res.status().as_u16(), dur = ?start.elapsed(), "istek");
    res
}

// --- yanıt yardımcıları ------------------------------------------------------

#[derive(Serialize)]
struct ErrInner<'a> {
    code: &'a str,
    message: &'a str,
}

#[derive(Serialize)]
struct ErrBody<'a> {
    error: ErrInner<'a>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        json_status(status, &ErrBody { error: ErrInner { code: self.code, message: &self.message } })
    }
}

/// JSON yanıt (verilen durumla).
pub fn json_status<T: Serialize + ?Sized>(status: StatusCode, v: &T) -> Response {
    match serde_json::to_vec(v) {
        Ok(b) => (status, [(header::CONTENT_TYPE, "application/json; charset=utf-8")], b).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            r#"{"error":{"code":"internal","message":"Yanıt üretilemedi."}}"#,
        )
            .into_response(),
    }
}

pub fn json<T: Serialize + ?Sized>(v: &T) -> Response {
    json_status(StatusCode::OK, v)
}

/// Hazır JSON baytları.
pub fn raw_json(b: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "application/json; charset=utf-8")], b).into_response()
}

pub fn no_content() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub fn redirect(status: StatusCode, to: &str) -> Response {
    (status, [(header::LOCATION, to.to_string())]).into_response()
}

pub type R = Result<Response, ApiError>;

/// JSON gövdesini (en fazla 1 MB) çözer; boş gövde varsayılan değerdir.
pub fn decode<T: DeserializeOwned + Default>(body: &[u8]) -> ApiResult<T> {
    if body.len() > 1 << 20 {
        return Err(ApiError::invalid("Geçersiz JSON gövdesi."));
    }
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body).map_err(|_| ApiError::invalid("Geçersiz JSON gövdesi."))
}

/// İstemci IP'si (bağlantı bilgisinden).
pub fn client_ip(ci: &ConnectInfo<SocketAddr>) -> String {
    ci.0.ip().to_canonical().to_string()
}

/// Origin başlığı Host ile aynı olmalı (WS için zorunlu).
pub fn origin_ok(h: &HeaderMap) -> bool {
    let (Some(o), Some(host)) = (h.get(header::ORIGIN).and_then(|v| v.to_str().ok()), h.get(header::HOST).and_then(|v| v.to_str().ok())) else { return false };
    let Some(rest) = o.split_once("://").map(|(_, r)| r) else { return false };
    let oh = rest.split(['/', '?', '#']).next().unwrap_or("");
    !oh.is_empty() && oh.eq_ignore_ascii_case(host)
}

/// İstekteki oturum.
pub fn session_of(s: &Server, h: &HeaderMap) -> ApiResult<auth::Session> {
    let cookies: Vec<&str> = h.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).collect();
    let joined = cookies.join("; ");
    auth::cookie_value(&joined, auth::COOKIE_NAME).and_then(|v| s.d.sessions.lookup(v)).ok_or_else(|| ApiError::unauthorized("Giriş gerekli."))
}

/// Yönetici oturumu.
pub fn admin_of(s: &Server, h: &HeaderMap) -> ApiResult<auth::Session> {
    let sess = session_of(s, h)?;
    if !sess.admin {
        return Err(ApiError::forbidden("Bu işlem yönetici yetkisi gerektirir."));
    }
    Ok(sess)
}

/// API hatasının iletisi.
pub fn err_msg(e: &ApiError) -> String {
    e.message.clone()
}

#[cfg(test)]
mod tests;
