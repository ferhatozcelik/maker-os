//! D-Bus sistem veriyoluna tek, tembel açılan bağlantı ve dinamik çağrı yardımcıları.

use std::time::Duration;
use tokio::sync::Mutex;
use zbus::zvariant::{DynamicType, OwnedValue, Value};
use zbus::Connection;

static CONN: Mutex<Option<Connection>> = Mutex::const_new(None);

const TIMEOUT: Duration = Duration::from_secs(25);

/// Paylaşılan sistem veriyolu bağlantısı; ilk kullanımda açılır.
pub async fn system() -> zbus::Result<Connection> {
    let mut g = CONN.lock().await;
    if let Some(c) = g.as_ref() {
        return Ok(c.clone());
    }
    let c = tokio::time::timeout(Duration::from_secs(5), Connection::system()).await.map_err(|_| zbus::Error::Failure("D-Bus bağlantısı zaman aşımı".into()))??;
    *g = Some(c.clone());
    Ok(c)
}

async fn drop_on_io(e: &zbus::Error) {
    if matches!(e, zbus::Error::InputOutput(_)) {
        *CONN.lock().await = None;
    }
}

/// Yöntem çağırır ve yanıt iletisini döndürür.
pub async fn call<B>(dest: &str, path: &str, iface: &str, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + DynamicType,
{
    let c = system().await?;
    let r = tokio::time::timeout(TIMEOUT, c.call_method(Some(dest), path, Some(iface), method, body)).await.map_err(|_| zbus::Error::Failure(format!("{method}: zaman aşımı")))?;
    if let Err(e) = &r {
        drop_on_io(e).await;
    }
    r
}

/// Özellik okur (org.freedesktop.DBus.Properties.Get).
pub async fn get(dest: &str, path: &str, iface: &str, name: &str) -> zbus::Result<OwnedValue> {
    let m = call(dest, path, "org.freedesktop.DBus.Properties", "Get", &(iface, name)).await?;
    m.body().deserialize::<OwnedValue>()
}

/// Özellik yazar (org.freedesktop.DBus.Properties.Set).
pub async fn set(dest: &str, path: &str, iface: &str, name: &str, v: Value<'_>) -> zbus::Result<()> {
    call(dest, path, "org.freedesktop.DBus.Properties", "Set", &(iface, name, v)).await.map(|_| ())
}

/// Özelliği belirli türe çevirerek okur.
pub async fn prop<T>(dest: &str, path: &str, iface: &str, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    get(dest, path, iface, name).await.ok().and_then(|v| T::try_from(v).ok())
}

/// D-Bus hata adı (ör. org.freedesktop.DBus.Error.AccessDenied).
pub fn err_name(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        zbus::Error::FDO(f) => format!("org.freedesktop.DBus.Error.{}", fdo_name(f)),
        _ => String::new(),
    }
}

fn fdo_name(e: &zbus::fdo::Error) -> String {
    let s = format!("{e:?}");
    s.split('(').next().unwrap_or("").to_string()
}

/// Nesne yolu "/a/b" biçiminde hedef adından (org.freedesktop.hostname1 → /org/freedesktop/hostname1).
pub fn path_of(dest: &str) -> String {
    format!("/{}", dest.replace('.', "/"))
}
