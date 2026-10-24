# Kamera: Raspberry Pi kamera modulleri (CSI, libcamera) ve USB web
# kameralari (UVC, ffmpeg). Canli goruntu tarayiciya MJPEG akisi
# (multipart/x-mixed-replace) olarak verilir; tek kamera sureci kareleri
# tum izleyicilere dagitir ve izleyici kalmayinca kapanir.
#
# Fotograflar kullanicinin Resimler/Kamera klasorune kaydedilir. Pi kamerasi
# icin fotograf, akis kisa sure durdurularak tam cozunurlukte cekilir.
import json
import os
import re
import secrets
import select
import shutil
import subprocess
import threading
import time

from common import read_first, run, user_info

SETTINGS_FILE = "/etc/pios/camera.json"
SIZES = ["640x480", "1280x720", "1920x1080"]
DEFAULTS = {"size": "1280x720", "fps": 15, "rotate": False}
IDLE_STOP_SECS = 8
_photo_lock = threading.Lock()
PHOTO_RE = re.compile(r"^[\w.-]+\.jpg$")

# config.txt'de elle secilebilen sensorler (otomatik algilama disinda).
MODELS = {
    "ov5647": "Kamera Modülü v1 (OV5647)",
    "imx219": "Kamera Modülü v2 (IMX219)",
    "imx477": "HQ Kamera (IMX477)",
    "imx708": "Kamera Modülü 3 (IMX708)",
    "imx296": "Global Shutter Kamera (IMX296)",
    "imx500": "AI Kamera (IMX500)",
}


def _tool(name):
    # rpicam-apps yeni adlari; Pardus 21 (bullseye) deposunda libcamera-*.
    for prefix in ("rpicam-", "libcamera-"):
        path = shutil.which(prefix + name)
        if path:
            return path
    return None


def load_settings():
    try:
        with open(SETTINGS_FILE) as f:
            s = dict(DEFAULTS, **json.load(f))
    except (OSError, ValueError):
        s = dict(DEFAULTS)
    if s["size"] not in SIZES:
        s["size"] = DEFAULTS["size"]
    return s


def save_settings(size, rotate):
    if size not in SIZES:
        return False, "gecersiz cozunurluk"
    s = dict(load_settings(), size=size, rotate=bool(rotate))
    os.makedirs(os.path.dirname(SETTINGS_FILE), exist_ok=True)
    with open(SETTINGS_FILE, "w") as f:
        json.dump(s, f)
    manager.restart()
    return True, "Kaydedildi"


def photo_dir():
    _, _, home = user_info()
    pics = os.path.join(home, "Resimler")
    for line in read_first(os.path.join(home, ".config/user-dirs.dirs")).splitlines():
        if line.startswith("XDG_PICTURES_DIR="):
            pics = line.split("=", 1)[1].strip('"').replace("$HOME", home)
    return os.path.join(pics, "Kamera")


def _ensure_photo_dir():
    uid, gid, _ = user_info()
    path = photo_dir()
    parent = os.path.dirname(path)
    for d in (parent, path):
        if not os.path.isdir(d):
            os.makedirs(d, exist_ok=True)
            os.chown(d, uid, gid)
    return path


# --- Kamera bulma --------------------------------------------------------------

_csi_cache = {"t": 0, "list": []}


def _csi_cameras():
    hello = _tool("hello")
    if not hello:
        return []
    # Akis acikken yeniden sorgulama yapma (kamera mesgul olabilir).
    if manager.proc and _csi_cache["list"] or time.time() - _csi_cache["t"] < 10:
        return _csi_cache["list"]
    _, out, err = run([hello, "--list-cameras"], timeout=15)
    cams = []
    for line in (out + "\n" + err).splitlines():
        m = re.match(r"^\s*(\d+)\s*:\s*(\S+)\s*\[(\d+)x(\d+)", line)
        if m:
            sensor = m.group(2)
            cams.append({"id": "csi:" + m.group(1), "kind": "csi", "sensor": sensor,
                         "name": MODELS.get(sensor, "Pi kamera (%s)" % sensor),
                         "max": "%sx%s" % (m.group(3), m.group(4))})
    _csi_cache.update(t=time.time(), list=cams)
    return cams


