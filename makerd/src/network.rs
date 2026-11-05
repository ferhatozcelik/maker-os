//! Ağ durumu, Wi-Fi tarama, bağlanma (başarısızlıkta önceki bağlantıya, o da
//! olmazsa kurulum erişim noktasına dönüş) ve kayıtlı ağı unutma.
//! NetworkManager D-Bus API'si kullanılır; NM yoksa durum /sys'ten okunur.

use crate::apierr::{ApiError, ApiResult};
use crate::dbus;
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

/// Kurulum erişim noktası (os/ tarafındaki fallback betiği aynı profili kullanmalı).
pub const HOTSPOT_ID: &str = "makeros-hotspot";
pub const HOTSPOT_SSID: &str = "MakerOS";
pub const HOTSPOT_IP: &str = "10.42.0.1";

/// GET /net gövdesi.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Status {
    pub online: bool,
    pub ifaces: Vec<Iface>,
    /// Wi-Fi radyosu açık mı (NM WirelessEnabled).
    pub wifi_enabled: bool,
    pub hotspot: Hotspot,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Iface {
    pub name: String,
    /// ethernet | wifi
    #[serde(rename = "type")]
    pub kind: String,
    pub state: String,
    pub ipv4: Vec<String>,
    pub mac: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_mbps: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ssid: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Hotspot {
    pub active: bool,
    pub ssid: String,
}

/// Tarama sonucu.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Ap {
    pub ssid: String,
    pub signal: i32,
    /// open | wpa2 | wpa3 | enterprise
    pub security: String,
    pub known: bool,
    pub active: bool,
}

/// Kurumsal ağ kimliği.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Eap {
    #[serde(default)]
    pub identity: String,
    #[serde(default)]
    pub password: String,
    /// peap (varsayılan) | ttls | pwd
    #[serde(default)]
    pub method: String,
}

/// POST /net/wifi/connect gövdesi.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct ConnectRequest {
    pub ssid: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub eap: Option<Eap>,
}

/// Bağlanma işinin ilerleme bildirimi.
pub type Progress<'a> = &'a (dyn Fn(i32, &str) + Send + Sync);

/// Ağ arka ucu (NM veya mock).
pub trait Provider: Send + Sync {
    fn status(&self) -> BoxFuture<'_, ApiResult<Status>>;
    fn scan(&self) -> BoxFuture<'_, ApiResult<Vec<Ap>>>;
    fn connect<'a>(&'a self, req: ConnectRequest, p: Progress<'a>) -> BoxFuture<'a, ApiResult<()>>;
    fn forget<'a>(&'a self, ssid: &'a str) -> BoxFuture<'a, ApiResult<()>>;
    /// Wi-Fi radyosunu açar/kapatır.
    fn set_wifi_enabled(&self, enabled: bool) -> BoxFuture<'_, ApiResult<()>>;
}

/// SSID 1-32 bayt ve NUL içermemeli.
pub fn valid_ssid(s: &str) -> bool {
    (1..=32).contains(&s.len()) && !s.contains('\0')
}

/// Aynı SSID'li erişim noktalarından en güçlüsünü tutar, kurulum ağını çıkarır,
/// etkin olanı başa ve sinyale göre sıralar.
pub fn dedupe_aps(list: Vec<Ap>) -> Vec<Ap> {
    let mut best: HashMap<String, Ap> = HashMap::new();
    for mut a in list {
        if a.ssid.is_empty() || a.ssid == HOTSPOT_SSID {
            continue;
        }
        match best.get_mut(&a.ssid) {
            Some(cur) if a.signal <= cur.signal => {
                cur.active |= a.active;
            }
            Some(cur) => {
                a.active |= cur.active;
                a.known |= cur.known;
                *cur = a;
            }
            None => {
                best.insert(a.ssid.clone(), a);
            }
        }
    }
    let mut out: Vec<Ap> = best.into_values().collect();
    out.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)).then_with(|| a.ssid.cmp(&b.ssid)));
    out
}

