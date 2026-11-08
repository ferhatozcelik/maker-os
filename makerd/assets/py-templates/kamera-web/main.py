"""{{name}}: Kamerayı http://<pi>:8000 adresinde canlı yayınlar.

rpicam-vid donanım MJPEG çıktısı kullanılır (MakerOS'ta hazır); Pi'nin işlemcisi yorulmaz.
Kamera uygulaması açıksa önce onu kapatın: kamerayı aynı anda tek program kullanabilir.
"""
import subprocess
import sys
import threading
from http import server

PAGE = b"<!doctype html><meta name=viewport content='width=device-width'><title>{{name}}</title><body style='margin:0;background:#111'><img src='/stream' style='width:100%'>"


class Frames:
    def __init__(self):
        self.frame, self.cond = None, threading.Condition()

    def put(self, f):
        with self.cond:
            self.frame = f
            self.cond.notify_all()


frames = Frames()


def camera():
    """rpicam-vid MJPEG akışını JPEG karelerine böler (FFD8 … FFD9)."""
    p = subprocess.Popen(["rpicam-vid", "-n", "-t", "0", "--codec", "mjpeg", "--width", "1280", "--height", "720",
                          "--framerate", "25", "-o", "-"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    buf = b""
    while True:
        chunk = p.stdout.read1(65536)  # read1: gelen kadarını hemen ver (read 64 KB dolana dek bekler)
        if not chunk:
            print("Kamera akışı durdu. Kamera takılı mı, başka bir program kullanıyor mu?", file=sys.stderr)
            return
        buf += chunk
        while True:
            a = buf.find(b"\xff\xd8")
            b = buf.find(b"\xff\xd9", a + 2)
            if a < 0 or b < 0:
                break
            frames.put(buf[a:b + 2])
            buf = buf[b + 2:]


class Handler(server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        if self.path == "/":
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(PAGE)
            return
        self.send_response(200)
        self.send_header("Content-Type", "multipart/x-mixed-replace; boundary=FRAME")
        self.end_headers()
        try:
            while True:
                with frames.cond:
                    frames.cond.wait(5)
                    f = frames.frame
                if f:
                    self.wfile.write(b"--FRAME\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n" % len(f) + f + b"\r\n")
        except (BrokenPipeError, ConnectionResetError):
            pass


threading.Thread(target=camera, daemon=True).start()
print("Yayın: http://localhost:8000  (/port/8000/)  (Ctrl+C ile durdur)")
server.ThreadingHTTPServer(("", 8000), Handler).serve_forever()
