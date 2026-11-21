"""{{name}}: Google LiteRT ile cihaz üzerinde görüntü sınıflandırma (EfficientNet-B1, ImageNet 1000 sınıf).

  uv run main.py                    web arayüzü http://<pi>:8000 (fotoğraf yükle / kameradan çek)
  uv run main.py --image foto.jpg   komut satırında sınıflandır
  uv run main.py --camera           kameradan bir kare çek ve sınıflandır

Model ve etiketler ilk çalıştırmada models/ klasörüne indirilir (int8 ~9 MB, fp32 ~31 MB); sonra çevrimdışı çalışır.
Ön işleme model kartındakiyle aynıdır: kısa kenar 255, ortadan 240x240 kırpma, ImageNet ortalama/sapma, NCHW.
"""
import argparse
import io
import json
import os
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps

HERE = Path(__file__).resolve().parent
MODEL_DIR = HERE / "models"
HF = "https://huggingface.co"
MODELS = {
    "int8": "efficientnet_b1_weight_only_wi8_afp32.tflite",
    "fp32": "efficientnet_b1.tflite",
}
MODEL_URL = HF + "/litert-community/efficientnet_b1/resolve/main/{}"
LABELS_URL = HF + "/datasets/huggingface/label-files/resolve/main/imagenet-1k-id2label.json"
MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)
MAX_IMG = 15 << 20


def download(url, dest):
    if dest.exists() and dest.stat().st_size > 0:
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    print(f"İndiriliyor: {dest.name} …", flush=True)
    tmp = dest.with_suffix(dest.suffix + ".part")
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "MakerOS-litert"}), timeout=60) as r, open(tmp, "wb") as f:
            while chunk := r.read(1 << 16):
                f.write(chunk)
    except OSError as e:
        raise SystemExit(f"İndirilemedi ({e}). İnternet bağlantısını kontrol edin ya da dosyayı {dest.parent} içine koyun.")
    os.replace(tmp, dest)
    return dest


class Classifier:
    def __init__(self, model="int8"):
        from ai_edge_litert.compiled_model import CompiledModel

        self.lock = threading.Lock()
        labels = download(LABELS_URL, MODEL_DIR / "imagenet-1k-id2label.json")
        self.labels = {int(k): v for k, v in json.loads(labels.read_text(encoding="utf-8")).items()}
        self.name = MODELS[model]
        self.model = CompiledModel.from_file(str(download(MODEL_URL.format(self.name), MODEL_DIR / self.name)))
        sig = next(iter(self.model.get_signature_list()))
        shape = next(iter(self.model.get_input_tensor_details(sig).values()))["shape"]  # [1, 3, H, W]
        self.h, self.w = shape[2], shape[3]
        self.n_out = int(np.prod(next(iter(self.model.get_output_tensor_details(sig).values()))["shape"]))
        self.inp, self.out = self.model.create_input_buffers(0), self.model.create_output_buffers(0)

    def prep(self, img):
        img = ImageOps.exif_transpose(img).convert("RGB")
        s = round(max(self.h, self.w) * 255 / 240)
        iw, ih = img.size
        img = img.resize((s, round(ih * s / iw)) if iw < ih else (round(iw * s / ih), s), Image.BILINEAR)
        left, top = (img.size[0] - self.w) // 2, (img.size[1] - self.h) // 2
        img = img.crop((left, top, left + self.w, top + self.h))
        x = (np.asarray(img, dtype=np.float32) / 255.0 - MEAN) / STD
        return np.ascontiguousarray(np.transpose(x, (2, 0, 1)))

    def classify(self, img, k=5):
        x = self.prep(img)
        t = time.perf_counter()
        with self.lock:
            self.inp[0].write(x)
            self.model.run_by_index(0, self.inp, self.out)
            y = self.out[0].read(self.n_out, np.float32).astype(np.float64)
        ms = round((time.perf_counter() - t) * 1000, 1)
        p = np.exp(y - y.max())
        p /= p.sum()
        idx = np.argsort(-p)[:k]
        return {"ms": ms, "top": [{"index": int(i), "label": self.labels.get(int(i), f"sınıf {i}"), "prob": round(float(p[i]), 4)} for i in idx]}