def _uvc_cameras():
    cams = []
    base = "/sys/class/video4linux"
    try:
        nodes = sorted(os.listdir(base), key=lambda n: int(re.sub(r"\D", "", n) or 0))
    except OSError:
        return cams
    for node in nodes:
        drv = os.path.basename(os.path.realpath(os.path.join(base, node, "device/driver")))
        # UVC her kamera icin iki dugum acar; goruntu dugumu index 0'dir.
        if drv != "uvcvideo" or read_first(os.path.join(base, node, "index"), "0") != "0":
            continue
        cams.append({"id": "uvc:" + node, "kind": "uvc", "dev": "/dev/" + node,
                     "name": read_first(os.path.join(base, node, "name"), "USB kamera")})
    return cams


def cameras():
    return _csi_cameras() + _uvc_cameras()


def find(cam_id):
    return next((c for c in cameras() if c["id"] == cam_id), None)


# --- Akis yoneticisi -----------------------------------------------------------

class Manager:
    def __init__(self):
        self.lock = threading.RLock()
        self.cond = threading.Condition()
        self.proc = None
        self.cam = None
        self.frame = None
        self.seq = 0
        self.viewers = 0
        self.last_seen = 0
        self.error = ""
        self.busy = False     # fotograf cekilirken akis bilerek duraklatildi
        self.encode = False   # UVC: kamera MJPEG vermiyorsa ffmpeg ile kodla

    def _cmd(self, cam):
        s = load_settings()
        w, h = s["size"].split("x")
        if cam["kind"] == "csi":
            cmd = [_tool("vid"), "-t", "0", "-n", "--camera", cam["id"].split(":")[1],
                   "--codec", "mjpeg", "--quality", "80", "--width", w, "--height", h,
                   "--framerate", str(s["fps"]), "--flush", "-o", "-"]
            return cmd + (["--rotation", "180"] if s["rotate"] else [])
        cmd = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "v4l2"]
        encode = self.encode or s["rotate"]
        if not self.encode:
            cmd += ["-input_format", "mjpeg"]
        cmd += ["-video_size", s["size"], "-framerate", str(s["fps"]), "-i", cam["dev"]]
        if encode:
            cmd += (["-vf", "hflip,vflip"] if s["rotate"] else []) + ["-c:v", "mjpeg", "-q:v", "5"]
        else:
            cmd += ["-c:v", "copy"]
        return cmd + ["-f", "mjpeg", "-"]

    def start(self, cam):
        with self.lock:
            if self.proc and self.cam and self.cam["id"] == cam["id"]:
                return True
            self.stop()
            cmd = self._cmd(cam)
            if not cmd[0]:
                self.error = "libcamera uygulamalari kurulu degil"
                return False
            try:
                proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        stdin=subprocess.DEVNULL)
            except OSError as e:
                self.error = str(e)
                return False
            self.proc, self.cam, self.error = proc, cam, ""
            self.last_seen = time.time()
            # libcamera her karede stderr'e satir yazar; okunmazsa boru dolar
            # ve surec takilir. Son satirlar hata mesaji icin saklanir.
            proc.err_tail = []
            threading.Thread(target=self._drain, args=(proc,), daemon=True).start()
            threading.Thread(target=self._reader, args=(proc, cam), daemon=True).start()
            return True

    def stop(self):
        with self.lock:
            proc, self.proc = self.proc, None
            if proc:
                proc.terminate()
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            with self.cond:
                self.frame = None
                self.cond.notify_all()

    def restart(self):
        with self.lock:
            cam = self.cam if self.proc else None
            self.stop()
            if cam:
                self.start(cam)

    @staticmethod
    def _drain(proc):
        try:
            for line in proc.stderr:
                line = line.decode(errors="replace").strip()
                if line and not line.startswith("#"):
                    proc.err_tail = (proc.err_tail + [line[-2000:]])[-20:]
        finally:
            proc.stderr.close()

    def _reader(self, proc, cam):
        try:
            self._read_stream(proc, cam)
        finally:
            proc.stdout.close()

    def _read_stream(self, proc, cam):
        # bytearray: parca ekleme/bastan silme yerinde yapilir (bytes ile her
        # parcada tum tampon kopyalanirdi).
        buf, started, got = bytearray(), time.time(), False
        fd = proc.stdout.fileno()
        last_data = time.monotonic()
        while True:
            if not select.select([fd], [], [], 2)[0]:
                if time.monotonic() - last_data > 20 or (self.viewers == 0 and time.time() - self.last_seen > IDLE_STOP_SECS):
                    with self.lock:
                        if self.proc is proc:
                            self.error = "Kamera 20 saniyedir veri göndermiyor veya izleyici kalmadı. Kamera kablosunu ve başka bir uygulamanın kamerayı kullanıp kullanmadığını kontrol edin."
                            self.stop()
                    return
                continue
            try:
                chunk = os.read(fd, 262144)
            except OSError:
                chunk = b""
            if not chunk:
                break
            last_data = time.monotonic()
            buf += chunk
            frame = None
            while True:
                soi = buf.find(b"\xff\xd8")
                if soi < 0:
                    del buf[:-1]   # isaretin ilk bayti parcanin sonunda kalmis olabilir
                    break
                eoi = buf.find(b"\xff\xd9", soi + 2)
                if eoi < 0:
                    del buf[:soi]
                    break
                frame = bytes(buf[soi:eoi + 2])
                del buf[:eoi + 2]
            if frame is not None:
                # Parcada birden cok kare varsa yalnizca en yenisi yayimlanir.
                with self.cond:
                    self.frame = frame
                    self.seq += 1
                    self.cond.notify_all()
                got = True
            if len(buf) > 16 << 20:
                buf.clear()
            if self.viewers == 0 and time.time() - self.last_seen > IDLE_STOP_SECS:
                with self.lock:
                    if self.proc is proc:
                        self.stop()
                return
        rc = proc.wait()
        time.sleep(0.2)
        err = [l for l in proc.err_tail if "ERROR" in l or "rror" in l] or proc.err_tail
        with self.lock:
            if self.proc is not proc:
                return  # bilerek durduruldu
            self.proc = None
            if cam["kind"] == "uvc" and not got and not self.encode and time.time() - started < 10:
                # Kamera MJPEG desteklemiyor: ham goruntuyu kodlayarak dene.
                self.encode = True
                self.start(cam)
                return
            self.error = "Kamera süreci durdu (kod %s).\n%s" % (rc, "\n".join(err)[-8000:])
        with self.cond:
            self.frame = None
            self.cond.notify_all()

    def frames(self, cam):
        """Izleyici icin JPEG kareleri ureten jenerator."""
        if not self.start(cam):
            return
        with self.lock:
            self.viewers += 1
        last = 0
        try:
            while True:
                with self.cond:
                    self.cond.wait_for(lambda: self.seq != last or (self.proc is None and not self.busy),
                                       timeout=10)
                    frame, seq = self.frame, self.seq
                self.last_seen = time.time()
                if self.proc is None and not self.busy:
                    return
                if frame is None or seq == last:
                    continue
                last = seq
                yield frame
        finally:
            with self.lock:
                self.viewers -= 1
                self.last_seen = time.time()

    def wait_frame(self, cam, timeout=8):
        if not self.start(cam):
            return None
        self.last_seen = time.time()
        with self.cond:
            self.cond.wait_for(lambda: self.frame is not None or self.proc is None, timeout=timeout)
            return self.frame


