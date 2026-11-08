//! PTY terminal oturumları. POST /term oturum ve 30 sn'lik tek kullanımlık
//! bilet üretir; WS /term/{id}?ticket= bağlanır. Bir oturuma aynı anda birden
//! çok istemci bağlanabilir (ör. telefondaki web arayüzü ve Pi'nin kendi
//! ekranı): çıktı hepsine yayılır, her biri yazabilir; boyutu son yeniden
//! boyutlandıran belirler. Bağlı istemci kalmazsa oturum `linger` süresi
//! yaşar; çıktı 256 KB'lık halka tamponda tutulur ve bağlanınca gönderilir.

use crate::apierr::{ApiError, ApiResult};
use crate::termproc::login::Command;
use crate::termproc::remote::{dial_open, DialError};
use crate::termproc::{clamp_size, pty, Ctl, Open, Proc};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use bytes::Bytes;
use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_MAX: usize = 6;
pub const DEFAULT_LINGER: Duration = Duration::from_secs(30 * 60);
pub const TICKET_TTL: Duration = Duration::from_secs(30);
pub const RING_SIZE: usize = 256 << 10;
/// makerd-user soketlerinin dizini (<uid>.sock).
pub const DEFAULT_USER_DIR: &str = "/run/makeros/user";

// --- halka tampon ----------------------------------------------------------

/// Sabit boyutlu bayt halka tamponu; yalnız son `size` baytı tutar.
pub struct Ring {
    buf: Vec<u8>,
    start: usize,
    n: usize,
}

impl Ring {
    pub fn new(size: usize) -> Self {
        Self { buf: vec![0; size], start: 0, n: 0 }
    }

    /// Veriyi ekler; taşan eski baytlar silinir.
    pub fn write(&mut self, p: &[u8]) {
        let size = self.buf.len();
        if p.len() >= size {
            self.buf.copy_from_slice(&p[p.len() - size..]);
            self.start = 0;
            self.n = size;
            return;
        }
        let end = (self.start + self.n) % size;
        let k = (size - end).min(p.len());
        self.buf[end..end + k].copy_from_slice(&p[..k]);
        self.buf[..p.len() - k].copy_from_slice(&p[k..]);
        self.n += p.len();
        if self.n > size {
            self.start = (self.start + self.n - size) % size;
            self.n = size;
        }
    }

