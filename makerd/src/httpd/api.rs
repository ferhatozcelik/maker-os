//! Oturum, sistem, ağ, tercihler, uygulamalar, süreçler, bildirimler, işler,
//! ilk kurulum ve yazılım güncellemesi uçları.

use super::*;
use crate::network::{valid_ssid, ConnectRequest};
use crate::privsep;
use crate::setup::BeginError;
use axum::extract::{Path, Query, State};
use bytes::Bytes;
use serde::Deserialize;
use std::collections::HashMap;

type Ci = ConnectInfo<SocketAddr>;

// --- auth ------------------------------------------------------------------

#[derive(Serialize)]
struct UserResp {
    user: String,
    admin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    setup_done: Option<bool>,
}

fn rate_limited(s: &Server, ip: &str) -> Option<Response> {
    let wait = s.d.limiter.allow(ip)?;
    let mut r = ApiError::new(429, "rate_limited", "Çok fazla başarısız deneme. Bir dakika sonra tekrar deneyin.").into_response();
    if let Ok(v) = HeaderValue::from_str(&(wait.as_secs() + 1).to_string()) {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    Some(r)
}

fn with_cookie(mut r: Response, cookie: &str) -> Response {
    if let Ok(v) = HeaderValue::from_str(cookie) {
        r.headers_mut().append(header::SET_COOKIE, v);
    }
    r
}

pub async fn login(State(s): State<S>, ci: Ci, body: Bytes) -> R {
    let ip = client_ip(&ci);
    if let Some(r) = rate_limited(&s, &ip) {
        return Ok(r);
    }
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        user: String,
        #[serde(default)]
        password: String,
    }
    let req: Req = decode(&body)?;
    let res = match s.d.checker.check(&req.user, &req.password).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(err = %e, "parola denetlenemedi");
            return Err(ApiError::unavailable("Parola şu anda denetlenemiyor."));
        }
    };
    if !res.ok {
        s.d.limiter.fail(&ip);
        tracing::warn!(user = %req.user, ip = %ip, "başarısız giriş");
        return Err(ApiError::unauthorized("Kullanıcı adı veya parola hatalı."));
    }
    s.d.limiter.reset(&ip);
    let admin = s.admin_of(&req.user, res.admin);
    let (_, cookie) = s.d.sessions.create(&req.user, admin);
    tracing::info!(user = %req.user, ip = %ip, "giriş");
    Ok(with_cookie(json(&UserResp { user: req.user, admin, setup_done: None }), &s.d.sessions.set_cookie(&cookie)))
}

pub async fn logout(State(s): State<S>, h: HeaderMap) -> Response {
    let cookies: Vec<&str> = h.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).collect();
    if let Some(v) = auth::cookie_value(&cookies.join("; "), auth::COOKIE_NAME) {
        s.d.sessions.delete(v);
    }
    with_cookie(no_content(), &s.d.sessions.clear_cookie())
}

/// Yerel kabuk kullanıcısı: kurulumda oluşturulan kullanıcı, yoksa local_user.
fn local_user(s: &Server) -> String {
    s.d.setup.as_ref().map(|st| st.primary_user()).filter(|u| !u.is_empty()).unwrap_or_else(|| s.d.local_user.clone())
}

