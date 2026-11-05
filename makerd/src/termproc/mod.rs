//! Terminal süreçlerinin ortak arayüzü: yerel PTY ve makerd-user'a unix soketi
//! üzerinden çerçeveli protokolle bağlanan uzak PTY. makerd-user sunucu
//! tarafını da buradan kullanır.
//!
//! Çerçeve: 1 bayt tür + 4 bayt (big-endian) uzunluk + yük.
//!   makerd → makerd-user: 'o' aç (ilk çerçeve), 'd' girdi, 'r' boyut {cols,rows}, 'k' kapat (SIGHUP)
//!   makerd-user → makerd: 'p' {pid} (açıldı), 'e' hata metni, 'd' çıktı, 'x' {code} (süreç bitti)

pub mod fsops;
pub mod login;
pub mod piped;
pub mod pty;
pub mod remote;
pub mod server;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

pub const FRAME_OPEN: u8 = b'o';
pub const FRAME_PID: u8 = b'p';
pub const FRAME_ERROR: u8 = b'e';
pub const FRAME_DATA: u8 = b'd';
pub const FRAME_RESIZE: u8 = b'r';
pub const FRAME_HANGUP: u8 = b'k';
pub const FRAME_EXIT: u8 = b'x';

/// En büyük çerçeve yükü.
pub const MAX_FRAME: usize = 1 << 20;

/// SIGHUP'tan SIGKILL'e kadar beklenen süre.
pub const HANGUP_GRACE: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Size {
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub rows: u16,
}

/// İlk çerçevenin gövdesi. `mode` boşsa PTY açılır (cmd boşsa giriş kabuğu,
/// doluysa "kabuk -lc cmd"). Diğer kipler kullanıcı adına iş yapar:
///   exec    cmd borularla (PTY yok); 'd' stdin, 'k' stdin sonu; çıktı 'd', sonunda 'x'.
///   read    path dosyası 'd' parçaları olarak gelir, sonra 'x'.
///   write   makerd 'd' parçaları gönderir, 'k' ile bitirir; yanıt 'x'.
///   list, mkdir, rename (path→to), remove, stat: tek 'd' JSON yanıtı + 'x'.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Open {
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub rows: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cmd: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Opened {
    #[serde(default)]
    pub pid: i32,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Exit {
    #[serde(default)]
    pub code: i32,
}

pub async fn write_frame<W: AsyncWrite + Unpin>(w: &mut W, typ: u8, payload: &[u8]) -> std::io::Result<()> {
    if payload.len() > MAX_FRAME {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "çerçeve çok büyük"));
    }
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.push(typ);
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(payload);
    w.write_all(&buf).await
}

pub async fn write_json<W: AsyncWrite + Unpin, T: Serialize>(w: &mut W, typ: u8, v: &T) -> std::io::Result<()> {
    write_frame(w, typ, &serde_json::to_vec(v).map_err(std::io::Error::other)?).await
}

pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 5];
    r.read_exact(&mut hdr).await?;
    let n = u32::from_be_bytes([hdr[1], hdr[2], hdr[3], hdr[4]]) as usize;
    if n > MAX_FRAME {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "çerçeve çok büyük"));
    }
    let mut p = vec![0u8; n];
    r.read_exact(&mut p).await?;
    Ok((hdr[0], p))
}

/// Çalışan terminal sürecine komutlar.
#[derive(Debug)]
pub enum Ctl {
    Data(Bytes),
    Resize(u16, u16),
    /// Süreç grubuna SIGHUP; kısa süre içinde çıkmazsa öldürülür.
    Hangup,
}

/// Çalışan bir terminal süreci (yerel veya uzak).
pub struct Proc {
    pub pid: i32,
    /// PTY çıktısı; süreç bitince kapanır.
    pub output: mpsc::Receiver<Bytes>,
    pub ctl: mpsc::UnboundedSender<Ctl>,
    /// Süreç bitince Some(çıkış kodu).
    pub exit: watch::Receiver<Option<i32>>,
}

/// Çıkış durumundan kod (sinyalle bitişte 128+n).
pub fn exit_code(st: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    if let Some(c) = st.code() {
        c
    } else if let Some(s) = st.signal() {
        128 + s
    } else {
        -1
    }
}

/// Boyutu 1..=1000 aralığında tutar.
pub fn clamp_size(v: i64, def: u16) -> u16 {
    if v <= 0 || v > 1000 { def } else { v as u16 }
}
