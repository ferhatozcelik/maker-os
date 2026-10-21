# Aygitlar: USB aygitlari, USB bellek/disk baglama ve guvenle cikarma,
# seri portlar (Arduino vb.), ekran cikislari (HDMI/DSI) ve ses aygitlari.
import glob
import json
import os
import re

from common import USER, read_first, run, spawn_user, user_info

# USB sinif kodu -> Turkce tur
USB_CLASSES = {
    "01": "Ses", "02": "Seri/iletişim", "03": "Klavye/fare (HID)", "06": "Kamera (PTP)",
    "07": "Yazıcı", "08": "Depolama", "09": "Hub", "0a": "Seri/iletişim", "0e": "Kamera",
    "e0": "Kablosuz (Bluetooth/Wi-Fi)", "ef": "Çok işlevli", "ff": "Üreticiye özel",
}
SYSTEM_MOUNTS = ("/", "/boot/firmware", "/boot")


def _sys(path, name, default=""):
    return read_first(os.path.join(path, name), default)


# --- USB aygitlari ---------------------------------------------------------------

def usb_devices():
    devs = []
    for path in sorted(glob.glob("/sys/bus/usb/devices/*")):
        name = os.path.basename(path)
        if ":" in name or name.startswith("usb"):
            continue  # arayuzler ve kok hub'lar
        classes = {_sys(path, "bDeviceClass")}
        for intf in glob.glob(path + "/" + name + ":*"):
            classes.add(_sys(intf, "bInterfaceClass"))
        classes.discard("00")
        classes.discard("")
        kinds = sorted({USB_CLASSES.get(c, "") for c in classes} - {""})
        if "Hub" in kinds and len(kinds) > 1:
            kinds.remove("Hub")
        product = _sys(path, "product")
        if "Hub" in kinds and not product:
            product = "USB hub"
        speed = _sys(path, "speed")
        devs.append({
            "port": name,
            "id": "%s:%s" % (_sys(path, "idVendor"), _sys(path, "idProduct")),
            "vendor": _sys(path, "manufacturer"),
            "product": product or "Bilinmeyen aygıt",
            "kinds": kinds,
            "speed": {"1.5": "USB 1.0", "12": "USB 1.1", "480": "USB 2.0",
                      "5000": "USB 3.0", "10000": "USB 3.1"}.get(speed, speed and speed + " Mb/s"),
            "hub": "Hub" in kinds,
        })
    return devs


def serial_ports():
    ports = []
    for dev in sorted(glob.glob("/dev/ttyUSB*") + glob.glob("/dev/ttyACM*")):
        sysdev = os.path.realpath("/sys/class/tty/%s/device" % os.path.basename(dev))
        # ttyUSB: .../usbX/X-Y/X-Y:1.0/ttyUSB0 ; ttyACM: .../X-Y/X-Y:1.0
        usb = sysdev
        while usb != "/" and not os.path.exists(os.path.join(usb, "idVendor")):
            usb = os.path.dirname(usb)
        ports.append({"dev": dev, "product": _sys(usb, "product", "Seri aygıt"),
                      "vendor": _sys(usb, "manufacturer")})
    for dev, name in (("/dev/serial0", "GPIO UART (GPIO14/15)"),):
        if os.path.exists(dev):
            ports.append({"dev": dev, "product": name, "vendor": ""})
    return ports


# --- USB depolama ----------------------------------------------------------------

def _lsblk():
    rc, out, _ = run(["lsblk", "-J", "-b", "-o",
                      "NAME,PATH,SIZE,TYPE,FSTYPE,LABEL,MOUNTPOINT,TRAN,MODEL,VENDOR"])
    if rc != 0:
        return []
    try:
        return json.loads(out).get("blockdevices", [])
    except ValueError:
        return []


def _usb_disks():
    return [d for d in _lsblk() if d.get("type") == "disk" and d.get("tran") == "usb"]


def storage():
    disks = []
    for d in _usb_disks():
        parts = d.get("children") or ([d] if d.get("fstype") else [])
        system = any(p.get("mountpoint") in SYSTEM_MOUNTS for p in parts)
        disks.append({
            "path": d["path"], "size": int(d.get("size") or 0), "system": system,
            "model": " ".join(x for x in ((d.get("vendor") or "").strip(), (d.get("model") or "").strip()) if x)
                     or "USB disk",
            "parts": [{"path": p["path"], "size": int(p.get("size") or 0), "fstype": p.get("fstype") or "",
                       "label": p.get("label") or "", "mountpoint": p.get("mountpoint") or ""}
                      for p in parts],
        })
    return disks


def _find_part(path):
    for d in _usb_disks():
        for p in d.get("children") or [d]:
            if p.get("path") == path:
                return d, p
    return None, None


