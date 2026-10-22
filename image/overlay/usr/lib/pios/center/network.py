"""NetworkManager işlemleri: tek işlem, sınırlı bekleme, geri dönüş ve tanılama."""
import copy
import fcntl
import os
import secrets
import threading
import time
from contextlib import contextmanager

from common import read_first, run

HOTSPOT = "pardus-hotspot"
_lock = threading.Lock()
_state_lock = threading.Lock()
_state = {"status": "idle", "kind": "", "message": "", "networks": [], "updated": 0}


class NetworkError(RuntimeError):
    pass


def split(line):
    """nmcli terse: iki nokta ve ters eğik çizgi kaçışlarını birlikte çöz."""
    parts, buf, escaped = [], "", False
    for char in line:
        if escaped:
            buf += char
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == ":":
            parts.append(buf)
            buf = ""
        else:
            buf += char
    parts.append(buf + ("\\" if escaped else ""))
    return parts


def nm(args, wait=15):
    return run(["nmcli", "--wait", str(wait)] + args, timeout=wait + 5,
               env=dict(os.environ, LC_ALL="C"))


def checked(args, stage, wait=15, secrets=()):
    rc, out, err = nm(args, wait)
    if rc:
        detail = err or out or "NetworkManager açıklama döndürmedi."
        for secret in secrets:
            if secret:
                detail = detail.replace(secret, "[gizlendi]")
        raise NetworkError("%s\nÇıkış kodu: %s\n%s" % (stage, rc, detail))
    return out


def device():
    out = checked(["-t", "-f", "DEVICE,TYPE", "device", "status"],
                  "Wi-Fi aygıtı sorgulanamadı. NetworkManager hizmetini kontrol edin.")
    devices = [split(l)[0] for l in out.splitlines() if split(l)[-1] == "wifi"]
    if not devices:
        raise NetworkError("Wi-Fi aygıtı bulunamadı. Adaptörü, sürücüyü ve rfkill durumunu kontrol edin.")
    return "wlan0" if "wlan0" in devices else devices[0]


def profiles():
    out = checked(["-t", "-f", "NAME,UUID,TYPE", "connection", "show"], "Kayıtlı ağlar okunamadı.")
    return [dict(zip(("name", "uuid", "type"), split(l))) for l in out.splitlines() if len(split(l)) == 3]


def saved():
    return [p["name"] for p in profiles() if p["type"] == "802-11-wireless" and p["name"] != HOTSPOT]


def overview():
    rc, out, err = nm(["-t", "-f", "RUNNING", "general"], 5)
    running = rc == 0 and out.strip() == "running"
    try:
        names = saved() if running else []
        warning = "" if running else "NetworkManager çalışmıyor.\n" + (err or out)
    except NetworkError as e:
        names, warning = [], str(e)
    return {"nm": running, "saved": names, "warning": warning}


def active_uuid(iface):
    rc, out, _ = nm(["-g", "GENERAL.CON-UUID", "device", "show", iface])
    return out if rc == 0 and out != "--" else ""


def is_hotspot(iface):
    rc, out, _ = nm(["-g", "GENERAL.CONNECTION", "device", "show", iface])
    return rc == 0 and out == HOTSPOT


def ensure_hotspot(iface):
    if any(p["name"] == HOTSPOT for p in profiles()):
        return
    suffix = read_first("/sys/class/net/%s/address" % iface).replace(":", "")[-4:].upper() or "0000"
    password = os.environ.get("PIOS_HOTSPOT_PSK", "pardus1234")
    checked(["connection", "add", "type", "wifi", "ifname", iface, "con-name", HOTSPOT,
             "autoconnect", "no", "ssid", "Pardus-Maker-" + suffix, "mode", "ap",
             "802-11-wireless.band", "bg", "802-11-wireless.channel", "6",
             "ipv4.method", "shared", "ipv4.addresses", "10.42.0.1/24", "ipv6.method", "ignore",
             "wifi-sec.key-mgmt", "wpa-psk", "wifi-sec.proto", "rsn", "wifi-sec.psk", password],
            "Erişim noktası profili oluşturulamadı.", secrets=(password,))


def _stage(message):
    with _state_lock:
        _state.update(message=message, updated=time.time())


def operation():
    with _state_lock:
        return copy.deepcopy(_state)


