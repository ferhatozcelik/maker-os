//! makerd-user sunucusu: her bağlantı bir terminal süreci (PTY) ya da kullanıcı
//! adına yapılan tek bir işlemdir (exec, read, write, list, ...). Çağıran
//! SO_PEERCRED ile doğrulanır; yalnız izinli UID'ler (makerd ve root) bağlanabilir.

use super::login::Command;
use super::{fsops, piped, pty, read_frame, write_frame, write_json, Ctl, Exit, Open, Opened, Size};
use super::{FRAME_DATA, FRAME_ERROR, FRAME_EXIT, FRAME_HANGUP, FRAME_OPEN, FRAME_PID, FRAME_RESIZE, HANGUP_GRACE};
use bytes::Bytes;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

/// Açma çerçevesi için bekleme süresi.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// 'd' çerçevesi başına en büyük yük (dosya ve komut çıktısı).
const DATA_CHUNK: usize = 256 << 10;

pub struct Server {
    /// İzinli çağıran UID'leri (makerd ve root).
    pub allowed: HashSet<u32>,
    /// Yeni giriş kabuğu komutunu hazırlar.
    pub command: Command,
    /// Aynı anda açık bağlantı sınırı (0: 16).
    pub max: usize,
    /// Dosya işlemlerinde göreli yolların kökü (kullanıcının ev dizini).
    pub home: String,
    active: AtomicUsize,
}

/// Etkin bağlantı sayacını düşürür.
struct Slot<'a>(&'a AtomicUsize);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

type Reader = BufReader<OwnedReadHalf>;
type Writer = OwnedWriteHalf;

impl Server {
    pub fn new(allowed: HashSet<u32>, command: Command, max: usize, home: String) -> Self {
        Self { allowed, command, max, home, active: AtomicUsize::new(0) }
    }