pub async fn local(State(s): State<S>, ci: Ci, Query(q): Query<HashMap<String, String>>) -> R {
    let ip = client_ip(&ci);
    if s.d.local_loopback_only && !ci.0.ip().to_canonical().is_loopback() {
        return Err(ApiError::forbidden("Yerel giriş yalnızca cihazın kendisinden yapılabilir."));
    }
    if let Some(r) = rate_limited(&s, &ip) {
        return Ok(r);
    }
    let tok = q.get("token").map(String::as_str).unwrap_or("");
    if s.d.local_token.is_empty() || !auth::constant_eq(tok.as_bytes(), s.d.local_token.as_bytes()) {
        s.d.limiter.fail(&ip);
        return Err(ApiError::forbidden("Geçersiz yerel belirteç."));
    }
    // İlk açılış: oturum açılmaz, kabuk kurulum sihirbazını gösterir.
    if !s.setup_done() {
        return Ok(redirect(StatusCode::FOUND, "/?ui=desktop&local=1"));
    }
    let user = local_user(&s);
    let admin = s.admin_of(&user, true);
    let (_, cookie) = s.d.sessions.create(&user, admin);
    // Pi'nin kendi ekranı (kiosk): masaüstü arayüzü; uzak tarayıcılar Web arayüzünü alır.
    Ok(with_cookie(redirect(StatusCode::FOUND, "/?ui=desktop&local=1"), &s.d.sessions.set_cookie(&cookie)))
}

pub async fn session(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    Ok(json(&UserResp { user: sess.user, admin: sess.admin, setup_done: Some(s.setup_done()) }))
}

// --- system ----------------------------------------------------------------

pub async fn system_info(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.system.info().await?))
}

pub async fn set_hostname(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = admin_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        hostname: String,
    }
    let req: Req = decode(&body)?;
    s.d.system.set_hostname(&req.hostname).await?;
    tracing::info!(hostname = %req.hostname, by = %sess.user, "cihaz adı değişti");
    Ok(no_content())
}

pub async fn get_time(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.system.time().await?))
}

pub async fn set_time(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    admin_of(&s, &h)?;
    let req: system::TimeUpdate = decode(&body)?;
    s.d.system.set_time(req).await?;
    Ok(json(&s.d.system.time().await?))
}

pub async fn timezones(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.system.timezones().await?))
}

pub async fn get_locale(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.system.locale().await?))
}

pub async fn set_locale(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    admin_of(&s, &h)?;
    let req: system::Locale = decode(&body)?;
    if req.lang.is_empty() && req.keymap.is_empty() {
        return Err(ApiError::invalid("lang veya keymap gerekli."));
    }
    s.d.system.set_locale(req).await?;
    Ok(json(&s.d.system.locale().await?))
}

pub async fn set_password(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        old: String,
        #[serde(default)]
        new: String,
    }
    let req: Req = decode(&body)?;
    s.d.system.set_password(&sess.user, &req.old, &req.new).await?;
    tracing::info!(user = %sess.user, "parola değişti");
    Ok(no_content())
}

pub async fn power(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = admin_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        action: String,
    }
    let req: Req = decode(&body)?;
    if req.action != "reboot" && req.action != "poweroff" {
        return Err(ApiError::invalid("action reboot veya poweroff olmalıdır."));
    }
    tracing::info!(action = %req.action, by = %sess.user, "güç işlemi");
    // Yanıt istemciye ulaşsın diye işlem kısa gecikmeyle yapılır.
    let s2 = s.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        match tokio::time::timeout(Duration::from_secs(20), s2.d.system.power(&req.action)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!(err = %e, "güç işlemi başarısız");
                s2.d.notify.add("makeros.settings", "Güç işlemi başarısız", &e.message);
            }
            Err(_) => {
                tracing::error!("güç işlemi zaman aşımı");
                s2.d.notify.add("makeros.settings", "Güç işlemi başarısız", "İşlem zaman aşımına uğradı.");
            }
        }
    });
    Ok(StatusCode::ACCEPTED.into_response())
}

// --- net -------------------------------------------------------------------

pub async fn net_status(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.network.status().await?))
}

fn net_busy() -> ApiError {
    ApiError::conflict("Bir ağ işlemi devam ediyor. İşlem durumunu bekleyin.")
}

async fn do_scan(s: &Server) -> R {
    let _permit = s.net_busy.clone().try_acquire_owned().map_err(|_| net_busy())?;
    match tokio::time::timeout(Duration::from_secs(30), s.d.network.scan()).await {
        Ok(r) => Ok(json(&r?)),
        Err(_) => Err(ApiError::unavailable("İşlem zaman aşımına uğradı.")),
    }
}

