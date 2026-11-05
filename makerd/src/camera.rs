//! Raspberry Pi kamera modülleri (libcamera: Pi 4'te unicam+ISP, Pi 5'te
//! CFE+PiSP) ve libcamera'nın tanıdığı USB kameralar. Canlı görüntü
//! rpicam-vid'in MJPEG çıktısıdır: tek üretici, çok izleyici; son izleyici
//! ayrılınca kamera birkaç saniye içinde kapanır (CPU/güç boşa gitmez).
//! Donanım yoksa (Docker önizleme) sentetik görüntü üretilir.

use crate::apierr::{ApiError, ApiResult};
use bytes::Bytes;
use futures_util::future::BoxFuture;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Cam {
    pub index: i32,
    /// imx708, imx500 (AI Camera), ov5647 ...
    pub sensor: String,
    pub modes: String,
    pub path: String,
    /// IMX500: sensör üstü sinir ağı.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub ai: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Status {
    pub cams: Vec<Cam>,
    pub streaming: bool,
    pub viewers: usize,
    pub sim: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// Kare yayıcısı.
pub type Emit = Arc<dyn Fn(Bytes) + Send + Sync>;

/// Kare üreticisi.
pub trait Source: Send + Sync {
    fn list(&self) -> BoxFuture<'_, ApiResult<Vec<Cam>>>;
    /// Kareleri `emit`'e verir; `stop` iptal edilince döner.
    fn run(&self, w: u32, h: u32, fps: u32, emit: Emit, stop: CancellationToken) -> BoxFuture<'_, ApiResult<()>>;
    /// Tam çözünürlüklü fotoğraf (akış yokken).
    fn still(&self) -> BoxFuture<'_, ApiResult<Bytes>>;
    fn sim(&self) -> bool;
}

#[derive(Default)]
struct HubState {
    subs: Vec<(u64, mpsc::Sender<Bytes>)>,
    next_id: u64,
    last: Option<Bytes>,
    stop: Option<CancellationToken>,
    running: bool,
    idle: Option<tokio::task::JoinHandle<()>>,
}

/// Tek üretici, çok izleyici yayın merkezi.
pub struct Hub {
    pub src: Arc<dyn Source>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub linger: Duration,
    st: Arc<Mutex<HubState>>,
}

/// Bir izleyicinin abonelik tutamacı; düşürülünce ayrılır.
pub struct Subscription {
    pub frames: mpsc::Receiver<Bytes>,
    id: u64,
    st: Arc<Mutex<HubState>>,
    linger: Duration,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut st = self.st.lock().unwrap();
        st.subs.retain(|(i, _)| *i != self.id);
        if st.subs.is_empty() && st.running && st.idle.is_none() {
            let (inner, linger) = (self.st.clone(), self.linger);
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                st.idle = Some(rt.spawn(async move {
                    tokio::time::sleep(linger).await;
                    let mut st = inner.lock().unwrap();
                    st.idle = None;
                    if st.subs.is_empty() {
                        if let Some(t) = &st.stop {
                            t.cancel();
                        }
                    }
                }));
            }
        }
    }
}

impl Hub {
    /// Varsayılan 1280x720@30.
    pub fn new(src: Arc<dyn Source>) -> Self {
        Self { src, width: 1280, height: 720, fps: 30, linger: Duration::from_secs(3), st: Default::default() }
    }

    pub async fn status(&self) -> Status {
        let cams = self.src.list().await.unwrap_or_default();
        let st = self.st.lock().unwrap();
        Status { cams, streaming: st.running, viewers: st.subs.len(), sim: self.src.sim(), width: self.width, height: self.height, fps: self.fps }
    }

    /// İzleyici ekler (gerekirse üreticiyi başlatır). Son kare hemen gönderilir.
    pub fn subscribe(&self) -> Subscription {
        let (tx, rx) = mpsc::channel(2);
        let mut st = self.st.lock().unwrap();
        let id = st.next_id;
        st.next_id += 1;
        if let Some(t) = st.idle.take() {
            t.abort();
        }
        if let Some(l) = &st.last {
            let _ = tx.try_send(l.clone());
        }
        st.subs.push((id, tx));
        if !st.running {
            let tok = CancellationToken::new();
            st.stop = Some(tok.clone());
            st.running = true;
            let (src, inner, w, h, fps) = (self.src.clone(), self.st.clone(), self.width, self.height, self.fps);
            let pubst = self.st.clone();
            let emit: Emit = Arc::new(move |frame: Bytes| {
                let mut st = pubst.lock().unwrap();
                st.last = Some(frame.clone());
                for (_, c) in &st.subs {
                    // Yavaş izleyici kare atlar.
                    let _ = c.try_send(frame.clone());
                }
            });
            tokio::spawn(async move {
                if let Err(e) = src.run(w, h, fps, emit, tok).await {
                    tracing::warn!(err = %e.message, "kamera akışı durdu");
                }
                let mut st = inner.lock().unwrap();
                st.running = false;
                st.stop = None;
                // Kanallar kapanır: izleyiciler akışın bittiğini görür.
                st.subs.clear();
            });
        }
        Subscription { frames: rx, id, st: self.st.clone(), linger: self.linger }
    }

