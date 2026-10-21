"""Pardus Maker kamera yardimcisi (MediaPipe orneklerinin ortak dosyasi).

Kaynak (kamera) secimi otomatiktir:
  1. Raspberry Pi kamera modulu (CSI) -> libcamera-vid / rpicam-vid MJPEG akisi
  2. USB web kamerasi                -> OpenCV VideoCapture
  3. Video veya resim dosyasi        -> python main.py --kaynak video.mp4

Sonuc iki yerde gosterilir:
  - Pi'nin masaustunde bir pencere (monitor veya uzak ekran)
  - Tarayicida canli yayin: http://pardus-maker.local:8000/  (telefondan da)

Kullanim:
    from pios_kamera import Kamera, Gosterici
    with Kamera() as kam, Gosterici("El takibi") as ekran:
        for kare in kam:
            ...  # kare: OpenCV BGR goruntusu (numpy dizisi)
            if not ekran.goster(kare):
                break
"""
import argparse
import os
import shutil
import socket
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import cv2
import numpy as np


def arguman_ayristirici(aciklama):
    """Tum orneklerde ortak komut satiri secenekleri."""
    p = argparse.ArgumentParser(description=aciklama)
    p.add_argument("--kaynak", default="otomatik",
                   help="otomatik | pi | usb | usb:1 | video/resim dosyasi (varsayilan: otomatik)")
    p.add_argument("--genislik", type=int, default=640)
    p.add_argument("--yukseklik", type=int, default=480)
    p.add_argument("--port", type=int, default=8000, help="web yayini portu (0: kapali)")
    p.add_argument("--pencere-yok", action="store_true", help="masaustu penceresi acma")
    return p


def _libcamera_vid():
    for ad in ("rpicam-vid", "libcamera-vid"):
        yol = shutil.which(ad)
        if yol:
            return yol
    return None


def _pi_kamera_var():
    for ad in ("rpicam-hello", "libcamera-hello"):
        yol = shutil.which(ad)
        if yol:
            try:
                p = subprocess.run([yol, "--list-cameras"], capture_output=True, text=True, timeout=15)
            except (OSError, subprocess.TimeoutExpired):
                return False
            return " : " in p.stdout + p.stderr
    return False


