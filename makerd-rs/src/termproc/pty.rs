//! Yerel PTY: komutu yeni bir PTY'de (ayrı oturum ve süreç grubu) başlatır.

use super::login::CmdSpec;
use super::{exit_code, Ctl, Proc, HANGUP_GRACE};
use bytes::Bytes;
use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::unix::AsyncFd;
use tokio::sync::{mpsc, watch};

nix::ioctl_write_ptr_bad!(set_winsize, libc::TIOCSWINSZ, Winsize);

fn set_flags(fd: &OwnedFd, nonblock: bool) {
    // SAFETY: geçerli tanıtıcı; yalnız bayrak değişikliği.
    unsafe {
        libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        if nonblock {
            let fl = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
    }
}

fn ws(cols: u16, rows: u16) -> Winsize {
    Winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 }
}

/// Süreç grubuna SIGHUP; HANGUP_GRACE içinde çıkmazsa SIGKILL.
pub fn hangup(pid: i32, mut exit: watch::Receiver<Option<i32>>) {
    let _ = kill(Pid::from_raw(-pid), Signal::SIGHUP);
    tokio::spawn(async move {
        let done = async {
            while exit.borrow().is_none() {
                if exit.changed().await.is_err() {
                    break;
                }
            }
        };
        if tokio::time::timeout(HANGUP_GRACE, done).await.is_err() {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
    });
}

/// Komutu verilen boyutta yeni bir PTY'de başlatır.
pub fn spawn(spec: &CmdSpec, cols: u16, rows: u16) -> std::io::Result<Proc> {
    let pty = openpty(Some(&ws(cols, rows)), None).map_err(std::io::Error::from)?;
    let (master, slave) = (pty.master, pty.slave);
    set_flags(&master, true);
    set_flags(&slave, false);
    let mut cmd = spec.std_command();
    cmd.stdin(Stdio::from(slave.try_clone()?)).stdout(Stdio::from(slave.try_clone()?)).stderr(Stdio::from(slave));
    // SAFETY: fork sonrası yalnız async-signal-safe çağrılar (setsid, ioctl).
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = tokio::process::Command::from(cmd).spawn()?;
    let pid = child.id().map(|p| p as i32).unwrap_or(-1);

    let master = Arc::new(AsyncFd::new(master)?);
    let (out_tx, out_rx) = mpsc::channel::<Bytes>(64);
    let (ctl_tx, mut ctl_rx) = mpsc::unbounded_channel::<Ctl>();
    let (exit_tx, exit_rx) = watch::channel(None);

    // Okuyucu: PTY çıktısı (slave kapanınca EIO → bitiş).
    let m = master.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 32 << 10];
        loop {
            let Ok(mut g) = m.readable().await else { break };
            let r = g.try_io(|fd| {
                // SAFETY: geçerli tanıtıcı ve tampon.
                let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
            });
            match r {
                Ok(Ok(0)) | Ok(Err(_)) => break,
                Ok(Ok(n)) => {
                    if out_tx.send(Bytes::copy_from_slice(&buf[..n])).await.is_err() {
                        break;
                    }
                }
                Err(_would_block) => continue,
            }
        }
    });

    // Bekleyici: süreç bitince çıkış kodu.
    tokio::spawn(async move {
        let code = match child.wait().await {
            Ok(st) => exit_code(st),
            Err(_) => -1,
        };
        let _ = exit_tx.send(Some(code));
    });

    // Denetim: girdi, boyut, kapatma.
    let m = master;
    let ex = exit_rx.clone();
    tokio::spawn(async move {
        while let Some(c) = ctl_rx.recv().await {
            match c {
                Ctl::Data(b) => {
                    if write_all(&m, &b).await.is_err() {
                        break;
                    }
                }
                Ctl::Resize(c, r) => {
                    // SAFETY: geçerli PTY tanıtıcısı ve winsize.
                    let _ = unsafe { set_winsize(m.get_ref().as_raw_fd(), &ws(c, r)) };
                }
                Ctl::Hangup => hangup(pid, ex.clone()),
            }
        }
    });

    Ok(Proc { pid, output: out_rx, ctl: ctl_tx, exit: exit_rx })
}

async fn write_all(m: &AsyncFd<OwnedFd>, mut b: &[u8]) -> std::io::Result<()> {
    while !b.is_empty() {
        let mut g = m.writable().await?;
        match g.try_io(|fd| {
            // SAFETY: geçerli tanıtıcı ve tampon.
            let n = unsafe { libc::write(fd.as_raw_fd(), b.as_ptr().cast(), b.len()) };
            if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
        }) {
            Ok(Ok(n)) => b = &b[n..],
            Ok(Err(e)) => return Err(e),
            Err(_) => continue,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_through_pty() {
        let spec = CmdSpec {
            path: "/bin/sh".into(),
            argv0: "sh".into(),
            args: vec!["-c".into(), "read x; echo got:$x; exit 3".into()],
            dir: "/".into(),
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        };
        let mut p = spawn(&spec, 80, 24).unwrap();
        assert!(p.pid > 0);
        p.ctl.send(Ctl::Data(Bytes::from_static(b"merhaba\n"))).unwrap();
        let mut out = Vec::new();
        while let Some(b) = tokio::time::timeout(std::time::Duration::from_secs(5), p.output.recv()).await.unwrap() {
            out.extend_from_slice(&b);
        }
        assert!(String::from_utf8_lossy(&out).contains("got:merhaba"));
        let mut ex = p.exit.clone();
        while ex.borrow().is_none() {
            ex.changed().await.unwrap();
        }
        assert_eq!(*ex.borrow(), Some(3));
    }
}
