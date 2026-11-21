//! Bluetooth: BlueZ (org.bluez, D-Bus) üzerinden radyo aç/kapat, cihaz tarama, eşleştirme, bağlanma, unutma.
//! Ses: kulaklık/hoparlör (A2DP) bağlanınca bluez-alsa ile varsayılan ALSA çıkışı ona yönlendirilir
//! (/run/makeros-audio/asound.conf; /etc/asound.conf bu dosyayı varsa yükler). Bağlantı kopunca dosya silinir
//! ve ses kartın kendi çıkışına (HDMI/3,5 mm) döner. Donanım yoksa (Docker önizleme) `Sim` kullanılır.
//!
//! Eşleştirme ajan olmadan yapılır (BlueZ "NoInputNoOutput" kabul eder): kulaklık, hoparlör, fare ve çoğu
//! oyun kumandası çalışır; PIN/şifre yazdıran klavyeler için terminalde `bluetoothctl` kullanılır.

use crate::apierr::{ApiError, ApiResult};
use crate::dbus;
use futures_util::future::BoxFuture;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Adapter {
    /// Bluetooth denetleyicisi var ve bluetoothd çalışıyor.
    pub available: bool,
    pub powered: bool,
    pub discovering: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub address: String,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub address: String,
    pub name: String,
    /// "audio" (kulaklık/hoparlör), "input" (klavye/fare/kumanda), "phone", "computer", "other"
    pub kind: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<u8>,
    /// Sistem sesi bu cihaza gidiyor.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub audio_out: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Status {
    pub adapter: Adapter,
    pub devices: Vec<Device>,
}

pub trait Provider: Send + Sync {
    fn status(&self) -> BoxFuture<'_, ApiResult<Status>>;
    fn set_power(&self, on: bool) -> BoxFuture<'_, ApiResult<()>>;
    /// Çevredeki cihazları arar (açıkken ~30 sn sonra kendiliğinden durur).
    fn discover(&self, on: bool) -> BoxFuture<'_, ApiResult<()>>;
    /// Eşleştirir, güvenilir yapar ve bağlanır.
    fn pair<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>>;
    fn connect<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>>;
    fn disconnect<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>>;
    /// Eşleştirmeyi siler.
    fn forget<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>>;
}

/// "AA:BB:CC:DD:EE:FF" (büyük harf, onaltılık).
pub fn valid_addr(a: &str) -> ApiResult<()> {
    let ok = a.len() == 17 && a.split(':').count() == 6 && a.split(':').all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
    if ok { Ok(()) } else { Err(ApiError::invalid("Geçersiz Bluetooth adresi.")) }
}

/// BlueZ simgesi ve hizmet UUID'lerinden cihaz türü.
pub fn kind_of(icon: &str, uuids: &[String]) -> &'static str {
    let has = |short: &str| uuids.iter().any(|u| u.to_ascii_lowercase().starts_with(&format!("0000{short}")));
    if icon.starts_with("audio-") || has("110b") || has("111e") {
        "audio"
    } else if icon.starts_with("input-") || has("1124") || has("1812") {
        "input"
    } else if icon.starts_with("phone") {
        "phone"
    } else if icon.starts_with("computer") {
        "computer"
    } else {
        "other"
    }
}

/// bluez-alsa ile varsayılan ALSA çıkışını cihaza yönlendiren yapılandırma.
pub fn asound_conf(addr: &str) -> String {
    format!(
        "# makerd tarafından yazıldı: sistem sesi Bluetooth cihazına ({addr}) gider.\n\
         pcm.!default {{\n  type plug\n  slave.pcm {{\n    type bluealsa\n    device \"{addr}\"\n    profile \"a2dp\"\n  }}\n}}\n\
         ctl.!default {{\n  type bluealsa\n}}\n"
    )
}