pub async fn wifi_scan(State(s): State<S>, h: HeaderMap) -> R {
    admin_of(&s, &h)?;
    do_scan(&s).await
}

pub async fn wifi_connect(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = admin_of(&s, &h)?;
    let req: ConnectRequest = decode(&body)?;
    if !valid_ssid(&req.ssid) {
        return Err(ApiError::invalid("Ağ adı 1–32 bayt olmalıdır; Türkçe karakterler birden çok bayt kullanabilir."));
    }
    if req.eap.as_ref().is_some_and(|e| e.identity.is_empty()) {
        return Err(ApiError::invalid("Kurumsal ağ için kimlik (identity) gerekli."));
    }
    let id = start_connect(&s, &sess.user, req)?;
    Ok(json_status(StatusCode::ACCEPTED, &HashMap::from([("job", id)])))
}

/// Wi-Fi bağlanma işini başlatır (aynı anda tek ağ işlemi).
fn start_connect(s: &S, user: &str, req: ConnectRequest) -> ApiResult<String> {
    let permit = s.net_busy.clone().try_acquire_owned().map_err(|_| net_busy())?;
    let s2 = s.clone();
    Ok(s.d.jobs.start("net.connect", user, move |h| async move {
        let _permit = permit;
        // HTTP yanıtı radyo değişmeden istemciye ulaşsın.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let ssid = req.ssid.clone();
        let step = move |p: i32, l: &str| h.step(p, l);
        let res = s2.d.network.connect(req, &step).await;
        match &res {
            Err(_) => {
                s2.d.notify.add("makeros.settings", "Wi-Fi bağlantısı başarısız", &ssid);
            }
            Ok(()) => {
                s2.d.notify.add("makeros.settings", "Wi-Fi bağlandı", &format!("{ssid} ağına bağlanıldı."));
                if let Ok(st) = s2.d.network.status().await {
                    s2.d.hub.publish("net", st);
                }
            }
        }
        res.map_err(|e| e.message)
    }))
}

pub async fn wifi_forget(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    admin_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        ssid: String,
    }
    let req: Req = decode(&body)?;
    if !valid_ssid(&req.ssid) {
        return Err(ApiError::invalid("Geçersiz ağ adı."));
    }
    s.d.network.forget(&req.ssid).await?;
    Ok(no_content())
}

pub async fn wifi_enable(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = admin_of(&s, &h)?;
    #[derive(Deserialize, Default)]
    struct Req {
        enabled: Option<bool>,
    }
    let req: Req = decode(&body)?;
    let en = req.enabled.ok_or_else(|| ApiError::invalid("enabled (true/false) gerekli."))?;
    s.d.network.set_wifi_enabled(en).await?;
    tracing::info!(enabled = en, by = %sess.user, "Wi-Fi radyosu");
    let st = s.d.network.status().await?;
    s.d.hub.publish("net", &st);
    Ok(json(&st))
}

// --- prefs -----------------------------------------------------------------

// Tercihler kullanıcının ev dizininde (~/.config/makeros/prefs.json; bkz. prefs.rs). Eski kurulumlarda
// <data-dir>/prefs/<kullanıcı>.json'da durur: ilk okumada ev dizinine taşınır.