    /// İçeriğin sıralı kopyası.
    pub fn bytes(&self) -> Vec<u8> {
        let first = (self.start + self.n).min(self.buf.len());
        let mut out = self.buf[self.start..first].to_vec();
        out.extend_from_slice(&self.buf[..self.n - out.len()]);
        out
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}

// --- süreç başlatıcılar ----------------------------------------------------

/// Kullanıcı için verilen boyutta bir terminal süreci başlatır (yerel PTY veya makerd-user).
pub type Spawner = Arc<dyn Fn(String, u16, u16, String) -> BoxFuture<'static, ApiResult<Proc>> + Send + Sync>;

/// Komutu makerd sürecinin içinde (aynı kullanıcı ve kum havuzuyla) yeni bir
/// PTY'de başlatır. Mock modda ve makerd-user yokken kullanılır.
pub fn local(command: Command) -> Spawner {
    Arc::new(move |_user, cols, rows, cmd| {
        let command = command.clone();
        Box::pin(async move {
            let spec = command().map_err(|e| ApiError::internal(format!("Kabuk hazırlanamadı: {e}")))?.with_cmd(&cmd);
            pty::spawn(&spec, cols, rows).map_err(|e| ApiError::internal(format!("Kabuk başlatılamadı: {e}")))
        })
    })
}

/// Oturumu kullanıcının makerd-user@<user>.service hizmetinde açar
/// (dir/<uid>.sock). Hizmet yoksa ve kullanıcı `self_user` ise `fallback`
/// kullanılır (makerd'nin kendi kullanıcısı; kum havuzu içinde, sudo çalışmaz).
pub fn remote(dir: PathBuf, uid_of: Arc<dyn Fn(&str) -> Option<u32> + Send + Sync>, self_user: String, fallback: Option<Spawner>) -> Spawner {
    Arc::new(move |user, cols, rows, cmd| {
        let (dir, uid_of, self_user, fallback) = (dir.clone(), uid_of.clone(), self_user.clone(), fallback.clone());
        Box::pin(async move {
            let uid = uid_of(&user).ok_or_else(|| ApiError::forbidden("Kullanıcı bulunamadı."))?;
            let o = Open { cols, rows, cmd: cmd.clone(), ..Default::default() };
            match dial_open(&dir.join(format!("{uid}.sock")), Some(uid), o).await {
                Ok(p) => Ok(p),
                Err(DialError::NotRunning) => match fallback {
                    Some(fb) if user == self_user => fb(user, cols, rows, cmd).await,
                    _ => Err(ApiError::unavailable(format!("Kullanıcı terminal hizmeti (makerd-user@{user}) çalışmıyor."))),
                },
                Err(DialError::Peer) => Err(ApiError::forbidden("Terminal hizmeti soketi güvenilir değil.")),
                Err(DialError::Remote(m)) => Err(ApiError::unavailable(m)),
                Err(DialError::Other(m)) => Err(ApiError::unavailable(format!("Terminal hizmetine bağlanılamadı: {m}"))),
            }
        })
    })
}

// --- oturumlar ---------------------------------------------------------------

/// Yeni oturum seçenekleri.
#[derive(Clone, Debug, Default)]
pub struct Opts {
    /// Boşsa giriş kabuğu; doluysa "kabuk -lc cmd".
    pub cmd: String,
    pub title: String,
}

/// Oturum listesindeki satır.
#[derive(Serialize, Clone, Debug)]
pub struct Info {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cmd: String,
    pub created: jiff::Timestamp,
    pub clients: usize,
    pub pid: i32,
}

struct SessInner {
    ring: Ring,
    clients: Vec<(u64, mpsc::Sender<Bytes>)>,
    next_client: u64,
    timer: Option<tokio::task::JoinHandle<()>>,
    exited: bool,
}

/// Tek bir PTY oturumu.
pub struct Session {
    pub id: String,
    pub user: String,
    pub title: String,
    pub cmd: String,
    pub created: jiff::Timestamp,
    pub pid: i32,
    ctl: mpsc::UnboundedSender<Ctl>,
    done: CancellationToken,
    inner: Mutex<SessInner>,
}

impl Session {
    /// Bağlı istemci sayısı.
    pub fn clients(&self) -> usize {
        self.inner.lock().unwrap().clients.len()
    }

    fn kill(&self) {
        let _ = self.ctl.send(Ctl::Hangup);
    }

    pub fn write(&self, b: Bytes) {
        let _ = self.ctl.send(Ctl::Data(b));
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.ctl.send(Ctl::Resize(cols, rows));
    }

    /// Süreç bitip çıktı tükenince tamamlanır.
    pub async fn done(&self) {
        self.done.cancelled().await
    }

    pub fn ring(&self) -> Vec<u8> {
        self.inner.lock().unwrap().ring.bytes()
    }
}

struct Ticket {
    id: String,
    user: String,
    exp: Instant,
}

struct MgrInner {
    max: usize,
    linger: Duration,
    spawn: Spawner,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    tickets: Mutex<HashMap<String, Ticket>>,
    now: Box<dyn Fn() -> Instant + Send + Sync>,
}

/// Oturum yöneticisi (ucuz kopyalanır).
#[derive(Clone)]
pub struct Manager(Arc<MgrInner>);

const SUFFIX: &str = r#"c=$?; if [ "$c" = 0 ]; then printf '\n\033[32m✔ Tamamlandı\033[0m\n'; else printf '\n\033[31m✘ Hata (çıkış kodu %s)\033[0m — yukarıdaki çıktıya bakın.\n' "$c"; fi; exec "${SHELL:-/bin/sh}" -l"#;

impl Manager {
    pub fn new(spawn: Spawner) -> Self {
        Self::with_limits(spawn, DEFAULT_MAX, DEFAULT_LINGER)
    }