/// Sıralama: bağlı → eşleşmiş → sinyal gücü → ad.
fn sort_devices(v: &mut [Device]) {
    v.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(b.rssi.unwrap_or(-127).cmp(&a.rssi.unwrap_or(-127)))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

// --- BlueZ ---------------------------------------------------------------------

const BLUEZ: &str = "org.bluez";
const IF_ADAPTER: &str = "org.bluez.Adapter1";
const IF_DEVICE: &str = "org.bluez.Device1";
const IF_BATTERY: &str = "org.bluez.Battery1";

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn v_bool(m: &HashMap<String, OwnedValue>, k: &str) -> bool {
    matches!(m.get(k).map(|v| &**v), Some(Value::Bool(true)))
}
fn v_str(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    match m.get(k).map(|v| &**v) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}
fn v_path(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    match m.get(k).map(|v| &**v) {
        Some(Value::ObjectPath(p)) => p.to_string(),
        _ => String::new(),
    }
}
fn v_strs(m: &HashMap<String, OwnedValue>, k: &str) -> Vec<String> {
    match m.get(k).map(|v| &**v) {
        Some(Value::Array(a)) => a.iter().filter_map(|x| if let Value::Str(s) = x { Some(s.to_string()) } else { None }).collect(),
        _ => Vec::new(),
    }
}

/// D-Bus/BlueZ hatası → kullanıcı iletisi.
fn bt_err(op: &str, e: &zbus::Error) -> ApiError {
    let n = dbus::err_name(e);
    let msg = match n.rsplit('.').next().unwrap_or("") {
        "AuthenticationFailed" | "AuthenticationRejected" | "AuthenticationCanceled" | "AuthenticationTimeout" => {
            "Eşleştirme başarısız. Cihazı eşleştirme moduna alıp tekrar deneyin.".to_string()
        }
        "ConnectionAttemptFailed" | "Failed" | "PageTimeout" => "Cihaza ulaşılamadı. Açık ve yakında olduğundan emin olun.".into(),
        "InProgress" => "Bir Bluetooth işlemi zaten sürüyor; birkaç saniye bekleyin.".into(),
        "NotReady" => "Bluetooth kapalı. Önce açın.".into(),
        "Blocked" | "Rfkill" => "Bluetooth engellenmiş (rfkill).".into(),
        "DoesNotExist" | "UnknownObject" => "Cihaz bulunamadı; yeniden arayın.".into(),
        "ServiceUnknown" | "NameHasNoOwner" => "Bluetooth hizmeti (bluetoothd) çalışmıyor.".into(),
        "AccessDenied" => "Bluetooth için izin yok ('bluetooth' grubu).".into(),
        _ => format!("{op}: {e}"),
    };
    ApiError::unavailable(msg)
}

/// Gerçek donanım: BlueZ.
pub struct Bluez {
    audio_dir: PathBuf,
    audio: Mutex<Option<String>>,
}

impl Bluez {
    /// `audio_dir`: ALSA yönlendirme dosyasının dizini (makerd.service RuntimeDirectory=makeros-audio).
    pub fn new(audio_dir: impl Into<PathBuf>) -> Self {
        let audio_dir = audio_dir.into();
        let cur = std::fs::read_to_string(audio_dir.join("asound.conf")).ok().and_then(|s| s.split('"').nth(1).map(str::to_string));
        Self { audio_dir, audio: Mutex::new(cur) }
    }

    fn route_audio(&self, addr: Option<&str>) {
        let f = self.audio_dir.join("asound.conf");
        match addr {
            Some(a) => {
                let _ = std::fs::create_dir_all(&self.audio_dir);
                if let Err(e) = std::fs::write(&f, asound_conf(a)) {
                    tracing::warn!(err = %e, "ses yönlendirmesi yazılamadı");
                    return;
                }
                tracing::info!(addr = a, "sistem sesi Bluetooth cihazına yönlendirildi");
            }
            None => {
                let _ = std::fs::remove_file(&f);
            }
        }
        *self.audio.lock().unwrap() = addr.map(str::to_string);
    }

    async fn objects() -> zbus::Result<Objects> {
        let m = dbus::call(BLUEZ, "/", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects", &()).await?;
        m.body().deserialize::<Objects>()
    }

    /// İlk denetleyicinin yolu (genelde /org/bluez/hci0).
    async fn adapter_path() -> ApiResult<String> {
        let objs = Self::objects().await.map_err(|e| bt_err("Bluetooth", &e))?;
        let mut paths: Vec<String> = objs.iter().filter(|(_, i)| i.contains_key(IF_ADAPTER)).map(|(p, _)| p.to_string()).collect();
        paths.sort();
        paths.into_iter().next().ok_or_else(|| ApiError::unavailable("Bluetooth denetleyicisi bulunamadı."))
    }

    async fn dev_path(addr: &str) -> ApiResult<String> {
        valid_addr(addr)?;
        Ok(format!("{}/dev_{}", Self::adapter_path().await?, addr.to_ascii_uppercase().replace(':', "_")))
    }

    async fn dev_call(addr: &str, method: &str) -> ApiResult<()> {
        let p = Self::dev_path(addr).await?;
        match dbus::call(BLUEZ, &p, IF_DEVICE, method, &()).await {
            Ok(_) => Ok(()),
            Err(e) if dbus::err_name(&e).ends_with("AlreadyExists") || dbus::err_name(&e).ends_with("AlreadyConnected") => Ok(()),
            Err(e) => Err(bt_err(method, &e)),
        }
    }

    /// Cihaz ses cihazıysa (A2DP) sistem sesini ona yönlendirir.
    async fn maybe_route(&self, addr: &str) {
        if let Ok(st) = self.status_inner().await {
            if st.devices.iter().any(|d| d.address.eq_ignore_ascii_case(addr) && d.kind == "audio" && d.connected) {
                self.route_audio(Some(&addr.to_ascii_uppercase()));
            }
        }
    }

    async fn status_inner(&self) -> ApiResult<Status> {
        let objs = match Self::objects().await {
            Ok(o) => o,
            // bluetoothd yok / D-Bus yok: "kullanılamıyor" durumu (hata değil).
            Err(e) => {
                tracing::debug!(err = %e, "BlueZ yok");
                return Ok(Status::default());
            }
        };
        let mut adapters: Vec<(&OwnedObjectPath, &HashMap<String, OwnedValue>)> =
            objs.iter().filter_map(|(p, i)| i.get(IF_ADAPTER).map(|a| (p, a))).collect();
        adapters.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        let Some((apath, a)) = adapters.first() else { return Ok(Status::default()) };
        let adapter = Adapter {
            available: true,
            powered: v_bool(a, "Powered"),
            discovering: v_bool(a, "Discovering"),
            name: v_str(a, "Alias"),
            address: v_str(a, "Address"),
        };
        let out = self.audio.lock().unwrap().clone();
        let mut devices: Vec<Device> = objs
            .values()
            .filter_map(|i| {
                let d = i.get(IF_DEVICE)?;
                if v_path(d, "Adapter") != apath.as_str() {
                    return None;
                }
                let address = v_str(d, "Address");
                let name = [v_str(d, "Alias"), v_str(d, "Name")].into_iter().find(|s| !s.is_empty()).unwrap_or_else(|| address.clone());
                let rssi = match d.get("RSSI").map(|v| &**v) {
                    Some(Value::I16(r)) => Some(*r),
                    _ => None,
                };
                let battery = match i.get(IF_BATTERY).and_then(|b| b.get("Percentage")).map(|v| &**v) {
                    Some(Value::U8(p)) => Some(*p),
                    _ => None,
                };
                let connected = v_bool(d, "Connected");
                Some(Device {
                    audio_out: connected && out.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&address)),
                    kind: kind_of(&v_str(d, "Icon"), &v_strs(d, "UUIDs")).into(),
                    paired: v_bool(d, "Paired"),
                    trusted: v_bool(d, "Trusted"),
                    connected,
                    rssi,
                    battery,
                    name,
                    address,
                })
            })
            // Adı olmayan, eşleşmemiş ve sinyali bilinmeyen eski kayıtlar listeyi kalabalıklaştırmasın.
            .filter(|d| d.paired || d.connected || d.rssi.is_some())
            .collect();
        sort_devices(&mut devices);
        // Ses cihazının bağlantısı koptuysa ses kartın kendi çıkışına döner.
        if let Some(o) = out {
            if !devices.iter().any(|d| d.connected && d.address.eq_ignore_ascii_case(&o)) {
                self.route_audio(None);
            }
        }
        Ok(Status { adapter, devices })
    }
}

