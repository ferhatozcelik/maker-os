//! makerd: MakerOS arka uç hizmeti ve yardımcıları.
//!
//! Üç ikili aynı kütüphaneyi paylaşır:
//! - `makerd`: yetkisiz web API'si (:80), web kabuğu ve WebSocket'ler.
//! - `makerd-priv`: soket etkinleştirmeli, sabit komutlu dar root yardımcı.
//! - `makerd-user`: kullanıcı başına hizmet (PTY terminal ve kullanıcı adına dosya işlemleri).

pub mod activity;
pub mod ai;
pub mod agents;
pub mod apierr;
pub mod apps;
pub mod auth;
pub mod autostart;
pub mod browse;
pub mod camera;
pub mod catalog;
pub mod dbus;
pub mod events;
pub mod flags;
pub mod gpio;
pub mod httpd;
pub mod jobs;
pub mod logging;
pub mod mock;
pub mod network;
pub mod notify;
pub mod ota;
pub mod plugins;
pub mod prefs;
pub mod privsep;
pub mod pyman;
pub mod sd;
pub mod setup;
pub mod system;
pub mod term;
pub mod termproc;
pub mod uagent;
pub mod util;

/// Sürüm: derlemede `MAKEROS_VERSION` ortam değişkeni (os/VERSION), yoksa paket sürümü.
pub const VERSION: &str = match option_env!("MAKEROS_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
