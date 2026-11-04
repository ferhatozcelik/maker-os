//! Kullanıcı adına iş yapan temsilci: komut çalıştırma ve ev dizinindeki dosya
//! işlemleri. Gerçek sistemde iş makerd-user@<kullanıcı> hizmetinde
//! (kullanıcının kendi hesabı ve izinleriyle) yapılır; mock modda ve `mos`
//! CLI'da aynı işlevler süreç içinde çalışır.

use crate::apierr::{ApiError, ApiResult};
use crate::termproc::fsops::{self, FsError};
use crate::termproc::login::Command;
use crate::termproc::remote::{DialError, OpFrame, RemoteOp};
use crate::termproc::{piped, Open};
use bytes::Bytes;
use futures_util::future::BoxFuture;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

/// Akış parçaları: komut stdin'i, indirilen dosya, komut çıktısı.
pub type Chunks = mpsc::Receiver<Bytes>;
/// Yükleme akışı: Err parçası yüklemenin yarıda kaldığını bildirir (dosya yazılmaz).
pub type Upload = mpsc::Receiver<std::io::Result<Bytes>>;

/// Kullanıcı bağlamında çalışan işlemler.
pub trait Agent: Send + Sync {
    /// cmd'yi kullanıcının giriş kabuğuyla (-l -c) çalıştırır; çıktı `out`'a gider. Çıkış kodu döner.
    fn exec<'a>(&'a self, user: &'a str, cmd: &'a str, stdin: Option<Chunks>, out: mpsc::Sender<Bytes>) -> BoxFuture<'a, ApiResult<i32>>;
    /// list/mkdir/rename/remove/stat; JSON yanıt.
    fn fs<'a>(&'a self, user: &'a str, op: Open) -> BoxFuture<'a, ApiResult<Vec<u8>>>;
    /// Dosyayı parça parça `out`'a yazar.
    fn read<'a>(&'a self, user: &'a str, path: &'a str, out: mpsc::Sender<Bytes>) -> BoxFuture<'a, ApiResult<()>>;
    /// Akışı dosyaya (atomik) yazar.
    fn write<'a>(&'a self, user: &'a str, path: &'a str, input: Upload) -> BoxFuture<'a, ApiResult<()>>;
}

/// Exec'i çalıştırıp tüm çıktıyı ve çıkış kodunu döner.
pub async fn output(a: &dyn Agent, user: &str, cmd: &str) -> ApiResult<(Vec<u8>, i32)> {
    let (tx, mut rx) = mpsc::channel::<Bytes>(16);
    let collect = async {
        let mut v = Vec::new();
        while let Some(b) = rx.recv().await {
            v.extend_from_slice(&b);
        }
        v
    };
    let (code, out) = tokio::join!(a.exec(user, cmd, None, tx), collect);
    Ok((out, code?))
}

/// Exec çıktısını JSON olarak çözer (çıkış kodu 0 olmalı).
pub async fn json<T: DeserializeOwned>(a: &dyn Agent, user: &str, cmd: &str) -> ApiResult<T> {
    let (out, code) = output(a, user, cmd).await?;
    if code != 0 {
        return Err(ApiError::unavailable(format!("Komut başarısız ({code}): {}", crate::util::tail(&out, 300))));
    }
    serde_json::from_slice(&out).map_err(|e| ApiError::internal(e.to_string()))
}

/// Dosyanın tamamını okur.
pub async fn read_all(a: &dyn Agent, user: &str, path: &str) -> ApiResult<Vec<u8>> {
    let (tx, mut rx) = mpsc::channel::<Bytes>(16);
    let collect = async {
        let mut v = Vec::new();
        while let Some(b) = rx.recv().await {
            v.extend_from_slice(&b);
        }
        v
    };
    let (r, v) = tokio::join!(a.read(user, path, tx), collect);
    r.map(|_| v)
}

/// İçeriği dosyaya (atomik) yazar.
pub async fn write_all(a: &dyn Agent, user: &str, path: &str, data: impl Into<Bytes>) -> ApiResult<()> {
    let (tx, rx) = mpsc::channel(1);
    let _ = tx.send(Ok(data.into())).await;
    drop(tx);
    a.write(user, path, rx).await
}

fn fs_api(e: FsError) -> ApiError {
    ApiError::invalid(e.0)
}