    pub fn with_limits(spawn: Spawner, max: usize, linger: Duration) -> Self {
        Self(Arc::new(MgrInner {
            max,
            linger,
            spawn,
            sessions: Mutex::new(HashMap::new()),
            tickets: Mutex::new(HashMap::new()),
            now: Box::new(Instant::now),
        }))
    }

    fn full(&self) -> ApiError {
        ApiError::conflict(format!("En fazla {} terminal oturumu açılabilir.", self.0.max))
    }

    /// Giriş kabuğuyla yeni oturum.
    pub async fn create(&self, user: &str, cols: i64, rows: i64) -> ApiResult<Arc<Session>> {
        self.create_with(user, cols, rows, Opts::default()).await
    }

    /// Seçeneklerle yeni oturum.
    pub async fn create_with(&self, user: &str, cols: i64, rows: i64, o: Opts) -> ApiResult<Arc<Session>> {
        if self.count() >= self.0.max {
            return Err(self.full());
        }
        // Komut bitince çıktı okunabilsin: sonuç açıkça yazılır, oturum etkileşimli kabukla sürer.
        let cmd = if o.cmd.is_empty() { String::new() } else { format!("{}\n{SUFFIX}", o.cmd) };
        let mut proc = (self.0.spawn)(user.into(), clamp_size(cols, 80), clamp_size(rows, 24), cmd).await?;
        let s = Arc::new(Session {
            id: crate::util::random_token(12),
            user: user.into(),
            title: if o.title.is_empty() { "kabuk".into() } else { o.title },
            cmd: o.cmd,
            created: jiff::Timestamp::now(),
            pid: proc.pid,
            ctl: proc.ctl.clone(),
            done: CancellationToken::new(),
            inner: Mutex::new(SessInner { ring: Ring::new(RING_SIZE), clients: Vec::new(), next_client: 0, timer: None, exited: false }),
        });
        {
            let mut ss = self.0.sessions.lock().unwrap();
            if ss.len() >= self.0.max {
                // Yarış: aynı anda iki istek.
                drop(ss);
                s.kill();
                return Err(self.full());
            }
            ss.insert(s.id.clone(), s.clone());
        }
        self.arm_linger(&s);
        // Okuyucu: çıktı halka tampona ve bağlı istemcilere.
        let rs = s.clone();
        tokio::spawn(async move {
            while let Some(b) = proc.output.recv().await {
                let mut g = rs.inner.lock().unwrap();
                g.ring.write(&b);
                // Yavaş istemci düşürülür; yeniden bağlanınca halka tampon gönderilir.
                g.clients.retain(|(_, c)| c.try_send(b.clone()).is_ok());
            }
            rs.inner.lock().unwrap().exited = true;
            rs.done.cancel();
            let mut ex = proc.exit;
            while ex.borrow().is_none() {
                if ex.changed().await.is_err() {
                    break;
                }
            }
        });
        // Bekleyici: süreç bitince oturum kaldırılır.
        let (m, ws) = (self.clone(), s.clone());
        tokio::spawn(async move {
            ws.done().await;
            m.0.sessions.lock().unwrap().remove(&ws.id);
            if let Some(t) = ws.inner.lock().unwrap().timer.take() {
                t.abort();
            }
            tracing::info!(id = %ws.id, user = %ws.user, "terminal oturumu kapandı");
        });
        tracing::info!(id = %s.id, user, pid = s.pid, "terminal oturumu açıldı");
        Ok(s)
    }

    fn arm_linger(&self, s: &Arc<Session>) {
        let (m, id, d) = (self.clone(), s.id.clone(), self.0.linger);
        let h = tokio::spawn(async move {
            tokio::time::sleep(d).await;
            m.close(&id);
        });
        if let Some(old) = s.inner.lock().unwrap().timer.replace(h) {
            old.abort();
        }
    }