def camera_jpeg():
    """Kameradan tek kare (Pi kamera modülü: rpicam-still; yoksa USB kamera: fswebcam)."""
    if shutil.which("rpicam-still"):
        cmd = ["rpicam-still", "-n", "-t", "500", "--width", "640", "--height", "480", "-e", "jpg", "-o", "-"]
    elif shutil.which("fswebcam"):
        cmd = ["fswebcam", "-q", "-r", "640x480", "--no-banner", "--jpeg", "90", "-"]
    else:
        raise RuntimeError("Kamera aracı yok (rpicam-still ya da fswebcam).")
    r = subprocess.run(cmd, capture_output=True, timeout=20)
    if r.returncode != 0 or not r.stdout:
        raise RuntimeError("Kamera görüntü vermedi: " + r.stderr.decode(errors="replace").strip()[-200:])
    return r.stdout


def has_camera():
    return bool(shutil.which("rpicam-still") or shutil.which("fswebcam"))


PAGE = r"""<!doctype html><html lang=tr><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>
<title>{{name}} · Görüntü tanıma</title><style>
:root{--a:#f07a1a;--bg:#fff;--fg:#1d1d1f;--mut:#6e6e73;--card:#f5f5f7;--bd:#e3e3e8;--bad:#d64545}
@media(prefers-color-scheme:dark){:root{--bg:#1e1f22;--fg:#ececee;--mut:#9a9aa2;--card:#2a2b30;--bd:#3a3b41}}
*{box-sizing:border-box}body{margin:0;padding:20px;background:var(--bg);color:var(--fg);font:15px/1.45 system-ui,-apple-system,'Segoe UI',sans-serif}
h1{font-size:20px;margin:0 0 2px}.sub{color:var(--mut);font-size:13px}.row{display:flex;gap:10px;align-items:center;flex-wrap:wrap}
button{font:inherit;padding:8px 14px;border-radius:8px;border:0;background:var(--a);color:#fff;cursor:pointer}button:disabled{opacity:.5}
.drop{margin-top:14px;border:2px dashed var(--bd);border-radius:14px;padding:28px;text-align:center;cursor:pointer;background:var(--card)}.drop.over{border-color:var(--a)}
.res{display:grid;grid-template-columns:minmax(0,260px) 1fr;gap:18px;margin-top:16px;align-items:start}@media(max-width:560px){.res{grid-template-columns:1fr}}
img{max-width:100%;border-radius:12px;display:block}.bar{margin:0 0 10px}.bar .t{display:flex;justify-content:space-between;font-size:14px;gap:10px}
.bar .t span:first-child{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.bar i{display:block;height:8px;border-radius:4px;background:var(--a);margin-top:4px;min-width:2px}
.bar:not(:first-child) i{opacity:.55}.err{color:var(--bad);margin-top:8px;white-space:pre-wrap}.hide{display:none}
</style>
<div class=row style="justify-content:space-between"><div><h1>Görüntü tanıma</h1><div class=sub id=info>Google LiteRT · cihaz üzerinde</div></div>
<button id=cam class=hide>Kameradan çek</button></div>
<div class=drop id=drop>Bir fotoğraf sürükleyin ya da <b>seçmek için tıklayın</b><div class=sub>JPEG, PNG, WebP · görüntü cihazdan çıkmaz</div></div>
<input type=file id=file accept="image/*" class=hide><div class=err id=err></div>
<div class="res hide" id=res><img id=img alt=""><div><div id=top></div><div class=sub id=ms></div></div></div>
<script>
const $=id=>document.getElementById(id);
async function post(p,body,type){const r=await fetch(p,{method:'POST',headers:type?{'Content-Type':type}:{},body});const d=await r.json().catch(()=>({error:'Geçersiz yanıt'}));if(!r.ok||d.error)throw new Error(d.error||('HTTP '+r.status));return d}
function show(d){$('top').innerHTML='';for(const r of d.top){const b=document.createElement('div');b.className='bar';const t=document.createElement('div');t.className='t';
 const a=document.createElement('span');a.textContent=r.label.split(',')[0];a.title=r.label;const p=document.createElement('span');p.textContent='%'+(r.prob*100).toFixed(1);
 const i=document.createElement('i');i.style.width=(r.prob*100)+'%';t.append(a,p);b.append(t,i);$('top').append(b)}$('ms').textContent=`Çıkarım ${d.ms} ms`}
async function run(f){if(!f)return;$('err').textContent='';$('img').src=URL.createObjectURL(f);$('res').classList.remove('hide');$('top').innerHTML='<div class=sub>Sınıflandırılıyor…</div>';$('ms').textContent='';
 try{show(await post('api/classify',f,f.type||'application/octet-stream'))}catch(e){$('top').innerHTML='';$('err').textContent=e.message}}
$('drop').onclick=()=>$('file').click();$('file').onchange=()=>run($('file').files[0]);
$('drop').ondragover=e=>{e.preventDefault();$('drop').classList.add('over')};$('drop').ondragleave=()=>$('drop').classList.remove('over');
$('drop').ondrop=e=>{e.preventDefault();$('drop').classList.remove('over');run(e.dataTransfer.files[0])};
$('cam').onclick=async()=>{$('cam').disabled=true;$('err').textContent='';try{const d=await post('api/camera');$('img').src='data:image/jpeg;base64,'+d.jpeg;$('res').classList.remove('hide');show(d)}catch(e){$('err').textContent=e.message}finally{$('cam').disabled=false}};
fetch('api/info').then(r=>r.json()).then(i=>{$('info').textContent=`Google LiteRT · ${i.model} · cihaz üzerinde`;if(i.camera)$('cam').classList.remove('hide')});
</script>"""