@contextmanager
def exclusive():
    os.makedirs("/run/pios", exist_ok=True)
    with open("/run/pios/network.lock", "a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise NetworkError("Başka bir ağ işlemi çalışıyor. İlk açılış ağ kurulumu tamamlandıktan sonra tekrar deneyin.")
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def start(kind, data):
    if kind not in ("scan", "connect", "hotspot", "forget"):
        return False, "Geçersiz ağ işlemi."
    if not _lock.acquire(blocking=False):
        return False, "Bir ağ işlemi devam ediyor. İşlem durumunu bekleyin."
    with _state_lock:
        _state.update(status="running", kind=kind, message="İşlem başlatılıyor…", updated=time.time())
    threading.Thread(target=_worker, args=(kind, dict(data)), daemon=True).start()
    return True, "İşlem başlatıldı. Ağ değişiminde bağlantınız geçici olarak kesilebilir."


def _worker(kind, data):
    try:
        # HTTP yanıtı radyoyu kapatmadan önce istemciye ulaşabilsin.
        time.sleep(1)
        with exclusive():
            if kind == "scan":
                result = scan(bool(data.get("interrupt")))
                with _state_lock:
                    _state["networks"] = result
                message = "%s ağ bulundu." % len(result)
            elif kind == "connect":
                message = connect(data)
            elif kind == "hotspot":
                if data.get("on"):
                    ensure_hotspot(device())
                checked(["connection", "up" if data.get("on") else "down", "id", HOTSPOT],
                        "Erişim noktası değiştirilemedi.", 30)
                message = "Erişim noktası ayarlandı."
            else:
                name = str(data.get("name", ""))
                if name not in saved():
                    raise NetworkError("Kayıtlı Wi-Fi profili bulunamadı: " + name)
                checked(["connection", "delete", "id", name], "Ağ profili silinemedi.")
                message = "Kayıtlı ağ silindi."
        with _state_lock:
            _state.update(status="ok", message=message, updated=time.time())
    except Exception as e:
        with _state_lock:
            _state.update(status="error", message=str(e), updated=time.time())
    finally:
        _lock.release()


def scan(interrupt=False):
    iface = device()
    ap = is_hotspot(iface)
    if ap and not interrupt:
        raise NetworkError("Erişim noktası açıkken bu Wi-Fi radyosu tarama yapamayabilir. "
                           "‘Erişim noktasını duraklat ve tara’ seçeneğini kullanın; "
                           "tarama sonunda erişim noktası yeniden açılır. Ağ adı biliniyorsa elle bağlanabilirsiniz.")
    try:
        if ap:
            _stage("Erişim noktası duraklatılıyor; tarama sonunda yeniden açılacak.")
            checked(["connection", "down", "id", HOTSPOT], "Erişim noktası duraklatılamadı.")
            time.sleep(2)
        checked(["radio", "wifi", "on"], "Wi-Fi radyosu açılamadı. rfkill engelini kontrol edin.")
        _stage("Yakındaki Wi-Fi ağları taranıyor…")
        out = checked(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list",
                       "ifname", iface, "--rescan", "yes"], "Wi-Fi taraması başarısız oldu.", 25)
        known = set(saved())
        nets = {}
        for line in out.splitlines():
            p = split(line)
            if len(p) != 4 or not p[1]:
                continue
            used, ssid, signal, security = p
            entry = {"ssid": ssid, "signal": int(signal) if signal.isdigit() else 0,
                     "secure": security not in ("", "--"), "enterprise": "802.1X" in security,
                     "active": used == "*", "saved": ssid in known}
            if ssid not in nets or entry["signal"] > nets[ssid]["signal"]:
                entry["active"] = entry["active"] or nets.get(ssid, {}).get("active", False)
                nets[ssid] = entry
            elif entry["active"]:
                nets[ssid]["active"] = True
        return sorted(nets.values(), key=lambda x: (not x["active"], -x["signal"]))
    finally:
        if ap and interrupt:
            checked(["connection", "up", "id", HOTSPOT],
                    "Tarama sonrası erişim noktası açılamadı. Ethernet veya yerel ekran üzerinden kontrol edin.", 30)


