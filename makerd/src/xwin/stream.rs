//! makerd ↔ makerd-user akışları (çerçeveli; her 'd' çerçevesi bir WebSocket iletisidir:
//! '{' ile başlayan metin JSON, diğerleri ikili).
//!
//! Denetim akışı (path = ""):
//!   → {"t":"list","wins":[…]}, {"t":"open","win"}, {"t":"close","id"}, {"t":"title","id","title"},
//!     {"t":"geom","win"}, {"t":"activate","id"}, {"t":"minimize","id"}, {"t":"reset"}
//!   ← {"t":"close"|"focus","id"}, {"t":"dismiss"}, {"t":"blur"}
//!
//! Pencere akışı (path = pencere kimliği):
//!   → {"t":"geom","w","h"}, {"t":"cursor",…}, {"t":"gone"}
//!     ikili: [1, kodlama, x, y, w, h (u16 LE)] + yük   (kodlama 0: ham RGBA deflate, 1: JPEG)
//!            [2, seq (u32 LE)]                       (toplu güncellemenin sonu; istemci onaylar)
//!   ← {"t":"ack"}, {"t":"m"|"b"|"w","x","y",…}, {"t":"k","ks","d","m","p"}, {"t":"text","s"},
//!     {"t":"size","w","h"}, {"t":"focus"}, {"t":"blur"}, {"t":"vis","v"}, {"t":"refresh"}

use super::wm::{Bridge, Ctx, Msg};
use crate::termproc::{read_frame, write_frame, FRAME_DATA, FRAME_EXIT, FRAME_HANGUP};
use serde::Deserialize;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use x11rb::protocol::xproto::Rectangle;

/// Bir parçadaki en çok piksel: ham hâli (×4) çerçeve sınırının (1 MB) altında kalır.
const MAX_PX: usize = 200_000;
/// Onaylanmamış en çok toplu güncelleme (yavaş bağlantıda kareler birikmez, atlanır).
const MAX_INFLIGHT: u32 = 2;