/// NetworkManager yokken /sys/class/net ve /proc/net/route'tan durum.
pub fn sysfs_status(root: &str) -> Status {
    let mut st = Status { hotspot: Hotspot { active: false, ssid: HOTSPOT_SSID.into() }, ..Default::default() };
    let base = Path::new(root).join("sys/class/net");
    let read = |name: &str, f: &str| crate::util::read_trim(base.join(name).join(f));
    let addrs = ipv4_addrs();
    let mut names: Vec<String> = std::fs::read_dir(&base).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.sort();
    for name in names {
        if name == "lo" {
            continue;
        }
        let kind = if base.join(&name).join("wireless").exists() {
            "wifi"
        } else if read(&name, "type") == "1"
            && !name.starts_with("veth")
            && !name.starts_with("docker")
            && !name.starts_with("br-")
            && (base.join(&name).join("device").exists() || name.starts_with("eth") || name.starts_with("en"))
        {
            "ethernet"
        } else {
            continue;
        };
        let mut ifc = Iface { name: name.clone(), kind: kind.into(), mac: read(&name, "address"), ..Default::default() };
        ifc.state = match read(&name, "operstate").as_str() {
            "up" => "connected",
            "down" | "lowerlayerdown" => "disconnected",
            _ => "unavailable",
        }
        .into();
        if kind == "ethernet" {
            ifc.speed_mbps = read(&name, "speed").parse::<i64>().ok().filter(|s| *s > 0);
        }
        for (ip, cidr) in addrs.iter().filter(|(n, _)| *n == name).map(|(_, a)| a) {
            ifc.ipv4.push(cidr.clone());
            if ip == HOTSPOT_IP {
                st.hotspot.active = true;
            }
        }
        if kind == "wifi" {
            st.wifi_enabled = true;
        }
        st.ifaces.push(ifc);
    }
    let route = std::fs::read_to_string(Path::new(root).join("proc/net/route")).unwrap_or_default();
    st.online = route.lines().skip(1).any(|l| l.split_whitespace().nth(1) == Some("00000000"));
    st
}

/// Arayüz adı → (adres, "adres/önek") listesi.
fn ipv4_addrs() -> Vec<(String, (String, String))> {
    let Ok(ifs) = nix::ifaddrs::getifaddrs() else { return Vec::new() };
    ifs.filter_map(|i| {
        let a = i.address?.as_sockaddr_in()?.ip();
        let bits = i.netmask.and_then(|m| m.as_sockaddr_in().map(|m| u32::from(m.ip()).count_ones())).unwrap_or(32);
        Some((i.interface_name, (a.to_string(), format!("{a}/{bits}"))))
    })
    .collect()
}

/// NM AP bayrakları → sözleşme değeri.
pub fn security(flags: u32, wpa: u32, rsn: u32) -> &'static str {
    let all = wpa | rsn;
    if all & SEC_8021X != 0 {
        "enterprise"
    } else if rsn & SEC_SAE != 0 && all & SEC_PSK == 0 {
        "wpa3"
    } else if all & (SEC_PSK | SEC_SAE) != 0 || flags & 1 != 0 {
        "wpa2" // WEP: eski; parola ister
    } else {
        "open"
    }
}

// --- NetworkManager --------------------------------------------------------------

const NM_DEST: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
const IF_NM: &str = "org.freedesktop.NetworkManager";
const IF_DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const IF_WIRED: &str = "org.freedesktop.NetworkManager.Device.Wired";
const IF_WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const IF_AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const IF_IP4: &str = "org.freedesktop.NetworkManager.IP4Config";
const IF_SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
const IF_CONN: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const IF_ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";

const DEV_ETHERNET: u32 = 1;
const DEV_WIFI: u32 = 2;
const MODE_AP: u32 = 3;

pub const SEC_PSK: u32 = 0x100;
pub const SEC_8021X: u32 = 0x200;
pub const SEC_SAE: u32 = 0x400;

const ACTIVE_ACTIVATED: u32 = 2;
const ACTIVE_DEACTIVATED: u32 = 4;

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

pub struct Nm {
    /// sysfs yedeği için kök ("/").
    pub root: String,
    mu: tokio::sync::Mutex<()>,
}

async fn p<T: TryFrom<OwnedValue>>(path: &str, iface: &str, name: &str) -> Option<T> {
    dbus::prop(NM_DEST, path, iface, name).await
}

async fn path_prop(path: &str, iface: &str, name: &str) -> Option<String> {
    p::<OwnedObjectPath>(path, iface, name).await.map(|o| o.as_str().to_string()).filter(|s| s != "/")
}

async fn nm_call<B>(path: &str, iface: &str, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    dbus::call(NM_DEST, path, iface, method, body).await
}