impl Provider for Bluez {
    fn status(&self) -> BoxFuture<'_, ApiResult<Status>> {
        Box::pin(self.status_inner())
    }

    fn set_power(&self, on: bool) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let p = Self::adapter_path().await?;
            dbus::set(BLUEZ, &p, IF_ADAPTER, "Powered", Value::from(on)).await.map_err(|e| bt_err("Bluetooth", &e))
        })
    }

    fn discover(&self, on: bool) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let p = Self::adapter_path().await?;
            let r = dbus::call(BLUEZ, &p, IF_ADAPTER, if on { "StartDiscovery" } else { "StopDiscovery" }, &()).await;
            match r {
                Ok(_) => {}
                Err(e) if on && dbus::err_name(&e).ends_with("InProgress") => {}
                Err(e) if !on && dbus::err_name(&e).ends_with("NotReady") => {}
                Err(e) if !on && dbus::err_name(&e).ends_with("Failed") => {} // zaten durmuş
                Err(e) => return Err(bt_err("Arama", &e)),
            }
            if on {
                // Arama pili ve Wi-Fi'yi (aynı yonga) yorar: 30 sn sonra durdurulur.
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    let _ = dbus::call(BLUEZ, &p, IF_ADAPTER, "StopDiscovery", &()).await;
                });
            }
            Ok(())
        })
    }

    fn pair<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let p = Self::dev_path(addr).await?;
            // Arama sürerken eşleştirme yavaşlar/başarısız olur.
            if let Ok(ap) = Self::adapter_path().await {
                let _ = dbus::call(BLUEZ, &ap, IF_ADAPTER, "StopDiscovery", &()).await;
            }
            Self::dev_call(addr, "Pair").await?;
            let _ = dbus::set(BLUEZ, &p, IF_DEVICE, "Trusted", Value::from(true)).await;
            Self::dev_call(addr, "Connect").await?;
            self.maybe_route(addr).await;
            Ok(())
        })
    }

    fn connect<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            Self::dev_call(addr, "Connect").await?;
            self.maybe_route(addr).await;
            Ok(())
        })
    }

    fn disconnect<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            Self::dev_call(addr, "Disconnect").await?;
            if self.audio.lock().unwrap().as_deref().is_some_and(|o| o.eq_ignore_ascii_case(addr)) {
                self.route_audio(None);
            }
            Ok(())
        })
    }

    fn forget<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let ap = Self::adapter_path().await?;
            let dp = Self::dev_path(addr).await?;
            let path = ObjectPath::try_from(dp.as_str()).map_err(|_| ApiError::invalid("Geçersiz Bluetooth adresi."))?;
            dbus::call(BLUEZ, &ap, IF_ADAPTER, "RemoveDevice", &(path,)).await.map_err(|e| bt_err("Unut", &e))?;
            if self.audio.lock().unwrap().as_deref().is_some_and(|o| o.eq_ignore_ascii_case(addr)) {
                self.route_audio(None);
            }
            Ok(())
        })
    }
}