    /// Bağlantıları `stop` true olana kadar kabul eder. Durunca açık terminaller
    /// kapatılır ve bağlantıların bitmesi kısa bir süre beklenir.
    pub async fn serve(self: Arc<Self>, ln: UnixListener, mut stop: watch::Receiver<bool>) -> std::io::Result<()> {
        let mut tasks = JoinSet::new();
        let res = loop {
            tokio::select! {
                r = stop.changed() => {
                    if r.is_err() || *stop.borrow() {
                        break Ok(());
                    }
                }
                Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
                r = ln.accept() => match r {
                    Ok((conn, _)) => {
                        let (me, st) = (self.clone(), stop.clone());
                        tasks.spawn(async move { me.handle(conn, st).await });
                    }
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::Interrupted) => {}
                    Err(e) => break Err(e),
                },
            }
        };
        drop(ln);
        let drain = async { while tasks.join_next().await.is_some() {} };
        if tokio::time::timeout(HANGUP_GRACE + Duration::from_secs(3), drain).await.is_err() {
            tracing::warn!(open = tasks.len(), "bağlantılar zamanında kapanmadı, kesiliyor");
            tasks.shutdown().await;
        }
        res
    }

    async fn handle(&self, conn: UnixStream, stop: watch::Receiver<bool>) {
        let (uid, pid) = match conn.peer_cred() {
            Ok(c) => (Some(c.uid()), c.pid().unwrap_or(0)),
            Err(_) => (None, 0),
        };
        let (r, mut w) = conn.into_split();
        let Some(uid) = uid.filter(|u| self.allowed.contains(u)) else {
            tracing::warn!(uid = uid.map(i64::from).unwrap_or(-1), pid, "izinsiz bağlantı reddedildi");
            let _ = write_frame(&mut w, FRAME_ERROR, "Bu işlem için yetkiniz yok.".as_bytes()).await;
            return;
        };
        let max = if self.max == 0 { 16 } else { self.max };
        if self.active.fetch_add(1, Ordering::SeqCst) >= max {
            self.active.fetch_sub(1, Ordering::SeqCst);
            let _ = write_frame(&mut w, FRAME_ERROR, "Terminal oturumu sınırına ulaşıldı.".as_bytes()).await;
            return;
        }
        let _slot = Slot(&self.active);

        let mut r = BufReader::with_capacity(64 << 10, r);
        let op = match tokio::time::timeout(OPEN_TIMEOUT, read_frame(&mut r)).await {
            Ok(Ok((FRAME_OPEN, p))) => serde_json::from_slice::<Open>(&p).unwrap_or_default(),
            _ => return,
        };
        if op.mode.is_empty() {
            self.serve_pty(r, w, op, uid, stop).await;
        } else {
            self.serve_op(r, w, op, stop).await;
        }
    }

    async fn serve_pty(&self, mut r: Reader, mut w: Writer, op: Open, caller_uid: u32, mut stop: watch::Receiver<bool>) {
        let cols = if op.cols == 0 || op.cols > 1000 { 80 } else { op.cols };
        let rows = if op.rows == 0 || op.rows > 1000 { 24 } else { op.rows };
        let spec = match (self.command)() {
            Ok(s) => s.with_cmd(&op.cmd),
            Err(e) => {
                let _ = write_frame(&mut w, FRAME_ERROR, format!("Kabuk hazırlanamadı: {e}").as_bytes()).await;
                return;
            }
        };
        let mut proc = match pty::spawn(&spec, cols, rows) {
            Ok(p) => p,
            Err(e) => {
                let _ = write_frame(&mut w, FRAME_ERROR, format!("Kabuk başlatılamadı: {e}").as_bytes()).await;
                return;
            }
        };
        let pid = proc.pid;
        let ctl = proc.ctl.clone();
        if write_json(&mut w, FRAME_PID, &Opened { pid }).await.is_err() {
            let _ = ctl.send(Ctl::Hangup);
            wait_exit(proc.exit).await;
            return;
        }
        tracing::info!(pid, caller_uid, "terminal açıldı");

        // Girdi: makerd koparsa, kapat isterse ya da hizmet durursa süreç kapatılır.
        let input = {
            let ctl = ctl.clone();
            tokio::spawn(async move {
                loop {
                    let f = tokio::select! {
                        f = read_frame(&mut r) => f,
                        _ = stopped(&mut stop) => Err(std::io::ErrorKind::Interrupted.into()),
                    };
                    match f {
                        Err(_) => {
                            let _ = ctl.send(Ctl::Hangup);
                            return;
                        }
                        Ok((FRAME_DATA, p)) => {
                            let _ = ctl.send(Ctl::Data(Bytes::from(p)));
                        }
                        Ok((FRAME_RESIZE, p)) => {
                            if let Ok(sz) = serde_json::from_slice::<Size>(&p) {
                                if (1..=1000).contains(&sz.cols) && (1..=1000).contains(&sz.rows) {
                                    let _ = ctl.send(Ctl::Resize(sz.cols, sz.rows));
                                }
                            }
                        }
                        Ok((FRAME_HANGUP, _)) => {
                            let _ = ctl.send(Ctl::Hangup);
                        }
                        Ok(_) => {}
                    }
                }
            })
        };

        // Çıktı: PTY kapanana kadar aktarılır; makerd'ye yazılamıyorsa süreç kapatılır
        // ama PTY boşaltılmaya devam eder (süreç çıktıda takılmasın).
        let mut broken = false;
        while let Some(b) = proc.output.recv().await {
            if !broken && write_frame(&mut w, FRAME_DATA, &b).await.is_err() {
                broken = true;
                let _ = ctl.send(Ctl::Hangup);
            }
        }
        let code = wait_exit(proc.exit).await;
        input.abort();
        let _ = write_json(&mut w, FRAME_EXIT, &Exit { code }).await;
        tracing::info!(pid, code, "terminal kapandı");
    }

    /// PTY dışı kipler (exec, read, write, list, mkdir, rename, remove, stat).
    async fn serve_op(&self, mut r: Reader, mut w: Writer, op: Open, mut stop: watch::Receiver<bool>) {
        let home = if self.home.is_empty() { super::login::home_of("/etc/passwd") } else { self.home.clone() };
        let res: Result<i32, String> = match op.mode.as_str() {
            "exec" => {
                let spec = match (self.command)() {
                    Ok(s) => s.with_cmd(&op.cmd),
                    Err(e) => return fail(&mut w, &e.to_string()).await,
                };
                // Girdi: 'd' çerçeveleri stdin'e; 'k' veya bağlantı sonu stdin'i kapatır.
                let (itx, irx) = mpsc::channel::<Bytes>(16);
                let input = tokio::spawn(async move {
                    while let Ok((typ, p)) = read_frame(&mut r).await {
                        match typ {
                            FRAME_DATA if !p.is_empty() => {
                                if itx.send(Bytes::from(p)).await.is_err() {
                                    break;
                                }
                            }
                            FRAME_HANGUP => break,
                            _ => {}
                        }
                    }
                });
                let (otx, mut orx) = mpsc::channel::<Bytes>(16);
                let mut run = std::pin::pin!(piped::run(&spec, Some(irx), otx));
                // Gelecek düşürülürse süreç grubu sonlandırılır (iptal ve bağlantı kopması).
                let res = loop {
                    tokio::select! {
                        r = &mut run => break r.map_err(|e| e.to_string()),
                        Some(b) = orx.recv() => {
                            if send_data(&mut w, &b).await.is_err() {
                                input.abort();
                                return;
                            }
                        }
                        _ = stopped(&mut stop) => {
                            input.abort();
                            return;
                        }
                    }
                };
                input.abort();
                while let Some(b) = orx.recv().await {
                    if send_data(&mut w, &b).await.is_err() {
                        return;
                    }
                }
                res
            }
            "read" => {
                let f = match fsops::open_read(&home, &op.path) {
                    Ok(f) => f,
                    Err(e) => return fail(&mut w, &e.0).await,
                };
                let mut f = tokio::fs::File::from_std(f);
                let mut buf = vec![0u8; DATA_CHUNK];
                loop {
                    match f.read(&mut buf).await {
                        Ok(0) => break Ok(0),
                        Ok(n) => {
                            if write_frame(&mut w, FRAME_DATA, &buf[..n]).await.is_err() {
                                return;
                            }
                        }
                        // Okuma yarıda kaldı: çıkış çerçevesi yok, makerd eksik dosyayı kabul etmez.
                        Err(_) => return,
                    }
                }
            }
            "write" => {
                let mut fw = match fsops::AtomicWriter::create(&home, &op.path) {
                    Ok(fw) => fw,
                    Err(e) => return fail(&mut w, &e.0).await,
                };
                loop {
                    match read_frame(&mut r).await {
                        Ok((FRAME_DATA, p)) => {
                            if let Err(e) = fw.write(&p) {
                                return fail(&mut w, &e.0).await;
                            }
                        }
                        Ok((FRAME_HANGUP, _)) => break fw.commit().map(|_| 0).map_err(|e| e.0),
                        Ok(_) => {}
                        // Bağlantı 'k' gelmeden koptu: yarım dosya yerine taşınmaz (geçici dosya silinir).
                        Err(_) => return,
                    }
                }
            }
            _ => match fsops::fs_op(&home, &op) {
                Ok(b) => {
                    if write_frame(&mut w, FRAME_DATA, &b).await.is_err() {
                        return;
                    }
                    Ok(0)
                }
                Err(e) => Err(e.0),
            },
        };
        match res {
            Ok(code) => {
                let _ = write_json(&mut w, FRAME_EXIT, &Exit { code }).await;
            }
            Err(e) => fail(&mut w, &e).await,
        }
    }
}