async fn paths(path: &str, iface: &str, method: &str) -> Option<Vec<String>> {
    let m = nm_call(path, iface, method, &()).await.ok()?;
    let v: Vec<OwnedObjectPath> = m.body().deserialize().ok()?;
    Some(v.into_iter().map(|o| o.as_str().to_string()).collect())
}

fn opath(s: &str) -> ObjectPath<'_> {
    ObjectPath::try_from(s).unwrap_or_else(|_| ObjectPath::from_static_str_unchecked("/"))
}

fn dev_state(s: u32) -> &'static str {
    match s {
        10 => "unmanaged",
        20 => "unavailable",
        30 => "disconnected",
        40..=99 => "connecting",
        100 => "connected",
        110 => "disconnecting",
        120 => "failed",
        _ => "unknown",
    }
}

#[derive(Clone)]
struct SavedConn {
    path: String,
    id: String,
    ssid: String,
    mode: String,
}

impl SavedConn {
    fn is_hotspot(&self) -> bool {
        self.id == HOTSPOT_ID || self.mode == "ap"
    }
}

fn str_of(v: Option<&OwnedValue>) -> String {
    v.and_then(|v| <&str>::try_from(&**v).ok()).unwrap_or("").to_string()
}

fn bytes_of(v: Option<&OwnedValue>) -> Vec<u8> {
    v.and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<u8>::try_from(v).ok()).unwrap_or_default()
}

fn new_uuid() -> String {
    let mut b: [u8; 16] = rand::random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex::encode(b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

type Out<'a> = HashMap<&'static str, HashMap<&'static str, Value<'a>>>;

fn wifi_settings(req: &ConnectRequest, key_mgmt: &str) -> Out<'static> {
    let mut s: Out = HashMap::new();
    s.insert(
        "connection",
        HashMap::from([
            ("id", Value::from(req.ssid.clone())),
            ("uuid", Value::from(new_uuid())),
            ("type", Value::from("802-11-wireless")),
            ("autoconnect", Value::from(true)),
        ]),
    );
    let mut w = HashMap::from([("ssid", Value::from(req.ssid.as_bytes().to_vec())), ("mode", Value::from("infrastructure"))]);
    if req.hidden {
        w.insert("hidden", Value::from(true));
    }
    s.insert("802-11-wireless", w);
    s.insert("ipv4", HashMap::from([("method", Value::from("auto"))]));
    s.insert("ipv6", HashMap::from([("method", Value::from("auto"))]));
    if let Some(e) = &req.eap {
        let method = if e.method.is_empty() { "peap".to_string() } else { e.method.clone() };
        s.insert("802-11-wireless-security", HashMap::from([("key-mgmt", Value::from("wpa-eap"))]));
        let mut x = HashMap::from([
            ("eap", Value::from(vec![method.clone()])),
            ("identity", Value::from(e.identity.clone())),
            ("password", Value::from(e.password.clone())),
        ]);
        if method == "peap" || method == "ttls" {
            x.insert("phase2-auth", Value::from("mschapv2"));
        }
        s.insert("802-1x", x);
    } else if !req.password.is_empty() {
        s.insert("802-11-wireless-security", HashMap::from([("key-mgmt", Value::from(key_mgmt.to_string())), ("psk", Value::from(req.password.clone()))]));
    }
    s
}

fn hotspot_settings(iface: &str) -> Out<'static> {
    let addr: HashMap<&str, Value> = HashMap::from([("address", Value::from(HOTSPOT_IP)), ("prefix", Value::from(24u32))]);
    HashMap::from([
        (
            "connection",
            HashMap::from([
                ("id", Value::from(HOTSPOT_ID)),
                ("uuid", Value::from(new_uuid())),
                ("type", Value::from("802-11-wireless")),
                ("autoconnect", Value::from(false)),
                ("interface-name", Value::from(iface.to_string())),
            ]),
        ),
        (
            "802-11-wireless",
            HashMap::from([
                ("ssid", Value::from(HOTSPOT_SSID.as_bytes().to_vec())),
                ("mode", Value::from("ap")),
                ("band", Value::from("bg")),
                ("channel", Value::from(6u32)),
            ]),
        ),
        ("ipv4", HashMap::from([("method", Value::from("shared")), ("address-data", Value::from(vec![addr]))])),
        ("ipv6", HashMap::from([("method", Value::from("ignore"))])),
    ])
}

