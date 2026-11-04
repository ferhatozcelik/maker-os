//! makerd: MakerOS 2 arka uç hizmeti (yetkisiz kullanıcı olarak çalışır).

use futures_util::future::BoxFuture;
use makerd::apierr::{ApiError, ApiResult};
use makerd::flags::Flags;
use makerd::httpd::{Accounts, Deps, Server, TOPICS};
use makerd::privsep::{self, client::Client, AuthCheckArgs, AuthCheckResult, UserCreateArgs, UserCreateResult};
use makerd::termproc::login;
use makerd::{activity, apps, auth, browser, camera, events, gpio, jobs, logging, mock, network, notify, ota, prefs, screen, sd, setup, system, term, uagent, util};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const USAGE: &str = "Kullanım: makerd [bayraklar]
  --listen ADR          dinleme adresi(leri), virgülle (systemd soket devri varsa yok sayılır) (varsayılan :80)
  --web DİZİN           web kabuk dizini (varsayılan /usr/share/makeros/shell)
  --mock                sahte veri arka uçları (kök/NM/D-Bus gerekmez)
  --data-dir DİZİN      kalıcı veri dizini (varsayılan /var/lib/makerd; mock: geçici dizin)
  --token-file YOL      yerel kabuk belirteci (varsayılan /run/makeros/token; mock: <data-dir>/token)
  --priv-socket YOL     makerd-priv soketi (varsayılan /run/makeros/priv.sock)
  --user-socket-dir D   makerd-user@ soketleri (<uid>.sock) (varsayılan /run/makeros/user)
  --log-level SEVİYE    debug|info|warn|error (varsayılan info)
  --log-format BİÇİM    text|json (varsayılan text)
  --vnc ADR             yerel VNC sunucusu (wayvnc); boş: uzak masaüstü kapalı (varsayılan 127.0.0.1:5900)
  --update-repo SAHİP/AD  yazılım güncellemeleri için GitHub deposu; boş: kapalı (varsayılan ferhatozcelik/maker-os)
  --version             sürümü yaz ve çık";

struct Opts {
    listen: String,
    web: String,
    mock: bool,
    data_dir: PathBuf,
    token_file: PathBuf,
    priv_socket: String,
    user_dir: String,
    vnc: String,
    update_repo: String,
}

fn main() {
    let known = ["listen", "web", "data-dir", "token-file", "priv-socket", "user-socket-dir", "log-level", "log-format", "vnc", "update-repo"];
    let f = match Flags::parse(std::env::args().skip(1), &known, &["mock", "version"]) {
        Ok(f) => f,
        Err(e) => {
            if e != "yardım" {
                eprintln!("{e}");
            }
            eprintln!("{USAGE}");
            std::process::exit(if e == "yardım" { 0 } else { 2 });
        }
    };
    if f.bool("version") {
        println!("makerd {}", makerd::VERSION);
        return;
    }
    logging::init(&f.str("log-level", "info"), &f.str("log-format", "text"));
    let mock_mode = f.bool("mock");
    let data_dir = match f.str("data-dir", "") {
        d if !d.is_empty() => PathBuf::from(d),
        _ if mock_mode => std::env::temp_dir().join("makerd-mock"),
        _ => PathBuf::from("/var/lib/makerd"),
    };
    let token_file = match f.str("token-file", "") {
        t if !t.is_empty() => PathBuf::from(t),
        _ if mock_mode => data_dir.join("token"),
        _ => PathBuf::from("/run/makeros/token"),
    };
    let _ = std::fs::create_dir_all(&data_dir);
    let o = Opts {
        listen: f.str("listen", ":80"),
        web: f.str("web", "/usr/share/makeros/shell"),
        mock: mock_mode,
        data_dir,
        token_file,
        priv_socket: f.str("priv-socket", privsep::DEFAULT_SOCKET),
        user_dir: f.str("user-socket-dir", term::DEFAULT_USER_DIR),
        vnc: f.str("vnc", "127.0.0.1:5900"),
        update_repo: f.str("update-repo", "ferhatozcelik/maker-os"),
    };
    // Soket devri ortam değişkenlerini temizler: çalışma zamanı kurulmadan yapılır.
    let fds = sd::listen_fds();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap_or_else(|e| fail(&e.to_string()));
    if let Err(e) = rt.block_on(run(o, fds)) {
        fail(&e);
    }
}

