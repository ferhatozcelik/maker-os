"""Pardus Maker OTA: GitHub surumlerinden sistem guncellemesi.

Akis (kullanici baslatir):
  denetle -> indir + sha256 -> ac -> yedekle -> kur -> kurulum kancasi ->
  saglik kontrolu (basarisizsa otomatik geri al) -> [cekirdek: tryboot ile
  bir kez dene] -> onay bekle -> onaylaninca yedek ve gecici dosyalar silinir.

Kurulum Merkez'i yeniden baslattigi icin Merkez'den bagimsiz ayri bir surecte
calisir (python3 ota.py apply); durum STATE dosyasindan okunur. Kurulum
yarida kalirsa (elektrik kesintisi) acilista boot-check yedekten geri doner.

Paket bicimi (image/stages/10-ota-bundle.sh uretir):
  ota.json        {"format": 1, "version": "1.0.1", "kernel": "6.18.57-v8+"}
  root/...        Pardus Maker katmani (/usr/lib/pios, servisler, ayarlar...)
  kernel/boot/... cekirdek, DTB, overlay'ler, acilis yazilimi
  kernel/modules/ /lib/modules/<surum>
  hooks/post-install.sh, enable.txt, packages.txt
"""
import fcntl
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request

ROOT = os.environ.get("PIOS_OTA_ROOT", "/")
REPO = os.environ.get("PIOS_OTA_REPO", "ferhatozcelik/pardus-maker")
# PIOS_OTA_API: yalnizca test (Docker'da sahte yayin sunucusu).
API = os.environ.get("PIOS_OTA_API") or "https://api.github.com/repos/%s/releases/latest" % REPO
FORMAT = 1
CENTER_PORT = os.environ.get("PIOS_CENTER_PORT", "80")
HEALTH_SECS = 45
# Silinmeyen yerel dosyalar: cihaz ayarlari pakette olsa da ezilmez.
KEEP = {"boot/firmware/config.txt", "boot/firmware/cmdline.txt"}
BOOT_FILES = re.compile(r"^(kernel8\.img|bcm27\w*\.dtb|start\w*\.elf|fixup\w*\.dat|bootcode\.bin|overlays/.*)$")


def _p(rel):
    return os.path.join(ROOT, rel.lstrip("/"))


BASE = _p("/var/lib/pios/ota")
STATE = os.path.join(BASE, "state.json")
TMP = os.path.join(BASE, "tmp")
BACKUP = os.path.join(BASE, "backup")
LOG = os.path.join(BASE, "ota.log")
VERSION_FILE = _p("/etc/pios-build-version")
BOOT = _p("/boot/firmware")
TRYBOOT_DIR = os.path.join(BOOT, "pios-ota")
TRYBOOT_FLAG = "/proc/device-tree/chosen/bootloader/tryboot"


class OtaError(RuntimeError):
    pass


# --- surum ve durum -------------------------------------------------------------

def parse(v):
    m = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", (v or "").strip())
    return tuple(int(x) for x in m.groups()) if m else None


def current_version():
    try:
        with open(VERSION_FILE) as f:
            return f.read().strip()
    except OSError:
        return ""


def is_newer(latest, current):
    """Gelistirme build'leri (zaman damgali surum) her yayini eski sayar."""
    lv = parse(latest)
    if not lv:
        return False
    cv = parse(current)
    return cv is None or lv > cv


def load_state():
    try:
        with open(STATE) as f:
            s = json.load(f)
        return s if isinstance(s, dict) else {}
    except (OSError, ValueError):
        return {}


def save_state(**changes):
    s = load_state()
    s.update(changes)
    os.makedirs(BASE, exist_ok=True)
    tmp = STATE + ".tmp"
    with open(tmp, "w") as f:
        json.dump(s, f, ensure_ascii=False)
    os.replace(tmp, STATE)
    return s


def log(msg):
    os.makedirs(BASE, exist_ok=True)
    with open(LOG, "a") as f:
        f.write("%s %s\n" % (time.strftime("%H:%M:%S"), msg))


def stage(text, progress=None):
    log(text)
    save_state(stage=text, **({"progress": progress} if progress is not None else {}))


