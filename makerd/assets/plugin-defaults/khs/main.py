"""Konya HackerSpace eklentisi: kapı durumu (açık/kapalı) ve haftalık açıklık bilgisi.

Kaynaklar (konyahackerspace.org sitesinin kendi kullandıkları):
  * Canlı durum: Firebase Realtime Database  presence/state.json  → true/false (veya {state|motion|value|on|occupied})
  * Geçmiş:      https://konyahackerspace.org/api/space.json      → state, since, günlük açık aralıklar, haftalık olasılık
Bağımlılık yok (standart kitaplık). Yalnız 127.0.0.1 dinler; MakerOS /plugin/khs/ üzerinden açar.
  GET /api/door  → {"open": bool|null, "state", "since", "updated", "usually": [...], "error"?}
  GET /          → tema renklerinde durum sayfası
"""
import json
import os
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("MAKEROS_PLUGIN_PORT", "8790"))
LIVE = "https://konyahackerspace-f75c4-default-rtdb.europe-west1.firebasedatabase.app/presence/state.json"
SPACE = "https://konyahackerspace.org/api/space.json"
UA = "MakerOS-KHS-eklentisi/1.0 (+https://github.com/ferhatozcelik/maker-os)"
LIVE_TTL, SPACE_TTL = 30, 300  # saniye: siteye yük bindirmeden güncel kal

_lock = threading.Lock()
_cache = {"live": (0, None), "space": (0, None)}


def _get(url, timeout=8):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Cache-Control": "no-store"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode())


def _cached(key, url, ttl):
    with _lock:
        t, v = _cache[key]
    if v is not None and time.time() - t < ttl:
        return v
    v = _get(url)
    with _lock:
        _cache[key] = (time.time(), v)
    return v


def _truthy(v):
    # Sitedeki betikle aynı yorum: bool ya da {state|motion|value|on|occupied}
    if isinstance(v, bool):
        return v
    if isinstance(v, (int, float)):
        return v != 0
    if isinstance(v, str):
        return v.lower() in ("open", "online", "true", "1", "on")
    if isinstance(v, dict):
        for k in ("state", "motion", "value", "on", "occupied"):
            if k in v:
                return _truthy(v[k])
    return False


def door():
    out = {"open": None, "state": "unknown", "since": None, "updated": int(time.time() * 1000), "usually": []}
    try:
        out["open"] = _truthy(_cached("live", LIVE, LIVE_TTL))
        out["state"] = "open" if out["open"] else "closed"
    except Exception as e:  # ağ yok: bilinmiyor
        out["error"] = f"canlı durum alınamadı: {e.__class__.__name__}"
    try:
        sp = _cached("space", SPACE, SPACE_TTL)
        if out["open"] is None:
            out["state"] = sp.get("state", "unknown")
            out["open"] = out["state"] == "open"
        out["since"] = sp.get("since")
        week = sp.get("week") or []
        # Haftalık olasılık (Pzt..Paz × 24 saat): en olası açık saatler
        out["usually"] = [{"day": d, "hour": h, "p": round(p, 2)} for d, row in enumerate(week) for h, p in enumerate(row) if p >= 0.1]
        out["days"] = sp.get("days", [])
    except Exception as e:
        out.setdefault("error", f"geçmiş alınamadı: {e.__class__.__name__}")
    return out


PAGE = """<!doctype html><html lang=tr><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>
<title>Konya HackerSpace</title><style>
:root{--ink:#0c0f1a;--lime:#c8f53c;--dim:#8a93a6}*{box-sizing:border-box}body{margin:0;background:var(--ink);color:#e8ebf2;font:15px/1.5 ui-monospace,'JetBrains Mono','DejaVu Sans Mono',monospace;min-height:100vh;display:grid;place-items:center;padding:24px}
.card{width:min(620px,100%)}.top{display:flex;align-items:center;gap:16px}h1{margin:0;font-size:20px;letter-spacing:.02em}.sub{color:var(--dim)}
.st{margin:28px 0 8px;font-size:44px;font-weight:800;letter-spacing:.04em}.st.open{color:var(--lime)}.st.closed{color:#ff6b6b}.st.unknown{color:var(--dim)}
.dot{display:inline-block;width:14px;height:14px;border-radius:50%;margin-right:12px;vertical-align:middle;background:currentColor;box-shadow:0 0 18px currentColor}
.grid{display:grid;grid-template-columns:auto repeat(24,1fr);gap:2px;margin-top:22px;font-size:10px;color:var(--dim)}.grid i{height:12px;border-radius:2px;background:#1b2133}
a{color:var(--lime)}</style>
<div class=card><div class=top><svg width=52 height=52 viewBox='-4 -4 108 108' fill=none stroke=#c8f53c stroke-width=6><path d='M50 5 63.18 18.18 81.82 18.18 81.82 36.82 95 50 81.82 63.18 81.82 81.82 63.18 81.82 50 95 36.82 81.82 18.18 81.82 18.18 63.18 5 50 18.18 36.82 18.18 18.18 36.82 18.18Z'/><path d='M33.8 36.5 47.3 50 33.8 63.5M52.12 62.73H68.03'/></svg>
<div><h1>Konya HackerSpace</h1><div class=sub>guest@khs:~$ door --status</div></div></div>
<div id=st class='st unknown'><span class=dot></span><span id=t>…</span></div><div id=since class=sub></div>
<div class=grid id=g></div><p class=sub>Haftalık "genelde açık" saatleri · kaynak: <a href='https://konyahackerspace.org/tr/' target=_blank rel=noopener>konyahackerspace.org</a></p></div>
<script>
const D=['Pzt','Sal','Çar','Per','Cum','Cmt','Paz'];
async function tick(){try{const d=await (await fetch('api/door')).json();const s=document.getElementById('st');s.className='st '+d.state;
document.getElementById('t').textContent=d.state==='open'?'AÇIK':d.state==='closed'?'KAPALI':'BİLİNMİYOR';
document.getElementById('since').textContent=d.since?'şu durumda: '+new Date(d.since).toLocaleString('tr-TR',{timeZone:'Europe/Istanbul',weekday:'long',hour:'2-digit',minute:'2-digit'})+' tarihinden beri':(d.error||'');
const g=document.getElementById('g');if(!g.childElementCount){const m={};for(const u of d.usually)m[u.day+':'+u.hour]=u.p;
for(let r=0;r<7;r++){const l=document.createElement('span');l.textContent=D[r];g.append(l);for(let h=0;h<24;h++){const c=document.createElement('i');const p=m[r+':'+h]||0;
if(p)c.style.background=`rgba(200,245,60,${.25+p*.75})`;c.title=`${D[r]} ${h}:00 · %${Math.round(p*100)}`;g.append(c)}}}}catch(e){document.getElementById('t').textContent='BAĞLANTI YOK'}}
tick();setInterval(tick,30000);
</script>"""