fn fail(msg: impl Into<String>) -> ApiError {
    ApiError::unavailable(msg)
}

impl Nm {
    pub fn new(root: &str) -> Self {
        Self { root: root.into(), mu: tokio::sync::Mutex::new(()) }
    }

    /// NM veriyolunda mı.
    async fn bus(&self) -> ApiResult<()> {
        if dbus::system().await.is_err() {
            return Err(fail("NetworkManager'a ulaşılamadı (D-Bus yok)."));
        }
        dbus::call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", &(NM_DEST,))
            .await
            .map(|_| ())
            .map_err(|_| fail("NetworkManager çalışmıyor."))
    }

    async fn nm_status(&self) -> ApiResult<Status> {
        if self.bus().await.is_err() {
            return Ok(sysfs_status(&self.root));
        }
        let mut st = Status { hotspot: Hotspot { active: false, ssid: HOTSPOT_SSID.into() }, ..Default::default() };
        let state: u32 = p(NM_PATH, IF_NM, "State").await.unwrap_or(0);
        let conn: u32 = p(NM_PATH, IF_NM, "Connectivity").await.unwrap_or(0);
        st.online = conn == 4 || (conn == 0 && state == 70);
        st.wifi_enabled = p(NM_PATH, IF_NM, "WirelessEnabled").await.unwrap_or(false);
        let Some(devs) = paths(NM_PATH, IF_NM, "GetDevices").await else { return Ok(sysfs_status(&self.root)) };
        for d in devs {
            let typ: u32 = p(&d, IF_DEVICE, "DeviceType").await.unwrap_or(0);
            if typ != DEV_ETHERNET && typ != DEV_WIFI {
                continue;
            }
            let mut ifc = Iface {
                name: p(&d, IF_DEVICE, "Interface").await.unwrap_or_default(),
                state: dev_state(p(&d, IF_DEVICE, "State").await.unwrap_or(0)).into(),
                mac: p(&d, IF_DEVICE, "HwAddress").await.unwrap_or_default(),
                ..Default::default()
            };
            if let Some(ip4) = path_prop(&d, IF_DEVICE, "Ip4Config").await {
                if let Some(ad) = p::<Vec<HashMap<String, OwnedValue>>>(&ip4, IF_IP4, "AddressData").await {
                    for a in ad {
                        let addr = str_of(a.get("address"));
                        let pfx = a.get("prefix").and_then(|v| u32::try_from(&**v).ok()).unwrap_or(0);
                        if !addr.is_empty() {
                            ifc.ipv4.push(format!("{addr}/{pfx}"));
                        }
                    }
                }
            }
            if typ == DEV_ETHERNET {
                ifc.kind = "ethernet".into();
                ifc.speed_mbps = p::<u32>(&d, IF_WIRED, "Speed").await.filter(|s| *s > 0).map(i64::from);
            } else {
                ifc.kind = "wifi".into();
                let mode: u32 = p(&d, IF_WIRELESS, "Mode").await.unwrap_or(0);
                if let Some(ap) = path_prop(&d, IF_WIRELESS, "ActiveAccessPoint").await {
                    ifc.ssid = String::from_utf8_lossy(&p::<Vec<u8>>(&ap, IF_AP, "Ssid").await.unwrap_or_default()).into_owned();
                }
                if mode == MODE_AP {
                    st.hotspot.active = true;
                    if !ifc.ssid.is_empty() {
                        st.hotspot.ssid = ifc.ssid.clone();
                    }
                }
            }
            st.ifaces.push(ifc);
        }
        Ok(st)
    }

    /// Wi-Fi aygıtı (wlan0 öncelikli): (yol, arayüz adı).
    async fn wifi_device(&self) -> ApiResult<(String, String)> {
        let devs = paths(NM_PATH, IF_NM, "GetDevices").await.ok_or_else(|| fail("Ağ aygıtları okunamadı."))?;
        let mut first: Option<(String, String)> = None;
        for d in devs {
            if p::<u32>(&d, IF_DEVICE, "DeviceType").await != Some(DEV_WIFI) {
                continue;
            }
            let name: String = p(&d, IF_DEVICE, "Interface").await.unwrap_or_default();
            if name == "wlan0" {
                return Ok((d, name));
            }
            first.get_or_insert((d, name));
        }
        first.ok_or_else(|| fail("Wi-Fi aygıtı bulunamadı. Adaptörü, sürücüyü ve rfkill durumunu kontrol edin."))
    }