manager = Manager()


def status():
    s = load_settings()
    cam = manager.cam if manager.proc else None
    return {"cameras": cameras(), "settings": s, "sizes": SIZES,
            "active": cam["id"] if cam else None, "viewers": manager.viewers,
            "error": manager.error, "dir": photo_dir(),
            "libcamera": bool(_tool("hello")), "ffmpeg": bool(shutil.which("ffmpeg")),
            "model": get_model(), "models": MODELS}


# --- Fotograf -------------------------------------------------------------------

def take_photo(cam_id, requested_name=""):
    if not _photo_lock.acquire(blocking=False):
        return False, "Başka bir fotoğraf çekimi sürüyor. Tamamlanmasını bekleyin.", None
    try:
        stem = requested_name.removesuffix(".jpg")
        if stem and not re.fullmatch(r"[\w -]{1,60}", stem):
            return False, "Fotoğraf adı en fazla 60 harf, rakam, boşluk, alt çizgi veya tire içerebilir.", None
        return _take_photo(cam_id, stem)
    finally:
        _photo_lock.release()


def _take_photo(cam_id, stem):
    cam = find(cam_id)
    if not cam:
        return False, "Kamera bulunamadi", None
    uid, gid, _ = user_info()
    name = "%s-%s-%s.jpg" % (stem or "foto", time.strftime("%Y%m%d-%H%M%S"), secrets.token_hex(3))
    path = os.path.join(_ensure_photo_dir(), name)
    if cam["kind"] == "csi":
        still = _tool("still")
        if not still:
            return False, "libcamera-still kurulu degil", None
        with manager.lock:
            was = manager.proc is not None
            # Izleyicilerin akisi kopmasin: kamera tek surece acilabildigi icin
            # canli goruntu durdurulur, tam cozunurlukte cekilir, yeniden baslar.
            manager.busy = True
            try:
                manager.stop()
                cmd = [still, "-n", "-t", "1500", "--camera", cam["id"].split(":")[1], "-o", path]
                if load_settings()["rotate"]:
                    cmd += ["--rotation", "180"]
                rc, _, err = run(cmd, timeout=40)
                if was:
                    manager.start(cam)
            finally:
                manager.busy = False
                with manager.cond:
                    manager.cond.notify_all()
        if rc != 0 or not os.path.isfile(path):
            if os.path.isfile(path):
                os.remove(path)
            return False, "Fotoğraf çekilemedi (kod %s).\n%s" % (rc, err[-8000:] or "Kamera dosya oluşturmadı."), None
    else:
        frame = manager.wait_frame(cam)
        if not frame:
            return False, manager.error or "kameradan goruntu alinamadi", None
        with open(path, "wb") as f:
            f.write(frame)
    os.chown(path, uid, gid)
    return True, "Kaydedildi: " + name, name