def status():
    """Merkez icin durum; gunlugun son satirlariyla."""
    s = load_state()
    cur = current_version()
    s["current"] = cur
    s.setdefault("status", "idle")
    if s.get("rolling") and time.time() - s.get("started", 0) > 30 and not _busy():
        s = save_state(rolling=False, message="Geri alma süreci beklenmedik şekilde durdu. Ayrıntı için günlüğe bakın.")
        s["current"] = cur
    if s["status"] == "running" and time.time() - s.get("started", 0) > 30 and not _busy():
        # Kurulum sureci kilidi birakmis ama durum "suruyor": surec coktu.
        s = save_state(status="error", progress=0,
                       message="Güncelleme süreci beklenmedik şekilde durdu. Ayrıntı için günlüğe bakın.")
        s["current"] = cur
    if s["status"] in ("idle", "available"):
        s["status"] = "available" if s.get("installable") and is_newer(s.get("latest", ""), cur) else "idle"
    try:
        with open(LOG) as f:
            s["log"] = f.read().splitlines()[-80:]
    except OSError:
        s["log"] = []
    return s


def _notify(title, body):
    """Pi'nin masaustunde bildirim (Merkez'de ayrica acilir pencere var)."""
    try:
        from common import spawn_user
        spawn_user(["notify-send", "-i", "pios-center", "-a", "Pardus Maker", title, body])
    except Exception:  # bildirim istege bagli
        pass


# --- GitHub ---------------------------------------------------------------------

def _get(url, timeout=20):
    req = urllib.request.Request(url, headers={"User-Agent": "pardus-maker-ota",
                                               "Accept": "application/vnd.github+json"})
    return urllib.request.urlopen(req, timeout=timeout)


def check():
    """Son yayini sorgular. Yalnizca haber verir; kurulum kullanicidadir."""
    try:
        with _get(API) as r:
            rel = json.load(r)
    except (urllib.error.URLError, OSError, ValueError) as e:
        save_state(checked=time.time(), check_error="GitHub'a ulaşılamadı: %s" % e)
        return status()
    latest = str(rel.get("tag_name", "")).lstrip("v")
    assets = {a.get("name"): a for a in rel.get("assets", []) if isinstance(a, dict)}
    bundle = assets.get("pardus-maker-%s-ota.tar.xz" % latest)
    digest = assets.get("pardus-maker-%s-ota.tar.xz.sha256" % latest)
    prev = load_state()
    s = save_state(checked=time.time(), check_error="", latest=latest,
                   notes=str(rel.get("body") or "")[:4000], page=rel.get("html_url", ""),
                   installable=bool(bundle and digest),
                   bundle_url=bundle["browser_download_url"] if bundle else "",
                   sha_url=digest["browser_download_url"] if digest else "",
                   size=int(bundle.get("size") or 0) if bundle else 0)
    if s.get("installable") and is_newer(latest, current_version()) and prev.get("notified") != latest:
        save_state(notified=latest)
        _notify("Yeni Pardus Maker sürümü: %s" % latest, "Merkez → Güncellemeler'den kurabilirsiniz.")
    return status()


def _download(url, dest, size):
    with _get(url, timeout=60) as r, open(dest + ".part", "wb") as f:
        done, last = 0, 0
        while True:
            chunk = r.read(1 << 20)
            if not chunk:
                break
            f.write(chunk)
            done += len(chunk)
            if size and done - last >= 4 << 20:
                last = done
                save_state(progress=min(40, 5 + int(35 * done / size)))
    os.replace(dest + ".part", dest)


def _sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# --- paket acma (guvenli) ----------------------------------------------------------

def safe_extract(archive, dest):
    """Mutlak yol, '..', aygit dosyasi ve sembolik baglanti uzerinden yazma reddedilir."""
    links = set()
    with tarfile.open(archive, "r:*") as tar:
        members = tar.getmembers()
        for m in members:
            name = os.path.normpath(m.name)
            parts = name.split(os.sep)
            if name.startswith(("/", "..")) or ".." in parts or not (m.isfile() or m.isdir() or m.issym()):
                raise OtaError("Pakette geçersiz girdi: %s" % m.name)
            if any(os.sep.join(parts[:i]) in links for i in range(1, len(parts))):
                raise OtaError("Pakette bağlantı üzerinden yazma: %s" % m.name)
            if m.issym():
                links.add(name)
        tar.extractall(dest, members)