class Kamera:
    """Kareleri BGR numpy dizisi olarak veren yineleyici."""

    def __init__(self, kaynak="otomatik", genislik=640, yukseklik=480, fps=30):
        self.genislik, self.yukseklik, self.fps = genislik, yukseklik, fps
        self.surec = self.cap = self.resim = None
        if kaynak == "otomatik":
            kaynak = "pi" if _libcamera_vid() and _pi_kamera_var() else "usb"
        self.kaynak = kaynak
        if kaynak == "pi":
            self._pi_baslat()
        elif kaynak.startswith("usb"):
            idx = int(kaynak.split(":")[1]) if ":" in kaynak else 0
            self.cap = cv2.VideoCapture(idx, cv2.CAP_V4L2)
            self.cap.set(cv2.CAP_PROP_FRAME_WIDTH, genislik)
            self.cap.set(cv2.CAP_PROP_FRAME_HEIGHT, yukseklik)
            if not self.cap.isOpened():
                raise SystemExit("Kamera bulunamadi. Pi kamera kablosunu veya USB kamerayi kontrol edin.\n"
                                 "Pardus Maker Merkezi > Kamera bolumunde kameranin gorundugunu dogrulayin.")
        elif os.path.isfile(kaynak):
            ext = os.path.splitext(kaynak)[1].lower()
            if ext in (".jpg", ".jpeg", ".png", ".bmp", ".webp"):
                self.resim = cv2.imread(kaynak)
            else:
                self.cap = cv2.VideoCapture(kaynak)
        else:
            raise SystemExit("Gecersiz kaynak: %s" % kaynak)
        print("Kamera kaynagi:", self.kaynak)

    def _pi_baslat(self):
        # Tek surec: libcamera-vid MJPEG'i stdout'a yazar, kareler JPEG
        # baslangic/bitis isaretlerinden ayrilir.
        self.surec = subprocess.Popen(
            [_libcamera_vid(), "-t", "0", "-n", "--codec", "mjpeg", "--width", str(self.genislik),
             "--height", str(self.yukseklik), "--framerate", str(self.fps), "--flush", "-o", "-"],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self._tampon = b""

    def _pi_kare(self):
        fd = self.surec.stdout.fileno()
        while True:
            bas = self._tampon.find(b"\xff\xd8")
            son = self._tampon.find(b"\xff\xd9", bas + 2) if bas >= 0 else -1
            if son > 0:
                jpeg, self._tampon = self._tampon[bas:son + 2], self._tampon[son + 2:]
                return cv2.imdecode(np.frombuffer(jpeg, np.uint8), cv2.IMREAD_COLOR)
            parca = os.read(fd, 262144)
            if not parca:
                raise SystemExit("Pi kamera akisi durdu (kamera baska bir uygulamada acik olabilir; "
                                 "Merkez > Kamera sayfasini kapatin).")
            self._tampon += parca

    def __iter__(self):
        while True:
            if self.resim is not None:
                yield self.resim.copy()
                time.sleep(1 / 15)
                continue
            if self.surec:
                kare = self._pi_kare()
            else:
                ok, kare = self.cap.read()
                if not ok:
                    if self.kaynak.startswith("usb"):
                        raise SystemExit("USB kameradan goruntu alinamadi")
                    return  # video dosyasi bitti
            if kare is not None:
                yield kare

    def kapat(self):
        if self.surec:
            self.surec.terminate()
            self.surec.wait(timeout=3)
        if self.cap:
            self.cap.release()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.kapat()


class _Yayin(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        g = self.server.gosterici
        if self.path != "/akis":
            body = ("<!doctype html><meta name=viewport content='width=device-width'><title>%s</title>"
                    "<body style='margin:0;background:#111;color:#eee;font-family:sans-serif;text-align:center'>"
                    "<h3>%s</h3><img src='/akis' style='max-width:100%%'></body>" % (g.baslik, g.baslik)).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(200)
        self.send_header("Content-Type", "multipart/x-mixed-replace; boundary=kare")
        self.end_headers()
        son = 0
        try:
            while not g.kapandi:
                with g.kosul:
                    g.kosul.wait_for(lambda: g.sira != son or g.kapandi, timeout=5)
                    jpeg, son = g.jpeg, g.sira
                if jpeg:
                    self.wfile.write(b"--kare\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n"
                                     % len(jpeg) + jpeg + b"\r\n")
        except OSError:
            pass


class Gosterici:
    """Kareyi masaustu penceresinde ve web yayininda gosterir."""

    def __init__(self, baslik, port=8000, pencere=True):
        self.baslik, self.kapandi = baslik, False
        self.pencere = pencere and bool(os.environ.get("DISPLAY"))
        self.jpeg, self.sira, self.kosul = None, 0, threading.Condition()
        self.sunucu = None
        self._fps_t, self._fps_n, self.fps = time.time(), 0, 0.0
        if port:
            try:
                ThreadingHTTPServer.daemon_threads = True
                self.sunucu = ThreadingHTTPServer(("", port), _Yayin)
                self.sunucu.gosterici = self
                threading.Thread(target=self.sunucu.serve_forever, daemon=True).start()
                print("Web yayini: http://%s.local:%d/  (cikmak icin Ctrl+C)" % (socket.gethostname(), port))
            except OSError as e:
                print("Web yayini acilamadi (port %d): %s" % (port, e))

    def yaz(self, kare, metin, satir=0, renk=(255, 255, 255)):
        """Kare uzerine okunakli (golgeli) yazi yazar."""
        y = 30 + 30 * satir
        cv2.putText(kare, metin, (12, y), cv2.FONT_HERSHEY_SIMPLEX, 0.8, (0, 0, 0), 4, cv2.LINE_AA)
        cv2.putText(kare, metin, (12, y), cv2.FONT_HERSHEY_SIMPLEX, 0.8, renk, 2, cv2.LINE_AA)

    def goster(self, kare):
        """Kareyi gosterir; kullanici pencerede q/ESC'ye basarsa False doner."""
        self._fps_n += 1
        if time.time() - self._fps_t >= 1:
            self.fps = self._fps_n / (time.time() - self._fps_t)
            self._fps_t, self._fps_n = time.time(), 0
        h = kare.shape[0]
        cv2.putText(kare, "%.1f FPS" % self.fps, (12, h - 12), cv2.FONT_HERSHEY_SIMPLEX, 0.6,
                    (0, 255, 0), 2, cv2.LINE_AA)
        if self.sunucu:
            ok, buf = cv2.imencode(".jpg", kare, [cv2.IMWRITE_JPEG_QUALITY, 80])
            if ok:
                with self.kosul:
                    self.jpeg, self.sira = buf.tobytes(), self.sira + 1
                    self.kosul.notify_all()
        if self.pencere:
            cv2.imshow(self.baslik, kare)
            if cv2.waitKey(1) & 0xFF in (ord("q"), 27):
                return False
        return True

    def kapat(self):
        self.kapandi = True
        with self.kosul:
            self.kosul.notify_all()
        if self.sunucu:
            self.sunucu.shutdown()
        if self.pencere:
            cv2.destroyAllWindows()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.kapat()


def mp_goruntu(kare):
    """OpenCV BGR karesini MediaPipe goruntusune cevirir."""
    import mediapipe as mp
    return mp.Image(image_format=mp.ImageFormat.SRGB, data=cv2.cvtColor(kare, cv2.COLOR_BGR2RGB))


def zaman_ms():
    """MediaPipe VIDEO modu icin artan zaman damgasi (ms)."""
    return int(time.monotonic() * 1000)
