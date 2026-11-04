//! Günlük: stderr'e metin veya JSON (journald satır satır alır).

use tracing::Level;

pub fn init(level: &str, format: &str) {
    let lvl = match level.to_ascii_lowercase().as_str() {
        "debug" => Level::DEBUG,
        "warn" | "warning" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    let b = tracing_subscriber::fmt().with_max_level(lvl).with_writer(std::io::stderr).with_target(false).with_ansi(false);
    if format == "json" {
        let _ = b.json().flatten_event(true).try_init();
    } else {
        // systemd zamanı zaten yazar; metin biçimde zaman damgası yok.
        let _ = b.without_time().try_init();
    }
}