# --- kurulum -------------------------------------------------------------------------

def _walk(top):
    """top altindaki dosya ve baglantilar (goreli yol), dizinler haric."""
    for d, dirs, files in os.walk(top):
        for name in sorted(files) + sorted(n for n in dirs if os.path.islink(os.path.join(d, n))):
            yield os.path.relpath(os.path.join(d, name), top)


def _copy_into(src, target):
    """Dosyayi/baglantiyi hedefe atomik yerlestirir (yarim dosya kalmaz)."""
    tmp = target + ".pios-ota-new"
    if os.path.lexists(tmp):
        os.remove(tmp)
    if os.path.islink(src):
        os.symlink(os.readlink(src), tmp)
    else:
        shutil.copy2(src, tmp)
        st = os.stat(src)
        try:
            os.chown(tmp, st.st_uid, st.st_gid)
        except PermissionError:
            pass  # testler root olmadan calisir
    os.replace(tmp, target)


def _install_tree(src_root, dest_prefix, backup_tar, created):
    """src_root agacini dest_prefix'e kurar; uzerine yazilanlar yedege,
    yeni olusanlar created listesine yazilir (geri alma icin)."""
    files = [r for r in _walk(src_root) if r not in KEEP]
    for rel in files:   # once catisma denetimi: hicbir sey degismeden dur
        target = os.path.join(dest_prefix, rel)
        if os.path.isdir(target) and not os.path.islink(target):
            raise OtaError("Dosya yerine dizin var: /%s" % rel)
    for rel in files:
        target = os.path.join(dest_prefix, rel)
        parent = os.path.dirname(target)
        missing = []
        while parent and not os.path.lexists(parent):
            missing.append(parent)
            parent = os.path.dirname(parent)
        for d in reversed(missing):
            os.mkdir(d, 0o755)
            created.append(d)
        if os.path.lexists(target):
            backup_tar.add(target, arcname=os.path.relpath(target, ROOT), recursive=False)
        else:
            created.append(target)
        _copy_into(os.path.join(src_root, rel), target)
    return len(files)


def _has_systemd():
    return os.path.isdir("/run/systemd/system") and ROOT == "/"


def _run(cmd, timeout=1800, env=None):
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                       timeout=timeout, env=env)
    for line in p.stdout.splitlines()[-200:]:
        log("  " + line)
    return p.returncode


def _healthy():
    """Merkez yeniden basladi ve istek yanitliyor mu."""
    if os.environ.get("PIOS_OTA_SKIP_HEALTH") == "1":
        return True
    if _has_systemd():
        subprocess.run(["systemctl", "restart", "pios-center.service"], timeout=60)
    deadline = time.time() + HEALTH_SECS
    while time.time() < deadline:
        try:
            with urllib.request.urlopen("http://127.0.0.1:%s/" % CENTER_PORT, timeout=5) as r:
                if r.status == 200:
                    return True
        except (urllib.error.URLError, OSError):
            pass
        time.sleep(2)
    return False


def _tryboot_supported():
    return os.path.exists(TRYBOOT_FLAG) and os.path.isdir(BOOT)


def _stage_kernel(xdir, kver, created):
    """Yeni cekirdegi pios-ota/ altina koyar; tryboot.txt yalnizca bir
    sonraki 'tryboot' acilisinda kullanilir. Normal acilis eskiyle devam eder."""
    if os.path.lexists(TRYBOOT_DIR):
        shutil.rmtree(TRYBOOT_DIR)
    shutil.copytree(os.path.join(xdir, "kernel", "boot"), TRYBOOT_DIR, symlinks=True)
    shutil.copy2(os.path.join(BOOT, "cmdline.txt"), os.path.join(TRYBOOT_DIR, "cmdline.txt"))
    mods = os.path.join(xdir, "kernel", "modules", kver)
    target = _p("/lib/modules/" + kver)
    if os.path.isdir(mods) and not os.path.exists(target):
        shutil.copytree(mods, target, symlinks=True)
        created.append(target)
    with open(os.path.join(BOOT, "config.txt")) as f:
        config = f.read()
    with open(os.path.join(BOOT, "tryboot.txt"), "w") as f:
        f.write(config.rstrip("\n") + "\n\n# Pardus Maker OTA: yeni cekirdek bir kez denenir.\n[all]\nos_prefix=pios-ota/\n")