def serve(clf, port, k):
    import base64

    class H(BaseHTTPRequestHandler):
        def log_message(self, fmt, *args):
            pass

        def _send(self, code, body, ctype="application/json"):
            if not isinstance(body, bytes):
                body = json.dumps(body, ensure_ascii=False).encode()
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            p = self.path.split("?")[0].rstrip("/")
            if p.endswith("/api/info"):
                return self._send(200, {"model": clf.name, "camera": has_camera()})
            self._send(200, PAGE.encode(), "text/html; charset=utf-8")

        def do_POST(self):
            p = self.path.split("?")[0].rstrip("/")
            n = int(self.headers.get("Content-Length") or 0)
            if n > MAX_IMG:
                return self._send(413, {"error": "Görüntü en fazla 15 MB olabilir."})
            data = self.rfile.read(n)
            try:
                if p.endswith("/api/camera"):
                    data = camera_jpeg()
                    out = clf.classify(Image.open(io.BytesIO(data)), k)
                    out["jpeg"] = base64.b64encode(data).decode()
                    return self._send(200, out)
                if p.endswith("/api/classify"):
                    try:
                        img = Image.open(io.BytesIO(data))
                        img.load()
                    except Exception:
                        return self._send(400, {"error": "Görüntü okunamadı (JPEG, PNG, WebP, BMP, GIF)."})
                    return self._send(200, clf.classify(img, k))
                self._send(404, {"error": "bulunamadı"})
            except (RuntimeError, subprocess.TimeoutExpired) as e:
                self._send(503, {"error": str(e)})

    srv = ThreadingHTTPServer(("", port), H)
    print(f"Arayüz: http://<pi>:{port}  (Ctrl+C ile çık)", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


def print_result(d):
    for r in d["top"]:
        print(f"  %{r['prob'] * 100:5.1f}  {r['label']}")
    print(f"  ({d['ms']} ms)")


def main():
    ap = argparse.ArgumentParser(description="Google LiteRT ile görüntü sınıflandırma")
    ap.add_argument("--image", help="sınıflandırılacak görüntü dosyası")
    ap.add_argument("--camera", action="store_true", help="kameradan bir kare çek ve sınıflandır")
    ap.add_argument("--model", default="int8", choices=sorted(MODELS))
    ap.add_argument("--top", type=int, default=5)
    ap.add_argument("--port", type=int, default=8000)
    a = ap.parse_args()
    clf = Classifier(a.model)
    print(f"Model hazır: {clf.name}", flush=True)
    if a.image:
        try:
            img = Image.open(a.image)
        except FileNotFoundError:
            sys.exit(f"Dosya yok: {a.image}")
        except OSError:
            sys.exit(f"Görüntü okunamadı: {a.image} (JPEG, PNG, WebP, BMP, GIF)")
        print_result(clf.classify(img, a.top))
    elif a.camera:
        try:
            print_result(clf.classify(Image.open(io.BytesIO(camera_jpeg())), a.top))
        except RuntimeError as e:
            sys.exit(str(e))
    else:
        serve(clf, a.port, a.top)


if __name__ == "__main__":
    main()
