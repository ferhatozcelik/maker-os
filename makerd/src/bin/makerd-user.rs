//! makerd-user: kullanıcı başına hizmet (makerd-user@<kullanıcı>.service,
//! User=<kullanıcı>). /run/makeros/user/<uid>.sock üzerinde dinler ve makerd'nin
//! açtığı her bağlantı için kullanıcının giriş kabuğunu bir PTY'de başlatır
//! (ya da kullanıcı adına dosya/komut işlemi yapar). makerd'nin kum havuzunun
//! dışında çalıştığı için terminalde sudo çalışır.

use makerd::termproc::{login, server::Server};
use makerd::{flags::Flags, logging, sd, util};
use std::collections::HashSet;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const USAGE: &str = "Kullanım: makerd-user [bayraklar]
  --socket-dir YOL   soket dizini (<uid>.sock) (varsayılan /run/makeros/user)
  --allow-user AD    bağlanabilecek kullanıcılar (virgülle); root her zaman izinli (varsayılan maker)
  --shell YOL        giriş kabuğu bulunamazsa kullanılacak kabuk (varsayılan /bin/sh)
  --max N            aynı anda en fazla terminal (varsayılan 16)
  --version          sürümü yaz ve çık";

struct Config {
    dir: String,
    allow: String,
    shell: String,
    max: usize,
}

fn main() {
    let f = match Flags::parse(std::env::args().skip(1), &["socket-dir", "allow-user", "shell", "max"], &["version"]) {
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
        println!("makerd-user {}", makerd::VERSION);
        return;
    }
    let max = match f.int("max", 16) {
        Ok(n) if n > 0 => n as usize,
        Ok(_) => 16,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let cfg = Config { dir: f.str("socket-dir", "/run/makeros/user"), allow: f.str("allow-user", "maker"), shell: f.str("shell", "/bin/sh"), max };
    logging::init("info", "text");

    let uid = util::getuid();
    if uid == 0 {
        fail("root olarak çalıştırılamaz; birim User=%i ile başlatmalı");
    }
    // Ortam değişkenleri çalışma zamanı (iş parçacıkları) kurulmadan ayarlanır.
    if std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|v| v.is_empty()) {
        let rd = format!("/run/user/{uid}");
        if Path::new(&rd).is_dir() {
            // SAFETY: henüz tek iş parçacığı var.
            unsafe { std::env::set_var("XDG_RUNTIME_DIR", rd) };
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap_or_else(|e| fail(&e.to_string()));
    if let Err(e) = rt.block_on(run(uid, cfg)) {
        fail(&e.to_string());
    }
}

fn fail(msg: &str) -> ! {
    tracing::error!(err = msg, "makerd-user durdu");
    std::process::exit(1);
}

async fn run(uid: u32, cfg: Config) -> std::io::Result<()> {
    let mut allowed: HashSet<u32> = [0, uid].into();
    for name in cfg.allow.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match util::uid_of(name) {
            Some(id) => {
                allowed.insert(id);
            }
            None => tracing::warn!(user = name, "izinli kullanıcı bulunamadı"),
        }
    }

    let path = PathBuf::from(&cfg.dir).join(format!("{uid}.sock"));
    let ln = listen(&path, uid)?;
    let srv = Arc::new(Server::new(allowed, login::login("/etc/passwd", &cfg.shell), cfg.max, login::home_of("/etc/passwd")));

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let (Ok(mut int), Ok(mut term)) = (signal(SignalKind::interrupt()), signal(SignalKind::terminate())) else { return };
        tokio::select! {
            _ = int.recv() => {}
            _ = term.recv() => {}
        }
        let _ = stop_tx.send(true);
    });

    sd::notify("READY=1");
    tracing::info!(addr = %path.display(), uid, version = makerd::VERSION, "makerd-user hazır");
    let res = srv.serve(ln, stop_rx).await;
    sd::notify("STOPPING=1");
    let _ = std::fs::remove_file(&path);
    res
}

/// Soketi açar. Aynı yolda kendimize ait eski bir soket varsa silinir; başkasına
/// aitse çalışmayı reddeder (dizin yapışkan bitli ve herkese yazılabilir).
fn listen(path: &Path, uid: u32) -> std::io::Result<tokio::net::UnixListener> {
    if let Ok(md) = std::fs::symlink_metadata(path) {
        if md.uid() != uid || !md.file_type().is_socket() {
            return Err(std::io::Error::other(format!("{} başka bir kullanıcıya ait veya soket değil", path.display())));
        }
        std::fs::remove_file(path)?;
    }
    // SAFETY: umask yalnız süreç durumunu değiştirir.
    let old = unsafe { libc::umask(0o111) };
    let ln = std::os::unix::net::UnixListener::bind(path);
    // SAFETY: önceki değere geri dönülür.
    unsafe { libc::umask(old) };
    let ln = ln?;
    // Kimlik denetimi SO_PEERCRED ile yapılır; makerd (başka kullanıcı) bağlanabilmeli.
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666)) {
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    ln.set_nonblocking(true)?;
    tokio::net::UnixListener::from_std(ln)
}
