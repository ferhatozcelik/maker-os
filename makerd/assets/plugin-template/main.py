"""MakerOS eklenti şablonu.

* plugin.json  kimlik, ad, sürüm; web_port verilirse arayüz MakerOS penceresinde açılır.
* pyproject.toml bağımlılıklar (uv ile ilk çalıştırmada kurulur).
* Sunucu YALNIZ 127.0.0.1'i dinlemeli: MakerOS oturum doğrulamasından sonra /plugin/<id>/ üzerinden yönlendirir.
* Ortam: MAKEROS_PLUGIN_PORT (dinlenecek port), MAKEROS_PLUGIN_ID.
Zip'leyip paylaşın: Eklentiler uygulamasında "Dışa aktar" aynı biçimi üretir.
"""
import json
import os
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("MAKEROS_PLUGIN_PORT", "8765"))
START = time.time()

PAGE = """<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>
<style>body{font:15px system-ui;margin:24px;color:#222}@media(prefers-color-scheme:dark){body{background:#1e1f22;color:#eee}}
button{font:inherit;padding:8px 14px;border-radius:8px;border:0;background:#f07a1a;color:#fff}</style>
<h1>Benim Eklentim</h1><p>Python eklentisi çalışıyor. Bu sayfayı <code>main.py</code> içinde değiştirin.</p>
<button onclick="fetch('api').then(r=>r.json()).then(d=>o.textContent=JSON.stringify(d,null,2))">Durumu getir</button>
<pre id=o></pre>"""


class App(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        print(fmt % args, flush=True)

    def do_GET(self):
        if self.path.rstrip("/").endswith("/api") or self.path == "/api":
            body, ctype = json.dumps({"calisma_suresi_sn": round(time.time() - START)}).encode(), "application/json"
        else:
            body, ctype = PAGE.encode(), "text/html; charset=utf-8"
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


if __name__ == "__main__":
    print("Eklenti başladı; port", PORT, flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), App).serve_forever()