fn fail(msg: &str) -> ! {
    tracing::error!(err = msg, "makerd durdu");
    std::process::exit(1);
}

/// Parolayı makerd-priv ile doğrular (yalnız insan hesapları, UID 1000–59999).
struct PrivChecker(Client);

impl auth::Checker for PrivChecker {
    fn check<'a>(&'a self, user: &'a str, password: &'a str) -> BoxFuture<'a, ApiResult<auth::CheckResult>> {
        Box::pin(async move {
            if !privsep::users::valid_username(user) || password.is_empty() {
                return Ok(auth::CheckResult::default());
            }
            let r: AuthCheckResult = self.0.call(privsep::CMD_AUTH_CHECK, &AuthCheckArgs { user: user.into(), password: password.into() }).await.map_err(ApiError::from)?;
            Ok(auth::CheckResult { ok: r.ok, admin: r.ok })
        })
    }
}

/// Kurulum hesabını makerd-priv user.create ile oluşturur.
struct PrivAccounts(Client);

impl Accounts for PrivAccounts {
    fn create_user(&self, a: UserCreateArgs) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let r: UserCreateResult = self.0.call(privsep::CMD_USER_CREATE, &a).await.map_err(ApiError::from)?;
            tracing::info!(user = %a.user, uid = r.uid, created = r.created, makerd_user_service = r.service, "kurulum hesabı");
            Ok(())
        })
    }
}

fn listen_addr(a: &str) -> String {
    let a = a.trim();
    if a.starts_with(':') { format!("0.0.0.0{a}") } else { a.to_string() }
}

