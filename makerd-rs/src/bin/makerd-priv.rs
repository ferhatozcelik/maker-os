//! makerd-priv: kök yetkili dar yardımcı. Yalnız sabit komutları (auth.check,
//! system.password, system.hostname, power, user.create, ota.*) izinli
//! UID'lerden kabul eder. systemd soket etkinleştirmesiyle çalışır ve boşta kalınca çıkar.

use makerd::privsep::{commands::Commands, server::Server, DEFAULT_SOCKET};
use makerd::{flags::Flags, logging, sd, util};
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "Kullanım: makerd-priv [bayraklar]
  --socket YOL        soket yolu (soket devri yoksa) (varsayılan /run/makeros/priv.sock)
  --allow-user AD     izinli çağıran kullanıcılar (virgülle); root her zaman izinli (varsayılan maker)
  --socket-group AD   kendi açtığı soketin grubu (varsayılan maker)
  --shadow YOL        shadow dosyası (varsayılan /etc/shadow)
  --setup-flag YOL    bu dosya varsa user.create reddedilir (varsayılan /var/lib/makerd/setup-done)
  --idle SÜRE         soket devrinde boşta çıkış süresi (0: çıkma) (varsayılan 30s)
  --version           sürümü yaz ve çık";

fn main() {
    let f = match Flags::parse(
        std::env::args().skip(1),
        &["socket", "allow-user", "socket-group", "shadow", "setup-flag", "idle"],
        &["version"],
    ) {
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
        println!("makerd-priv {}", makerd::VERSION);
        return;
    }
    let idle = f.duration("idle", Duration::from_secs(30)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    logging::init("info", "text");

    let mut allowed: HashSet<u32> = [0].into();
    for name in f.str("allow-user", "maker").split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match util::uid_of(name) {
            Some(id) => {
                allowed.insert(id);
            }
            None => tracing::warn!(user = name, "izinli kullanıcı bulunamadı"),
        }
    }

    // Soket devri ortam değişkenlerini temizler: çalışma zamanı kurulmadan yapılır.
    let fds = sd::listen_fds();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap_or_else(|e| fail(&e.to_string()));
    let res = rt.block_on(async move {
        let (ln, idle_exit, addr) = match fds.into_iter().next() {
            Some(fd) => {
                let std_ln = std::os::unix::net::UnixListener::from(fd);
                std_ln.set_nonblocking(true)?;
                (tokio::net::UnixListener::from_std(std_ln)?, idle, "systemd soketi".to_string())
            }
            None => {
                let sock = f.str("socket", DEFAULT_SOCKET);
                (listen(&sock, &f.str("socket-group", "maker"))?, Duration::ZERO, sock)
            }
        };
        let mut cmds = Commands::default();
        cmds.shadow = f.str("shadow", "/etc/shadow");
        cmds.users.setup_flag = f.str("setup-flag", "/var/lib/makerd/setup-done");
        let srv = Arc::new(Server { allowed, handlers: cmds.handlers(), idle: idle_exit });

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
        tracing::info!(addr = %addr, idle = ?idle_exit, version = makerd::VERSION, "makerd-priv hazır");
        srv.serve(ln, stop_rx).await
    });
    if let Err(e) = res {
        fail(&format!("sunucu: {e}"));
    }
}

fn fail(msg: &str) -> ! {
    tracing::error!(err = msg, "makerd-priv durdu");
    std::process::exit(1);
}

/// Soket devri yoksa soketi kendisi açar: 0660, sahip root, grup `group`.
fn listen(sock: &str, group: &str) -> std::io::Result<tokio::net::UnixListener> {
    let _ = std::fs::remove_file(sock);
    if let Some(dir) = Path::new(sock).parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    let ln = std::os::unix::net::UnixListener::bind(sock)?;
    let _ = std::fs::set_permissions(sock, std::fs::Permissions::from_mode(0o660));
    if let Some(gid) = util::gid_of(group) {
        let _ = std::os::unix::fs::chown(sock, Some(0), Some(gid));
    }
    ln.set_nonblocking(true)?;
    tokio::net::UnixListener::from_std(ln)
}
