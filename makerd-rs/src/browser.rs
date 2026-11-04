//! MakerOS'taki "Chromium" penceresinin arkasındaki uzak tarayıcı oturumu: ayrı,
//! monitörsüz bir labwc + Chromium + wayvnc (yalnız 127.0.0.1:5901;
//! /usr/lib/makeros/browser-session). Pi'nin kendi ekranı etkilenmez; Ekran
//! Modu ayarından bağımsızdır. İlk pencere açılınca başlar, son pencere
//! kapandıktan `grace` süre sonra durur (Pi 4'te belleği boşa tutmasın).

use crate::screen::{runtime_dir, systemctl_user};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// systemd kullanıcı birimi (os/board/.../systemd/user/makeros-browser.service).
pub const UNIT: &str = "makeros-browser.service";

#[derive(Default)]
struct Inner {
    refs: usize,
    timer: Option<tokio::task::JoinHandle<()>>,
    pid: Option<i32>,
    systemd: bool,
}

pub struct Manager {
    /// 127.0.0.1:5901
    pub addr: String,
    /// /usr/lib/makeros/browser-session
    pub script: String,
    /// Son pencere kapandıktan sonra bekleme (0: 45 sn).
    pub grace: Duration,
    inner: Arc<Mutex<Inner>>,
}

impl Manager {
    pub fn new(addr: &str, script: &str, grace: Duration) -> Self {
        Self { addr: addr.into(), script: script.into(), grace, inner: Default::default() }
    }

    async fn listening(&self) -> bool {
        matches!(tokio::time::timeout(Duration::from_millis(300), tokio::net::TcpStream::connect(&self.addr)).await, Ok(Ok(_)))
    }

    /// Bir pencere için oturumu hazırlar (gerekirse başlatır) ve VNC portu açılana dek bekler.
    /// Hata dönerse referans alınmamış sayılır (Release çağrılmamalı).
    pub async fn acquire(&self) -> Result<(), String> {
        {
            let mut g = self.inner.lock().await;
            g.refs += 1;
            if let Some(t) = g.timer.take() {
                t.abort();
            }
            if !self.listening().await {
                if let Err(e) = self.start_locked(&mut g).await {
                    g.refs -= 1;
                    return Err(e);
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(25);
        while !self.listening().await {
            if Instant::now() > deadline {
                self.release().await;
                return Err("tarayıcı oturumu zamanında açılmadı".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Ok(())
    }

    /// Bir pencere kapandı; sonuncusuysa `grace` sonra oturum durur.
    pub async fn release(&self) {
        let mut g = self.inner.lock().await;
        g.refs = g.refs.saturating_sub(1);
        if g.refs > 0 {
            return;
        }
        let grace = if self.grace.is_zero() { Duration::from_secs(45) } else { self.grace };
        if let Some(t) = g.timer.take() {
            t.abort();
        }
        let inner = self.inner.clone();
        g.timer = Some(tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            let mut g = inner.lock().await;
            g.timer = None;
            if g.refs == 0 {
                stop_locked(&mut g).await;
            }
        }));
    }

    /// makerd kapanırken oturumu durdurur.
    pub async fn close(&self) {
        let mut g = self.inner.lock().await;
        if let Some(t) = g.timer.take() {
            t.abort();
        }
        stop_locked(&mut g).await;
    }

    async fn start_locked(&self, g: &mut Inner) -> Result<(), String> {
        // Önceki (kendiliğinden bitmiş) oturumdan kalan süreç grubu temizlenir.
        kill_locked(g);
        if systemctl_user("start", UNIT).await.is_ok() {
            g.systemd = true;
            tracing::info!(via = "systemd", "tarayıcı oturumu başladı");
            return Ok(());
        }
        if !std::path::Path::new(&self.script).exists() {
            return Err(format!("tarayıcı oturumu bu sistemde yok ({})", self.script));
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        let mut child = tokio::process::Command::new(&self.script)
            .current_dir(&home)
            .env("XDG_RUNTIME_DIR", runtime_dir())
            .env("HOME", &home)
            .process_group(0)
            .spawn()
            .map_err(|e| e.to_string())?;
        let pid = child.id().map(|p| p as i32);
        g.pid = pid;
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        tracing::info!(via = "doğrudan", pid, "tarayıcı oturumu başladı");
        Ok(())
    }

    #[cfg(test)]
    async fn refs(&self) -> usize {
        self.inner.lock().await.refs
    }
}

async fn stop_locked(g: &mut Inner) {
    if g.systemd {
        let _ = systemctl_user("stop", UNIT).await;
        g.systemd = false;
        tracing::info!(via = "systemd", "tarayıcı oturumu durdu");
    }
    if g.pid.is_some() {
        kill_locked(g);
        tracing::info!(via = "doğrudan", "tarayıcı oturumu durdu");
    }
}

fn kill_locked(g: &mut Inner) {
    if let Some(pid) = g.pid.take() {
        // Süreç grubunun tamamı: labwc, wayvnc, Chromium.
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(-pid), nix::sys::signal::Signal::SIGTERM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oturum betiği yerine portu dinleyen küçük bir süreç; süre dolunca grup sonlandırılır.
    #[tokio::test]
    async fn lifecycle() {
        let dir = std::env::temp_dir().join(format!("mk-br-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let ln = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = ln.local_addr().unwrap().to_string();
        drop(ln);
        let port = addr.rsplit(':').next().unwrap();
        // perl (Debian'da perl-base hep var), yoksa python3 ya da nc; hiçbiri yoksa test atlanır.
        let server = if crate::util::look_path("perl").is_some() {
            format!("exec perl -MIO::Socket::INET -e '$s=IO::Socket::INET->new(LocalAddr=>\"127.0.0.1:{port}\",Listen=>5,ReuseAddr=>1) or die; sleep 60'")
        } else if crate::util::look_path("python3").is_some() {
            format!("exec python3 -c 'import socket,time;s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);s.bind((\"127.0.0.1\",{port}));s.listen();time.sleep(60)'")
        } else if crate::util::look_path("nc").is_some() {
            format!("exec nc -l -p {port}")
        } else {
            return;
        };
        let script = dir.join("browser-session");
        std::fs::write(&script, format!("#!/bin/sh\n{server}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let m = Manager::new(&addr, script.to_str().unwrap(), Duration::from_millis(300));
        m.acquire().await.unwrap();
        m.acquire().await.unwrap();
        assert_eq!(m.refs().await, 2);
        m.release().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(m.listening().await, "bir pencere hâlâ açık");
        m.release().await;
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!m.listening().await, "son pencereden sonra durmalı");

        let none = Manager::new("127.0.0.1:1", "/yok/browser-session", Duration::ZERO);
        assert!(none.acquire().await.unwrap_err().contains("bu sistemde yok"));
        assert_eq!(none.refs().await, 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