def photos(limit=60):
    d = photo_dir()
    try:
        names = [n for n in os.listdir(d) if PHOTO_RE.match(n)]
    except OSError:
        return []
    items = []
    for n in names:
        try:
            st = os.stat(os.path.join(d, n))
        except OSError:
            continue
        items.append({"name": n, "size": st.st_size, "time": int(st.st_mtime)})
    items.sort(key=lambda x: -x["time"])
    return items[:limit]


def photo_path(name):
    if not PHOTO_RE.match(name or ""):
        return None
    path = os.path.join(photo_dir(), name)
    return path if os.path.isfile(path) else None


def thumbnail(path):
    """libcamera-still JPEG'lerine gomulu EXIF kucuk resmini dondurur
    (galeri tam boy 8-12 MP dosyalari indirmesin); yoksa None."""
    with open(path, "rb") as f:
        head = f.read(131072)
    soi = head.find(b"\xff\xd8", 2)
    if soi > 0:
        eoi = head.find(b"\xff\xd9", soi)
        if eoi > 0:
            return head[soi:eoi + 2]
    return None


def delete_photo(name):
    path = photo_path(name)
    if not path:
        return False, "bulunamadi"
    os.remove(path)
    return True, "Silindi"


# --- config.txt: kamera modeli --------------------------------------------------

def get_model():
    import system
    model, auto = "", True
    for line in system._read_config():
        line = line.strip()
        if line.startswith("camera_auto_detect="):
            auto = line.split("=", 1)[1].strip() == "1"
        m = re.match(r"dtoverlay=(\w+)", line)
        if m and m.group(1) in MODELS:
            model = m.group(1)
    return model if not auto and model else ("auto" if auto else "off")


def set_model(model):
    import system
    if model not in MODELS and model not in ("auto", "off"):
        return False, "gecersiz kamera modeli"
    out = []
    for line in system._read_config():
        s = line.strip()
        if s.startswith("camera_auto_detect="):
            continue
        m = re.match(r"dtoverlay=(\w+)", s)
        if m and m.group(1) in MODELS:
            continue
        out.append(line)
    out.append("camera_auto_detect=%d" % (model == "auto"))
    if model in MODELS:
        out.append("dtoverlay=" + model)
    system._write_config(out)
    return True, "Yeniden baslatinca gecerli olur"
