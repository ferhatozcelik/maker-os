"""{{name}}: Raspberry Pi'de gerçek zamanlı nesne tespiti (NanoDet-m + ncnn).

  uv run main.py                 kamera (Pi kamera modülü / USB), arayüz http://<pi>:8000 (ui.html)
  uv run main.py --source test   kamerasız deneme (sentetik görüntü, çıkarım gerçek)
  uv run main.py --image foto.jpg

Neden hızlı (Pi 4, 4 GB):
  * Görüntü: kamera → VideoCore donanım MJPEG kodlayıcı (rpicam-vid) → tarayıcı. Tespit karesi yarım boyutta çözülür.
  * Tespit: küçük "lores" akış (320x180) → ncnn, 1-3. çekirdeklere sabit; 0. çekirdek arayüze kalır.
    16:9 girdi 32'nin katına (320x192) doldurulur: kare 320x320'ye göre ~%40 daha az hesap.
  * Kutular tarayıcıda çizilir (Pi'de ayrıca JPEG kodlama yok). En yeni kare işlenir, kuyruk birikmez.
  * Son işleme yalnız eşik üstü adaylarda yapılır (softmax/kod çözme 1000 yerine ~20 aday).

Model: NanoDet-m (COCO, 80 sınıf), RangiLyu/nanodet (Apache-2.0); ncnn dönüşümü nihui/ncnn-assets.
"""
import argparse
import json
import os
import threading
import time
from http import server

import numpy as np
import ncnn

MODEL_DIRS = [os.environ.get("MAKEROS_MODELS", ""), "/usr/share/makeros/models", os.path.expanduser("~/.ncnn/models")]
CLASSES = ("insan bisiklet araba motosiklet uçak otobüs tren kamyon tekne trafik-ışığı yangın-musluğu dur-levhası parkmetre bank "
           "kuş kedi köpek at koyun inek fil ayı zebra zürafa sırt-çantası şemsiye el-çantası kravat bavul frizbi kayak snowboard "
           "top uçurtma beyzbol-sopası beyzbol-eldiveni kaykay sörf-tahtası tenis-raketi şişe şarap-kadehi fincan çatal bıçak kaşık "
           "kase muz elma sandviç portakal brokoli havuç sosisli pizza çörek pasta sandalye kanepe saksı yatak yemek-masası tuvalet "
           "televizyon dizüstü fare kumanda klavye telefon mikrodalga fırın tost-makinesi lavabo buzdolabı kitap saat vazo makas "
           "oyuncak-ayı saç-kurutma diş-fırçası").split()


MODEL_URL = "https://raw.githubusercontent.com/nihui/ncnn-assets/master/models/"


def model_file(name):
    """Modeli bilinen dizinlerde arar; yoksa ilk çalıştırmada ~/.ncnn/models altına indirir (bir kez, ~2 MB)."""
    for d in MODEL_DIRS:
        if d and os.path.isfile(os.path.join(d, name)):
            return os.path.join(d, name)
    import urllib.request
    dst_dir = os.path.expanduser("~/.ncnn/models")
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, name)
    print(f"Model indiriliyor: {name} → {dst_dir}", flush=True)
    try:
        with urllib.request.urlopen(MODEL_URL + name, timeout=60) as r, open(dst + ".part", "wb") as f:
            while chunk := r.read(1 << 16):
                f.write(chunk)
        os.replace(dst + ".part", dst)
    except OSError as e:
        raise SystemExit(f"Model indirilemedi ({e}). İnternet bağlantısını kontrol edin ya da {name} dosyasını {dst_dir} altına koyun.")
    return dst