// --- benzetim (Docker önizleme) ----------------------------------------------------

struct SimState {
    adapter: Adapter,
    /// adres → cihaz; `seen`: arama sırasında görünür olanlar
    devices: Vec<Device>,
    hidden: Vec<Device>,
}

/// Donanımsız önizleme: eşleşmiş bir kulaklık ve klavye; arama yeni cihazlar bulur.
pub struct Sim {
    st: Mutex<SimState>,
    step: Duration,
}

impl Sim {
    pub fn new() -> Self {
        Self::with_step(Duration::from_millis(900))
    }

    pub fn with_step(step: Duration) -> Self {
        let dev = |address: &str, name: &str, kind: &str, paired: bool, rssi: Option<i16>, battery: Option<u8>| Device {
            address: address.into(),
            name: name.into(),
            kind: kind.into(),
            paired,
            trusted: paired,
            rssi,
            battery,
            ..Default::default()
        };
        Self {
            st: Mutex::new(SimState {
                adapter: Adapter { available: true, powered: true, discovering: false, name: "maker-os".into(), address: "DC:A6:32:4F:11:20".into() },
                devices: vec![
                    dev("38:18:4C:A1:22:7B", "Sony WH-1000XM4", "audio", true, None, Some(80)),
                    dev("F4:73:35:0B:9C:42", "Logitech K380", "input", true, None, Some(60)),
                ],
                hidden: vec![
                    dev("B8:D5:0B:6A:31:E0", "JBL Flip 5", "audio", false, Some(-52), None),
                    dev("64:03:7F:C2:8D:15", "Galaxy Buds2", "audio", false, Some(-61), Some(90)),
                    dev("A4:C1:38:02:5E:77", "Xbox Wireless Controller", "input", false, Some(-70), None),
                    dev("5C:E9:1E:44:A0:3B", "iPhone", "phone", false, Some(-78), None),
                ],
            }),
            step,
        }
    }