// --- süreç içi -----------------------------------------------------------

/// İşleri bu süreçte yapar (mock, CLI ve makerd-user yokken kendi kullanıcısı).
pub struct Local {
    /// Giriş kabuğu (termproc::login::login).
    pub command: Command,
    pub home: String,
}

impl Local {
    fn home(&self) -> String {
        if self.home.is_empty() { crate::termproc::login::home_of("/etc/passwd") } else { self.home.clone() }
    }
}

impl Agent for Local {
    fn exec<'a>(&'a self, _user: &'a str, cmd: &'a str, stdin: Option<Chunks>, out: mpsc::Sender<Bytes>) -> BoxFuture<'a, ApiResult<i32>> {
        Box::pin(async move {
            let spec = (self.command)().map_err(|e| ApiError::internal(e.to_string()))?.with_cmd(cmd);
            piped::run(&spec, stdin, out).await.map_err(|e| ApiError::internal(e.to_string()))
        })
    }

    fn fs<'a>(&'a self, _user: &'a str, op: Open) -> BoxFuture<'a, ApiResult<Vec<u8>>> {
        Box::pin(async move { fsops::fs_op(&self.home(), &op).map_err(fs_api) })
    }

    fn read<'a>(&'a self, _user: &'a str, path: &'a str, out: mpsc::Sender<Bytes>) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let mut f = tokio::fs::File::from_std(fsops::open_read(&self.home(), path).map_err(fs_api)?);
            let mut buf = vec![0u8; 256 << 10];
            loop {
                let n = f.read(&mut buf).await?;
                if n == 0 {
                    return Ok(());
                }
                if out.send(Bytes::copy_from_slice(&buf[..n])).await.is_err() {
                    return Err(ApiError::internal("okuyucu kapandı"));
                }
            }
        })
    }

    fn write<'a>(&'a self, _user: &'a str, path: &'a str, mut input: Upload) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let mut w = fsops::AtomicWriter::create(&self.home(), path).map_err(fs_api)?;
            while let Some(b) = input.recv().await {
                w.write(&b?).map_err(fs_api)?;
            }
            w.commit().map_err(fs_api)
        })
    }
}

// --- makerd-user ---------------------------------------------------------

/// İşleri kullanıcının makerd-user hizmetinde yapar. Hizmet çalışmıyorsa ve
/// kullanıcı `self_user` ise `fallback` kullanılır.
pub struct Remote {
    pub dir: PathBuf,
    pub uid_of: Arc<dyn Fn(&str) -> Option<u32> + Send + Sync>,
    pub self_user: String,
    pub fallback: Option<Arc<dyn Agent>>,
}

enum Dialed {
    Op(RemoteOp),
    Fallback(Arc<dyn Agent>),
}

fn remote_err(e: DialError) -> ApiError {
    match e {
        DialError::Remote(m) => ApiError::invalid(m),
        e => ApiError::internal(e.to_string()),
    }
}

impl Remote {
    async fn dial(&self, user: &str, o: Open) -> ApiResult<Dialed> {
        let uid = (self.uid_of)(user).ok_or_else(|| ApiError::forbidden("Kullanıcı bulunamadı."))?;
        match RemoteOp::dial(&self.dir.join(format!("{uid}.sock")), Some(uid), o).await {
            Ok(op) => Ok(Dialed::Op(op)),
            Err(DialError::NotRunning) if user == self.self_user && self.fallback.is_some() => Ok(Dialed::Fallback(self.fallback.clone().unwrap())),
            Err(DialError::Peer) => Err(ApiError::forbidden("Kullanıcı hizmeti soketi güvenilir değil.")),
            Err(DialError::Remote(m)) => Err(ApiError::invalid(m)),
            Err(_) => Err(ApiError::unavailable(format!("Kullanıcı hizmeti (makerd-user@{user}) çalışmıyor."))),
        }
    }
}

/// Çıktı çerçevelerini `out`'a aktarır; çıkış kodunu döner.
async fn pump(r: &mut crate::termproc::remote::OpReader, out: Option<&mpsc::Sender<Bytes>>) -> ApiResult<i32> {
    loop {
        match r.next().await.map_err(remote_err)? {
            OpFrame::Data(p) => {
                if let Some(o) = out {
                    if o.send(Bytes::from(p)).await.is_err() {
                        return Err(ApiError::internal("okuyucu kapandı"));
                    }
                }
            }
            OpFrame::Exit(c) => return Ok(c),
        }
    }
}