def connect(data):
    ssid = str(data.get("ssid", ""))
    password = str(data.get("password", ""))
    identity = str(data.get("identity", "")).strip()
    if not ssid or len(ssid.encode("utf-8")) > 32 or "\0" in ssid:
        raise NetworkError("Ağ adı 1–32 bayt olmalıdır; Türkçe karakterler birden çok bayt kullanabilir.")
    iface = device()
    previous = active_uuid(iface)
    ap = is_hotspot(iface)
    temporary = None
    candidate_ref = None
    _stage("%s ağına bağlanılıyor. Telefonunuzu da aynı ağa bağlayıp cihazın .local adresini açın." % ssid)
    try:
        checked(["radio", "wifi", "on"], "Wi-Fi radyosu açılamadı.")
        if ap:
            checked(["connection", "down", "id", HOTSPOT], "Erişim noktası kapatılamadı.")
        existing = next((p for p in profiles() if p["name"] == ssid and p["type"] == "802-11-wireless"), None)
        if identity or data.get("hidden") or existing:
            if existing:
                if password or identity:
                    temporary = "pios-attempt-" + secrets.token_hex(6)
                    checked(["connection", "clone", "uuid", existing["uuid"], temporary], "Kayıtlı ağın deneme profili oluşturulamadı.")
                    cloned = next(p for p in profiles() if p["name"] == temporary)
                    ref = ["uuid", cloned["uuid"]]
                    candidate_ref = ref
                else:
                    ref = ["uuid", existing["uuid"]]
            else:
                checked(["connection", "add", "type", "wifi", "ifname", iface, "con-name", ssid,
                         "ssid", ssid, "connection.autoconnect", "no"], "Wi-Fi profili oluşturulamadı.")
                ref = ["id", ssid]
            props = [] if existing and "hidden" not in data else ["802-11-wireless.hidden", "yes" if data.get("hidden") else "no"]
            if temporary:
                props += ["connection.autoconnect", "no"]
            if identity:
                props += ["wifi-sec.key-mgmt", "wpa-eap", "802-1x.eap", "peap",
                          "802-1x.phase2-auth", "mschapv2", "802-1x.identity", identity,
                          "802-1x.password", password]
            elif password:
                key_mgmt = checked(["-g", "802-11-wireless-security.key-mgmt", "connection", "show", "uuid", existing["uuid"]],
                                   "Kayıtlı ağın güvenlik türü okunamadı.") if existing else "wpa-psk"
                if key_mgmt == "wpa-eap":
                    props += ["802-1x.password", password]
                else:
                    props += ["wifi-sec.key-mgmt", "sae" if key_mgmt == "sae" else "wpa-psk", "wifi-sec.psk", password]
            if props:
                checked(["connection", "modify"] + ref + props, "Wi-Fi profili güncellenemedi.", secrets=(password,))
            checked(["connection", "up"] + ref + ["ifname", iface], "Ağa bağlanılamadı.", 45, (password,))
            checked(["connection", "modify"] + ref + ["connection.autoconnect", "yes"], "Otomatik bağlantı kaydedilemedi.")
            if temporary:
                checked(["connection", "modify"] + ref + ["connection.id", existing["name"]], "Bağlantı adı kaydedilemedi.")
                checked(["connection", "delete", "uuid", existing["uuid"]], "Eski ağ profili temizlenemedi.")
                temporary = None
        else:
            args = ["device", "wifi", "connect", ssid, "ifname", iface]
            if password:
                args += ["password", password]
            checked(args, "Ağa bağlanılamadı. Parolayı, sinyali ve modem DHCP ayarlarını kontrol edin.", 45, (password,))
        return "%s ağına bağlanıldı. Yeni adres: http://%s.local/" % (ssid, os.uname().nodename)
    except Exception as e:
        detail = str(e)
        if password:
            detail = detail.replace(password, "[gizlendi]")
        _stage("Bağlantı başarısız; önceki bağlantı geri yükleniyor…")
        if previous:
            rc, _, err = nm(["connection", "up", "uuid", previous], 25)
        else:
            rc, err = 1, "Önceki bağlantı yok."
        if rc:
            try:
                ensure_hotspot(iface)
                rc, _, err = nm(["connection", "up", "id", HOTSPOT], 25)
            except NetworkError as recovery_error:
                rc, err = 1, str(recovery_error)
        if temporary and candidate_ref:
            # Yeni parola başarısızsa önceki profil ve parolası korunur.
            nm(["connection", "delete"] + candidate_ref)
        recovery = "Önceki bağlantı veya erişim noktası geri açıldı." if not rc else "Geri dönüş de başarısız: " + err
        raise NetworkError(detail + "\n" + recovery)


def diagnostics():
    reports = []
    for args in (["-t", "-f", "STATE,CONNECTIVITY,WIFI-HW,WIFI", "general"],
                 ["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device", "status"]):
        rc, out, err = nm(args, 5)
        reports.append(out or err or "Çıkış kodu: %s" % rc)
    rc, out, err = run(["rfkill", "list"], timeout=5)
    reports.append(out or err)
    return {"operation": operation(), "details": "\n\n".join(reports),
            "hint": "Ağ değişirken telefonunuzu hedef ağa bağlayıp http://%s.local/ adresini açın." % os.uname().nodename}