def mount(path):
    disk, part = _find_part(path)
    if not part:
        return False, "USB bolumu bulunamadi"
    if part.get("mountpoint"):
        return True, part["mountpoint"]
    fstype = part.get("fstype") or ""
    if not fstype:
        return False, "Dosya sistemi taninmadi (bicimlendirilmemis olabilir)"
    uid, gid, _ = user_info()
    label = re.sub(r"[^\w.-]+", "_", part.get("label") or os.path.basename(path)).strip("_") or "usb"
    target = "/media/%s/%s" % (USER, label)
    n = 1
    while os.path.ismount(target):
        n += 1
        target = "/media/%s/%s_%d" % (USER, label, n)
    os.makedirs(target, exist_ok=True)
    opts = "nosuid,nodev,noatime"
    cmd = ["mount"]
    if fstype in ("vfat", "exfat", "ntfs"):
        opts += ",uid=%d,gid=%d,umask=022" % (uid, gid)
        if fstype == "ntfs":
            fstype = "ntfs-3g" if os.path.exists("/sbin/mount.ntfs-3g") else "ntfs3"
    cmd += ["-t", fstype, "-o", opts, path, target]
    rc, _, err = run(cmd, timeout=60)
    if rc != 0:
        try:
            os.rmdir(target)
        except OSError:
            pass
        return False, err or "baglanamadi"
    if fstype not in ("vfat", "exfat", "ntfs-3g", "ntfs3"):
        # ext4 vb.: kok dizini kullanicinin degilse en azindan yazabilsin.
        st = os.stat(target)
        if st.st_uid == 0:
            os.chown(target, uid, gid)
    return True, target


def eject(disk_path):
    disk = next((d for d in _usb_disks() if d["path"] == disk_path), None)
    if not disk:
        return False, "USB disk bulunamadi"
    parts = disk.get("children") or [disk]
    if any(p.get("mountpoint") in SYSTEM_MOUNTS for p in parts):
        return False, "Sistem bu diskten calisiyor; cikarilamaz"
    run(["sync"], timeout=120)
    for p in parts:
        mp = p.get("mountpoint")
        if mp:
            rc, _, err = run(["umount", mp], timeout=60)
            if rc != 0:
                return False, "%s ayrilamadi: %s (acik dosya olabilir)" % (mp, err)
            if mp.startswith("/media/"):
                try:
                    os.rmdir(mp)
                except OSError:
                    pass
    # Aygiti cekirdekten cikar: guc kesilir, fiziksel olarak cekilebilir.
    name = os.path.basename(disk_path)
    try:
        with open("/sys/block/%s/device/delete" % name, "w") as f:
            f.write("1")
    except OSError:
        pass
    return True, "Güvenle çıkarabilirsiniz"


def open_folder(path):
    """Bagli USB bellegi Pi'nin masaustunde Dosyalar (Thunar) ile acar."""
    real = os.path.realpath(path)
    if not real.startswith("/media/") or not os.path.ismount(real):
        return False, "gecersiz klasor"
    spawn_user(["thunar", real])
    return True, "Dosyalar acildi"


# --- Ekranlar (DRM baglayicilari) -----------------------------------------------

def _edid_name(edid):
    # EDID 18 baytlik tanimlayicilari: 0xFC = monitor adi.
    for off in (54, 72, 90, 108):
        block = edid[off:off + 18]
        if len(block) == 18 and block[:3] == b"\0\0\0" and block[3] == 0xFC:
            return block[5:].split(b"\n")[0].decode(errors="replace").strip()
    return ""


def displays():
    out, seen = [], set()
    for path in sorted(glob.glob("/sys/class/drm/card*-*")):
        conn = os.path.basename(path).split("-", 1)[1]
        if conn.startswith("Writeback") or conn in seen:
            continue
        seen.add(conn)
        status = _sys(path, "status")
        modes = _sys(path, "modes").splitlines()
        try:
            with open(os.path.join(path, "edid"), "rb") as f:
                name = _edid_name(f.read())
        except OSError:
            name = ""
        kind = "HDMI" if conn.startswith("HDMI") else "DSI (dokunmatik ekran)" if conn.startswith("DSI") \
            else "Kompozit (TV)" if conn.startswith("Composite") else conn
        port = {"HDMI-A-1": "HDMI 0 (USB-C yanı)", "HDMI-A-2": "HDMI 1"}.get(conn, "")
        out.append({"connector": conn, "kind": kind, "port": port, "connected": status == "connected",
                    "monitor": name, "modes": list(dict.fromkeys(modes))[:12],
                    "enabled": _sys(path, "enabled") == "enabled"})
    return out


def current_modes():
    """Calisan X oturumunda her cikisin etkin cozunurlugu (xrandr)."""
    from common import run_user
    rc, out, _ = run_user(["xrandr", "--current"], timeout=10)
    res = {}
    if rc == 0:
        for line in out.splitlines():
            m = re.match(r"^(\S+) connected (?:primary )?(\d+x\d+)\+", line)
            if m:
                res[m.group(1)] = m.group(2)
    return res


# --- Ses -------------------------------------------------------------------------

def audio():
    cards = {}
    for line in read_first("/proc/asound/cards").splitlines():
        m = re.match(r"^\s*(\d+)\s+\[(\S+)\s*\]:\s*(.*?)\s+-\s+(.*)$", line)
        if m:
            cards[m.group(1)] = {"card": int(m.group(1)), "id": m.group(2), "name": m.group(4).strip(),
                                 "playback": False, "capture": False}
    for line in read_first("/proc/asound/pcm").splitlines():
        m = re.match(r"^(\d+)-\d+:", line)
        if m and m.group(1) in cards:
            cards[m.group(1)]["playback"] |= "playback" in line
            cards[m.group(1)]["capture"] |= "capture" in line
    return list(cards.values())