impl Agent for Remote {
    fn exec<'a>(&'a self, user: &'a str, cmd: &'a str, stdin: Option<Chunks>, out: mpsc::Sender<Bytes>) -> BoxFuture<'a, ApiResult<i32>> {
        Box::pin(async move {
            let op = match self.dial(user, Open { mode: "exec".into(), cmd: cmd.into(), ..Default::default() }).await? {
                Dialed::Fallback(fb) => return fb.exec(user, cmd, stdin, out).await,
                Dialed::Op(op) => op,
            };
            let (mut r, mut w) = op.split();
            let feed = tokio::spawn(async move {
                if let Some(mut rx) = stdin {
                    while let Some(b) = rx.recv().await {
                        if w.send(&b).await.is_err() {
                            return;
                        }
                    }
                }
                let _ = w.close_input().await;
                // Yazma yarısı, sunucu bitene kadar açık kalmalı (kapanırsa bağlantı yarım kapanır).
                std::future::pending::<()>().await;
            });
            let res = pump(&mut r, Some(&out)).await;
            feed.abort();
            res
        })
    }

    fn fs<'a>(&'a self, user: &'a str, o: Open) -> BoxFuture<'a, ApiResult<Vec<u8>>> {
        Box::pin(async move {
            match self.dial(user, o.clone()).await? {
                Dialed::Fallback(fb) => fb.fs(user, o).await,
                Dialed::Op(mut op) => op.drain().await.map(|(b, _)| b).map_err(remote_err),
            }
        })
    }

    fn read<'a>(&'a self, user: &'a str, path: &'a str, out: mpsc::Sender<Bytes>) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            match self.dial(user, Open { mode: "read".into(), path: path.into(), ..Default::default() }).await? {
                Dialed::Fallback(fb) => fb.read(user, path, out).await,
                Dialed::Op(op) => {
                    let (mut r, _w) = op.split();
                    pump(&mut r, Some(&out)).await.map(|_| ())
                }
            }
        })
    }

    fn write<'a>(&'a self, user: &'a str, path: &'a str, mut input: Upload) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let mut op = match self.dial(user, Open { mode: "write".into(), path: path.into(), ..Default::default() }).await? {
                Dialed::Fallback(fb) => return fb.write(user, path, input).await,
                Dialed::Op(op) => op,
            };
            while let Some(b) = input.recv().await {
                // Yükleme yarıda kaldı: 'k' gönderilmeden bağlantı kapanır, sunucu dosyayı yazmaz.
                let b = b?;
                op.send(&b).await?;
            }
            op.close_input().await?;
            op.drain().await.map(|_| ()).map_err(remote_err)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termproc::fsops::Listing;
    use crate::termproc::login::CmdSpec;
    use crate::termproc::server::Server;

    fn sh() -> Command {
        Arc::new(|| {
            Ok(CmdSpec { path: "/bin/sh".into(), argv0: "sh".into(), args: Vec::new(), dir: "/".into(), env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("LANG".into(), "C.UTF-8".into())] })
        })
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mk-ua-{tag}-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Aynı süreçte gerçek makerd-user sunucusu: Remote ve Local aynı sonucu vermeli.
    fn agents() -> (Vec<(&'static str, Arc<dyn Agent>)>, tokio::sync::watch::Sender<bool>) {
        let home = tmp("home");
        let dir = tmp("sock");
        let uid = crate::util::getuid();
        let ln = tokio::net::UnixListener::bind(dir.join(format!("{uid}.sock"))).unwrap();
        let srv = Arc::new(Server::new([uid].into(), sh(), 0, home.to_string_lossy().into_owned()));
        let (stop, rx) = tokio::sync::watch::channel(false);
        tokio::spawn(srv.serve(ln, rx));
        let local: Arc<dyn Agent> = Arc::new(Local { command: sh(), home: home.to_string_lossy().into_owned() });
        let remote: Arc<dyn Agent> = Arc::new(Remote { dir, uid_of: Arc::new(move |_| Some(uid)), self_user: String::new(), fallback: None });
        (vec![("local", local), ("remote", remote)], stop)
    }

    #[tokio::test]
    async fn agent_ops() {
        let (list, _stop) = agents();
        for (name, a) in list {
            let a = a.as_ref();
            let (out, code) = output(a, "u", "echo merhaba; exit 3").await.unwrap();
            assert_eq!((String::from_utf8_lossy(&out).trim(), code), ("merhaba", 3), "{name}");

            let (itx, irx) = mpsc::channel::<Bytes>(4);
            let (otx, mut orx) = mpsc::channel::<Bytes>(4);
            itx.send(Bytes::from_static(b"stdin akisi")).await.unwrap();
            drop(itx);
            let collect = async {
                let mut v = Vec::new();
                while let Some(b) = orx.recv().await {
                    v.extend_from_slice(&b);
                }
                v
            };
            let (code, got) = tokio::join!(a.exec("u", "tr a-z A-Z", Some(irx), otx), collect);
            assert_eq!((code.unwrap(), got.as_slice()), (0, &b"STDIN AKISI"[..]), "{name}");

            let big = b"0123456789".repeat(70000); // 700 KB: çerçevelere bölünür
            let (utx, urx) = mpsc::channel(4);
            let path = format!("{name}/dosya.bin");
            let up = big.clone();
            tokio::spawn(async move {
                for c in up.chunks(100_000) {
                    utx.send(Ok(Bytes::copy_from_slice(c))).await.unwrap();
                }
            });
            a.write("u", &path, urx).await.unwrap();

            let (otx, mut orx) = mpsc::channel::<Bytes>(4);
            let rpath = format!("~/{name}/dosya.bin");
            let collect = async {
                let mut v = Vec::new();
                while let Some(b) = orx.recv().await {
                    v.extend_from_slice(&b);
                }
                v
            };
            let (r, got) = tokio::join!(a.read("u", &rpath, otx), collect);
            r.unwrap();
            assert!(got == big, "{name}: {} bayt", got.len());

            let l: Listing = serde_json::from_slice(&a.fs("u", Open { mode: "list".into(), path: name.into(), ..Default::default() }).await.unwrap()).unwrap();
            assert_eq!(l.entries.len(), 1);
            assert_eq!((l.entries[0].name.as_str(), l.entries[0].size), ("dosya.bin", big.len() as i64));
            a.fs("u", Open { mode: "rename".into(), path: path.clone(), to: format!("{name}/yeni.bin"), ..Default::default() }).await.unwrap();
            let e = a.fs("u", Open { mode: "stat".into(), path: format!("{name}/yok"), ..Default::default() }).await.unwrap_err();
            assert_eq!((e.code, e.message.as_str()), ("invalid", "Dosya veya klasör bulunamadı."), "{name}");

            // Yarıda kalan yükleme dosya bırakmaz.
            let (utx, urx) = mpsc::channel(4);
            utx.send(Ok(Bytes::from_static(b"yarim"))).await.unwrap();
            utx.send(Err(std::io::Error::other("koptu"))).await.unwrap();
            drop(utx);
            assert!(a.write("u", &format!("{name}/yarim.txt"), urx).await.is_err());
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let l: Listing = serde_json::from_slice(&a.fs("u", Open { mode: "list".into(), path: name.into(), ..Default::default() }).await.unwrap()).unwrap();
            assert_eq!(l.entries.len(), 1, "{name}: {:?}", l.entries.iter().map(|e| &e.name).collect::<Vec<_>>());
        }
    }

    #[tokio::test]
    async fn remote_fallback_and_missing_service() {
        let local: Arc<dyn Agent> = Arc::new(Local { command: sh(), home: tmp("fb").to_string_lossy().into_owned() });
        let r = Remote { dir: tmp("empty"), uid_of: Arc::new(|_| Some(4321)), self_user: "maker".into(), fallback: Some(local) };
        assert_eq!(output(&r, "maker", "echo yedek").await.unwrap().0, b"yedek\n");
        assert_eq!(output(&r, "ayse", "true").await.unwrap_err().code, "unavailable");
        let r = Remote { dir: tmp("empty"), uid_of: Arc::new(|_| None), self_user: String::new(), fallback: None };
        assert_eq!(output(&r, "yok", "true").await.unwrap_err().code, "forbidden");
    }
}