pub async fn serve<R, W>(b: Arc<Bridge>, r: R, w: W, path: &str)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (otx, mut orx) = mpsc::channel::<Vec<u8>>(64);
    let writer = tokio::spawn(async move {
        let mut w = w;
        while let Some(m) = orx.recv().await {
            if write_frame(&mut w, FRAME_DATA, &m).await.is_err() {
                return;
            }
        }
        let _ = write_frame(&mut w, FRAME_EXIT, br#"{"code":0}"#).await;
    });
    // Okuma ayrı görevde: read_frame iptale dayanıklı değildir (select! içinde yarım kalmasın).
    let (itx, irx) = mpsc::channel::<Vec<u8>>(256);
    let reader = tokio::spawn(async move {
        let mut r = r;
        loop {
            match read_frame(&mut r).await {
                Ok((FRAME_DATA, p)) => {
                    if itx.send(p).await.is_err() {
                        break;
                    }
                }
                Ok((FRAME_HANGUP, _)) | Err(_) => break,
                _ => {}
            }
        }
    });
    if path.is_empty() {
        control(&b, irx, &otx).await;
    } else {
        match path.parse::<u32>() {
            Ok(wid) => window(&b, wid, irx, &otx).await,
            Err(_) => {
                let _ = otx.send(br#"{"t":"gone"}"#.to_vec()).await;
            }
        }
    }
    reader.abort();
    drop(otx);
    let _ = writer.await;
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct In {
    t: String,
    id: u32,
    x: i32,
    y: i32,
    b: u8,
    d: u8,
    dx: i32,
    dy: i32,
    ks: u32,
    m: u8,
    p: u8,
    w: u16,
    h: u16,
    s: String,
    v: bool,
}

async fn control(b: &Arc<Bridge>, mut input: mpsc::Receiver<Vec<u8>>, out: &mpsc::Sender<Vec<u8>>) {
    let mut rx = b.subscribe_ctl();
    loop {
        tokio::select! {
            m = rx.recv() => match m {
                Some(s) => if out.send(s.as_bytes().to_vec()).await.is_err() { break },
                None => break,
            },
            f = input.recv() => {
                let Some(p) = f else { break };
                let Ok(c) = serde_json::from_slice::<In>(&p) else { continue };
                let Some(ctx) = b.ctx() else { continue };
                match c.t.as_str() {
                    "close" => ctx.close(c.id),
                    "focus" => ctx.focus(c.id),
                    "dismiss" => ctx.dismiss(),
                    "blur" => ctx.release_all(),
                    _ => {}
                }
            }
        }
    }
}

/// Kodlayıcı durumu (toplu güncellemeler arasında taşınır).
#[derive(Default)]
struct Enc {
    /// Kayıpsız sıkıştırma iyi sonuç vermiyorsa (kamera, video, oyun) bir süre JPEG.
    photo_until: Option<Instant>,
    /// JPEG ile gönderilmiş bölgeler: hareket durunca kayıpsız yenilenir.
    lossy: Vec<Rectangle>,
    force_lossless: bool,
}

fn full(w: u16, h: u16) -> Rectangle {
    Rectangle { x: 0, y: 0, width: w, height: h }
}

/// Dikdörtgeni pencere sınırlarına kırpar.
fn clip(r: Rectangle, w: u16, h: u16) -> Option<Rectangle> {
    let x0 = (r.x as i32).max(0);
    let y0 = (r.y as i32).max(0);
    let x1 = (r.x as i32 + r.width as i32).min(w as i32);
    let y1 = (r.y as i32 + r.height as i32).min(h as i32);
    (x1 > x0 && y1 > y0).then(|| Rectangle { x: x0 as i16, y: y0 as i16, width: (x1 - x0) as u16, height: (y1 - y0) as u16 })
}

/// Bekleyen bölgelere ekler; çok parçalıysa tek sınır kutusuna indirger.
fn merge(p: &mut Vec<Rectangle>, add: impl IntoIterator<Item = Rectangle>, w: u16, h: u16) {
    p.extend(add.into_iter().filter_map(|r| clip(r, w, h)));
    if p.len() > 24 {
        let x0 = p.iter().map(|r| r.x as i32).min().unwrap_or(0);
        let y0 = p.iter().map(|r| r.y as i32).min().unwrap_or(0);
        let x1 = p.iter().map(|r| r.x as i32 + r.width as i32).max().unwrap_or(0);
        let y1 = p.iter().map(|r| r.y as i32 + r.height as i32).max().unwrap_or(0);
        *p = vec![Rectangle { x: x0 as i16, y: y0 as i16, width: (x1 - x0) as u16, height: (y1 - y0) as u16 }];
    }
}

fn geom_msg(w: u16, h: u16) -> Vec<u8> {
    serde_json::json!({"t": "geom", "w": w, "h": h}).to_string().into_bytes()
}

async fn wait_job<T: Send + 'static>(j: &mut Option<JoinHandle<T>>) -> Option<T> {
    match j {
        Some(h) => {
            let r = h.await.ok();
            *j = None;
            r
        }
        None => std::future::pending().await,
    }
}

async fn window(b: &Arc<Bridge>, wid: u32, mut input: mpsc::Receiver<Vec<u8>>, out: &mpsc::Sender<Vec<u8>>) {
    let gone = || br#"{"t":"gone"}"#.to_vec();
    let Some(ctx) = b.ctx() else {
        let _ = out.send(gone()).await;
        return;
    };
    let (Some(win), Some((sid, mut rx, cursor))) = (ctx.win(wid), ctx.subscribe(wid)) else {
        let _ = out.send(gone()).await;
        return;
    };
    let mut size = (win.w, win.h);
    let _ = out.send(geom_msg(size.0, size.1)).await;
    if let Some(c) = cursor {
        let _ = out.send(c.as_bytes().to_vec()).await;
    }
    let mut pending = vec![full(size.0, size.1)];
    let (mut inflight, mut seq, mut visible) = (0u32, 0u32, true);
    let mut enc = Some(Enc::default());
    let mut job: Option<JoinHandle<(Vec<Vec<u8>>, Enc)>> = None;
    let mut refresh: Option<tokio::time::Instant> = None;
    'main: loop {
        if job.is_none() && visible && inflight < MAX_INFLIGHT && !pending.is_empty() {
            let rects = std::mem::take(&mut pending);
            let (c, mut e, s, sz) = (ctx.clone(), enc.take().unwrap_or_default(), seq.wrapping_add(1), size);
            seq = s;
            job = Some(tokio::task::spawn_blocking(move || {
                let m = encode(&c, wid, sz, rects, &mut e, s);
                (m, e)
            }));
        }
        let refresh_at = refresh.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            res = wait_job(&mut job) => {
                if let Some((msgs, e)) = res {
                    refresh = (!e.lossy.is_empty()).then(|| tokio::time::Instant::now() + Duration::from_millis(400));
                    enc = Some(e);
                    if !msgs.is_empty() {
                        inflight += 1;
                        for m in msgs {
                            if out.send(m).await.is_err() {
                                break 'main;
                            }
                        }
                    }
                } else {
                    enc = Some(Enc::default());
                }
            }
            m = rx.recv() => match m {
                Some(Msg::Damage(rs)) => merge(&mut pending, rs, size.0, size.1),
                Some(Msg::Geom(w, h)) => {
                    size = (w, h);
                    pending = vec![full(w, h)];
                    if out.send(geom_msg(w, h)).await.is_err() {
                        break;
                    }
                }
                Some(Msg::Cursor(c)) => {
                    if out.send(c.as_bytes().to_vec()).await.is_err() {
                        break;
                    }
                }
                Some(Msg::Gone) | None => {
                    let _ = out.send(gone()).await;
                    break;
                }
            },
            f = input.recv() => {
                let Some(p) = f else { break };
                let Ok(i) = serde_json::from_slice::<In>(&p) else { continue };
                match i.t.as_str() {
                    "ack" => inflight = inflight.saturating_sub(1),
                    "m" => ctx.motion(wid, i.x, i.y, i.m),
                    "b" => ctx.button(wid, i.x, i.y, i.b, i.d == 1, i.m),
                    "w" => ctx.wheel(wid, i.x, i.y, i.dx, i.dy, i.m),
                    "k" => ctx.key(wid, i.ks, i.d == 1, i.m, i.p == 1),
                    "text" => ctx.text(wid, &i.s),
                    "size" => ctx.resize(wid, i.w, i.h),
                    "focus" => ctx.focus(wid),
                    "blur" => ctx.release_all(),
                    "vis" => visible = i.v,
                    "refresh" => { pending = vec![full(size.0, size.1)]; inflight = 0; }
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(refresh_at), if refresh.is_some() && job.is_none() && pending.is_empty() => {
                refresh = None;
                if let Some(e) = enc.as_mut() {
                    let lossy = std::mem::take(&mut e.lossy);
                    e.force_lossless = true;
                    e.photo_until = None;
                    merge(&mut pending, lossy, size.0, size.1);
                }
            }
        }
    }
    if let Some(j) = job {
        j.abort();
    }
    ctx.unsubscribe(wid, sid);
}

fn header(enc: u8, r: Rectangle, cap: usize) -> Vec<u8> {
    let mut m = Vec::with_capacity(10 + cap);
    m.push(1);
    m.push(enc);
    for v in [r.x as u16, r.y as u16, r.width, r.height] {
        m.extend_from_slice(&v.to_le_bytes());
    }
    m
}

/// Bölgeleri okuyup kodlar; boş dönerse gönderilecek bir şey yoktur.
fn encode(c: &Ctx, wid: u32, (w, h): (u16, u16), rects: Vec<Rectangle>, e: &mut Enc, seq: u32) -> Vec<Vec<u8>> {
    let lossless = std::mem::take(&mut e.force_lossless);
    let now = Instant::now();
    let photo = !lossless && e.photo_until.is_some_and(|t| t > now);
    let (mut raw, mut packed) = (0usize, 0usize);
    let mut out = Vec::new();
    for r in rects.into_iter().filter_map(|r| clip(r, w, h)) {
        let rows = (MAX_PX / r.width.max(1) as usize).clamp(1, r.height as usize) as u16;
        let mut y = r.y as i32;
        let end = r.y as i32 + r.height as i32;
        while y < end {
            let sh = rows.min((end - y) as u16);
            let sub = Rectangle { x: r.x, y: y as i16, width: r.width, height: sh };
            y += sh as i32;
            let Some((mut data, depth)) = c.grab(wid, sub) else { continue };
            let px = sub.width as usize * sub.height as usize;
            data.truncate(px * 4);
            if photo && depth != 32 && px >= 4096 {
                let mut m = header(1, sub, data.len() / 8);
                if jpeg_encoder::Encoder::new(&mut m, 78).encode(&data, sub.width, sub.height, jpeg_encoder::ColorType::Bgra).is_ok() {
                    e.lossy.push(sub);
                    out.push(m);
                    continue;
                }
            }
            for p in data.chunks_exact_mut(4) {
                p.swap(0, 2);
                if depth != 32 {
                    p[3] = 255;
                }
            }
            let mut z = flate2::write::DeflateEncoder::new(header(0, sub, data.len() / 4), flate2::Compression::new(1));
            if z.write_all(&data).is_err() {
                continue;
            }
            let Ok(m) = z.finish() else { continue };
            raw += data.len();
            packed += m.len();
            out.push(m);
        }
    }
    // Büyük ve iyi sıkışmayan güncellemeler (fotoğraf/video): bir süre JPEG'e geç.
    if !lossless && raw >= 256 << 10 && packed * 4 > raw {
        e.photo_until = Some(now + Duration::from_millis(1500));
    }
    if !out.is_empty() {
        let mut m = vec![2u8];
        m.extend_from_slice(&seq.to_le_bytes());
        out.push(m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_and_merge() {
        let r = |x, y, w, h| Rectangle { x, y, width: w, height: h };
        assert_eq!(clip(r(-5, -5, 10, 10), 100, 100).map(|r| (r.x, r.y, r.width, r.height)), Some((0, 0, 5, 5)));
        assert!(clip(r(200, 0, 10, 10), 100, 100).is_none());
        let mut p = Vec::new();
        merge(&mut p, (0..30).map(|i| r(i, i, 2, 2)), 100, 100);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].x, p[0].y, p[0].width, p[0].height), (0, 0, 31, 31));
    }
}