def _busy():
    """Bir OTA sureci kilidi tutuyor mu."""
    try:
        with open(os.path.join(BASE, ".lock"), "a") as fd:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    except BlockingIOError:
        return True
    except OSError:
        return False


def _locked(fn):
    """Islemi OTA kilidi altinda calistirir; kilit hata durumunda da birakilir."""
    def run():
        fd = _lock()
        try:
            return fn()
        finally:
            fd.close()
    run.__name__ = fn.__name__.lstrip("_")
    return run


def _lock():
    os.makedirs(BASE, exist_ok=True)
    fd = open(os.path.join(BASE, ".lock"), "w")
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        raise OtaError("Başka bir güncelleme işlemi sürüyor.")
    return fd


def _apply():
    s = load_state()
    target = s.get("latest", "")
    if not (s.get("installable") and is_newer(target, current_version())):
        raise OtaError("Kurulacak yeni sürüm yok.")
    if os.path.isdir(BACKUP):
        raise OtaError("Önceki güncelleme onay bekliyor; önce onaylayın veya geri alın.")
    old = current_version()
    open(LOG, "w").close()
    save_state(status="running", progress=1, message="", target=target, from_version=old, kernel="")
    shutil.rmtree(TMP, ignore_errors=True)
    os.makedirs(TMP)
    archive = os.path.join(TMP, "ota.tar.xz")
    try:
        need = max(s.get("size", 0), 1 << 20) * 4
        free = shutil.disk_usage(BASE).free
        if free < need:
            raise OtaError("Yetersiz disk alanı: %d MB gerekli, %d MB boş." % (need >> 20, free >> 20))
        stage("%s indiriliyor (%d MB)…" % (target, s.get("size", 0) >> 20), 5)
        _download(s["bundle_url"], archive, s.get("size", 0))
        stage("SHA-256 doğrulanıyor…", 42)
        with _get(s["sha_url"]) as r:
            expected = r.read(4096).decode().split()[0].lower()
        if _sha256(archive) != expected:
            raise OtaError("SHA-256 uyuşmuyor; paket bozuk veya değiştirilmiş. Hiçbir şey kurulmadı.")
        stage("Paket açılıyor…", 48)
        xdir = os.path.join(TMP, "x")
        safe_extract(archive, xdir)
        os.remove(archive)
        with open(os.path.join(xdir, "ota.json")) as f:
            meta = json.load(f)
        if meta.get("format") != FORMAT or meta.get("version") != target:
            raise OtaError("Paket bu sürüm için değil (%s)." % meta.get("version"))
    except Exception as e:
        shutil.rmtree(TMP, ignore_errors=True)
        save_state(status="error", progress=0, message=str(e))
        log("HATA: %s" % e)
        raise
    # Bu noktadan sonra sistem degisir: her hata yedekten geri donusle biter.
    created = []
    os.makedirs(BACKUP)
    try:
        stage("Yedek alınıyor ve dosyalar kuruluyor…", 55)
        with tarfile.open(os.path.join(BACKUP, "files.tar"), "w") as bt:
            n = _install_tree(os.path.join(xdir, "root"), ROOT, bt, created)
        _save_created(created)
        log("%d dosya kuruldu; yedek: %s" % (n, BACKUP))
        hook = os.path.join(xdir, "hooks", "post-install.sh")
        if os.path.isfile(hook):
            stage("Kurulum betiği çalışıyor (yeni paketler, servisler)…", 70)
            env = dict(os.environ, PIOS_OTA_DIR=xdir, OTA_FROM=old, OTA_TO=target, PIOS_OTA_ROOT=ROOT,
                       PIOS_USER=_device_user())
            if _run(["sh", hook], env=env) != 0:
                raise OtaError("Kurulum betiği başarısız oldu.")
        stage("Merkez yeniden başlatılıyor, sağlık kontrolü…", 85)
        if not _healthy():
            raise OtaError("Merkez yeni sürümle %d sn içinde yanıt vermedi." % HEALTH_SECS)
        kver = meta.get("kernel", "")
        kernel = ""
        if kver and kver != os.uname().release and os.path.isdir(os.path.join(xdir, "kernel", "boot")):
            if _tryboot_supported():
                stage("Yeni çekirdek (%s) deneme için hazırlanıyor…" % kver, 92)
                _stage_kernel(xdir, kver, created)
                _save_created(created)
                kernel = "staged"
            else:
                log("Bu önyükleyici tryboot desteklemiyor; çekirdek güncellenmedi.")
                kernel = "unsupported"
    except Exception as e:
        log("HATA: %s — yedek geri yükleniyor" % e)
        _restore()
        save_state(status="error", progress=0, message="%s Önceki sürüm geri yüklendi." % e, kernel="")
        raise
    shutil.rmtree(xdir, ignore_errors=True)
    if kernel == "staged":
        save_state(status="reboot", progress=95, kernel_version=kver, kernel=kernel, trial=False,
                   stage="Yeni çekirdeği denemek için yeniden başlatın.")
        _notify("Pardus Maker %s kuruldu" % target, "Yeni çekirdeği denemek için yeniden başlatın.")
    else:
        save_state(status="confirm", progress=100, kernel=kernel, stage="Kuruldu; onayınız bekleniyor.")
        _notify("Pardus Maker %s kuruldu" % target, "Merkez'de onaylayın; yedek ve geçici dosyalar silinir.")
    log("Kurulum tamamlandı: %s -> %s" % (old, target))


