//! makerd: MakerOS arka uç hizmeti ve yardımcıları.
//!
//! Üç ikili aynı kütüphaneyi paylaşır:
//! - `makerd`: yetkisiz web API'si (:80), web kabuğu ve WebSocket'ler.
//! - `makerd-priv`: soket etkinleştirmeli, sabit komutlu dar root yardımcı.
//! - `makerd-user`: kullanıcı başına hizmet (PTY terminal ve kullanıcı adına dosya işlemleri).

pub mod activity;
pub mod agents;
pub mod apierr;
// pub mod apps; // Go'dan taşınacak
pub mod auth;
pub mod browser;
// pub mod camera; // Go'dan taşınacak
pub mod dbus;
pub mod events;
pub mod flags;
// pub mod gpio; // Go'dan taşınacak
// pub mod httpd; // Go'dan taşınacak
pub mod jobs;
pub mod logging;
// pub mod mock; // Go'dan taşınacak
// pub mod network; // Go'dan taşınacak
pub mod notify;
// pub mod ota; // Go'dan taşınacak
// pub mod plugins; // Go'dan taşınacak
pub mod prefs;
pub mod privsep;
// pub mod pyman; // Go'dan taşınacak
pub mod screen;
pub mod sd;
pub mod setup;
// pub mod system; // Go'dan taşınacak
// pub mod term; // Go'dan taşınacak
pub mod termproc;
pub mod uagent;
pub mod util;

/// Sürüm: derlemede `MAKEROS_VERSION` ortam değişkeni (os/VERSION), yoksa paket sürümü.
pub const VERSION: &str = match option_env!("MAKEROS_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