    /// Akış açıksa son kare, değilse tam çözünürlüklü fotoğraf.
    pub async fn photo(&self) -> ApiResult<Bytes> {
        let last = {
            let st = self.st.lock().unwrap();
            if st.running { st.last.clone() } else { None }
        };
        match last {
            Some(l) => Ok(l),
            None => self.src.still().await,
        }
    }
}

// --- rpicam ----------------------------------------------------------------

/// rpicam-apps arka ucu.
pub struct Rpicam;

/// `rpicam-hello --list-cameras` çıktısı.
/// Satır biçimi: `<n> : <sensör> [<kipler>] (<yol>)`.
pub fn parse_list(out: &str) -> Vec<Cam> {
    fn line(l: &str) -> Option<Cam> {
        let (idx, rest) = l.split_once(':')?;
        let index: i32 = idx.trim().parse().ok()?;
        let rest = rest.trim_start();
        let sensor_end = rest.find(|c: char| c.is_whitespace() || c == '[')?;
        let sensor = &rest[..sensor_end];
        let rest = rest[sensor_end..].trim_start().strip_prefix('[')?;
        let (modes, rest) = rest.split_once(']')?;
        let rest = rest.trim_start().strip_prefix('(')?;
        let (path, _) = rest.split_once(')')?;
        if sensor.is_empty() {
            return None;
        }
        Some(Cam { index, sensor: sensor.into(), modes: modes.into(), path: path.into(), ai: sensor.contains("imx500") })
    }
    out.lines().filter_map(line).collect()
}

impl Source for Rpicam {
    fn list(&self) -> BoxFuture<'_, ApiResult<Vec<Cam>>> {
        Box::pin(async {
            let run = tokio::process::Command::new("rpicam-hello").arg("--list-cameras").kill_on_drop(true).output();
            let out = match tokio::time::timeout(Duration::from_secs(8), run).await {
                Ok(Ok(o)) => {
                    let mut b = o.stdout;
                    b.extend_from_slice(&o.stderr);
                    b
                }
                Ok(Err(e)) => return Err(ApiError::unavailable(format!("rpicam-hello çalışmadı: {e}"))),
                Err(_) => return Err(ApiError::unavailable("rpicam-hello çalışmadı: zaman aşımı")),
            };
            Ok(parse_list(&String::from_utf8_lossy(&out)))
        })
    }

    fn sim(&self) -> bool {
        false
    }

    fn run(&self, w: u32, h: u32, fps: u32, emit: Emit, stop: CancellationToken) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let mut child = tokio::process::Command::new("rpicam-vid")
                .args(["-t", "0", "-n", "--codec", "mjpeg", "--quality", "80"])
                .args(["--width", &w.to_string(), "--height", &h.to_string(), "--framerate", &fps.to_string(), "-o", "-"])
                .stdout(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| ApiError::unavailable(format!("rpicam-vid başlatılamadı: {e}")))?;
            let mut out = child.stdout.take().ok_or_else(|| ApiError::internal("stdout yok"))?;
            let mut sp = JpegSplitter::default();
            let mut buf = vec![0u8; 256 << 10];
            loop {
                let n = tokio::select! {
                    _ = stop.cancelled() => break,
                    r = out.read(&mut buf) => r?,
                };
                if n == 0 {
                    break;
                }
                sp.push(&buf[..n], |f| emit(f));
            }
            let _ = child.start_kill();
            let _ = child.wait().await;
            Ok(())
        })
    }

    fn still(&self) -> BoxFuture<'_, ApiResult<Bytes>> {
        Box::pin(async {
            let run = tokio::process::Command::new("rpicam-still").args(["-n", "-t", "800", "--quality", "92", "-o", "-"]).kill_on_drop(true).output();
            match tokio::time::timeout(Duration::from_secs(15), run).await {
                Ok(Ok(o)) if o.status.success() && o.stdout.len() >= 4 => Ok(Bytes::from(o.stdout)),
                _ => Err(ApiError::unavailable("Fotoğraf çekilemedi (kamera takılı mı, başka program kullanıyor mu?).")),
            }
        })
    }
}

/// Ardışık JPEG akışını karelere (SOI 0xFFD8 .. EOI 0xFFD9) ayırır.
#[derive(Default)]
pub struct JpegSplitter {
    buf: Vec<u8>,
    prev: u8,
    inside: bool,
}

impl JpegSplitter {
    pub fn push(&mut self, data: &[u8], mut emit: impl FnMut(Bytes)) {
        for &b in data {
            if !self.inside {
                if self.prev == 0xFF && b == 0xD8 {
                    self.inside = true;
                    self.buf.clear();
                    self.buf.extend_from_slice(&[0xFF, 0xD8]);
                }
                self.prev = b;
                continue;
            }
            self.buf.push(b);
            if self.prev == 0xFF && b == 0xD9 {
                emit(Bytes::from(std::mem::take(&mut self.buf)));
                self.inside = false;
            }
            self.prev = b;
        }
    }
}