pub async fn get_prefs(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    let old = s.d.prefs.as_ref();
    let Some(a) = &s.d.agent else {
        let p = old.ok_or_else(|| ApiError::unavailable("Tercih deposu yok."))?;
        return Ok(raw_json(p.get(&sess.user)?));
    };
    if let Ok(b) = crate::uagent::read_all(a.as_ref(), &sess.user, prefs::HOME_PATH).await {
        if prefs::valid(&b) {
            return Ok(raw_json(b));
        }
    }
    // Taşıma: eski depodaki tercihler ev dizinine yazılır.
    let b = old.and_then(|p| p.get(&sess.user).ok()).unwrap_or_else(|| b"{}".to_vec());
    if b != b"{}" {
        match crate::uagent::write_all(a.as_ref(), &sess.user, prefs::HOME_PATH, b.clone()).await {
            Ok(()) => tracing::info!(user = %sess.user, "tercihler ev dizinine taşındı"),
            Err(e) => tracing::warn!(user = %sess.user, "tercihler taşınamadı: {}", e.message),
        }
    }
    Ok(raw_json(b))
}

pub async fn put_prefs(State(s): State<S>, h: HeaderMap, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    let compact = prefs::normalize(&body)?;
    match &s.d.agent {
        Some(a) => crate::uagent::write_all(a.as_ref(), &sess.user, prefs::HOME_PATH, compact.clone()).await?,
        None => {
            s.d.prefs.as_ref().ok_or_else(|| ApiError::unavailable("Tercih deposu yok."))?.put(&sess.user, &compact)?;
        }
    }
    Ok(raw_json(compact))
}

// --- uygulamalar, süreçler, bildirimler, işler ---------------------------------

pub async fn apps_list(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.apps.list()))
}

pub async fn app_launch(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    session_of(&s, &h)?;
    s.d.apps.start(&id).await?;
    Ok(StatusCode::ACCEPTED.into_response())
}

pub async fn processes(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.activity.processes().await?))
}

pub async fn signal(State(s): State<S>, h: HeaderMap, Path(pid): Path<String>, body: Bytes) -> R {
    session_of(&s, &h)?;
    let pid: i32 = pid.parse().ok().filter(|p| *p > 0).ok_or_else(|| ApiError::invalid("Geçersiz süreç numarası."))?;
    #[derive(Deserialize, Default)]
    struct Req {
        #[serde(default)]
        signal: String,
    }
    let req: Req = decode(&body)?;
    s.d.activity.signal(pid, &req.signal).await?;
    Ok(no_content())
}

pub async fn notifications(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&s.d.notify.list()))
}

pub async fn notification_read(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    session_of(&s, &h)?;
    if !s.d.notify.mark_read(&id) {
        return Err(ApiError::not_found("Bildirim bulunamadı."));
    }
    Ok(no_content())
}

pub async fn notifications_clear(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    s.d.notify.clear();
    Ok(no_content())
}

pub async fn job(State(s): State<S>, h: HeaderMap, Path(id): Path<String>) -> R {
    let sess = session_of(&s, &h)?;
    match s.d.jobs.get(&id) {
        Some(j) if j.user == sess.user => Ok(json(&j)),
        _ => Err(ApiError::not_found("İş bulunamadı.")),
    }
}

// --- ilk kurulum -----------------------------------------------------------------

/// Kurulum bitmişse 409; istek yerel ağdan değilse 403. Kimlik gerekmez.
fn setup_only(s: &Server, ci: &Ci) -> ApiResult<()> {
    if s.d.setup.is_none() || s.setup_done() {
        return Err(ApiError::conflict("Kurulum zaten tamamlandı."));
    }
    let ip = client_ip(ci);
    if !setup::local_addr(&ip) {
        tracing::warn!(ip = %ip, "kurulum isteği yerel ağ dışından reddedildi");
        return Err(ApiError::forbidden("Kurulum yalnızca yerel ağdan veya cihazın erişim noktasından yapılabilir."));
    }
    Ok(())
}

#[derive(Serialize)]
struct SetupDefaults {
    lang: String,
    keymap: String,
    timezone: String,
}

#[derive(Serialize)]
struct SetupState {
    setup_done: bool,
    hostname: String,
    defaults: SetupDefaults,
    languages: &'static [system::Opt],
    keymaps: &'static [system::Opt],
    timezones: Vec<String>,
    online: bool,
    wifi_needed: bool,
    hotspot: bool,
    min_password: usize,
}

