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

# Eklenti kiti: kabuğun tema belirteçleri (/shell/css/tokens.css) ve dili/canlı eşitlemesi (/shell/js/plugin-kit.js → window.MK).
# Renkler için var(--accent), var(--text), var(--bg-window)… kullanın; metinleri MK.i18n ile iki dilli yazın.
# Kabuk dışında açılırsa (kit yok) küçük yedek devreye girer.
PAGE = """<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>
<link rel=stylesheet href="/shell/css/tokens.css"><script src="/shell/js/plugin-kit.js"></script>
<script>window.MK=window.MK||{lang:'tr',locale:'tr-TR',i18n(d){this.d=d;this.translate();return this},t(k){return((this.d||{})[this.lang]||{})[k]??k},
translate(){for(const n of document.querySelectorAll('[data-t]'))n.textContent=this.t(n.dataset.t)},on(){},theme(){return{}}};</script>
<style>body{font:14px/1.45 var(--font-sans,system-ui);margin:24px;color:var(--text,#222);background:var(--bg-window,#fff)}
button{font:inherit;padding:8px 14px;border-radius:8px;border:0;background:var(--accent,#f07a1a);color:var(--on-accent,#fff)}</style>
<h1 data-t=title></h1><p data-t=desc></p>
<button id=b data-t=status></button>
<pre id=o></pre>
<script>
MK.i18n({tr:{title:'Benim Eklentim',desc:'Python eklentisi çalışıyor. Bu sayfayı main.py içinde değiştirin.',status:'Durumu getir'},
         en:{title:'My Plugin',desc:'The Python plugin is running. Change this page in main.py.',status:'Get status'}});
// Sunucu dili bilsin diye X-MakerOS-Lang başlığı; tema/dil değişince MK.on çağrılır (data-t metinleri kendiliğinden çevrilir).
b.onclick=()=>fetch('api',{headers:{'X-MakerOS-Lang':MK.lang}}).then(r=>r.json()).then(d=>o.textContent=JSON.stringify(d,null,2));
MK.on((mk,changed)=>console.log('MakerOS değişti:',[...changed],mk.lang,mk.dark));
</script>"""


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