async fn run(o: Opts, fds: Vec<std::os::fd::OwnedFd>) -> Result<(), String> {
    let stop = CancellationToken::new();
    let hub = events::Hub::new(TOPICS);
    let jobs = jobs::Manager::new(Some(hub.clone()));
    let notes = notify::Store::new(Some(hub.clone()));
    let local_user = if o.mock { mock::USER.to_string() } else { util::name_of(util::getuid()).unwrap_or_else(|| mock::USER.into()) };
    let token = match auth::write_token(&o.token_file) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(path = %o.token_file.display(), err = %e, "yerel belirteç yazılamadı; /auth/local kapalı");
            String::new()
        }
    };
    let setup_store = Arc::new(setup::Store::new(o.data_dir.clone()));
    let shell = login::login("/etc/passwd", "/bin/sh");

    let (checker, accounts, system, network, activity, term_mgr, agent, update, is_admin, loopback, desktop_dirs, launch, extra): (
        Arc<dyn auth::Checker>,
        Arc<dyn Accounts>,
        Arc<dyn system::Provider>,
        Arc<dyn network::Provider>,
        Arc<dyn activity::Source>,
        term::Manager,
        Arc<dyn uagent::Agent>,
        Arc<ota::Updater>,
        Option<makerd::httpd::IsAdmin>,
        bool,
        Vec<PathBuf>,
        apps::Launcher,
        Vec<apps::App>,
    ) = if o.mock {
        let fresh = std::env::var("MAKERD_MOCK_FRESH").as_deref() == Ok("1");
        let st = if fresh { mock::State::fresh() } else { mock::State::new() };
        if fresh {
            setup_store.reset().map_err(|e| e.to_string())?;
        } else if !setup_store.done() {
            setup_store.mark_done(mock::USER).map_err(|e| e.to_string())?;
        }
        notes.add("makeros.settings", "MakerOS'a hoş geldiniz", "Bu bir önizlemedir (mock mod).");
        if fresh {
            tracing::warn!("MOCK MODU (ilk açılış): kurulum sihirbazı bekleniyor (MAKERD_MOCK_FRESH=1)");
        } else {
            tracing::warn!(user = mock::USER, password = mock::PASSWORD, "MOCK MODU: sahte veriler kullanılıyor");
        }
        // Docker önizlemesi: güncelleme GitHub'dan gerçekten denetlenir, kurulum benzetilir.
        let upd = ota::Updater::new(&o.update_repo, makerd::VERSION, o.data_dir.join("ota"), Some(Arc::new(ota::SimInstaller::default())));
        (
            Arc::new(mock::Checker(st.clone())),
            Arc::new(st.clone()),
            Arc::new(mock::System::new(st, makerd::VERSION)),
            Arc::new(mock::Network::new()),
            Arc::new(mock::Activity::new()),
            term::Manager::new(term::local(shell.clone())),
            Arc::new(uagent::Local { command: shell.clone(), home: String::new() }),
            Arc::new(upd),
            None,
            false,
            Vec::new(),
            mock::launcher(Some(apps::system_launcher())),
            mock::native_apps(),
        )
    } else {
        let pc = Client::new(&o.priv_socket);
        let uid_of: Arc<dyn Fn(&str) -> Option<u32> + Send + Sync> = Arc::new(util::uid_of);
        let ss = setup_store.clone();
        let upd = ota::Updater::new(&o.update_repo, makerd::VERSION, privsep::OTA_DIR, Some(Arc::new(ota::PrivInstaller(pc.clone()))));
        // Terminal kullanıcının makerd-user@ hizmetinde açılır; o yoksa yalnız makerd'nin
        // kendi kullanıcısı için süreç içi (kum havuzlu) kabuğa düşülür.
        let local_agent: Arc<dyn uagent::Agent> = Arc::new(uagent::Local { command: shell.clone(), home: String::new() });
        (
            Arc::new(PrivChecker(pc.clone())),
            Arc::new(PrivAccounts(pc.clone())),
            Arc::new(system::Real::new("/", makerd::VERSION, pc)),
            Arc::new(network::Nm::new("/")),
            Arc::new(activity::Proc::new()),
            term::Manager::new(term::remote(PathBuf::from(&o.user_dir), uid_of.clone(), local_user.clone(), Some(term::local(shell.clone())))),
            Arc::new(uagent::Remote { dir: PathBuf::from(&o.user_dir), uid_of, self_user: local_user.clone(), fallback: Some(local_agent) }),
            Arc::new(upd),
            Some(Arc::new(move |u: &str| auth::is_admin("/etc/group", u) || u == ss.primary_user()) as makerd::httpd::IsAdmin),
            true,
            vec!["/usr/share/applications".into(), "/usr/local/share/applications".into()],
            apps::system_launcher(),
            Vec::new(),
        )
    };

    // Donanım: gerçek kamera/GPIO varsa onlar, yoksa (Docker önizleme) benzetim.
    let model = std::fs::read_to_string("/proc/device-tree/model").map(|m| m.trim_end_matches(['\0', '\n']).to_string()).unwrap_or_else(|_| "Raspberry Pi".into());
    let cam_src: Arc<dyn camera::Source> = if !o.mock && util::look_path("rpicam-vid").is_some() { Arc::new(camera::Rpicam) } else { Arc::new(camera::Synthetic) };
    let gpio_p: Arc<dyn gpio::Provider> = match gpio::Real::detect(&model).filter(|_| !o.mock) {
        Some(g) => Arc::new(g),
        None => Arc::new(gpio::Sim::new("Raspberry Pi 4 Model B (benzetim)")),
    };
    let screen_m = (!o.vnc.is_empty()).then(|| {
        // Ekran Modu varsayılan kapalı; açıksa (Ayarlar) wayvnc başlatılır.
        Arc::new(screen::Manager::new(o.data_dir.join("screen-mode"), &o.vnc))
    });
    if let Some(sc) = &screen_m {
        sc.apply().await;
    }
    let update = (!o.update_repo.is_empty()).then_some(update);
    if let Some(u) = &update {
        tokio::spawn(watch_updates(u.clone(), notes.clone(), stop.clone()));
    }
    // Uzak tarayıcı oturumu (MakerOS'taki "Chromium" penceresi).
    let browser_m = Arc::new(browser::Manager::new("127.0.0.1:5901", "/usr/lib/makeros/browser-session", Duration::ZERO));

    let d = Deps {
        web_dir: o.web.clone(),
        sessions: Arc::new(auth::Manager::new()),
        checker,
        limiter: Arc::new(auth::Limiter::new(5, Duration::from_secs(60))),
        is_admin,
        local_user,
        local_token: token,
        local_loopback_only: loopback,
        setup_done: None,
        setup: Some(setup_store),
        accounts: Some(accounts),
        prefs: Some(Arc::new(prefs::Store::new(o.data_dir.join("prefs")))),
        hub,
        jobs,
        notify: notes,
        system,
        network,
        apps: Arc::new(apps::Registry::new(desktop_dirs, &o.web, extra, launch)),
        activity,
        term: term_mgr.clone(),
        agent: Some(agent),
        camera: Some(Arc::new(camera::Hub::new(cam_src))),
        gpio: Some(gpio_p),
        vnc_addr: o.vnc.clone(),
        screen: screen_m.clone(),
        browser: Some(browser_m.clone()),
        update,
    };
    let srv = Server::new(d, stop.clone());

    let mut listeners = Vec::new();
    for fd in fds {
        let l = std::net::TcpListener::from(fd);
        l.set_nonblocking(true).map_err(|e| e.to_string())?;
        listeners.push(tokio::net::TcpListener::from_std(l).map_err(|e| e.to_string())?);
    }
    if listeners.is_empty() {
        for a in o.listen.split(',').filter(|a| !a.trim().is_empty()) {
            listeners.push(tokio::net::TcpListener::bind(listen_addr(a)).await.map_err(|e| format!("{a}: {e}"))?);
        }
    }
    let mut tasks = tokio::task::JoinSet::new();
    for l in listeners {
        tracing::info!(addr = %l.local_addr().map(|a| a.to_string()).unwrap_or_default(), web = %o.web, mock = o.mock, version = makerd::VERSION, "dinleniyor");
        tasks.spawn(srv.clone().serve(l));
    }
    sd::notify("READY=1\nSTATUS=makerd hazır");

    use tokio::signal::unix::{signal, SignalKind};
    let (mut int, mut term_sig) = (signal(SignalKind::interrupt()).map_err(|e| e.to_string())?, signal(SignalKind::terminate()).map_err(|e| e.to_string())?);
    let res = tokio::select! {
        _ = int.recv() => Ok(()),
        _ = term_sig.recv() => Ok(()),
        r = tasks.join_next() => match r {
            Some(Ok(Err(e))) => Err(e.to_string()),
            _ => Ok(()),
        },
    };
    tracing::info!("kapanıyor");
    sd::notify("STOPPING=1");
    stop.cancel();
    term_mgr.close_all();
    browser_m.close().await;
    if let Some(sc) = &screen_m {
        sc.close();
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), async { while tasks.join_next().await.is_some() {} }).await;
    res
}

/// Açılıştan biraz sonra ve 12 saatte bir yeni sürüm denetler; yeni sürüm bir kez bildirilir.
async fn watch_updates(u: Arc<ota::Updater>, notes: notify::Store, stop: CancellationToken) {
    let mut told = String::new();
    let mut wait = Duration::from_secs(120);
    loop {
        tokio::select! {
            _ = stop.cancelled() => return,
            _ = tokio::time::sleep(wait) => {}
        }
        if let Ok(st) = u.check().await {
            if let Some(av) = st.available.filter(|a| a.version != told) {
                told = av.version.clone();
                notes.add("makeros.settings", "Yazılım güncellemesi", &format!("MakerOS {told} kullanılabilir. Ayarlar › Genel › Yazılım güncelleme."));
                tracing::info!(version = %told, "yeni sürüm var");
            }
        }
        wait = Duration::from_secs(12 * 3600);
    }
}