// --- benzetim ----------------------------------------------------------------

/// Donanımsız önizleme kaynağı (hareketli test görüntüsü).
pub struct Synthetic;

fn frame(t: f64, w: usize, h: usize) -> Bytes {
    let mut img = vec![0u8; w * h * 3];
    for y in 0..h {
        let v = (30.0 + 25.0 * y as f64 / h as f64) as u8;
        for x in 0..w {
            let i = (y * w + x) * 3;
            img[i] = v;
            img[i + 1] = v + 2;
            img[i + 2] = v + 6;
        }
    }
    let cx = (w as f64 / 2.0 + t.cos() * w as f64 / 3.0) as i64;
    let cy = (h as f64 / 2.0 + (t * 1.3).sin() * h as f64 / 4.0) as i64;
    for y in (cy - 40)..(cy + 40) {
        for x in (cx - 40)..(cx + 40) {
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && (x - cx).pow(2) + (y - cy).pow(2) < 1600 {
                let i = (y as usize * w + x as usize) * 3;
                img[i..i + 3].copy_from_slice(&[255, 122, 26]);
            }
        }
    }
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, 70);
    let _ = enc.encode(&img, w as u16, h as u16, jpeg_encoder::ColorType::Rgb);
    Bytes::from(out)
}

impl Source for Synthetic {
    fn sim(&self) -> bool {
        true
    }

    fn list(&self) -> BoxFuture<'_, ApiResult<Vec<Cam>>> {
        Box::pin(async { Ok(vec![Cam { index: 0, sensor: "benzetim".into(), modes: "640x360 15 FPS".into(), path: "test".into(), ai: false }]) })
    }

    fn run(&self, _w: u32, _h: u32, _fps: u32, emit: Emit, stop: CancellationToken) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let mut tk = tokio::time::interval(Duration::from_secs(1) / 15);
            let start = std::time::Instant::now();
            loop {
                tokio::select! {
                    _ = stop.cancelled() => return Ok(()),
                    _ = tk.tick() => {
                        let t = start.elapsed().as_secs_f64();
                        // Kodlama ~ms sürer: çalışma zamanını tıkamasın.
                        let f = tokio::task::spawn_blocking(move || frame(t, 640, 360)).await.unwrap_or_default();
                        emit(f);
                    }
                }
            }
        })
    }

    fn still(&self) -> BoxFuture<'_, ApiResult<Bytes>> {
        Box::pin(async { Ok(tokio::task::spawn_blocking(|| frame(1.0, 1280, 720)).await.unwrap_or_default()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parsing() {
        let out = "Available cameras\n-----------------\n0 : imx708_wide [4608x2592 10-bit RGGB] (/base/soc/i2c0mux/i2c@1/imx708@1a)\n    Modes: 'SRGGB10_CSI2P' : 1536x864 [120.13 fps]\n1 : imx500 [4056x3040 10-bit RGGB] (/base/axi/pcie@120000/rp1/i2c@88000/imx500@1a)\n";
        let c = parse_list(out);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].index, c[0].sensor.as_str(), c[0].modes.as_str(), c[0].ai), (0, "imx708_wide", "4608x2592 10-bit RGGB", false));
        assert!(c[1].ai);
    }

    #[test]
    fn split_jpeg_across_chunks() {
        let mut sp = JpegSplitter::default();
        let mut got = Vec::new();
        let stream = [&b"\x00\x01"[..], b"\xFF\xD8ab\xFF", b"\xD9junk\xFF\xD8c\xFF\xD9"].concat();
        for c in stream.chunks(3) {
            sp.push(c, |f| got.push(f));
        }
        assert_eq!(got, vec![Bytes::from_static(b"\xFF\xD8ab\xFF\xD9"), Bytes::from_static(b"\xFF\xD8c\xFF\xD9")]);
    }

    #[tokio::test]
    async fn hub_lifecycle() {
        let mut hub = Hub::new(Arc::new(Synthetic));
        hub.linger = Duration::from_millis(200);
        let mut a = hub.subscribe();
        let f = tokio::time::timeout(Duration::from_secs(3), a.frames.recv()).await.unwrap().unwrap();
        assert_eq!(&f[..2], b"\xFF\xD8");
        let b = hub.subscribe();
        let st = hub.status().await;
        assert!(st.streaming && st.sim);
        assert_eq!(st.viewers, 2);
        assert_eq!(&hub.photo().await.unwrap()[..2], b"\xFF\xD8");
        drop(a);
        drop(b);
        assert_eq!(hub.status().await.viewers, 0);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(!hub.status().await.streaming, "son izleyiciden sonra durmalı");
        // Akış yokken tam çözünürlüklü fotoğraf.
        assert!(hub.photo().await.unwrap().len() > 1000);
    }
}
