"""{{name}}: http://<pi>:8080 adresinde çalışan küçük web uygulaması."""
import json
import os
import platform
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("PORT", "8080"))


class App(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/api":
            body = json.dumps({"proje": "{{name}}", "makine": platform.node(), "python": platform.python_version()}).encode()
            ctype = "application/json"
        else:
            body = "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><h1>{{name}}</h1><p>MakerOS üzerinde çalışıyor.</p>".encode()
            ctype = "text/html; charset=utf-8"
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


print(f"http://0.0.0.0:{PORT}")
ThreadingHTTPServer(("", PORT), App).serve_forever()
