# Ayarlar: durum, Wi-Fi, gorunum, cihaz adi, parola, ekran, donanim
# arayuzleri, uzak erisim, guncellemeler, guc.
import os
import re
import shutil
import socket
import subprocess

from common import USER, jobs, read_first, run, run_user

HOTSPOT_CON = "pardus-hotspot"
BOOT = "/boot/firmware"
CONFIG_TXT = BOOT + "/config.txt"
CMDLINE_TXT = BOOT + "/cmdline.txt"
VNC_PASS = "/etc/pios/vnc.pass"
VNC_SECRET = "/etc/pios/vnc.secret"
# "auto": monitorun EDID'deki tercih ettigi mod (monitor yoksa 1024x768).
RESOLUTIONS = ["auto", "1280x720", "1600x900", "1920x1080"]

# Gorunum: GTK temasi, ikon temasi (Pardus 100. Yil), pencere temasi.
THEMES = {
    "light": {"gtk": "pardus", "icons": "pardus-xfce-yuzyil", "wm": "pardus-default"},
    "dark": {"gtk": "pardus-dark", "icons": "pardus-xfce-yuzyil-dark", "wm": "pardus-dark-default"},
}

# config.txt anahtari -> (acik deger, kapali deger)
INTERFACES = {
    "i2c": ("dtparam=i2c_arm", "on", "off"),
    "spi": ("dtparam=spi", "on", "off"),
    "serial": ("enable_uart", "1", "0"),
}


def status():
    temp = read_first("/sys/class/thermal/thermal_zone0/temp")
    mem = {}
    for line in read_first("/proc/meminfo").splitlines():
        k, v = line.split(":", 1)
        mem[k] = int(v.split()[0])
    disk = shutil.disk_usage("/")
    _, addrs, _ = run(["ip", "-4", "-o", "addr", "show", "scope", "global"])
    ips = [{"iface": l.split()[1], "addr": l.split()[3]} for l in addrs.splitlines()]
    _, active, _ = run(["nmcli", "-t", "-f", "NAME,TYPE,DEVICE", "con", "show", "--active"])
    conns = []
    for line in active.splitlines():
        name, ctype, dev = (line.rsplit(":", 2) + ["", ""])[:3]
        if ctype != "loopback":
            conns.append({"name": name, "type": ctype, "device": dev})
    model = read_first("/proc/device-tree/model").rstrip("\0")
    return {
        "hostname": socket.gethostname(),
        "model": model or "Raspberry Pi",
        "version": read_first("/etc/pios-build-version"),
        "os": next((l.split("=", 1)[1].strip('"') for l in read_first("/etc/os-release").splitlines()
                    if l.startswith("PRETTY_NAME=")), ""),
        "kernel": os.uname().release,
        "uptime_s": int(float(read_first("/proc/uptime", "0").split()[0])),
        "load": read_first("/proc/loadavg").split()[:3],
        "temp_c": round(int(temp) / 1000, 1) if temp.isdigit() else None,
        "mem_total_mb": mem.get("MemTotal", 0) // 1024,
        "mem_avail_mb": mem.get("MemAvailable", 0) // 1024,
        "disk_total_gb": round(disk.total / 1e9, 1),
        "disk_free_gb": round(disk.free / 1e9, 1),
        "ips": ips,
        "connections": conns,
        "hotspot": any(c["name"] == HOTSPOT_CON for c in conns),
    }


# --- Wi-Fi -----------------------------------------------------------------