pub async fn setup_state(State(s): State<S>, ci: Ci) -> R {
    setup_only(&s, &ci)?;
    let mut r = SetupState {
        setup_done: false,
        hostname: "makeros".into(),
        defaults: SetupDefaults { lang: "tr_TR.UTF-8".into(), keymap: "trq".into(), timezone: "Europe/Istanbul".into() },
        languages: system::LANGUAGES,
        keymaps: system::KEYMAPS,
        timezones: Vec::new(),
        online: false,
        wifi_needed: false,
        hotspot: false,
        min_password: privsep::MIN_PASSWORD_LEN,
    };
    if let Ok(l) = s.d.system.locale().await {
        if !l.lang.is_empty() && l.lang != "C.UTF-8" {
            r.defaults.lang = l.lang;
        }
        if !l.keymap.is_empty() && l.keymap != "us" {
            r.defaults.keymap = l.keymap;
        }
    }
    if let Ok(t) = s.d.system.time().await {
        if !t.timezone.is_empty() && t.timezone != "UTC" {
            r.defaults.timezone = t.timezone;
        }
    }
    r.timezones = s.d.system.timezones().await.unwrap_or_else(|_| system::FALLBACK_TIMEZONES.iter().map(|z| z.to_string()).collect());
    if let Ok(st) = s.d.network.status().await {
        r.online = st.online;
        r.hotspot = st.hotspot.active;
        r.wifi_needed = !st.online || st.hotspot.active;
    }
    r.hostname = setup::DEFAULT_HOSTNAME.into();
    Ok(json(&r))
}

pub async fn setup_wifi_scan(State(s): State<S>, ci: Ci) -> R {
    setup_only(&s, &ci)?;
    do_scan(&s).await
}

#[derive(Deserialize, Default)]
struct SetupRequest {
    #[serde(default)]
    user: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    fullname: String,
    #[serde(default)]
    hostname: String,
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    keymap: String,
    #[serde(default)]
    wifi: Option<ConnectRequest>,
}