async fn fail(w: &mut Writer, msg: &str) {
    let _ = write_frame(w, FRAME_ERROR, msg.as_bytes()).await;
}

/// Veriyi DATA_CHUNK'lık 'd' çerçevelerine bölerek yazar.
async fn send_data(w: &mut Writer, b: &[u8]) -> std::io::Result<()> {
    for c in b.chunks(DATA_CHUNK) {
        write_frame(w, FRAME_DATA, c).await?;
    }
    Ok(())
}

/// Hizmet durdurulana (ya da durdurucu düşene) kadar bekler.
async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|s| *s).await;
}

async fn wait_exit(mut ex: watch::Receiver<Option<i32>>) -> i32 {
    loop {
        if let Some(c) = *ex.borrow() {
            return c;
        }
        if ex.changed().await.is_err() {
            return ex.borrow().unwrap_or(-1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::login::CmdSpec;
    use super::super::remote::{dial_open, DialError, RemoteOp};
    use super::*;
    use std::path::PathBuf;

    struct Env {
        dir: PathBuf,
        sock: PathBuf,
        stop: watch::Sender<bool>,
        done: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    fn sh() -> Command {
        Arc::new(|| {
            Ok(CmdSpec {
                path: "/bin/sh".into(),
                argv0: "-sh".into(),
                args: Vec::new(),
                dir: "/".into(),
                env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("TERM".into(), "dumb".into())],
            })
        })
    }

    fn start(allowed: &[u32], max: usize) -> Env {
        let dir = std::env::temp_dir().join(format!("mk-srv-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("u.sock");
        let ln = UnixListener::bind(&sock).unwrap();
        let srv = Arc::new(Server::new(allowed.iter().copied().collect(), sh(), max, dir.to_string_lossy().into_owned()));
        let (stop, rx) = watch::channel(false);
        let done = tokio::spawn(srv.serve(ln, rx));
        Env { dir, sock, stop, done }
    }

    impl Env {
        async fn finish(self) {
            self.stop.send(true).unwrap();
            self.done.await.unwrap().unwrap();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn me() -> u32 {
        crate::util::getuid()
    }

    fn open(mode: &str, path: &str, cmd: &str) -> Open {
        Open { mode: mode.into(), path: path.into(), cmd: cmd.into(), ..Default::default() }
    }

    #[tokio::test]
    async fn pty_session() {
        let env = start(&[me()], 0);
        let mut p = dial_open(&env.sock, Some(me()), Open { cmd: "read x; echo got:$x; exit 7".into(), ..Default::default() }).await.unwrap();
        assert!(p.pid > 0);
        p.ctl.send(Ctl::Resize(100, 30)).unwrap();
        p.ctl.send(Ctl::Data(Bytes::from_static(b"selam\n"))).unwrap();
        let mut out = Vec::new();
        while let Some(b) = tokio::time::timeout(Duration::from_secs(5), p.output.recv()).await.unwrap() {
            out.extend_from_slice(&b);
        }
        assert!(String::from_utf8_lossy(&out).contains("got:selam"), "{}", String::from_utf8_lossy(&out));
        assert_eq!(wait_exit(p.exit).await, 7);
        env.finish().await;
    }

    #[tokio::test]
    async fn hangup_and_stop_close_shell() {
        let env = start(&[me()], 0);
        let p = dial_open(&env.sock, None, Open { cmd: "sleep 60".into(), ..Default::default() }).await.unwrap();
        p.ctl.send(Ctl::Hangup).unwrap();
        let code = tokio::time::timeout(Duration::from_secs(5), wait_exit(p.exit)).await.unwrap();
        assert_ne!(code, 0);
        // Hizmet dururken açık terminal kapatılır ve serve döner.
        let p = dial_open(&env.sock, None, Open { cmd: "sleep 60".into(), ..Default::default() }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), env.finish()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), wait_exit(p.exit)).await.unwrap();
    }

    #[tokio::test]
    async fn rejects_unknown_caller_and_limit() {
        let env = start(&[me().wrapping_add(4242)], 0);
        match dial_open(&env.sock, None, Open::default()).await {
            Err(DialError::Remote(m)) => assert_eq!(m, "Bu işlem için yetkiniz yok."),
            other => panic!("beklenmeyen: {:?}", other.err()),
        }
        env.finish().await;

        let env = start(&[me()], 1);
        let p = dial_open(&env.sock, None, Open { cmd: "sleep 60".into(), ..Default::default() }).await.unwrap();
        match dial_open(&env.sock, None, Open::default()).await {
            Err(DialError::Remote(m)) => assert_eq!(m, "Terminal oturumu sınırına ulaşıldı."),
            other => panic!("beklenmeyen: {:?}", other.err()),
        }
        p.ctl.send(Ctl::Hangup).unwrap();
        wait_exit(p.exit).await;
        env.finish().await;
    }

    #[tokio::test]
    async fn exec_with_stdin() {
        let env = start(&[me()], 0);
        let mut op = RemoteOp::dial(&env.sock, None, open("exec", "", "tr a-z A-Z; echo hata >&2; exit 3")).await.unwrap();
        op.send(b"maker").await.unwrap();
        op.close_input().await.unwrap();
        let (out, code) = op.drain().await.unwrap();
        assert_eq!(String::from_utf8_lossy(&out), "MAKERhata\n");
        assert_eq!(code, 3);
        env.finish().await;
    }

    #[tokio::test]
    async fn file_ops() {
        let env = start(&[me()], 0);
        let big: Vec<u8> = (0..(600 << 10)).map(|i| (i % 251) as u8).collect();
        let mut op = RemoteOp::dial(&env.sock, None, open("write", "a/b.bin", "")).await.unwrap();
        op.send(&big).await.unwrap();
        op.close_input().await.unwrap();
        assert_eq!(op.drain().await.unwrap(), (Vec::new(), 0));

        let (got, code) = RemoteOp::dial(&env.sock, None, open("read", "~/a/b.bin", "")).await.unwrap().drain().await.unwrap();
        assert_eq!(code, 0);
        assert!(got == big);

        let (l, _) = RemoteOp::dial(&env.sock, None, open("list", "a", "")).await.unwrap().drain().await.unwrap();
        let l: fsops::Listing = serde_json::from_slice(&l).unwrap();
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].size, big.len() as i64);

        match RemoteOp::dial(&env.sock, None, open("read", "yok", "")).await.unwrap().drain().await {
            Err(DialError::Remote(m)) => assert_eq!(m, "Dosya veya klasör bulunamadı."),
            other => panic!("beklenmeyen: {:?}", other.err()),
        }

        // 'k' gelmeden kopan yükleme yarım dosya bırakmaz.
        let mut op = RemoteOp::dial(&env.sock, None, open("write", "yarim.txt", "")).await.unwrap();
        op.send(b"yarim").await.unwrap();
        drop(op);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let names: Vec<String> = std::fs::read_dir(&env.dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(!names.iter().any(|n| n == "yarim.txt" || n.starts_with(".mk-yukleme-")), "{names:?}");
        env.finish().await;
    }
}
