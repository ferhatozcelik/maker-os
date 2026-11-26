//! makerd ↔ makerd-user ses akışı (`Open { mode: "audio" }`; her 'd' çerçevesi bir WebSocket iletisidir).
//!
//!   → {"t":"fmt","rate":48000,"ch":2,"in_rate":48000,"in_ch":1}   bağlanınca
//!     ikili: çıkış PCM (s16le, araya geçmeli stereo)               yalnız hoparlör açıkken
//!   ← {"t":"spk","on":bool}, {"t":"mic","on":bool}
//!     ikili: mikrofon PCM (s16le mono)                             yalnız mikrofon açıkken

use super::{Sound, IN_CH, OUT_CH, RATE};
use crate::termproc::{read_frame, write_frame, FRAME_DATA, FRAME_EXIT, FRAME_HANGUP};
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};

#[derive(Deserialize, Default)]
#[serde(default)]
struct In {
    t: String,
    on: bool,
}

pub async fn serve<R, W>(s: Arc<Sound>, r: R, w: W)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let id = s.new_client();
    let (otx, mut orx) = mpsc::channel::<Vec<u8>>(16);
    let writer = tokio::spawn(async move {
        let mut w = w;
        while let Some(m) = orx.recv().await {
            if write_frame(&mut w, FRAME_DATA, &m).await.is_err() {
                return;
            }
        }
        let _ = write_frame(&mut w, FRAME_EXIT, br#"{"code":0}"#).await;
    });
    let fmt = serde_json::json!({"t": "fmt", "rate": RATE, "ch": OUT_CH, "in_rate": RATE, "in_ch": IN_CH}).to_string();
    if otx.send(fmt.into_bytes()).await.is_err() {
        return;
    }
    // Okuma ayrı görevde: read_frame iptale dayanıklı değildir.
    let (itx, mut irx) = mpsc::channel::<Vec<u8>>(64);
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
    let mut out: Option<broadcast::Receiver<Bytes>> = None;
    loop {
        tokio::select! {
            m = irx.recv() => {
                let Some(m) = m else { break };
                if m.first() == Some(&b'{') {
                    let Ok(c) = serde_json::from_slice::<In>(&m) else { continue };
                    match c.t.as_str() {
                        "spk" => out = c.on.then(|| s.subscribe()),
                        "mic" if c.on => s.mic_on(id),
                        "mic" => s.mic_off(id),
                        _ => {}
                    }
                } else if !m.is_empty() {
                    s.mic_data(id, Bytes::from(m));
                }
            }
            b = async { out.as_mut().unwrap().recv().await }, if out.is_some() => match b {
                // Yavaş bağlantı: kuyruk doluysa parça atlanır (ses kesilir ama gecikme birikmez).
                Ok(b) => { let _ = otx.try_send(b.to_vec()); }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    s.mic_off(id);
    reader.abort();
    drop(otx);
    let _ = writer.await;
}