WIDGET = """<!doctype html><html lang=tr><meta charset=utf-8><meta name=viewport content='width=device-width'><style>
html,body{margin:0;background:transparent;overflow:hidden}*{box-sizing:border-box}
.c{position:relative;border-radius:16px;background:rgba(12,15,26,.86);border:1px solid rgba(200,245,60,.28);box-shadow:0 10px 30px rgba(0,0,0,.35);
 color:#e8ebf2;font:12px/1.4 ui-monospace,'JetBrains Mono','DejaVu Sans Mono',monospace;padding:14px 16px;cursor:pointer}
.h{display:flex;align-items:center;gap:10px}.h b{font-size:13px}.h small{display:block;color:#8a93a6}
.st{margin:12px 0 2px;font-size:26px;font-weight:800;letter-spacing:.04em;display:flex;align-items:center;gap:10px}.st i{width:11px;height:11px;border-radius:50%;background:currentColor;box-shadow:0 0 14px currentColor}
.open{color:#c8f53c}.closed{color:#ff6b6b}.unknown{color:#8a93a6}.s{color:#8a93a6;min-height:17px}
.g{display:grid;grid-template-columns:repeat(24,1fr);gap:2px;margin-top:10px}.g i{height:6px;border-radius:1px;background:#1b2133}</style>
<div class=c id=c><div class=h><svg width=30 height=30 viewBox='-4 -4 108 108' fill=none stroke=#c8f53c stroke-width=7><path d='M50 5 63.18 18.18 81.82 18.18 81.82 36.82 95 50 81.82 63.18 81.82 81.82 63.18 81.82 50 95 36.82 81.82 18.18 81.82 18.18 63.18 5 50 18.18 36.82 18.18 18.18 36.82 18.18Z'/><path d='M33.8 36.5 47.3 50 33.8 63.5M52.12 62.73H68.03'/></svg>
<div><b>Konya HackerSpace</b><small>door --status</small></div></div>
<div id=st class='st unknown'><i></i><span id=t>…</span></div><div class=s id=s></div><div class=g id=g></div><div class=s style='margin-top:6px;font-size:10px'>bugün genelde açık saatler (Türkiye saati)</div></div>
<script>
document.getElementById('c').onclick=()=>parent.postMessage({mk:'open',app:'makeros.plugin.khs.site'},location.origin);
// Yükseklik içeriğe göre bildirilir: kabuk iframe'i bu boya ayarlar (altta boşluk kalmaz).
new ResizeObserver(()=>parent.postMessage({mk:'widget-size',h:Math.ceil(document.getElementById('c').getBoundingClientRect().height)},location.origin)).observe(document.getElementById('c'));
async function tick(){try{const d=await (await fetch('api/door')).json();const st=document.getElementById('st');st.className='st '+d.state;
document.getElementById('t').textContent=d.state==='open'?'AÇIK':d.state==='closed'?'KAPALI':'BİLİNMİYOR';
document.getElementById('s').textContent=d.since?new Date(d.since).toLocaleString('tr-TR',{timeZone:'Europe/Istanbul',weekday:'long',hour:'2-digit',minute:'2-digit'})+' tarihinden beri':'';
// Bugünün "genelde açık" saatleri (Pzt=0)
const tr=new Date(new Date().toLocaleString('en-US',{timeZone:'Europe/Istanbul'})),today=(tr.getDay()+6)%7,m={};for(const u of d.usually)if(u.day===today)m[u.hour]=u.p;const g=document.getElementById('g');g.replaceChildren();
for(let h=0;h<24;h++){const c=document.createElement('i');if(m[h])c.style.background=`rgba(200,245,60,${.25+m[h]*.75})`;if(h===tr.getHours())c.style.outline='1px solid #e8ebf2';c.title=h+':00';g.append(c)}}catch(e){document.getElementById('t').textContent='BAĞLANTI YOK'}}
tick();setInterval(tick,30000);
</script>"""


class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        pass

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        p = self.path.split("?")[0].rstrip("/")
        if p.endswith("/widget"):
            self._send(200, WIDGET.encode(), "text/html; charset=utf-8")
        elif p.endswith("/api/door") or p == "/api/door":
            self._send(200, json.dumps(door()).encode(), "application/json")
        else:
            self._send(200, PAGE.encode(), "text/html; charset=utf-8")


if __name__ == "__main__":
    print("Konya HackerSpace eklentisi; port", PORT, flush=True)
    ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