def wifi_scan():
    saved = set(_saved_wifi())
    run(["nmcli", "dev", "wifi", "rescan"], timeout=15)
    _, out, _ = run(["nmcli", "-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "dev", "wifi", "list"])
    seen = {}
    for line in out.splitlines():
        # nmcli -t ':' karakterini '\:' olarak kacirir.
        parts = line.replace("\\:", "\0").split(":")
        if len(parts) < 4:
            continue
        in_use, ssid, signal, sec = [p.replace("\0", ":") for p in parts[:4]]
        if not ssid:
            continue
        sig = int(signal) if signal.isdigit() else 0
        if ssid not in seen or seen[ssid]["signal"] < sig:
            seen[ssid] = {"ssid": ssid, "signal": sig, "secure": bool(sec and sec != "--"),
                          "enterprise": "802.1X" in sec, "active": in_use == "*",
                          "saved": ssid in saved}
    return sorted(seen.values(), key=lambda n: -n["signal"])


def _saved_wifi():
    _, out, _ = run(["nmcli", "-t", "-f", "NAME,TYPE", "con", "show"])
    names = []
    for line in out.splitlines():
        name, _, ctype = line.replace("\\:", "\0").rpartition(":")
        name = name.replace("\0", ":")
        if ctype == "802-11-wireless" and name != HOTSPOT_CON:
            names.append(name)
    return names


def wifi_saved():
    return _saved_wifi()


def wifi_forget(name):
    if name not in _saved_wifi():
        return False, "kayitli ag bulunamadi"
    rc, out, err = run(["nmcli", "con", "delete", "id", name], timeout=20)
    return rc == 0, out or err


def wifi_connect(ssid, password, hidden=False, identity=""):
    if not ssid or len(ssid) > 32:
        return False, "gecersiz ag adi"
    # Tek Wi-Fi radyosu var: erisim noktasi aciksa once kapatilmali.
    run(["nmcli", "con", "down", HOTSPOT_CON], timeout=15)
    if identity:
        # Kurumsal ag (WPA2-Enterprise; eduroam, okul/kurum aglari): PEAP/MSCHAPv2.
        run(["nmcli", "con", "delete", "id", ssid], timeout=15)
        rc, out, err = run(["nmcli", "con", "add", "type", "wifi", "ifname", "wlan0", "con-name", ssid,
                            "ssid", ssid, "wifi-sec.key-mgmt", "wpa-eap", "802-1x.eap", "peap",
                            "802-1x.phase2-auth", "mschapv2", "802-1x.identity", identity,
                            "802-1x.password", password, "802-11-wireless.hidden", "yes" if hidden else "no",
                            "connection.autoconnect", "yes"], timeout=20)
        if rc == 0:
            rc, out, err = run(["nmcli", "con", "up", "id", ssid], timeout=60)
    elif not password and ssid in _saved_wifi():
        rc, out, err = run(["nmcli", "con", "up", "id", ssid], timeout=60)
    else:
        cmd = ["nmcli", "dev", "wifi", "connect", ssid, "ifname", "wlan0"]
        if password:
            cmd += ["password", password]
        if hidden:
            cmd += ["hidden", "yes"]
        rc, out, err = run(cmd, timeout=60)
    if rc != 0:
        # Baglanamazsa kullanici erisim noktasindan geri donebilsin.
        run(["nmcli", "con", "up", HOTSPOT_CON], timeout=30)
        if "Secrets were required" in err or "802-11-wireless-security.psk" in err:
            err = "Parola yanlis"
    return rc == 0, out or err


def ethernet():
    """Kablolu ag (eth0) durumu: kablo takili mi, hiz, IP."""
    if not os.path.isdir("/sys/class/net/eth0"):
        return None
    carrier = read_first("/sys/class/net/eth0/carrier") == "1"
    speed = read_first("/sys/class/net/eth0/speed")
    _, addr, _ = run(["ip", "-4", "-o", "addr", "show", "dev", "eth0", "scope", "global"])
    _, gw, _ = run(["ip", "-4", "route", "show", "default", "dev", "eth0"])
    return {"carrier": carrier,
            "speed": int(speed) if speed.lstrip("-").isdigit() and int(speed) > 0 else None,
            "ip": addr.split()[3] if addr else "",
            "gateway": gw.split()[2] if gw.startswith("default via") else "",
            "mac": read_first("/sys/class/net/eth0/address")}


def nm_running():
    rc, out, _ = run(["nmcli", "-t", "-f", "RUNNING", "general"], timeout=5)
    return rc == 0 and out.strip() == "running"


def internet():
    rc, _, _ = run(["ping", "-c", "1", "-W", "2", "1.1.1.1"], timeout=5)
    if rc != 0:
        return {"ok": False, "dns": False}
    try:
        socket.getaddrinfo("depo.pardus.org.tr", 80)
        dns = True
    except OSError:
        dns = False
    return {"ok": True, "dns": dns}


def hotspot(on):
    rc, out, err = run(["nmcli", "con", "up" if on else "down", HOTSPOT_CON], timeout=30)
    return rc == 0, out or err


# --- Gorunum ----------------------------------------------------------------

def _xfconf(channel, prop, value, vtype="string"):
    return run_user(["xfconf-query", "-c", channel, "-p", prop, "-n", "-t", vtype, "-s", value])


def get_theme():
    rc, out, _ = run_user(["xfconf-query", "-c", "xsettings", "-p", "/Net/ThemeName"])
    return "dark" if rc == 0 and out.endswith("dark") else "light"


def set_theme(mode):
    t = THEMES.get(mode)
    if not t:
        return False, "gecersiz tema"
    results = [
        _xfconf("xsettings", "/Net/ThemeName", t["gtk"]),
        _xfconf("xsettings", "/Net/IconThemeName", t["icons"]),
        _xfconf("xfwm4", "/general/theme", t["wm"]),
    ]
    run_user(["dconf", "write", "/net/launchpad/plank/docks/dock1/theme",
              "'Transparent'" if mode == "dark" else "'Default'"])
    failed = [e or o for rc, o, e in results if rc != 0]
    return not failed, "; ".join(failed)


# --- Tarih ve saat -------------------------------------------------------------
# Pi'de pilli donanim saati (RTC) yok: kapaliyken saat ilerlemez, acilista
# fake-hwclock son kayitli saati yukler, ag gelince systemd-timesyncd (NTP)
# duzeltir. Ag yoksa saat tarayicidan veya elle ayarlanabilir.
ZONEINFO = "/usr/share/zoneinfo"


def _timezones():
    rc, out, _ = run(["timedatectl", "list-timezones"], timeout=10)
    if rc == 0 and out:
        return out.splitlines()
    zones = []
    tab = read_first(ZONEINFO + "/zone1970.tab") or read_first(ZONEINFO + "/zone.tab")
    for line in tab.splitlines():
        if line and not line.startswith("#"):
            parts = line.split("\t")
            if len(parts) >= 3:
                zones.append(parts[2])
    return sorted(set(zones + ["UTC"]))


def _timezone():
    tz = read_first("/etc/timezone")
    if not tz:
        link = os.path.realpath("/etc/localtime")
        tz = link.split(ZONEINFO + "/", 1)[1] if ZONEINFO + "/" in link else "UTC"
    return tz


def time_status():
    rc, out, _ = run(["timedatectl", "show", "-p", "NTP", "-p", "NTPSynchronized", "-p", "Timezone"], timeout=10)
    props = dict(l.split("=", 1) for l in out.splitlines() if "=" in l) if rc == 0 else {}
    if not props:
        # systemd yoksa (Docker testi) timesyncd servis durumuna bakilir.
        rc2, _, _ = run(["pgrep", "-x", "systemd-timesyn"])
        props = {"NTP": "yes" if rc2 == 0 else "no", "NTPSynchronized": "no"}
    import time
    return {"epoch": time.time(), "timezone": props.get("Timezone") or _timezone(),
            "utc_offset": -time.altzone if time.localtime().tm_isdst > 0 else -time.timezone,
            "ntp": props.get("NTP") == "yes", "synced": props.get("NTPSynchronized") == "yes",
            "systemd": rc == 0, "timezones": _timezones()}


def set_timezone(tz):
    if tz not in _timezones() or not os.path.isfile(os.path.join(ZONEINFO, tz)):
        return False, "gecersiz saat dilimi"
    rc, _, err = run(["timedatectl", "set-timezone", tz])
    if rc != 0:
        tmp = "/etc/localtime.pios"
        if os.path.lexists(tmp):
            os.remove(tmp)
        os.symlink(os.path.join(ZONEINFO, tz), tmp)
        os.replace(tmp, "/etc/localtime")
    with open("/etc/timezone", "w") as f:
        f.write(tz + "\n")
    # Masaustu saati (panel) yeni dilimi oturum yenilenince gosterir.
    return True, "Saat dilimi: " + tz


def set_ntp(on):
    rc, _, err = run(["timedatectl", "set-ntp", "true" if on else "false"], timeout=20)
    if rc != 0:
        rc, _, err = run(["systemctl", "enable" if on else "disable", "--now", "systemd-timesyncd.service"], timeout=20)
    return rc == 0, err or ("Otomatik saat acildi" if on else "Otomatik saat kapatildi")


def sync_time():
    rc, _, err = run(["systemctl", "restart", "systemd-timesyncd.service"], timeout=20)
    return rc == 0, err or "Saat sunucusuyla esitleniyor"


def set_time(epoch):
    """Saati verilen Unix zamanina ayarlar (NTP kapatilir; aksi halde ag
    gelince timesyncd zaten dogru saate cekecegi icin elle ayar anlamsiz)."""
    import time
    try:
        epoch = float(epoch)
    except (TypeError, ValueError):
        return False, "gecersiz zaman"
    if not 946684800 < epoch < 4102444800:   # 2000-2100
        return False, "gecersiz zaman"
    stamp = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(epoch))
    run(["timedatectl", "set-ntp", "false"], timeout=20)
    rc, _, err = run(["timedatectl", "set-time", stamp], timeout=20)
    if rc != 0:
        rc, _, err = run(["date", "-s", "@%d" % int(epoch)], timeout=10)
    if rc != 0:
        return False, err or "saat ayarlanamadi"
    # Yeniden acilista ayni saatten devam etsin.
    run(["fake-hwclock", "save", "force"], timeout=10)
    return True, "Saat ayarlandi: " + stamp


# --- Dil ve klavye -------------------------------------------------------------
# Sistem dili /etc/default/locale'e yazilir (masaustu oturumu, uygulamalar,
# Chromium); masaustu yeniden baslatilinca gecerli olur. Imajda tr_TR ve
# en_US hazir; digerleri secilince locale-gen ile uretilir.
LOCALES = {
    "tr_TR.UTF-8": "Türkçe", "en_US.UTF-8": "English (US)", "en_GB.UTF-8": "English (UK)",
    "de_DE.UTF-8": "Deutsch", "fr_FR.UTF-8": "Français", "es_ES.UTF-8": "Español",
    "it_IT.UTF-8": "Italiano", "ru_RU.UTF-8": "Русский", "ar_SA.UTF-8": "العربية",
    "az_AZ.UTF-8": "Azərbaycan dili",
}
# (duzen, varyant) -> ad
KEYBOARDS = {
    "tr:": "Türkçe Q", "tr:f": "Türkçe F", "us:": "İngilizce (ABD)", "gb:": "İngilizce (İngiltere)",
    "de:": "Almanca", "fr:": "Fransızca", "es:": "İspanyolca", "it:": "İtalyanca", "ru:": "Rusça",
    "ara:": "Arapça", "az:": "Azerbaycan",
}
LOCALE_FILE = "/etc/default/locale"
KEYBOARD_FILE = "/etc/default/keyboard"


def _kv_file(path):
    out = {}
    for line in read_first(path).splitlines():
        k, _, v = line.partition("=")
        if v:
            out[k.strip()] = v.strip().strip('"')
    return out


def _write_kv(path, values):
    lines, done = [], set()
    for line in read_first(path).splitlines():
        k = line.partition("=")[0].strip()
        if k in values:
            lines.append('%s="%s"' % (k, values[k]))
            done.add(k)
        else:
            lines.append(line)
    lines += ['%s="%s"' % (k, v) for k, v in values.items() if k not in done]
    with open(path + ".tmp", "w") as f:
        f.write("\n".join(lines) + "\n")
    os.replace(path + ".tmp", path)


def language_status():
    loc = _kv_file(LOCALE_FILE)
    kb = _kv_file(KEYBOARD_FILE)
    _, avail, _ = run(["locale", "-a"])
    have = {l.lower().replace("utf8", "utf-8") for l in avail.splitlines()}
    return {"locale": loc.get("LANG", "tr_TR.UTF-8"), "locales": LOCALES,
            "installed": [k for k in LOCALES if k.lower() in have],
            "keyboard": "%s:%s" % (kb.get("XKBLAYOUT", "tr").split(",")[0], kb.get("XKBVARIANT", "").split(",")[0]),
            "keyboards": KEYBOARDS,
            "switch": "," in kb.get("XKBLAYOUT", "")}


def set_language(loc):
    if loc not in LOCALES:
        return False, "gecersiz dil"
    if loc not in language_status()["installed"]:
        gen = read_first("/etc/locale.gen")
        pattern = re.compile(r"^#\s*(%s UTF-8)\s*$" % re.escape(loc), re.M)
        if pattern.search(gen):
            gen = pattern.sub(r"\1", gen)
        elif not re.search(r"^%s UTF-8" % re.escape(loc), gen, re.M):
            gen += "\n%s UTF-8\n" % loc
        with open("/etc/locale.gen", "w") as f:
            f.write(gen)
        rc, _, err = run(["/usr/sbin/locale-gen"], timeout=300)
        if rc != 0:
            return False, err or "dil paketi uretilemedi"
    lang = loc.split(".")[0]
    _write_kv(LOCALE_FILE, {"LANG": loc, "LANGUAGE": "%s:%s" % (lang, lang.split("_")[0])})
    return True, "Masaüstü yeniden başlatılınca geçerli olur"


def set_keyboard(key, switch):
    if key not in KEYBOARDS:
        return False, "gecersiz klavye duzeni"
    layout, variant = key.split(":")
    values = {"XKBLAYOUT": layout, "XKBVARIANT": variant, "XKBOPTIONS": ""}
    if switch and layout != "us":
        # Ikinci duzen olarak ABD; Alt+Shift ile gecis.
        values = {"XKBLAYOUT": layout + ",us", "XKBVARIANT": variant + ",", "XKBOPTIONS": "grp:alt_shift_toggle"}
    _write_kv(KEYBOARD_FILE, values)
    # Calisan masaustune hemen uygula (uzak ekran dahil).
    cmd = ["setxkbmap", "-layout", values["XKBLAYOUT"], "-variant", values["XKBVARIANT"]]
    cmd += ["-option", "", "-option", values["XKBOPTIONS"]] if values["XKBOPTIONS"] else ["-option", ""]
    run_user(cmd, timeout=10)
    run(["setupcon", "-k", "--force"], timeout=20)
    return True, "Klavye düzeni: " + KEYBOARDS[key]


# --- Cihaz adi / parola -----------------------------------------------------

def set_hostname(name):
    if not re.fullmatch(r"[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?", name):
        return False, "Yalnizca kucuk harf, rakam ve '-' (en fazla 32 karakter)"
    old = socket.gethostname()
    rc, _, err = run(["hostnamectl", "set-hostname", name])
    if rc != 0:
        with open("/etc/hostname", "w") as f:
            f.write(name + "\n")
        run(["hostname", name])
    hosts = read_first("/etc/hosts")
    hosts = re.sub(r"^127\.0\.1\.1\s+.*$", "127.0.1.1\t" + name, hosts, flags=re.M)
    with open("/etc/hosts", "w") as f:
        f.write(hosts + "\n")
    run(["systemctl", "restart", "avahi-daemon.service"])
    return True, "%s -> %s.local" % (old, name)


def set_password(new):
    if len(new) < 6:
        return False, "Parola en az 6 karakter olmali"
    rc, _, err = run(["chpasswd"], input="%s:%s\n" % (USER, new))
    if rc != 0:
        return False, err
    # VNC parolasi en fazla 8 karakter kullanir (protokol siniri).
    run(["x11vnc", "-storepasswd", new, VNC_PASS])
    os.chmod(VNC_PASS, 0o600)
    with open(VNC_SECRET, "w") as f:
        f.write(new[:8])
    os.chmod(VNC_SECRET, 0o600)
    return True, "Parola degistirildi (giris, SSH, sudo ve uzak ekran)"


def vnc_password():
    return read_first(VNC_SECRET, "pardus")


# --- Ekran / donanim (config.txt, cmdline.txt; yeniden baslatma gerekir) ---

def _read_config():
    return read_first(CONFIG_TXT).splitlines()


def _write_config(lines):
    with open(CONFIG_TXT + ".tmp", "w") as f:
        f.write("\n".join(lines) + "\n")
    os.replace(CONFIG_TXT + ".tmp", CONFIG_TXT)


def hardware():
    lines = _read_config()
    state = {}
    for key, (prefix, on, _off) in INTERFACES.items():
        val = None
        for line in lines:
            if line.strip().startswith(prefix + "="):
                val = line.split("=", 2)[-1].strip()
        state[key] = val == on
    m = re.search(r"video=HDMI-A-1:(\S+)", read_first(CMDLINE_TXT))
    mode = re.match(r"\d+x\d+", m.group(1)) if m else None
    state["resolution"] = mode.group(0) if mode else ("auto" if m else "")
    state["resolutions"] = RESOLUTIONS
    return state


def set_interface(key, enabled):
    if key not in INTERFACES:
        return False, "gecersiz arayuz"
    prefix, on, off = INTERFACES[key]
    want = "%s=%s" % (prefix, on if enabled else off)
    lines = _read_config()
    out, done = [], False
    for line in lines:
        if line.strip().startswith(prefix + "="):
            if not done:
                out.append(want)
                done = True
            continue
        out.append(line)
    if not done:
        # [all] bolumunden once degil, dosya sonuna (gecerli bolum [all]).
        out.append(want)
    _write_config(out)
    return True, "Yeniden baslatinca gecerli olur"


def set_resolution(res):
    if res not in RESOLUTIONS:
        return False, "gecersiz cozunurluk"
    cmdline = read_first(CMDLINE_TXT)
    # "D": monitor takili olmasa da cikisi etkinlestir (ekransiz uzak masaustu).
    value = "video=HDMI-A-1:" + ("D" if res == "auto" else "%sM@60D" % res)
    if "video=HDMI-A-1:" in cmdline:
        cmdline = re.sub(r"video=HDMI-A-1:\S+", value, cmdline)
    else:
        cmdline += " " + value
    with open(CMDLINE_TXT, "w") as f:
        f.write(cmdline + "\n")
    return True, "Yeniden baslatinca gecerli olur"


# --- Uzak erisim -------------------------------------------------------------

REMOTE_UNITS = ["pios-vnc.service", "pios-novnc.service"]


def remote_enabled():
    rc, out, _ = run(["systemctl", "is-enabled"] + REMOTE_UNITS)
    return rc == 0 and all(l == "enabled" for l in out.splitlines())


def set_remote(on):
    rc, _, err = run(["systemctl", "enable" if on else "disable", "--now"] + REMOTE_UNITS)
    return rc == 0, err


# --- Guncellemeler -----------------------------------------------------------

def updates_check():
    return jobs.start("Guncellemeler denetleniyor", [
        "sh", "-c", "apt-get update && echo && echo '== Guncellenebilir paketler ==' && "
                    "apt list --upgradable 2>/dev/null | tail -n +2"])


def updates_install():
    env = dict(os.environ, DEBIAN_FRONTEND="noninteractive")
    return jobs.start("Sistem guncelleniyor", [
        "sh", "-c", "apt-get update && apt-get -y -o Dpkg::Options::=--force-confold full-upgrade "
                    "&& apt-get -y autoremove"], env=env)


# --- Guc / oturum ------------------------------------------------------------

def power(action):
    if action not in ("reboot", "poweroff"):
        return False
    subprocess.Popen(["sh", "-c", "sleep 2; systemctl %s" % action])
    return True


def restart_desktop():
    rc, _, err = run(["systemctl", "restart", "lightdm.service", "pios-vnc.service"])
    return rc == 0, err
