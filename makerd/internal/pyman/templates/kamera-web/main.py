"""{{name}}: Kamerayı http://<pi>:8000 adresinde canlı yayınlar (donanım MJPEG kodlayıcı)."""
import io
import threading
from http import server

from picamera2 import Picamera2
from picamera2.encoders import MJPEGEncoder
from picamera2.outputs import FileOutput

PAGE = b"<!doctype html><meta name=viewport content='width=device-width'><title>{{name}}</title><body style='margin:0;background:#111'><img src='/stream' style='width:100%'>"


class Frames(io.BufferedIOBase):
    def __init__(self):
        self.frame, self.cond = None, threading.Condition()

    def write(self, buf):
        with self.cond:
            self.frame = buf
            self.cond.notify_all()


frames = Frames()


class Handler(server.BaseHTTPRequestHandler):
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
                    frames.cond.wait()
                    f = frames.frame
                self.wfile.write(b"--FRAME\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n" % len(f) + f + b"\r\n")
        except (BrokenPipeError, ConnectionResetError):
            pass


cam = Picamera2()
cam.configure(cam.create_video_configuration(main={"size": (1280, 720)}))
cam.start_recording(MJPEGEncoder(), FileOutput(frames))
print("Yayın: http://0.0.0.0:8000  (Ctrl+C ile durdur)")
try:
    server.ThreadingHTTPServer(("", 8000), Handler).serve_forever()
finally:
    cam.stop_recording()
