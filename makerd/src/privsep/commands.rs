//! makerd-priv komutlarının gerçek (kök) uygulaması.

use super::server::{Handler, Peer};
use super::shadow::{self, ShadowError};
use super::users::{self, UserFiles};
use super::*;
use crate::dbus;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use zbus::zvariant::Value as ZValue;

pub struct Commands {
    pub shadow: String,
    pub hostname: String,
    pub hosts: String,
    /// Bu UID altındaki (sistem) hesapların parolası değiştirilemez ve doğrulanamaz.
    pub min_uid: u32,
    /// Başarısız doğrulamadan sonra bekleme (kaba kuvvete karşı).
    pub fail_delay: Duration,
    pub users: UserFiles,
}

impl Default for Commands {
    fn default() -> Self {
        Self {
            shadow: "/etc/shadow".into(),
            hostname: "/etc/hostname".into(),
            hosts: "/etc/hosts".into(),
            min_uid: 1000,
            fail_delay: Duration::from_millis(300),
            users: UserFiles::default(),
        }
    }
}

fn decode<T: DeserializeOwned>(raw: &Value) -> Result<T, PrivError> {
    if raw.is_null() {
        return Err(PrivError::new("invalid", "Argüman eksik."));
    }
    serde_json::from_value(raw.clone()).map_err(|_| PrivError::new("invalid", "Geçersiz argüman."))
}

fn ok<T: serde::Serialize>(v: T) -> Result<Value, PrivError> {
    Ok(serde_json::to_value(v).unwrap_or(Value::Null))
}

fn empty() -> Result<Value, PrivError> {
    Ok(serde_json::json!({}))
}