impl SetupRequest {
    fn validate(&self) -> ApiResult<()> {
        let bad = |m: &str| Err(ApiError::invalid(m));
        if !privsep::users::valid_username(&self.user) {
            return bad("Kullanıcı adı küçük harfle başlamalı; yalnızca a-z, 0-9, '-' ve '_' içerebilir (en fazla 32).");
        }
        if !privsep::users::valid_new_password(&self.password) {
            return bad("Parola en az 6 karakter olmalıdır.");
        }
        if !privsep::users::valid_fullname(&self.fullname) {
            return bad("Ad soyad geçersiz karakter içeriyor.");
        }
        if !privsep::valid_hostname(&self.hostname) {
            return bad("Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir (en fazla 63).");
        }
        if !system::valid_timezone(&self.timezone) {
            return bad("Geçersiz saat dilimi.");
        }
        if !system::valid_lang(&self.lang) {
            return bad("Geçersiz dil.");
        }
        if !system::valid_keymap(&self.keymap) {
            return bad("Geçersiz klavye düzeni.");
        }
        if self.wifi.as_ref().is_some_and(|w| !valid_ssid(&w.ssid)) {
            return bad("Ağ adı 1–32 bayt olmalıdır.");
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct SetupResp {
    user: String,
    admin: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    job: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

/// Tek seferlik: hesap → cihaz adı/saat/dil → setup-done → oturum çerezi;
/// Wi-Fi isteniyorsa yanıttan sonra iş olarak bağlanır.
pub async fn setup_complete(State(s): State<S>, ci: Ci, body: Bytes) -> R {
    setup_only(&s, &ci)?;
    let ip = client_ip(&ci);
    if let Some(r) = rate_limited(&s, &ip) {
        return Ok(r);
    }
    let req: SetupRequest = decode(&body)?;
    req.validate()?;
    let accounts = s.d.accounts.clone().ok_or_else(|| ApiError::unavailable("Hesap oluşturucu yok."))?;
    let store = s.d.setup.clone().ok_or_else(|| ApiError::conflict("Kurulum zaten tamamlandı."))?;
    // İstemci koparsa da kurulum yarıda kalmasın: ayrı görevde çalışır.
    let s2 = s.clone();
    tokio::spawn(async move {
        let _guard = match store.begin() {
            Ok(g) => g,
            Err(BeginError::Done) => return Err(ApiError::conflict("Kurulum zaten tamamlandı.")),
            Err(BeginError::Busy) => return Err(ApiError::conflict("Kurulum şu anda sürüyor.")),
            Err(BeginError::Io(e)) => return Err(ApiError::internal(format!("Kurulum kilidi alınamadı: {e}"))),
        };
        // Yalnız hesap oluşturma süre sınırına tabidir; ardından setup-done hemen yazılır. Böylece
        // yavaş/askıda bir D-Bus hizmeti (ilk açılışta soğuk polkit, hostnamed...) kurulumu sonsuza
        // dek "bitmemiş" bırakıp sihirbazı başa döndüremez.
        let args = privsep::UserCreateArgs { user: req.user.clone(), password: req.password.clone(), fullname: req.fullname.clone(), ..Default::default() };
        match tokio::time::timeout(Duration::from_secs(45), accounts.create_user(args)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!(user = %req.user, err = %e, "kurulum: hesap oluşturulamadı");
                s2.d.limiter.fail(&ip);
                return Err(e);
            }
            Err(_) => {
                tracing::error!(user = %req.user, "kurulum: hesap oluşturma zaman aşımı");
                return Err(ApiError::unavailable("Hesap oluşturma zaman aşımına uğradı; lütfen yeniden deneyin."));
            }
        }
        if let Err(e) = store.mark_done(&req.user) {
            tracing::error!(err = %e, "kurulum: setup-done yazılamadı");
            return Err(ApiError::internal("Kurulum durumu kaydedilemedi."));
        }
        let work = async {
            // Ayarlar birbirinden bağımsız: paralel ve her biri kısa süre sınırıyla; hata yalnız uyarıdır.
            const STEP: Duration = Duration::from_secs(12);
            let step = |what: &'static str, r: Result<ApiResult<()>, tokio::time::error::Elapsed>| match r {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(format!("{what} ayarlanamadı: {}", err_msg(&e))),
                Err(_) => Some(format!("{what} ayarlanamadı: zaman aşımı")),
            };
            let (h, t, l) = tokio::join!(
                tokio::time::timeout(STEP, s2.d.system.set_hostname(&req.hostname)),
                tokio::time::timeout(STEP, s2.d.system.set_time(system::TimeUpdate { timezone: Some(req.timezone.clone()), ..Default::default() })),
                tokio::time::timeout(STEP, s2.d.system.set_locale(system::Locale { lang: req.lang.clone(), keymap: req.keymap.clone() })),
            );
            let warn: Vec<String> = [step("Cihaz adı", h), step("Saat dilimi", t), step("Dil/klavye", l)].into_iter().flatten().collect();
            s2.d.limiter.reset(&ip);
            let admin = s2.admin_of(&req.user, true);
            let (_, cookie) = s2.d.sessions.create(&req.user, admin);
            let mut resp = SetupResp { user: req.user.clone(), admin, job: String::new(), warnings: warn.clone() };
            if let Some(w) = req.wifi.clone() {
                match start_connect(&s2, &req.user, w) {
                    Ok(id) => resp.job = id,
                    Err(e) => resp.warnings.push(format!("Wi-Fi bağlantısı başlatılamadı: {}", err_msg(&e))),
                }
            }
            for m in &warn {
                tracing::warn!(msg = %m, "kurulum uyarısı");
            }
            tracing::info!(user = %req.user, hostname = %req.hostname, ip = %ip, "kurulum tamamlandı");
            Ok(with_cookie(json(&resp), &s2.d.sessions.set_cookie(&cookie)))
        };
        work.await
    })
    .await
    .unwrap_or_else(|_| Err(ApiError::internal("İstek tamamlanamadı.")))
}

// --- yazılım güncellemesi ---------------------------------------------------------

fn updater(s: &Server) -> ApiResult<Arc<ota::Updater>> {
    s.d.update.clone().ok_or_else(|| ApiError::unavailable("Yazılım güncellemesi bu sistemde yok."))
}

pub async fn update_status(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    Ok(json(&updater(&s)?.status()))
}

pub async fn update_check(State(s): State<S>, h: HeaderMap) -> R {
    session_of(&s, &h)?;
    let u = updater(&s)?;
    // GitHub API'si kimliksiz istemciye saatte 60 istek verir: bir dakikadan yeni denetim yeniden yapılmaz.
    let st = u.status();
    let recent = st.checked.is_some_and(|c| jiff::Timestamp::now().duration_since(c) < jiff::SignedDuration::from_secs(60));
    if recent && st.error.is_empty() {
        return Ok(json(&st));
    }
    match u.check().await {
        Ok(st) => Ok(json(&st)),
        Err(_) => Err(ApiError::unavailable(u.status().error)),
    }
}

pub async fn update_apply(State(s): State<S>, h: HeaderMap) -> R {
    let sess = admin_of(&s, &h)?;
    let u = updater(&s)?;
    let st = u.status();
    let Some(av) = st.available else { return Err(ApiError::conflict("Kurulacak yeni bir sürüm yok; önce denetleyin.")) };
    if st.phase != "idle" && st.phase != "error" {
        return Err(ApiError::conflict("Bir güncelleme zaten sürüyor."));
    }
    let id = s.d.jobs.start_for("update", &sess.user, Duration::from_secs(45 * 60), move |h| async move {
        let id = h.id().to_string();
        let step = move |p: i32, l: &str| h.step(p, l);
        u.apply(&id, &step).await
    });
    tracing::info!(version = %av.version, user = %sess.user, "yazılım güncellemesi başladı");
    Ok(json_status(StatusCode::ACCEPTED, &HashMap::from([("job", id)])))
}

// --- Tarayıcı penceresi: sayfa aktarımı (crate::browse) ------------------------

/// Oturum sahibine aktarım bileti (iframe içindeki sayfa çerez göndermez; bkz. browse.rs).
pub async fn browse_ticket(State(s): State<S>, h: HeaderMap) -> R {
    let sess = session_of(&s, &h)?;
    #[derive(Serialize)]
    struct Resp {
        ticket: String,
        /// Yeni sekmede açılacak (bot korumalı) alan adları.
        walled: Vec<String>,
    }
    Ok(json(&Resp { ticket: crate::browse::ticket(&sess.user), walled: crate::browse::walled_hosts() }))
}

pub async fn browse(Query(q): Query<HashMap<String, String>>, h: HeaderMap) -> R {
    let t = q.get("t").map(String::as_str).unwrap_or("");
    let url = q.get("url").cloned().unwrap_or_default();
    let Some(user) = crate::browse::check(t) else {
        // Bilet bellekte tutulur: makerd yeniden başlayınca eskir. JSON yerine kabuğa haber veren bir sayfa döner,
        // Tarayıcı penceresi yeni bilet alıp sayfayı kendiliğinden yeniden yükler.
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header("content-security-policy", "sandbox allow-scripts")
            .body(Body::from(crate::browse::expired_page(&url)))
            .map_err(|e| ApiError::internal(e.to_string()));
    };
    let lang = h.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let (status, ct, body) = match crate::browse::fetch(&url, lang).await {
        Ok(p) => {
            let html = p.content_type.contains("text/html") || p.content_type.contains("xhtml");
            tracing::debug!(user = %user, url = %p.url, status = p.status, "tarayıcı");
            let body = if html { crate::browse::rewrite(&p.body, &p.url, t) } else { p.body };
            (StatusCode::from_u16(p.status).unwrap_or(StatusCode::OK), p.content_type, body)
        }
        Err(e) => (StatusCode::BAD_GATEWAY, "text/html; charset=utf-8".to_string(), crate::browse::error_page(&url, &e)),
    };
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&ct).unwrap_or(HeaderValue::from_static("application/octet-stream")))
        // Opak köken: aktarılan sayfa MakerOS ile aynı adreste olsa da kabuğa/API'ye erişemez.
        .header("content-security-policy", "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-modals allow-downloads")
        .body(Body::from(body))
        .map_err(|e| ApiError::internal(e.to_string()))
}