    async fn saved_wifi(&self) -> Vec<SavedConn> {
        let mut out = Vec::new();
        for path in paths(NM_SETTINGS, IF_SETTINGS, "ListConnections").await.unwrap_or_default() {
            let Ok(m) = nm_call(&path, IF_CONN, "GetSettings", &()).await else { continue };
            let Ok(s) = m.body().deserialize::<Settings>() else { continue };
            let c = s.get("connection");
            if str_of(c.and_then(|c| c.get("type"))) != "802-11-wireless" {
                continue;
            }
            let w = s.get("802-11-wireless");
            out.push(SavedConn {
                path,
                id: str_of(c.and_then(|c| c.get("id"))),
                ssid: String::from_utf8_lossy(&bytes_of(w.and_then(|w| w.get("ssid")))).into_owned(),
                mode: str_of(w.and_then(|w| w.get("mode"))),
            });
        }
        out
    }

    async fn in_ap_mode(&self, dev: &str) -> bool {
        p::<u32>(dev, IF_WIRELESS, "Mode").await == Some(MODE_AP)
    }

    async fn radio_on(&self) {
        let _ = dbus::set(NM_DEST, NM_PATH, IF_NM, "WirelessEnabled", Value::from(true)).await;
    }

    async fn deactivate_device(&self, dev: &str) {
        if let Some(ac) = path_prop(dev, IF_DEVICE, "ActiveConnection").await {
            let _ = nm_call(NM_PATH, IF_NM, "DeactivateConnection", &(opath(&ac),)).await;
        }
    }

    async fn activate(&self, conn: &str, dev: &str) -> Result<String, String> {
        let m = nm_call(NM_PATH, IF_NM, "ActivateConnection", &(opath(conn), opath(dev), opath("/"))).await.map_err(|e| e.to_string())?;
        m.body().deserialize::<OwnedObjectPath>().map(|o| o.as_str().to_string()).map_err(|e| e.to_string())
    }

    async fn wait_active(&self, ac: &str, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match p::<u32>(ac, IF_ACTIVE, "State").await {
                None => return Err("bağlantı sonlandı (parola, sinyal veya DHCP sorunu olabilir)".into()),
                Some(ACTIVE_ACTIVATED) => return Ok(()),
                Some(ACTIVE_DEACTIVATED) => return Err("bağlantı reddedildi (parolayı kontrol edin)".into()),
                _ => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
        Err("bağlantı zaman aşımına uğradı".into())
    }

    /// Kurulum ağı profilini (yoksa oluşturup) etkinleştirir.
    async fn activate_hotspot(&self, dev: &str) -> Result<(), String> {
        let mut path = self.saved_wifi().await.into_iter().find(|s| s.id == HOTSPOT_ID).map(|s| s.path);
        if path.is_none() {
            let iface: String = p(dev, IF_DEVICE, "Interface").await.unwrap_or_default();
            let m = nm_call(NM_SETTINGS, IF_SETTINGS, "AddConnection", &(hotspot_settings(&iface),))
                .await
                .map_err(|e| format!("erişim noktası profili oluşturulamadı: {e}"))?;
            path = Some(m.body().deserialize::<OwnedObjectPath>().map_err(|e| e.to_string())?.as_str().to_string());
        }
        let ac = self.activate(path.as_deref().unwrap_or("/"), dev).await?;
        self.wait_active(&ac, Duration::from_secs(30)).await
    }

    /// Görünür AP'nin bayraklarından PSK/SAE seçimi.
    async fn ap_key_mgmt(&self, dev: &str, ssid: &str) -> &'static str {
        if let Ok(m) = nm_call(dev, IF_WIRELESS, "GetAllAccessPoints", &()).await {
            for ap in m.body().deserialize::<Vec<OwnedObjectPath>>().unwrap_or_default() {
                if p::<Vec<u8>>(ap.as_str(), IF_AP, "Ssid").await.as_deref() != Some(ssid.as_bytes()) {
                    continue;
                }
                let rsn: u32 = p(ap.as_str(), IF_AP, "RsnFlags").await.unwrap_or(0);
                let wpa: u32 = p(ap.as_str(), IF_AP, "WpaFlags").await.unwrap_or(0);
                if (rsn | wpa) & SEC_PSK == 0 && rsn & SEC_SAE != 0 {
                    return "sae";
                }
            }
        }
        "wpa-psk"
    }

