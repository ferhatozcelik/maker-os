//! PTY'siz komut: stdin akıştan, stdout+stderr tek borudan; iptal (future düşürülünce)
//! süreç grubuna SIGTERM, süre sonunda SIGKILL.

use super::login::CmdSpec;
use super::{exit_code, HANGUP_GRACE};
use bytes::Bytes;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::os::fd::OwnedFd;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

struct GroupGuard {
    pid: i32,
    done: bool,
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        if self.done || self.pid <= 0 {
            return;
        }
        let pid = self.pid;
        let _ = kill(Pid::from_raw(-pid), Signal::SIGTERM);
        std::thread::spawn(move || {
            std::thread::sleep(HANGUP_GRACE);
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
        });
    }
}

/// Komutu çalıştırır; çıktı parçaları `out`'a gider, `stdin` kapanınca girdi biter.
/// Dönen değer çıkış kodudur.
pub async fn run(spec: &CmdSpec, stdin: Option<mpsc::Receiver<Bytes>>, out: mpsc::Sender<Bytes>) -> std::io::Result<i32> {
    let (r, w): (OwnedFd, OwnedFd) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).map_err(std::io::Error::from)?;
    let mut cmd = spec.std_command();
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.stdout(Stdio::from(w.try_clone()?)).stderr(Stdio::from(w));
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = tokio::process::Command::from(cmd).spawn()?;
    let mut guard = GroupGuard { pid: child.id().map(|p| p as i32).unwrap_or(-1), done: false };
    if let (Some(mut rx), Some(mut sin)) = (stdin, child.stdin.take()) {
        tokio::spawn(async move {
            while let Some(b) = rx.recv().await {
                if sin.write_all(&b).await.is_err() {
                    break;
                }
            }
        });
    }
    let mut rd = tokio::net::unix::pipe::Receiver::from_owned_fd(r)?;
    let mut buf = vec![0u8; 32 << 10];
    loop {
        match rd.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.send(Bytes::copy_from_slice(&buf[..n])).await.is_err() {
                    break;
                }
            }
        }
    }
    let st = child.wait().await?;
    guard.done = true;
    Ok(exit_code(st))
}

/// Komutu çalıştırıp tüm çıktıyı toplar.
pub async fn output(spec: &CmdSpec) -> std::io::Result<(Vec<u8>, i32)> {
    let (tx, mut rx) = mpsc::channel::<Bytes>(16);
    let collect = tokio::spawn(async move {
        let mut v = Vec::new();
        while let Some(b) = rx.recv().await {
            v.extend_from_slice(&b);
        }
        v
    });
    let code = run(spec, None, tx).await?;
    let out = collect.await.unwrap_or_default();
    Ok((out, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(c: &str) -> CmdSpec {
        CmdSpec { path: "/bin/sh".into(), argv0: "sh".into(), args: vec!["-c".into(), c.into()], dir: "/".into(), env: vec![("PATH".into(), "/usr/bin:/bin".into())] }
    }

    #[tokio::test]
    async fn combined_output_and_code() {
        let (o, c) = output(&sh("echo a; echo b >&2; exit 4")).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&o), "a\nb\n");
        assert_eq!(c, 4);
    }

    #[tokio::test]
    async fn stdin_stream() {
        let (itx, irx) = mpsc::channel(4);
        let (otx, mut orx) = mpsc::channel(4);
        let h = tokio::spawn(async move { run(&sh("tr a-z A-Z"), Some(irx), otx).await });
        itx.send(Bytes::from_static(b"maker")).await.unwrap();
        drop(itx);
        let mut s = Vec::new();
        while let Some(b) = orx.recv().await {
            s.extend_from_slice(&b);
        }
        assert_eq!(s, b"MAKER");
        assert_eq!(h.await.unwrap().unwrap(), 0);
    }
}
