//! makerd tarafı: makerd-user soketine bağlanan uzak PTY ve PTY dışı işlemler.

use super::*;
use std::path::Path;
use tokio::io::BufReader;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;

#[derive(Debug)]
pub enum DialError {
    /// Soket yok veya bağlantı reddedildi (hizmet çalışmıyor).
    NotRunning,
    /// Soketin karşı ucu beklenen kullanıcı değil (soket ele geçirilmiş olabilir).
    Peer,
    /// makerd-user'ın 'e' çerçevesiyle bildirdiği hata.
    Remote(String),
    Other(String),
}

impl std::fmt::Display for DialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialError::NotRunning => f.write_str("kullanıcı hizmeti çalışmıyor"),
            DialError::Peer => f.write_str("terminal hizmetinin kullanıcısı beklenen kullanıcı değil"),
            DialError::Remote(s) | DialError::Other(s) => f.write_str(s),
        }
    }
}

/// Bağlanır, karşı ucu doğrular (want_uid None ise denetlemez) ve açma çerçevesini gönderir.
async fn dial_raw(path: &Path, want_uid: Option<u32>, o: &Open) -> Result<(BufReader<OwnedReadHalf>, OwnedWriteHalf), DialError> {
    let conn = match tokio::time::timeout(Duration::from_secs(10), UnixStream::connect(path)).await {
        Err(_) => return Err(DialError::Other("bağlantı zaman aşımı".into())),
        Ok(Err(e)) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => return Err(DialError::NotRunning),
        Ok(Err(e)) => return Err(DialError::Other(e.to_string())),
        Ok(Ok(c)) => c,
    };
    if let Some(u) = want_uid {
        match conn.peer_cred() {
            Ok(c) if c.uid() == u => {}
            _ => return Err(DialError::Peer),
        }
    }
    let (r, mut w) = conn.into_split();
    tokio::time::timeout(Duration::from_secs(10), write_json(&mut w, FRAME_OPEN, o))
        .await
        .map_err(|_| DialError::Other("zaman aşımı".into()))?
        .map_err(|e| DialError::Other(e.to_string()))?;
    Ok((BufReader::with_capacity(64 << 10, r), w))
}

/// Yeni bir uzak PTY açar.
pub async fn dial_open(path: &Path, want_uid: Option<u32>, o: Open) -> Result<Proc, DialError> {
    let (mut r, mut w) = dial_raw(path, want_uid, &o).await?;
    let (typ, p) = tokio::time::timeout(Duration::from_secs(10), read_frame(&mut r))
        .await
        .map_err(|_| DialError::Other("terminal hizmeti yanıt vermedi".into()))?
        .map_err(|e| DialError::Other(format!("terminal hizmeti yanıt vermedi: {e}")))?;
    match typ {
        FRAME_PID => {}
        FRAME_ERROR => return Err(DialError::Remote(String::from_utf8_lossy(&p).into_owned())),
        t => return Err(DialError::Other(format!("beklenmeyen çerçeve {:?}", t as char))),
    }
    let pid = serde_json::from_slice::<Opened>(&p).map(|o| o.pid).unwrap_or(0);
    let (out_tx, out_rx) = mpsc::channel::<Bytes>(64);
    let (ctl_tx, mut ctl_rx) = mpsc::unbounded_channel::<Ctl>();
    let (exit_tx, exit_rx) = watch::channel(None);
    let (kill_tx, mut kill_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        let code = loop {
            let f = tokio::select! {
                f = read_frame(&mut r) => f,
                _ = &mut kill_rx => break -1,
            };
            match f {
                Err(_) => break -1,
                Ok((FRAME_DATA, p)) => {
                    if out_tx.send(Bytes::from(p)).await.is_err() {
                        break -1;
                    }
                }
                Ok((FRAME_EXIT, p)) => break serde_json::from_slice::<Exit>(&p).map(|e| e.code).unwrap_or(0),
                Ok(_) => {}
            }
        };
        let _ = exit_tx.send(Some(code));
    });

    let ex = exit_rx.clone();
    tokio::spawn(async move {
        let mut kill_tx = Some(kill_tx);
        while let Some(c) = ctl_rx.recv().await {
            let res = match c {
                Ctl::Data(b) => {
                    let mut res = Ok(());
                    for chunk in b.chunks(32 << 10) {
                        res = write_frame(&mut w, FRAME_DATA, chunk).await;
                        if res.is_err() {
                            break;
                        }
                    }
                    res
                }
                Ctl::Resize(c, r) => write_json(&mut w, FRAME_RESIZE, &Size { cols: c, rows: r }).await,
                Ctl::Hangup => {
                    let r = write_frame(&mut w, FRAME_HANGUP, &[]).await;
                    // Sunucu bağlantı kopunca süreci kapatır; yanıt gelmezse bağlantı kesilir.
                    let mut ex = ex.clone();
                    let k = kill_tx.take();
                    tokio::spawn(async move {
                        let done = async {
                            while ex.borrow().is_none() {
                                if ex.changed().await.is_err() {
                                    break;
                                }
                            }
                        };
                        if tokio::time::timeout(HANGUP_GRACE + Duration::from_secs(3), done).await.is_err() {
                            if let Some(k) = k {
                                let _ = k.send(());
                            }
                        }
                    });
                    r
                }
            };
            if res.is_err() {
                if let Some(k) = kill_tx.take() {
                    let _ = k.send(());
                }
                break;
            }
        }
    });

    Ok(Proc { pid, output: out_rx, ctl: ctl_tx, exit: exit_rx })
}