def _device_user():
    try:
        from common import USER
        return USER
    except Exception:
        return "maker"


def _save_created(created):
    with open(os.path.join(BACKUP, "created.json"), "w") as f:
        json.dump(created, f)


def _restore():
    """Yedekten geri doner: ezilen dosyalar geri yazilir, yeni olusanlar silinir."""
    try:
        with open(os.path.join(BACKUP, "created.json")) as f:
            created = json.load(f)
    except (OSError, ValueError):
        created = []
    running_modules = _p("/lib/modules/" + os.uname().release)
    for path in sorted(created, key=len, reverse=True):
        if path == running_modules:
            # Calisan cekirdegin modulleri silinmez; eski cekirdek yeniden
            # baslatinca acilir, bu klasor bir sonraki guncellemede temizlenir.
            log("Çalışan çekirdeğin modülleri korunuyor; yeniden başlatın.")
            continue
        try:
            if os.path.isdir(path) and not os.path.islink(path):
                shutil.rmtree(path) if path.startswith(_p("/lib/modules/")) else os.rmdir(path)
            elif os.path.lexists(path):
                os.remove(path)
        except OSError as e:
            log("silinemedi %s: %s" % (path, e))
    files = os.path.join(BACKUP, "files.tar")
    if os.path.isfile(files):
        with tarfile.open(files) as t:
            for m in t.getmembers():
                target = _p(m.name)
                if os.path.lexists(target) and not os.path.isdir(target):
                    os.remove(target)
                t.extract(m, ROOT)
    boot_backup = os.path.join(BACKUP, "boot")
    if os.path.isdir(boot_backup):
        for rel in _walk(boot_backup):
            _copy_into(os.path.join(boot_backup, rel), os.path.join(BOOT, rel))
    _drop_trial()
    if _has_systemd():
        subprocess.run(["systemctl", "daemon-reload"], timeout=60)
        subprocess.run(["systemctl", "restart", "pios-center.service"], timeout=60)
    shutil.rmtree(BACKUP, ignore_errors=True)
    shutil.rmtree(TMP, ignore_errors=True)
    log("Yedek geri yüklendi.")


def _drop_trial():
    for path in (os.path.join(BOOT, "tryboot.txt"),):
        if os.path.exists(path):
            os.remove(path)
    shutil.rmtree(TRYBOOT_DIR, ignore_errors=True)


def reboot_trial():
    s = load_state()
    if s.get("status") != "reboot":
        raise OtaError("Denenecek çekirdek yok.")
    save_state(trial=True)
    log("Yeni çekirdekle bir kez açılıyor (tryboot).")
    if _has_systemd():
        subprocess.Popen(["sh", "-c", "sleep 2; reboot '0 tryboot'"])


