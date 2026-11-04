//! makerd-core: makerd, makerd-priv ve makerd-user arasında paylaşılan kod.
//!
//! - [`privproto`]: makerd ↔ makerd-priv tek satır JSON protokolü (istemci dahil)
//! - [`frame`], [`pty`], [`termserver`]: makerd ↔ makerd-user çerçeveli PTY protokolü
//! - [`crypt`]: SHA-512 crypt (`$6$`) doğrulama ve üretim
//! - [`sd`]: sd_notify ve soket devri (saf Rust)
//! - [`valid`]: ortak doğrulayıcılar (kullanıcı adı, cihaz adı, dil …)

pub mod b64;
pub mod clock;
pub mod crypt;
pub mod flags;
pub mod frame;
pub mod logging;
pub mod passwd;
pub mod privproto;
pub mod pty;
pub mod rand;
pub mod sd;
pub mod sys;
pub mod termserver;
pub mod timefmt;
pub mod valid;

/// Derleme sırasında `MAKEROS_VERSION` ortam değişkeniyle ayarlanır.
pub const VERSION: &str = match option_env!("MAKEROS_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