/// makerd-user'da açılmış PTY dışı işlem.
pub struct RemoteOp {
    r: BufReader<OwnedReadHalf>,
    w: OwnedWriteHalf,
}

/// İşlem çerçevesi.
pub enum OpFrame {
    Data(Vec<u8>),
    Exit(i32),
}

impl RemoteOp {
    pub async fn dial(path: &Path, want_uid: Option<u32>, o: Open) -> Result<Self, DialError> {
        let (r, w) = dial_raw(path, want_uid, &o).await?;
        Ok(Self { r, w })
    }

    /// Girdiyi 'd' çerçeveleri olarak gönderir.
    pub async fn send(&mut self, b: &[u8]) -> std::io::Result<()> {
        for c in b.chunks(256 << 10) {
            write_frame(&mut self.w, FRAME_DATA, c).await?;
        }
        Ok(())
    }

    /// Girdinin bittiğini bildirir ('k').
    pub async fn close_input(&mut self) -> std::io::Result<()> {
        write_frame(&mut self.w, FRAME_HANGUP, &[]).await
    }

    /// Sonraki çıktı çerçevesi; 'e' gelirse Remote hatası.
    pub async fn next(&mut self) -> Result<OpFrame, DialError> {
        loop {
            match read_frame(&mut self.r).await {
                Err(e) => return Err(DialError::Other(format!("kullanıcı hizmeti bağlantısı koptu: {e}"))),
                Ok((FRAME_DATA, p)) => return Ok(OpFrame::Data(p)),
                Ok((FRAME_ERROR, p)) => return Err(DialError::Remote(String::from_utf8_lossy(&p).into_owned())),
                Ok((FRAME_EXIT, p)) => return Ok(OpFrame::Exit(serde_json::from_slice::<Exit>(&p).map(|e| e.code).unwrap_or(0))),
                Ok(_) => {}
            }
        }
    }

    /// Tüm çıktıyı toplar ve çıkış kodunu döner.
    pub async fn drain(&mut self) -> Result<(Vec<u8>, i32), DialError> {
        let mut out = Vec::new();
        loop {
            match self.next().await? {
                OpFrame::Data(p) => out.extend_from_slice(&p),
                OpFrame::Exit(c) => return Ok((out, c)),
            }
        }
    }
}