def boot_check():
    """Acilista: tryboot sonucunu degerlendir, yarim kalan kurulumu geri al."""
    s = load_state()
    st = s.get("status")
    if st == "running" and os.path.isdir(BACKUP):
        log("Kurulum yarıda kalmış (elektrik kesintisi?); yedekten geri dönülüyor.")
        _restore()
        save_state(status="error", progress=0, message="Kurulum yarıda kaldı; önceki sürüm geri yüklendi.")
        return
    if st != "reboot" or not s.get("trial"):
        return
    kver = s.get("kernel_version", "")
    if os.uname().release == kver:
        log("Yeni çekirdek açıldı (%s); kalıcı hale getiriliyor." % kver)
        backup = os.path.join(BACKUP, "boot")
        os.makedirs(backup, exist_ok=True)
        for rel in _walk(TRYBOOT_DIR):
            if not BOOT_FILES.match(rel):
                continue
            dst = os.path.join(BOOT, rel)
            if os.path.exists(dst):
                os.makedirs(os.path.dirname(os.path.join(backup, rel)), exist_ok=True)
                shutil.copy2(dst, os.path.join(backup, rel))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            _copy_into(os.path.join(TRYBOOT_DIR, rel), dst)
        _drop_trial()
        save_state(status="confirm", progress=100, kernel="promoted", trial=False,
                   stage="Yeni çekirdek çalışıyor; onayınız bekleniyor.")
    else:
        log("Yeni çekirdek açılamadı; eski çekirdekle devam ediliyor.")
        _drop_trial()
        mods = _p("/lib/modules/" + kver)
        if kver and kver != os.uname().release:
            shutil.rmtree(mods, ignore_errors=True)
        save_state(status="confirm", progress=100, kernel="failed", trial=False,
                   stage="Yeni çekirdek açılamadı; eski çekirdek kullanılıyor. Diğer güncellemeler kurulu.")


def _confirm():
    s = load_state()
    if s.get("status") != "confirm":
        raise OtaError("Onay bekleyen güncelleme yok.")
    shutil.rmtree(BACKUP, ignore_errors=True)
    shutil.rmtree(TMP, ignore_errors=True)
    log("Onaylandı; yedek ve geçici dosyalar silindi.")
    save_state(status="idle", progress=0, stage="", message="", kernel="", trial=False,
               last_update={"from": s.get("from_version"), "to": current_version(), "at": time.time()})


def _rollback():
    s = load_state()
    if s.get("status") not in ("confirm", "reboot") or not os.path.isdir(BACKUP):
        save_state(rolling=False)
        raise OtaError("Geri alınacak güncelleme yok.")
    log("Kullanıcı isteğiyle geri alınıyor: %s -> %s" % (current_version(), s.get("from_version")))
    promoted = s.get("kernel") == "promoted"
    _restore()
    save_state(status="idle", progress=0, stage="", kernel="", trial=False, rolling=False,
               message="Güncelleme geri alındı; %s sürümüne dönüldü.%s" % (
                   current_version(), " Eski çekirdek için Pi'yi yeniden başlatın." if promoted else ""))


apply = _locked(_apply)
confirm = _locked(_confirm)
rollback = _locked(_rollback)


def spawn(command):
    """Merkez'den bagimsiz calistir: kurulum Merkez'i yeniden baslatir."""
    me = os.path.abspath(__file__)
    if _has_systemd():
        cmd = ["systemd-run", "--unit=pios-ota-%s-%d" % (command, time.time()), "--collect",
               "--property=KillMode=process", "/usr/bin/python3", "-I", me, command]
    else:
        cmd = [sys.executable, "-I", me, command]
    subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)


if __name__ == "__main__":
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    actions = {"check": check, "apply": apply, "confirm": confirm, "rollback": rollback,
               "boot-check": boot_check, "reboot-trial": reboot_trial}
    if len(sys.argv) != 2 or sys.argv[1] not in actions:
        sys.exit("kullanim: ota.py " + "|".join(actions))
    try:
        result = actions[sys.argv[1]]()
    except OtaError as e:
        log("HATA: %s" % e)
        sys.exit(str(e))
    if isinstance(result, dict):
        print(json.dumps({k: v for k, v in result.items() if k != "log"}, ensure_ascii=False, indent=1))