class NanoDet:
    """NanoDet-m çıkarımı. Kod çözme ncnn model_zoo (BSD-3) ile aynı, vektörize ve eşik öncelikli."""
    MEAN, NORM = [103.53, 116.28, 123.675], [0.017429, 0.017507, 0.017125]
    STRIDES, REG_MAX = (8, 16, 32), 7
    SCORE_OUT, BOX_OUT = ("792", "814", "836"), ("795", "817", "839")

    def __init__(self, size=320, threads=3, prob=0.4, nms=0.3):
        self.size, self.prob, self.nms_thr = size, prob, nms
        self.net = ncnn.Net()
        self.net.opt.use_vulkan_compute = False
        self.net.opt.num_threads = threads
        self.net.opt.lightmode = True
        self.net.load_param(model_file("nanodet_m.param"))
        self.net.load_model(model_file("nanodet_m.bin"))
        self._centers = {}
        self._reg = np.arange(self.REG_MAX + 1, dtype=np.float32)

    def centers(self, pw, ph, stride):
        k = (pw, ph, stride)
        if k not in self._centers:
            fw, fh = pw // stride, ph // stride
            ww, hh = np.meshgrid(np.arange(fw), np.arange(fh))
            cx, cy = (ww.ravel() + 0.5) * stride, (hh.ravel() + 0.5) * stride
            self._centers[k] = np.stack((cx, cy, cx, cy), axis=1).astype(np.float32)
        return self._centers[k]

    def __call__(self, bgr):
        ih, iw = bgr.shape[:2]
        scale = self.size / max(iw, ih)
        w, h = int(iw * scale), int(ih * scale)
        m = ncnn.Mat.from_pixels_resize(bgr, ncnn.Mat.PixelType.PIXEL_BGR, iw, ih, w, h)
        wpad, hpad = (w + 31) // 32 * 32 - w, (h + 31) // 32 * 32 - h
        m = ncnn.copy_make_border(m, hpad // 2, hpad - hpad // 2, wpad // 2, wpad - wpad // 2, ncnn.BorderType.BORDER_CONSTANT, 0)
        m.substract_mean_normalize(self.MEAN, self.NORM)
        pw, ph = m.w, m.h
        ex = self.net.create_extractor()
        ex.input("input.1", m)
        boxes, scores, labels = [], [], []
        for stride, so, bo in zip(self.STRIDES, self.SCORE_OUT, self.BOX_OUT):
            sc = np.array(ex.extract(so)[1]).reshape(-1, 80)
            best = sc.max(axis=1)
            idx = np.nonzero(best > self.prob)[0]          # eşik önce: yalnız adaylar çözülür
            if idx.size == 0:
                continue
            raw = np.array(ex.extract(bo)[1]).reshape(-1, 32)[idx].reshape(-1, self.REG_MAX + 1)
            e = np.exp(raw - raw.max(axis=1, keepdims=True))
            dist = ((e / e.sum(axis=1, keepdims=True)) @ self._reg).reshape(-1, 4) * stride
            b = self.centers(pw, ph, stride)[idx] + dist * np.array([-1, -1, 1, 1], np.float32)
            boxes.append(b)
            scores.append(best[idx])
            labels.append(sc[idx].argmax(axis=1))
        if not boxes:
            return []
        b, s, l = np.concatenate(boxes), np.concatenate(scores), np.concatenate(labels)
        keep = nms(b + l[:, None] * 4096.0, s, self.nms_thr)  # sınıf başına NMS (kaydırma hilesi)
        out = []
        for i in keep:
            x0, y0, x1, y1 = b[i]
            x0, y0 = max(0.0, (x0 - wpad / 2) / scale), max(0.0, (y0 - hpad / 2) / scale)
            x1, y1 = min(iw, (x1 - wpad / 2) / scale), min(ih, (y1 - hpad / 2) / scale)
            out.append({"c": CLASSES[int(l[i])], "p": round(float(s[i]), 2),
                        "b": [round(float(x0) / iw, 4), round(float(y0) / ih, 4), round(float(x1 - x0) / iw, 4), round(float(y1 - y0) / ih, 4)]})
        return out


def nms(boxes, scores, thr):
    order, keep = scores.argsort()[::-1], []
    x0, y0, x1, y1 = boxes.T
    area = (x1 - x0) * (y1 - y0)
    while order.size:
        i = order[0]
        keep.append(i)
        xx0, yy0 = np.maximum(x0[i], x0[order[1:]]), np.maximum(y0[i], y0[order[1:]])
        xx1, yy1 = np.minimum(x1[i], x1[order[1:]]), np.minimum(y1[i], y1[order[1:]])
        inter = np.clip(xx1 - xx0, 0, None) * np.clip(yy1 - yy0, 0, None)
        order = order[1:][inter / (area[i] + area[order[1:]] - inter + 1e-6) <= thr]
    return keep


def pin(cpus):
    try:
        os.sched_setaffinity(0, cpus)  # çağıran iş parçacığına uygulanır; ncnn iş parçacıkları devralır
    except (AttributeError, OSError):
        pass


# --------------------------------------------------------------- kaynaklar
class TestSource:
    """Kamerasız deneme: hareketli sentetik sahne (çıkarım gerçek, sonuç anlamsız olabilir)."""
    def __init__(self, w=320, h=180, fps=30):
        self.w, self.h, self.dt, self.t = w, h, 1 / fps, 0.0
        self.next = time.perf_counter()

    def lores(self):
        self.next += self.dt  # sabit kare hızı (işlem süresi düşülür)
        time.sleep(max(0.0, self.next - time.perf_counter()))
        self.t += self.dt
        img = np.full((self.h, self.w, 3), 40, np.uint8)
        x = int((np.sin(self.t) * 0.4 + 0.5) * (self.w - 60))
        img[50:130, x:x + 60] = (40, 160, 230)
        return img


class CameraSource:
    """rpicam-vid donanım MJPEG akışı (MakerOS'ta hazır): kareler önizlemeye olduğu gibi gider, tespit için
    küçültülüp çözülür. Kamera uygulaması açıksa önce onu kapatın (kamerayı aynı anda tek program kullanır)."""
    def __init__(self, frames, lores=(320, 180), size=(640, 360), fps=20):
        import cv2
        import subprocess
        self.cv2, self.frames, self.lw, self.lh = cv2, frames, lores[0], lores[1]
        self.p = subprocess.Popen(["rpicam-vid", "-n", "-t", "0", "--codec", "mjpeg", "--width", str(size[0]), "--height", str(size[1]),
                                   "--framerate", str(fps), "-o", "-"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.buf = b""
        self.jpeg = None
        self.cond = threading.Condition()
        threading.Thread(target=self._read, daemon=True).start()
        with self.cond:
            if not self.cond.wait_for(lambda: self.jpeg is not None, timeout=8):
                self.p.kill()
                raise RuntimeError("kamera görüntü vermedi (takılı mı, başka program kullanıyor mu?)")

    def _read(self):
        pin({0})  # okuyucu 0. çekirdekte: çıkarım çekirdeklerindeki OpenMP döngüsü onu aç bırakmasın
        while chunk := self.p.stdout.read1(65536):
            self.buf += chunk
            while (a := self.buf.find(b"\xff\xd8")) >= 0 and (b := self.buf.find(b"\xff\xd9", a + 2)) >= 0:
                f, self.buf = self.buf[a:b + 2], self.buf[b + 2:]
                self.frames.write(f)
                with self.cond:
                    self.jpeg = f
                    self.cond.notify_all()

    def lores(self):
        with self.cond:  # yeni kareyi bekle: kuyruk birikmez, en yeni kare işlenir
            self.cond.wait(1)
            f = self.jpeg
        img = self.cv2.imdecode(np.frombuffer(f, np.uint8), self.cv2.IMREAD_REDUCED_COLOR_2)  # 640x360 → 320x180 çözerken küçült
        return img


# --------------------------------------------------------------- web önizleme
class Frames:
    def __init__(self):
        self.frame, self.cond = None, threading.Condition()

    def write(self, buf):
        with self.cond:
            self.frame = bytes(buf)
            self.cond.notify_all()
        return len(buf)

    def flush(self):
        pass


UI = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ui.html")  # arayüz: her istekte okunur, düzenleyip sayfayı yenileyin


def serve(port, frames, state, det):
    class H(server.BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def do_POST(self):
            # Arayüzden ayar: {"prob": 0.1-0.9} güven eşiği.
            if self.path != "/config":
                return self.send_error(404)
            try:
                d = json.loads(self.rfile.read(min(int(self.headers.get("Content-Length") or 0), 4096)) or b"{}")
                if "prob" in d:
                    det.prob = min(0.9, max(0.1, float(d["prob"])))
                    state["last"]["info"]["prob"] = det.prob
            except (ValueError, TypeError):
                return self.send_error(400)
            self.send_response(204); self.end_headers()

        def do_GET(self):
            path = self.path.split("?")[0]
            if path == "/":
                try:
                    with open(UI, "rb") as f:
                        body = f.read()
                except OSError:
                    body = b"<p>ui.html bulunamadi</p>"
                self.send_response(200); self.send_header("Content-Type", "text/html; charset=utf-8"); self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
            elif path == "/stream":
                self.send_response(200); self.send_header("Content-Type", "multipart/x-mixed-replace; boundary=F"); self.end_headers()
                try:
                    while True:
                        with frames.cond:
                            frames.cond.wait(2)
                            f = frames.frame
                        if f:
                            self.wfile.write(b"--F\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n%s\r\n" % (len(f), f))
                except (BrokenPipeError, ConnectionResetError):
                    pass
            elif path == "/events":
                self.send_response(200); self.send_header("Content-Type", "text/event-stream"); self.send_header("Cache-Control", "no-cache"); self.end_headers()
                try:
                    while True:
                        with state["cond"]:
                            state["cond"].wait(2)
                            msg = json.dumps(state["last"])
                        self.wfile.write(f"data: {msg}\n\n".encode())
                except (BrokenPipeError, ConnectionResetError):
                    pass
            else:
                self.send_error(404)

    httpd = server.ThreadingHTTPServer(("", port), H)
    httpd.daemon_threads = True  # açık akışlar çıkışı bekletmesin
    threading.Thread(target=httpd.serve_forever, daemon=True).start()


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--source", default="camera", choices=["camera", "test"])
    ap.add_argument("--image")
    ap.add_argument("--size", type=int, default=320, help="girdi kenarı (256 daha hızlı, 320 daha doğru)")
    ap.add_argument("--threads", type=int, default=3)
    ap.add_argument("--cpus", default="1,2,3", help="çıkarım çekirdekleri (0 arayüze kalır)")
    ap.add_argument("--port", type=int, default=8000)
    a = ap.parse_args()
    cpus = {int(c) for c in a.cpus.split(",") if c.strip().isdigit() and int(c) < os.cpu_count()} or None
    if cpus:
        pin(cpus)
        a.threads = min(a.threads, len(cpus))
    det = NanoDet(size=a.size, threads=a.threads)

    if a.image:
        import cv2
        img = cv2.imread(a.image)
        if img is None:
            raise SystemExit("Görüntü okunamadı: " + a.image)
        for o in det(img):
            print(f"{o['c']:>16}  %{o['p'] * 100:4.0f}  kutu={o['b']}")
        return

    frames = Frames()
    state = {"cond": threading.Condition(), "last": {"o": [], "fps": 0, "ms": 0, "info": {"source": "camera", "prob": det.prob, "size": a.size}}}
    try:
        src = CameraSource(frames) if a.source == "camera" else TestSource()
    except (ImportError, RuntimeError, IndexError, OSError) as e:
        print(f"Kamera açılamadı ({e}); --source test ile devam ediliyor.")
        src = TestSource()
    state["last"]["info"]["source"] = "test" if isinstance(src, TestSource) else "camera"
    if isinstance(src, TestSource):
        import cv2

        def enc(img):  # yazılım JPEG yalnız deneme kaynağında
            frames.write(cv2.imencode(".jpg", img, [cv2.IMWRITE_JPEG_QUALITY, 70])[1])
    serve(a.port, frames, state, det)
    print(f"Arayüz: http://0.0.0.0:{a.port}   (Ctrl+C ile durdur)", flush=True)
    n, t_last, ms_acc = 0, time.perf_counter(), 0.0
    while True:
        img = src.lores()
        if isinstance(src, TestSource):
            enc(img)
        t = time.perf_counter()
        objs = det(img)
        ms_acc += (time.perf_counter() - t) * 1000
        n += 1
        now = time.perf_counter()
        if now - t_last >= 1:
            fps, ms = round(n / (now - t_last), 1), round(ms_acc / n, 1)
            print(f"tespit {fps} FPS · {ms} ms · {len(objs)} nesne", flush=True)
            n, t_last, ms_acc = 0, now, 0.0
            state["last"]["fps"], state["last"]["ms"] = fps, ms
        with state["cond"]:
            state["last"]["o"] = objs
            state["cond"].notify_all()


if __name__ == "__main__":
    main()