    /// Tek radyo erişim noktası kipindeyken tarayamaz: kurulum ağı birkaç saniye
    /// kapatılır ve tarama sonunda geri açılır.
    async fn nm_scan(&self) -> ApiResult<Vec<Ap>> {
        let _g = self.mu.lock().await;
        self.bus().await?;
        let (dev, _) = self.wifi_device().await?;
        let was_ap = self.in_ap_mode(&dev).await;
        if was_ap {
            self.deactivate_device(&dev).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let res = self.scan_inner(&dev).await;
        if was_ap {
            if let Err(e) = self.activate_hotspot(&dev).await {
                tracing::error!(err = %e, "tarama sonrası erişim noktası açılamadı");
            }
        }
        res
    }

    async fn scan_inner(&self, dev: &str) -> ApiResult<Vec<Ap>> {
        self.radio_on().await;
        let before: i64 = p(dev, IF_WIRELESS, "LastScan").await.unwrap_or(0);
        let _ = nm_call(dev, IF_WIRELESS, "RequestScan", &(HashMap::<&str, Value>::new(),)).await;
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline {
            if p::<i64>(dev, IF_WIRELESS, "LastScan").await.unwrap_or(0) != before {
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        let known: std::collections::HashSet<String> = self.saved_wifi().await.into_iter().filter(|s| !s.is_hotspot()).map(|s| s.ssid).collect();
        let active = path_prop(dev, IF_WIRELESS, "ActiveAccessPoint").await.unwrap_or_default();
        let m = nm_call(dev, IF_WIRELESS, "GetAllAccessPoints", &()).await.map_err(|_| fail("Wi-Fi taraması başarısız oldu."))?;
        let aps: Vec<OwnedObjectPath> = m.body().deserialize().map_err(|_| fail("Wi-Fi taraması başarısız oldu."))?;
        let mut list = Vec::with_capacity(aps.len());
        for ap in aps {
            let a = ap.as_str();
            let ssid = String::from_utf8_lossy(&p::<Vec<u8>>(a, IF_AP, "Ssid").await.unwrap_or_default()).into_owned();
            let strength: u8 = p(a, IF_AP, "Strength").await.unwrap_or(0);
            let flags: u32 = p(a, IF_AP, "Flags").await.unwrap_or(0);
            let wpa: u32 = p(a, IF_AP, "WpaFlags").await.unwrap_or(0);
            let rsn: u32 = p(a, IF_AP, "RsnFlags").await.unwrap_or(0);
            list.push(Ap { known: known.contains(&ssid), active: a == active, ssid, signal: i32::from(strength), security: security(flags, wpa, rsn).into() });
        }
        Ok(dedupe_aps(list))
    }

    /// Bağlantı mantığı:
    ///   - kayıtlı profil ve yeni parola yoksa profil etkinleştirilir;
    ///   - yeni parola/kimlik varsa yeni profil eklenir, başarıda eskisi silinir
    ///     (başarısızlıkta eski parola korunur);
    ///   - başarısızlıkta önceki bağlantı, o da olmazsa kurulum ağı (MakerOS) açılır.
    async fn nm_connect(&self, req: ConnectRequest, pr: Progress<'_>) -> ApiResult<()> {
        if !valid_ssid(&req.ssid) {
            return Err(ApiError::invalid("Ağ adı 1–32 bayt olmalıdır; Türkçe karakterler birden çok bayt kullanabilir."));
        }
        let _g = self.mu.lock().await;
        self.bus().await?;
        let (dev, iface) = self.wifi_device().await?;
        let mut previous = match path_prop(&dev, IF_DEVICE, "ActiveConnection").await {
            Some(ac) => path_prop(&ac, IF_ACTIVE, "Connection").await,
            None => None,
        };
        let was_ap = self.in_ap_mode(&dev).await;
        pr(10, &format!("{} ağına bağlanılıyor…", req.ssid));
        self.radio_on().await;
        if was_ap {
            pr(15, "Kurulum ağı (MakerOS) kapatılıyor…");
            self.deactivate_device(&dev).await;
            previous = None;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let existing: Vec<SavedConn> = self.saved_wifi().await.into_iter().filter(|s| s.ssid == req.ssid && !s.is_hotspot()).collect();
        let mut created: Option<String> = None;
        let res: Result<String, String> = if !existing.is_empty() && req.password.is_empty() && req.eap.is_none() {
            pr(30, "Kayıtlı profil etkinleştiriliyor…");
            self.activate(&existing[0].path, &dev).await
        } else {
            pr(30, "Yeni profil oluşturuluyor…");
            let km = self.ap_key_mgmt(&dev, &req.ssid).await;
            match nm_call(NM_PATH, IF_NM, "AddAndActivateConnection", &(wifi_settings(&req, km), opath(&dev), opath("/"))).await {
                Err(e) => Err(e.to_string()),
                Ok(m) => match m.body().deserialize::<(OwnedObjectPath, OwnedObjectPath)>() {
                    Ok((c, a)) => {
                        created = Some(c.as_str().to_string());
                        Ok(a.as_str().to_string())
                    }
                    Err(e) => Err(e.to_string()),
                },
            }
        };
        let res = match res {
            Ok(ac) => {
                pr(50, "Kimlik doğrulanıyor ve adres alınıyor…");
                self.wait_active(&ac, Duration::from_secs(45)).await
            }
            Err(e) => Err(e),
        };
        let err = match res {
            Ok(()) => {
                if created.is_some() {
                    for s in &existing {
                        let _ = nm_call(&s.path, IF_CONN, "Delete", &()).await;
                    }
                }
                pr(100, &format!("{} ağına bağlanıldı ({iface}).", req.ssid));
                return Ok(());
            }
            Err(e) => e,
        };
        // Geri dönüş.
        if let Some(c) = &created {
            let _ = nm_call(c, IF_CONN, "Delete", &()).await;
        }
        let detail = format!("Ağa bağlanılamadı: {err}");
        pr(80, "Bağlantı başarısız; önceki bağlantı geri yükleniyor…");
        let restored = match &previous {
            Some(prev) => match self.activate(prev, &dev).await {
                Ok(pac) => self.wait_active(&pac, Duration::from_secs(25)).await.is_ok(),
                Err(_) => false,
            },
            None => false,
        };
        if restored {
            return Err(fail(format!("{detail}\nÖnceki bağlantı geri açıldı.")));
        }
        pr(90, "Kurulum ağı (MakerOS) açılıyor…");
        if let Err(h) = self.activate_hotspot(&dev).await {
            return Err(fail(format!("{detail}\nGeri dönüş de başarısız: {h}")));
        }
        Err(fail(format!("{detail}\nKurulum ağı (MakerOS) açıldı; http://{HOTSPOT_IP}/ adresinden tekrar deneyin.")))
    }

    async fn nm_forget(&self, ssid: &str) -> ApiResult<()> {
        let _g = self.mu.lock().await;
        self.bus().await?;
        let mut found = false;
        for s in self.saved_wifi().await.into_iter().filter(|s| s.ssid == ssid && !s.is_hotspot()) {
            found = true;
            nm_call(&s.path, IF_CONN, "Delete", &()).await.map_err(|e| ApiError::internal(format!("Ağ profili silinemedi: {e}")))?;
        }
        if found { Ok(()) } else { Err(ApiError::not_found(format!("Kayıtlı Wi-Fi profili bulunamadı: {ssid}"))) }
    }

    /// WirelessEnabled (polkit: enable-disable-wifi).
    async fn nm_set_wifi(&self, enabled: bool) -> ApiResult<()> {
        self.bus().await?;
        match dbus::set(NM_DEST, NM_PATH, IF_NM, "WirelessEnabled", Value::from(enabled)).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let n = dbus::err_name(&e);
                if n.ends_with("PermissionDenied") || n.ends_with("AccessDenied") {
                    Err(ApiError::forbidden("Wi-Fi radyosu değiştirilemedi: yetki yok."))
                } else {
                    Err(fail(format!("Wi-Fi radyosu değiştirilemedi: {e}")))
                }
            }
        }
    }
}

impl Provider for Nm {
    fn status(&self) -> BoxFuture<'_, ApiResult<Status>> {
        Box::pin(self.nm_status())
    }
    fn scan(&self) -> BoxFuture<'_, ApiResult<Vec<Ap>>> {
        Box::pin(self.nm_scan())
    }
    fn connect<'a>(&'a self, req: ConnectRequest, p: Progress<'a>) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(self.nm_connect(req, p))
    }
    fn forget<'a>(&'a self, ssid: &'a str) -> BoxFuture<'a, ApiResult<()>> {
        Box::pin(self.nm_forget(ssid))
    }
    fn set_wifi_enabled(&self, enabled: bool) -> BoxFuture<'_, ApiResult<()>> {
        Box::pin(self.nm_set_wifi(enabled))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_flags() {
        for (f, w, r, want) in [
            (0, 0, 0, "open"),
            (1, 0, 0, "wpa2"),
            (1, 0, SEC_PSK, "wpa2"),
            (1, SEC_PSK, SEC_PSK, "wpa2"),
            (1, 0, SEC_SAE, "wpa3"),
            (1, 0, SEC_SAE | SEC_PSK, "wpa2"),
            (1, 0, SEC_8021X, "enterprise"),
        ] {
            assert_eq!(security(f, w, r), want, "{f:x},{w:x},{r:x}");
        }
    }

    #[test]
    fn dedupe() {
        let ap = |s: &str, sig: i32, active: bool, known: bool| Ap { ssid: s.into(), signal: sig, security: String::new(), known, active };
        let out = dedupe_aps(vec![ap("A", 30, true, false), ap("A", 70, false, false), ap("B", 90, false, false), ap("", 99, false, false), ap(HOTSPOT_SSID, 99, false, false), ap("C", 50, false, true)]);
        assert_eq!(out.iter().map(|a| (a.ssid.as_str(), a.signal, a.active)).collect::<Vec<_>>(), vec![("A", 70, true), ("B", 90, false), ("C", 50, false)]);
        assert!(valid_ssid("Kafe") && !valid_ssid("") && !valid_ssid(&"ç".repeat(18)));
    }

    #[test]
    fn sysfs() {
        let root = std::env::temp_dir().join(format!("mk-net-{}", crate::util::random_hex(4)));
        let net = root.join("sys/class/net");
        for (n, files) in [
            ("lo", vec![("type", "772")]),
            ("eth0", vec![("type", "1"), ("address", "aa:bb"), ("operstate", "up"), ("speed", "1000")]),
            ("wlan0", vec![("type", "1"), ("address", "cc:dd"), ("operstate", "down")]),
            ("docker0", vec![("type", "1")]),
        ] {
            std::fs::create_dir_all(net.join(n)).unwrap();
            for (f, v) in files {
                std::fs::write(net.join(n).join(f), format!("{v}\n")).unwrap();
            }
        }
        std::fs::create_dir_all(net.join("wlan0/wireless")).unwrap();
        std::fs::create_dir_all(root.join("proc/net")).unwrap();
        std::fs::write(root.join("proc/net/route"), "Iface\tDestination\tGateway\neth0\t00000000\t0101A8C0\n").unwrap();
        let st = sysfs_status(root.to_str().unwrap());
        assert!(st.online && st.wifi_enabled);
        let got: Vec<(&str, &str, &str, Option<i64>)> = st.ifaces.iter().map(|i| (i.name.as_str(), i.kind.as_str(), i.state.as_str(), i.speed_mbps)).collect();
        assert_eq!(got, vec![("eth0", "ethernet", "connected", Some(1000)), ("wlan0", "wifi", "disconnected", None)]);
        let j = serde_json::to_value(&st.ifaces[1]).unwrap();
        assert_eq!(j["type"], "wifi");
        assert!(j.get("speed_mbps").is_none() && j.get("ssid").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn settings_shape() {
        let s = wifi_settings(&ConnectRequest { ssid: "Ev".into(), password: "parola12".into(), ..Default::default() }, "sae");
        assert_eq!(<&str>::try_from(&s["802-11-wireless-security"]["key-mgmt"]).unwrap(), "sae");
        let e = wifi_settings(&ConnectRequest { ssid: "Okul".into(), eap: Some(Eap { identity: "a".into(), password: "b".into(), method: String::new() }), ..Default::default() }, "");
        assert!(e["802-1x"].contains_key("phase2-auth"));
        let u = new_uuid();
        assert_eq!((u.len(), &u[14..15]), (36, "4"));
        // D-Bus imzası a{sa{sv}} olmalı.
        use zbus::zvariant::Type;
        assert_eq!(Out::SIGNATURE.to_string(), "a{sa{sv}}");
        assert!(hotspot_settings("wlan0")["ipv4"].contains_key("address-data"));
    }
}
