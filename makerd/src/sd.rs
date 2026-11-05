//! systemd ile etkileşim: sd_notify ve soket etkinleştirme (LISTEN_FDS).

use std::os::fd::{FromRawFd, OwnedFd, RawFd};

/// NOTIFY_SOCKET ayarlıysa systemd'ye durum iletir (ör. "READY=1"); yoksa sessizdir.
pub fn notify(state: &str) {
    let Ok(addr) = std::env::var("NOTIFY_SOCKET") else { return };
    if addr.is_empty() {
        return;
    }
    let Ok(sock) = std::os::unix::net::UnixDatagram::unbound() else { return };
    if let Some(name) = addr.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(a) = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()) {
            let _ = sock.send_to_addr(state.as_bytes(), &a);
        }
    } else {
        let _ = sock.send_to(state.as_bytes(), &addr);
    }
}

/// Devredilen soket tanıtıcıları (fd 3'ten). Devir yoksa boş döner; ortam değişkenleri temizlenir.
pub fn listen_fds() -> Vec<OwnedFd> {
    let pid_ok = std::env::var("LISTEN_PID").ok().and_then(|v| v.parse::<u32>().ok()) == Some(std::process::id());
    let n = std::env::var("LISTEN_FDS").ok().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    if !pid_ok || n <= 0 {
        return Vec::new();
    }
    // SAFETY: tek iş parçacıklı başlangıçta çağrılır (çalışma zamanı kurulmadan önce).
    unsafe {
        std::env::remove_var("LISTEN_PID");
        std::env::remove_var("LISTEN_FDS");
        std::env::remove_var("LISTEN_FDNAMES");
    }
    (3..3 + n)
        .map(|fd: RawFd| {
            // SAFETY: systemd bu tanıtıcıları bu sürece devretti; sahiplik bizde.
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                OwnedFd::from_raw_fd(fd)
            }
        })
        .collect()
}

/// Tanıtıcının soket ailesi (AF_UNIX, AF_INET, AF_INET6).
pub fn socket_family(fd: &OwnedFd) -> i32 {
    use std::os::fd::AsRawFd;
    let mut v: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: geçerli tanıtıcı ve doğru boyutlu çıktı tamponu.
    let r = unsafe { libc::getsockopt(fd.as_raw_fd(), libc::SOL_SOCKET, libc::SO_DOMAIN, (&mut v as *mut libc::c_int).cast(), &mut len) };
    if r == 0 { v } else { -1 }
}