impl Commands {
    /// Komut izin listesi.
    pub fn handlers(self) -> HashMap<&'static str, Handler> {
        let c = Arc::new(self);
        let mut m: HashMap<&'static str, Handler> = HashMap::new();
        macro_rules! h {
            ($name:expr, $f:ident) => {{
                let c = c.clone();
                m.insert($name, Arc::new(move |peer: Peer, raw: Value| {
                    let c = c.clone();
                    Box::pin(async move { c.$f(peer, raw).await })
                }));
            }};
        }
        h!(CMD_AUTH_CHECK, auth_check);
        h!(CMD_SYSTEM_PASSWORD, password);
        h!(CMD_SYSTEM_HOSTNAME, hostname);
        h!(CMD_POWER, power);
        h!(CMD_USER_CREATE, user_create);
        h!(CMD_OTA_INSTALL, ota_install);
        h!(CMD_OTA_STATUS, ota_status);
        m
    }

    fn login_user(&self, name: &str) -> bool {
        crate::util::uid_of(name).is_some_and(|u| u >= self.min_uid && u < 60000)
    }

    async fn auth_check(self: Arc<Self>, _p: Peer, raw: Value) -> Result<Value, PrivError> {
        let a: AuthCheckArgs = decode(&raw)?;
        let mut good = false;
        if !a.user.is_empty() && self.login_user(&a.user) {
            let me = self.clone();
            let r = tokio::task::spawn_blocking(move || shadow::check_shadow(&me.shadow, &a.user, &a.password)).await.map_err(|e| PrivError::new("internal", e.to_string()))?;
            good = match r {
                Ok(v) => v,
                Err(ShadowError::Unsupported) => return Err(PrivError::new("internal", "Parola özeti biçimi desteklenmiyor (SHA-512 gerekli).")),
                Err(e) => return Err(PrivError::new("internal", e.to_string())),
            };
        }
        if !good && !self.fail_delay.is_zero() {
            tokio::time::sleep(self.fail_delay).await;
        }
        ok(AuthCheckResult { ok: good })
    }

    async fn password(self: Arc<Self>, peer: Peer, raw: Value) -> Result<Value, PrivError> {
        let a: PasswordArgs = decode(&raw)?;
        let mut target = peer.user.clone();
        if peer.uid == 0 && !a.user.is_empty() {
            target = a.user.clone();
        }
        if !a.user.is_empty() && a.user != target {
            return Err(PrivError::new("forbidden", "Yalnızca kendi parolanızı değiştirebilirsiniz."));
        }
        if target.is_empty() || !self.login_user(&target) {
            return Err(PrivError::new("forbidden", "Bu hesabın parolası değiştirilemez."));
        }
        if !valid_new_password(&a.new) {
            return Err(PrivError::new("invalid", "Yeni parola en az 6 karakter olmalıdır."));
        }
        let me = self.clone();
        let res = tokio::task::spawn_blocking(move || -> Result<bool, PrivError> {
            let good = match shadow::check_shadow(&me.shadow, &target, &a.old) {
                Ok(v) => v,
                Err(ShadowError::Unsupported) => false,
                Err(e) => return Err(PrivError::new("internal", e.to_string())),
            };
            if !good {
                return Ok(false);
            }
            let h = crypt::hash_password(&a.new);
            shadow::set_shadow_hash(&me.shadow, &target, &h, jiff::Timestamp::now().as_second()).map_err(|e| PrivError::new("internal", e.to_string()))?;
            Ok(true)
        })
        .await
        .map_err(|e| PrivError::new("internal", e.to_string()))??;
        if !res {
            if !self.fail_delay.is_zero() {
                tokio::time::sleep(self.fail_delay).await;
            }
            return Err(PrivError::new("forbidden", "Mevcut parola hatalı."));
        }
        empty()
    }

    async fn hostname(self: Arc<Self>, _p: Peer, raw: Value) -> Result<Value, PrivError> {
        let a: HostnameArgs = decode(&raw)?;
        if !valid_hostname(&a.hostname) {
            return Err(PrivError::new("invalid", "Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir."));
        }
        const H: &str = "org.freedesktop.hostname1";
        if dbus::call(H, &dbus::path_of(H), H, "SetStaticHostname", &(a.hostname.as_str(), false)).await.is_ok() {
            let _ = dbus::call(H, &dbus::path_of(H), H, "SetHostname", &(a.hostname.as_str(), false)).await;
            self.update_hosts(&a.hostname);
            return empty();
        }
        std::fs::write(&self.hostname, format!("{}\n", a.hostname))?;
        nix::unistd::sethostname(&a.hostname).map_err(|e| PrivError::new("internal", e.to_string()))?;
        self.update_hosts(&a.hostname);
        empty()
    }

    /// /etc/hosts içindeki 127.0.1.1 satırını günceller (yoksa ekler).
    fn update_hosts(&self, name: &str) {
        let Ok(data) = std::fs::read_to_string(&self.hosts) else { return };
        let mut found = false;
        let mut lines: Vec<String> = data
            .trim_end_matches('\n')
            .split('\n')
            .map(|l| {
                if l.starts_with("127.0.1.1") {
                    found = true;
                    format!("127.0.1.1\t{name}")
                } else {
                    l.to_string()
                }
            })
            .collect();
        if !found {
            lines.push(format!("127.0.1.1\t{name}"));
        }
        let _ = std::fs::write(&self.hosts, lines.join("\n") + "\n");
    }

    async fn power(self: Arc<Self>, _p: Peer, raw: Value) -> Result<Value, PrivError> {
        let a: PowerArgs = decode(&raw)?;
        if a.action == "tryboot" {
            // Pi firmware'e tek seferlik deneme açılışı bayrağı: reboot argümanı "0 tryboot".
            nix::unistd::sync();
            let st = tokio::process::Command::new("/usr/bin/systemctl").args(["reboot", "0 tryboot"]).status().await?;
            if !st.success() {
                return Err(PrivError::new("internal", format!("systemctl reboot: {st}")));
            }
            return empty();
        }
        let method = match a.action.as_str() {
            "reboot" => "Reboot",
            "poweroff" => "PowerOff",
            _ => return Err(PrivError::new("invalid", "Geçersiz güç işlemi.")),
        };
        nix::unistd::sync();
        const L: &str = "org.freedesktop.login1";
        if dbus::call(L, &dbus::path_of(L), "org.freedesktop.login1.Manager", method, &(false,)).await.is_ok() {
            return empty();
        }
        let st = tokio::process::Command::new("/usr/bin/systemctl").arg(&a.action).status().await?;
        if !st.success() {
            return Err(PrivError::new("internal", format!("systemctl {}: {st}", a.action)));
        }
        empty()
    }

    async fn user_create(self: Arc<Self>, _p: Peer, raw: Value) -> Result<Value, PrivError> {
        let a: UserCreateArgs = decode(&raw)?;
        let me = self.clone();
        let r = tokio::task::spawn_blocking(move || users::user_create(&me.shadow, me.min_uid, &me.users, &a)).await.map_err(|e| PrivError::new("internal", e.to_string()))??;
        ok(r)
    }

    // --- OTA (RAUC) -------------------------------------------------------------
    // Kurulumu RAUC hizmeti (de.pengutronix.rauc, kök) yapar; makerd-priv yalnız başlatır ve
    // durumu okur. Paket imzası RAUC'ta /etc/rauc/keyring.pem ile doğrulanır.

    async fn ota_install(self: Arc<Self>, _p: Peer, raw: Value) -> Result<Value, PrivError> {
        let a: OtaInstallArgs = decode(&raw)?;
        if !valid_ota_path(&a.path) {
            return Err(PrivError::new("invalid", "Geçersiz güncelleme paketi yolu."));
        }
        match std::fs::symlink_metadata(&a.path) {
            Ok(m) if m.file_type().is_file() => {}
            _ => return Err(PrivError::new("not_found", "Güncelleme paketi bulunamadı.")),
        }
        let args: HashMap<&str, ZValue> = HashMap::new();
        dbus::call(RAUC, "/", RAUC_IFACE, "InstallBundle", &(a.path.as_str(), args))
            .await
            .map_err(|e| PrivError::new("internal", format!("Kurulum başlatılamadı: {e}")))?;
        empty()
    }

    async fn ota_status(self: Arc<Self>, _p: Peer, _raw: Value) -> Result<Value, PrivError> {
        let mut st = OtaStatus::default();
        match dbus::get(RAUC, "/", RAUC_IFACE, "Operation").await {
            Ok(v) => st.operation = String::try_from(v).unwrap_or_default(),
            Err(e) => return Err(PrivError::new("internal", format!("RAUC durumu okunamadı: {e}"))),
        }
        if let Some(s) = dbus::prop::<String>(RAUC, "/", RAUC_IFACE, "LastError").await {
            st.last_error = s;
        }
        // Progress (isi): yüzde, ileti, iç içelik derinliği
        if let Ok(v) = dbus::get(RAUC, "/", RAUC_IFACE, "Progress").await {
            if let Ok((p, m, _d)) = <(i32, String, i32)>::try_from(v) {
                st.progress = p;
                st.message = m;
            }
        }
        ok(st)
    }
}

const RAUC: &str = "de.pengutronix.rauc";
const RAUC_IFACE: &str = "de.pengutronix.rauc.Installer";