// --- Duvar kâğıtları: kullanıcının yüklediği resimler (cihaz geneli) -------------------------
// <data-dir>/wallpapers/{desktop,desktop-blur,lock}. Kilit ekranı girişten önce de görünmeli: GET herkese açık
// (yalnız bu üç sabit ad; resim baytları olduğu gibi, image/* türüyle). Yükleme/silme oturum ister.

const WALL_NAMES: [&str; 3] = ["desktop", "desktop-blur", "lock"];
const WALL_MAX: usize = 12 << 20;

fn wall_path(s: &S, name: &str) -> ApiResult<std::path::PathBuf> {
    if !WALL_NAMES.contains(&name) {
        return Err(ApiError::not_found("Duvar kâğıdı yok."));
    }
    let p = s.d.prefs.as_ref().ok_or_else(|| ApiError::unavailable("Veri dizini yok."))?;
    Ok(p.dir.parent().unwrap_or(&p.dir).join("wallpapers").join(name))
}

/// Resim türü baytlardan (uzantıya/başlığa güvenilmez).
fn image_type(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub async fn wallpaper_get(State(s): State<S>, Path(name): Path<String>) -> R {
    let path = wall_path(&s, &name)?;
    let b = tokio::fs::read(&path).await.map_err(|_| ApiError::not_found("Duvar kâğıdı yok."))?;
    let ct = image_type(&b).unwrap_or("application/octet-stream");
    Response::builder()
        .header(header::CONTENT_TYPE, ct)
        // Adres sürüm içerir (?v=) ama kilit ekranı sürümsüz de istenebilir: her seferinde doğrula.
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(b))
        .map_err(|e| ApiError::internal(e.to_string()))
}

pub async fn wallpaper_put(State(s): State<S>, h: HeaderMap, Path(name): Path<String>, body: Bytes) -> R {
    let sess = session_of(&s, &h)?;
    let path = wall_path(&s, &name)?;
    if body.len() > WALL_MAX {
        return Err(ApiError::invalid("Resim çok büyük (en fazla 12 MB)."));
    }
    if image_type(&body).is_none() {
        return Err(ApiError::invalid("Yalnız JPEG, PNG ya da WebP resim yüklenebilir."));
    }
    if let Some(d) = path.parent() {
        tokio::fs::create_dir_all(d).await.map_err(|e| ApiError::internal(e.to_string()))?;
    }
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, &body).await.map_err(|e| ApiError::internal(e.to_string()))?;
    tokio::fs::rename(&tmp, &path).await.map_err(|e| ApiError::internal(e.to_string()))?;
    tracing::info!(user = %sess.user, name = %name, bytes = body.len(), "duvar kâğıdı yüklendi");
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn wallpaper_delete(State(s): State<S>, h: HeaderMap, Path(name): Path<String>) -> R {
    session_of(&s, &h)?;
    let path = wall_path(&s, &name)?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(ApiError::internal(e.to_string())),
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}