    fn find<'a>(st: &'a mut SimState, addr: &str) -> ApiResult<&'a mut Device> {
        valid_addr(addr)?;
        st.devices.iter_mut().find(|d| d.address.eq_ignore_ascii_case(addr)).ok_or_else(|| ApiError::unavailable("Cihaz bulunamadı; yeniden arayın."))
    }

    fn need_power(st: &SimState) -> ApiResult<()> {
        if st.adapter.powered { Ok(()) } else { Err(ApiError::unavailable("Bluetooth kapalı. Önce açın.")) }
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for Sim {
    fn status(&self) -> BoxFuture<'_, ApiResult<Status>> {
        Box::pin(async move {
            let st = self.st.lock().unwrap();
            let mut devices = if st.adapter.powered { st.devices.clone() } else { st.devices.iter().filter(|d| d.paired).cloned().collect() };
            sort_devices(&mut devices);
            Ok(Status { adapter: st.adapter.clone(), devices })
        })
    }

    fn set_power(&self, on: bool) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            let mut st = self.st.lock().unwrap();
            st.adapter.powered = on;
            if !on {
                st.adapter.discovering = false;
                for d in st.devices.iter_mut() {
                    d.connected = false;
                    d.audio_out = false;
                }
            }
            Ok(())
        })
    }

    fn discover(&self, on: bool) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(async move {
            {
                let mut st = self.st.lock().unwrap();
                Self::need_power(&st)?;
                st.adapter.discovering = on;
                if !on {
                    return Ok(());
                }
            }
            // Cihazlar birer birer "bulunur".
            loop {
                tokio::time::sleep(self.step).await;
                let mut st = self.st.lock().unwrap();
                if !st.adapter.discovering {
                    return Ok(());
                }
                if st.hidden.is_empty() {
                    st.adapter.discovering = false;
                    return Ok(());
                }
                let d = st.hidden.remove(0);
                st.devices.push(d);
            }
        })
    }

    fn pair<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            {
                let mut st = self.st.lock().unwrap();
                Self::need_power(&st)?;
                if Self::find(&mut st, addr)?.kind == "phone" {
                    return Err(ApiError::unavailable("Eşleştirme başarısız. Cihazı eşleştirme moduna alıp tekrar deneyin."));
                }
            }
            tokio::time::sleep(self.step * 2).await;
            {
                let mut st = self.st.lock().unwrap();
                st.adapter.discovering = false;
                let d = Self::find(&mut st, addr)?;
                d.paired = true;
                d.trusted = true;
            }
            self.connect(addr).await
        })
    }

    fn connect<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            {
                let st = self.st.lock().unwrap();
                Self::need_power(&st)?;
            }
            tokio::time::sleep(self.step).await;
            let mut st = self.st.lock().unwrap();
            let audio = {
                let d = Self::find(&mut st, addr)?;
                d.connected = true;
                d.kind == "audio"
            };
            if audio {
                for d in st.devices.iter_mut() {
                    d.audio_out = d.address.eq_ignore_ascii_case(addr);
                }
            }
            Ok(())
        })
    }

    fn disconnect<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let mut st = self.st.lock().unwrap();
            let d = Self::find(&mut st, addr)?;
            d.connected = false;
            d.audio_out = false;
            Ok(())
        })
    }

    fn forget<'a>(&'a self, addr: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(async move {
            let mut st = self.st.lock().unwrap();
            Self::find(&mut st, addr)?;
            st.devices.retain(|d| !d.address.eq_ignore_ascii_case(addr));
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(valid_addr("38:18:4C:A1:22:7B").is_ok() && valid_addr("38:18:4c:a1:22:7b").is_ok());
        assert!(valid_addr("38:18:4C:A1:22").is_err() && valid_addr("38:18:4C:A1:22:ZZ").is_err() && valid_addr("../x").is_err());
        assert_eq!(kind_of("audio-headset", &[]), "audio");
        assert_eq!(kind_of("", &["0000110b-0000-1000-8000-00805f9b34fb".into()]), "audio");
        assert_eq!(kind_of("input-keyboard", &[]), "input");
        assert_eq!(kind_of("", &["00001812-0000-1000-8000-00805f9b34fb".into()]), "input");
        assert_eq!(kind_of("phone", &[]), "phone");
        assert_eq!(kind_of("", &[]), "other");
        let c = asound_conf("38:18:4C:A1:22:7B");
        assert!(c.contains("type bluealsa") && c.contains("device \"38:18:4C:A1:22:7B\"") && c.contains("pcm.!default"));
        // Kaydedilmiş yönlendirme dosyasından adres geri okunur (makerd yeniden başlarsa).
        let dir = std::env::temp_dir().join(format!("mk-bt-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("asound.conf"), c).unwrap();
        assert_eq!(Bluez::new(&dir).audio.lock().unwrap().as_deref(), Some("38:18:4C:A1:22:7B"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sim_flow() {
        let s = Sim::with_step(Duration::from_millis(1));
        let st = s.status().await.unwrap();
        assert!(st.adapter.available && st.adapter.powered);
        assert_eq!(st.devices.len(), 2);
        s.discover(true).await.unwrap();
        let st = s.status().await.unwrap();
        assert_eq!(st.devices.len(), 6);
        assert!(!st.adapter.discovering);
        // Yeni kulaklığı eşleştir → bağlı ve ses ona gider.
        s.pair("B8:D5:0B:6A:31:E0").await.unwrap();
        let st = s.status().await.unwrap();
        let jbl = st.devices.iter().find(|d| d.name == "JBL Flip 5").unwrap();
        assert!(jbl.paired && jbl.connected && jbl.audio_out);
        assert_eq!(st.devices[0].name, "JBL Flip 5", "bağlı cihaz başta");
        // Başka bir ses cihazı bağlanınca ses ona geçer.
        s.connect("38:18:4C:A1:22:7B").await.unwrap();
        let st = s.status().await.unwrap();
        assert_eq!(st.devices.iter().filter(|d| d.audio_out).map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["Sony WH-1000XM4"]);
        // Telefon eşleştirmesi başarısız (benzetim).
        assert!(s.pair("5C:E9:1E:44:A0:3B").await.is_err());
        s.disconnect("38:18:4C:A1:22:7B").await.unwrap();
        s.forget("B8:D5:0B:6A:31:E0").await.unwrap();
        assert!(s.status().await.unwrap().devices.iter().all(|d| d.name != "JBL Flip 5"));
        assert!(s.connect("00:11:22:33:44:55").await.is_err());
        // Kapalıyken işlem yapılmaz; yalnız eşleşmişler listelenir.
        s.set_power(false).await.unwrap();
        assert!(s.connect("38:18:4C:A1:22:7B").await.is_err());
        assert!(s.status().await.unwrap().devices.iter().all(|d| d.paired && !d.connected));
    }
}