    /// Kullanıcının oturumu.
    pub fn get(&self, id: &str, user: &str) -> Option<Arc<Session>> {
        self.0.sessions.lock().unwrap().get(id).filter(|s| s.user == user).cloned()
    }

    /// Kullanıcının açık oturumları (en yeni sonda).
    pub fn list(&self, user: &str) -> Vec<Info> {
        let mut ss: Vec<Arc<Session>> = self.0.sessions.lock().unwrap().values().filter(|s| s.user == user).cloned().collect();
        ss.sort_by_key(|s| s.created);
        ss.iter().map(|s| Info { id: s.id.clone(), title: s.title.clone(), cmd: s.cmd.clone(), created: s.created, clients: s.clients(), pid: s.pid }).collect()
    }

    pub fn count(&self) -> usize {
        self.0.sessions.lock().unwrap().len()
    }

    /// Oturum için tek kullanımlık bilet.
    pub fn ticket(&self, id: &str, user: &str) -> ApiResult<String> {
        if self.get(id, user).is_none() {
            return Err(ApiError::not_found("Terminal oturumu bulunamadı."));
        }
        let t = crate::util::random_token(24);
        let now = (self.0.now)();
        let mut tk = self.0.tickets.lock().unwrap();
        tk.retain(|_, v| now <= v.exp);
        tk.insert(t.clone(), Ticket { id: id.into(), user: user.into(), exp: now + TICKET_TTL });
        Ok(t)
    }

    /// Bileti tüketir; geçerliyse oturumu döndürür.
    pub fn redeem(&self, t: &str, id: &str) -> Option<Arc<Session>> {
        let tk = self.0.tickets.lock().unwrap().remove(t)?;
        if tk.id != id || (self.0.now)() > tk.exp {
            return None;
        }
        self.get(id, &tk.user)
    }

    /// Oturumu sonlandırır ve listeden hemen kaldırır.
    pub fn close(&self, id: &str) {
        let s = self.0.sessions.lock().unwrap().remove(id);
        if let Some(s) = s {
            if let Some(t) = s.inner.lock().unwrap().timer.take() {
                t.abort();
            }
            s.kill();
        }
    }

    /// Tüm oturumları sonlandırır (kapanışta).
    pub fn close_all(&self) {
        let list: Vec<Arc<Session>> = self.0.sessions.lock().unwrap().values().cloned().collect();
        for s in list {
            s.kill();
        }
    }

