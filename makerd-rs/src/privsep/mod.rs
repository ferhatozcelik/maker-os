//! makerd ile kök yetkili dar yardımcı makerd-priv arasındaki protokol.
//! Unix soketi üzerinden bağlantı başına tek satır JSON istek ve tek satır JSON
//! yanıt; komutlar sabit bir izin listesindedir.

pub mod client;
pub mod commands;
pub mod crypt;
pub mod server;
pub mod shadow;
pub mod users;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CMD_AUTH_CHECK: &str = "auth.check";
pub const CMD_SYSTEM_PASSWORD: &str = "system.password";
pub const CMD_SYSTEM_HOSTNAME: &str = "system.hostname";
pub const CMD_POWER: &str = "power";
/// İlk açılış sihirbazının yönetici hesabını oluşturur; kurulumdan sonra reddedilir.
pub const CMD_USER_CREATE: &str = "user.create";
/// İmzalı RAUC paketini (yalnız OTA_DIR altındaki .raucb) kurmaya başlatır; hemen döner.
pub const CMD_OTA_INSTALL: &str = "ota.install";
/// RAUC kurulum durumu.
pub const CMD_OTA_STATUS: &str = "ota.status";

/// makerd'nin güncelleme paketlerini indirdiği dizin.
pub const OTA_DIR: &str = "/var/lib/makerd/ota";
pub const DEFAULT_SOCKET: &str = "/run/makeros/priv.sock";
pub const MIN_PASSWORD_LEN: usize = 6;

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub args: Value,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Response {
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<PrivError>,
}

/// makerd-priv hatası (code: invalid, forbidden, not_found, conflict, internal).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PrivError {
    pub code: String,
    pub message: String,
}

impl PrivError {
    pub fn new(code: &str, msg: impl Into<String>) -> Self {
        Self { code: code.into(), message: msg.into() }
    }
}

impl std::fmt::Display for PrivError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PrivError {}

impl From<std::io::Error> for PrivError {
    fn from(e: std::io::Error) -> Self {
        PrivError::new("internal", e.to_string())
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct AuthCheckArgs {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct AuthCheckResult {
    #[serde(default)]
    pub ok: bool,
}

/// system.password; `user` boşsa çağıranın kullanıcısı.
#[derive(Serialize, Deserialize, Default)]
pub struct PasswordArgs {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default)]
    pub old: String,
    #[serde(default)]
    pub new: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct HostnameArgs {
    #[serde(default)]
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct OtaInstallArgs {
    #[serde(default)]
    pub path: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct OtaStatus {
    #[serde(default)]
    pub operation: String,
    #[serde(default)]
    pub progress: i32,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub last_error: String,
}

/// power: reboot | poweroff | tryboot (yeni slottan tek seferlik deneme açılışı).
#[derive(Serialize, Deserialize, Default)]
pub struct PowerArgs {
    #[serde(default)]
    pub action: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct UserCreateArgs {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fullname: String,
}

/// created=false: kurulumdan önce var olan (ör. kilitli) bir hesap güncellendi.
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct UserCreateResult {
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub created: bool,
    /// makerd-user@<user>.service etkinleştirildi mi.
    #[serde(default)]
    pub service: bool,
}

pub use users::{valid_fullname, valid_new_password, valid_username};

/// Küçük harf/rakam/'-' ile en fazla 63 karakterlik ad.
pub fn valid_hostname(name: &str) -> bool {
    let b = name.as_bytes();
    let ok = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty() && b.len() <= 63 && ok(&b[0]) && ok(&b[b.len() - 1]) && b.iter().all(|c| ok(c) || *c == b'-')
}

/// Yalnız OTA_DIR altındaki düz .raucb dosyaları.
pub fn valid_ota_path(p: &str) -> bool {
    let Some(rest) = p.strip_prefix(&format!("{OTA_DIR}/")) else { return false };
    p.ends_with(".raucb") && !rest.is_empty() && !rest.contains(['/', '\\', '\0']) && !rest.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames() {
        assert!(valid_hostname("makeros-ab12"));
        assert!(valid_hostname("a"));
        assert!(!valid_hostname("-a"));
        assert!(!valid_hostname("a-"));
        assert!(!valid_hostname("Maker"));
        assert!(!valid_hostname(&"a".repeat(64)));
    }

    #[test]
    fn ota_paths() {
        assert!(valid_ota_path("/var/lib/makerd/ota/makeros-2.0.1-rpi4.raucb"));
        assert!(!valid_ota_path("/var/lib/makerd/ota/../x.raucb"));
        assert!(!valid_ota_path("/var/lib/makerd/ota/.x.raucb"));
        assert!(!valid_ota_path("/tmp/x.raucb"));
        assert!(!valid_ota_path("/var/lib/makerd/ota/x.img"));
    }
}