    /// WebSocket'i oturuma ekler ve bağlantı bitene kadar sürer. Diğer istemciler
    /// bağlı kalır (ortak terminal).
    pub async fn attach(&self, s: Arc<Session>, ws: WebSocket) {
        let (tx, mut out) = mpsc::channel::<Bytes>(256);
        let (cid, replay) = {
            let mut g = s.inner.lock().unwrap();
            if let Some(t) = g.timer.take() {
                t.abort();
            }
            let cid = g.next_client;
            g.next_client += 1;
            g.clients.push((cid, tx));
            (cid, g.ring.bytes())
        };
        let (mut wtx, mut wrx) = ws.split();
        let close = |code: u16, reason: &'static str| Message::Close(Some(CloseFrame { code, reason: reason.into() }));
        let gone = CancellationToken::new();
        // Girdi: ikili ileti PTY'ye, metin ileti denetim (resize).
        let rs = s.clone();
        let g2 = gone.clone();
        let reader = tokio::spawn(async move {
            while let Some(Ok(m)) = wrx.next().await {
                match m {
                    Message::Binary(b) => rs.write(b),
                    Message::Text(t) => {
                        #[derive(Deserialize)]
                        struct Control {
                            #[serde(rename = "type")]
                            kind: String,
                            #[serde(default)]
                            cols: i64,
                            #[serde(default)]
                            rows: i64,
                        }
                        if let Ok(c) = serde_json::from_str::<Control>(t.as_str()) {
                            if c.kind == "resize" {
                                rs.resize(clamp_size(c.cols, 80), clamp_size(c.rows, 24));
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            g2.cancel();
        });
        let mut ok = replay.is_empty() || wtx.send(Message::Binary(replay.into())).await.is_ok();
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        ping.tick().await;
        while ok {
            tokio::select! {
                d = out.recv() => match d {
                    Some(d) => ok = wtx.send(Message::Binary(d)).await.is_ok(),
                    None => {
                        // Yavaş istemci listeden düşürüldü.
                        let _ = wtx.send(close(4000, "slow")).await;
                        break;
                    }
                },
                _ = s.done() => {
                    // Kalan çıktıyı boşalt.
                    while let Ok(d) = out.try_recv() {
                        if wtx.send(Message::Binary(d)).await.is_err() {
                            break;
                        }
                    }
                    let _ = wtx.send(close(1000, "exit")).await;
                    break;
                }
                _ = ping.tick() => {
                    ok = tokio::time::timeout(Duration::from_secs(10), wtx.send(Message::Ping(Bytes::new()))).await.is_ok_and(|r| r.is_ok());
                }
                _ = gone.cancelled() => {
                    let _ = wtx.send(close(1001, "")).await;
                    break;
                }
            }
        }
        reader.abort();
        let mut g = s.inner.lock().unwrap();
        g.clients.retain(|(i, _)| *i != cid);
        let rearm = g.clients.is_empty() && !g.exited && g.timer.is_none();
        drop(g);
        if rearm {
            self.arm_linger(&s);
        }
    }

    #[cfg(test)]
    fn set_clock(&mut self, f: impl Fn() -> Instant + Send + Sync + 'static) {
        Arc::get_mut(&mut self.0).expect("tek sahip").now = Box::new(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termproc::login::CmdSpec;

    fn cmd(path: &str) -> Command {
        let p = path.to_string();
        Arc::new(move || {
            Ok(CmdSpec { path: p.clone(), argv0: p.clone(), args: vec![], dir: "/".into(), env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("TERM".into(), "dumb".into())] })
        })
    }

    #[test]
    fn ring() {
        let mut r = Ring::new(8);
        for (w, want) in [("abc", "abc"), ("defgh", "abcdefgh"), ("ij", "cdefghij"), ("0123456789", "23456789"), ("x", "3456789x")] {
            r.write(w.as_bytes());
            assert_eq!(String::from_utf8(r.bytes()).unwrap(), want, "{w}");
        }
        assert_eq!(r.len(), 8);
    }

    #[tokio::test]
    async fn tickets() {
        let mut m = Manager::new(local(cmd("/bin/cat")));
        let now = Arc::new(Mutex::new(Instant::now()));
        let n2 = now.clone();
        m.set_clock(move || *n2.lock().unwrap());
        let s = m.create("maker", 80, 24).await.unwrap();
        assert!(m.ticket(&s.id, "someone").is_err());
        let tk = m.ticket(&s.id, "maker").unwrap();
        assert!(m.redeem(&tk, "wrong-id").is_none());
        assert!(m.redeem(&tk, &s.id).is_none(), "başarısız denemeden sonra bilet geçersiz");
        let tk = m.ticket(&s.id, "maker").unwrap();
        assert!(Arc::ptr_eq(&m.redeem(&tk, &s.id).unwrap(), &s));
        assert!(m.redeem(&tk, &s.id).is_none(), "bilet iki kez kullanılamaz");
        let tk = m.ticket(&s.id, "maker").unwrap();
        *now.lock().unwrap() += TICKET_TTL + Duration::from_secs(1);
        assert!(m.redeem(&tk, &s.id).is_none(), "süresi geçmiş bilet");
        m.close_all();
    }

    async fn read_until(s: &Session, want: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if String::from_utf8_lossy(&s.ring()).contains(want) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{want:?} görülmedi; halka {:?}", String::from_utf8_lossy(&s.ring()));
    }

    #[tokio::test]
    async fn max_sessions_and_exit() {
        let m = Manager::with_limits(local(cmd("/bin/cat")), 2, DEFAULT_LINGER);
        let a = m.create("maker", 0, 0).await.unwrap();
        m.create("maker", 0, 0).await.unwrap();
        assert_eq!(m.create("maker", 0, 0).await.err().map(|e| e.code), Some("conflict"));
        a.write(Bytes::from_static(b"merhaba\n"));
        read_until(&a, "merhaba").await;
        m.close(&a.id);
        tokio::time::timeout(Duration::from_secs(5), a.done()).await.expect("oturum kapanmadı");
        for _ in 0..50 {
            if m.count() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(m.count(), 1);
        assert_eq!(m.list("maker").len(), 1);
        m.close_all();
    }

    #[tokio::test]
    async fn linger_closes_unattached() {
        let m = Manager::with_limits(local(cmd("/bin/cat")), 2, Duration::from_millis(200));
        let a = m.create("maker", 0, 0).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), a.done()).await.expect("bekleme süresi dolunca kapanmalı");
    }

    #[tokio::test]
    async fn remote_spawner_with_user_service() {
        use crate::termproc::server::Server;
        let dir = std::env::temp_dir().join(format!("mk-term-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let uid = crate::util::getuid();
        let ln = tokio::net::UnixListener::bind(dir.join(format!("{uid}.sock"))).unwrap();
        let (stop, rx) = tokio::sync::watch::channel(false);
        tokio::spawn(Arc::new(Server::new([uid].into(), cmd("/bin/sh"), 0, "/".into())).serve(ln, rx));
        let m = Manager::new(remote(dir.clone(), Arc::new(move |_| Some(uid)), String::new(), None));
        let s = m.create("ali", 90, 20).await.unwrap();
        assert!(s.pid > 0);
        s.resize(100, 33);
        s.write(Bytes::from_static(b"stty size; echo UZAK_$((40+2))\n"));
        read_until(&s, "UZAK_42").await;
        read_until(&s, "33 100").await;
        s.write(Bytes::from_static(b"exit\n"));
        tokio::time::timeout(Duration::from_secs(5), s.done()).await.expect("uzak oturum bitmedi");
        // Kapatma uzun süren komutu sonlandırır.
        let s2 = m.create("ali", 0, 0).await.unwrap();
        s2.write(Bytes::from_static(b"sleep 60\n"));
        tokio::time::sleep(Duration::from_millis(100)).await;
        m.close(&s2.id);
        tokio::time::timeout(Duration::from_secs(8), s2.done()).await.expect("kapatma uzak oturumu bitirmedi");
        let _ = stop.send(true);
    }

    #[tokio::test]
    async fn remote_fallback_and_errors() {
        let dir = std::env::temp_dir().join(format!("mk-term-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let uid = crate::util::getuid();
        let sp = remote(dir.clone(), Arc::new(move |_| Some(uid)), "maker".into(), Some(local(cmd("/bin/cat"))));
        // Soket yok: yalnız makerd'nin kendi kullanıcısı yerel kabuğa düşer.
        assert_eq!(sp("ali".into(), 80, 24, String::new()).await.err().map(|e| e.status), Some(503));
        let p = sp("maker".into(), 80, 24, String::new()).await.unwrap();
        p.ctl.send(Ctl::Hangup).unwrap();
        // İzinsiz çağıran 'e' çerçevesi alır.
        let ln = tokio::net::UnixListener::bind(dir.join(format!("{uid}.sock"))).unwrap();
        let (stop, rx) = tokio::sync::watch::channel(false);
        tokio::spawn(Arc::new(crate::termproc::server::Server::new(Default::default(), cmd("/bin/sh"), 0, "/".into())).serve(ln, rx));
        assert!(sp("ali".into(), 80, 24, String::new()).await.is_err());
        // Karşı uç beklenen UID değilse bağlanılmaz.
        let r = dial_open(&dir.join(format!("{uid}.sock")), Some(uid + 1), Open::default()).await;
        assert!(matches!(r, Err(DialError::Peer)));
        let _ = stop.send(true);
    }
}
