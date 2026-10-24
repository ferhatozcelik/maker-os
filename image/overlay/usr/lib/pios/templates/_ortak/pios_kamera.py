"""MediaPipe için kamera/resim girişi, sınırlı yük ve son-kare web yayını."""
import argparse
import dataclasses
import html
import json
import os
import pathlib
import secrets
import select
import shutil
import socket
import subprocess
import threading
import time
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# Örnekler bu yardımcıyı OpenCV/MediaPipe'tan önce içe aktarır.
for _name in ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'MKL_NUM_THREADS', 'NUMEXPR_NUM_THREADS'):
    os.environ.setdefault(_name, '2')
os.environ.setdefault('OPENCV_IO_MAX_IMAGE_PIXELS', '40000000')

import cv2
import numpy as np

RESIM_UZANTILARI = {'.jpg', '.jpeg', '.png', '.bmp', '.webp', '.tif', '.tiff'}
PROFILLER = {'tasarruf': (480, 360, 6, 1, 55), 'dengeli': (640, 480, 10, 2, 65),
             'kalite': (960, 720, 15, 3, 75)}


def resim_kaynagi(kaynak):
    return os.path.isdir(kaynak) or pathlib.Path(kaynak).suffix.lower() in RESIM_UZANTILARI


def _pozitif(value):
    n = int(value)
    if n < 1:
        raise argparse.ArgumentTypeError('Değer pozitif olmalıdır.')
    return n


class _Argumanlar(argparse.ArgumentParser):
    def parse_args(self, *args, **kwargs):
        a = super().parse_args(*args, **kwargs)
        w, h, fps, cores, quality = PROFILLER[a.profil]
        for key, default in [('genislik', w), ('yukseklik', h), ('fps', fps), ('cekirdek', cores), ('jpeg_kalite', quality)]:
            if getattr(a, key) is None:
                setattr(a, key, default)
        if not 1 <= a.fps <= 30 or not 1 <= a.cekirdek <= 8:
            self.error('FPS 1–30, çekirdek sayısı 1–8 olmalıdır.')
        if not 55 <= a.sicaklik_siniri <= 75 or not 1 <= a.jpeg_kalite <= 95:
            self.error('Sıcaklık sınırı 55–75 °C; JPEG kalitesi 1–95 olmalıdır.')
        if a.genislik > 1920 or a.yukseklik > 1080 or not 0 <= a.port <= 65535:
            self.error('En fazla 1920x1080 çözünürlük ve 0–65535 port kullanın.')
        return a


def arguman_ayristirici(aciklama):
    p = _Argumanlar(description=aciklama)
    p.add_argument('--kaynak', default='otomatik', help='otomatik | pi | usb:0 | video | resim | resim klasörü')
    p.add_argument('--profil', choices=PROFILLER, default='dengeli')
    p.add_argument('--genislik', type=_pozitif)
    p.add_argument('--yukseklik', type=_pozitif)
    p.add_argument('--fps', type=_pozitif, help='Saniyedeki en fazla analiz (1–30)')
    p.add_argument('--cekirdek', type=_pozitif, help='İşleme ayrılacak en fazla CPU çekirdeği')
    p.add_argument('--sicaklik-siniri', type=int, default=75)
    p.add_argument('--jpeg-kalite', type=int)
    p.add_argument('--cikti', default='sonuclar', help='Resim analizi PNG ve JSON çıktı klasörü')
    p.add_argument('--port', type=int, default=int(os.environ.get('PIOS_PLUGIN_PORT', '8000')), help='Web yayını portu (0: kapalı)')
    p.add_argument('--pencere-yok', action='store_true')
    return p


class IsiDenetimi:
    def __init__(self, fps=10, sinir=75):
        self.fps, self.sinir, self.son = fps, sinir, None
        self.sicaklik, self.durum, self.hedef_fps = None, 'normal', fps
        self._okuma = -float('inf')

    def oku(self):
        now = time.monotonic()
        if now - self._okuma >= 1:
            try:
                self.sicaklik = int(pathlib.Path('/sys/class/thermal/thermal_zone0/temp').read_text().strip()) / 1000
            except (OSError, ValueError):
                self.sicaklik = None
            self._okuma = now
        return self.sicaklik

    def hedef(self, temp):
        if temp is None or temp < self.sinir - 7:
            return self.fps, 'normal' if temp is not None else 'sensör yok'
        if temp >= self.sinir + 5:
            return 0, 'soğuma bekleniyor'
        return max(1, self.fps / (4 if temp >= self.sinir else 2)), 'yük azaltıldı'

    def bekle(self):
        temp = self.oku()
        self.hedef_fps, self.durum = self.hedef(temp)
        if not self.hedef_fps:
            print('Sıcaklık %.1f °C; analiz duraklatıldı. Soğutmayı kontrol edin.' % temp, flush=True)
            start = time.monotonic()
            while temp is not None and temp > self.sinir - 3:
                if time.monotonic() - start > 120:
                    raise RuntimeError('İşlemci 120 saniyede soğumadı; analiz durduruldu. Fanı ve havalandırmayı kontrol edin.')
                time.sleep(1)
                temp = self.oku()
            self.hedef_fps, self.durum = self.hedef(temp)
        now = time.monotonic()
        if self.son is not None:
            time.sleep(max(0, 1 / self.hedef_fps - (now - self.son)))
        self.son = time.monotonic()


def performans_ayarla(args):
    cv2.setNumThreads(args.cekirdek)
    cv2.ocl.setUseOpenCL(False)
    try:
        allowed = sorted(os.sched_getaffinity(0))
        os.sched_setaffinity(0, set(allowed[-args.cekirdek:]))
    except (AttributeError, OSError):
        pass
    args.denetim = IsiDenetimi(args.fps, args.sicaklik_siniri)
    print('Profil: %s · en fazla %s FPS · %sx%s · %s çekirdek' %
          (args.profil, args.fps, args.genislik, args.yukseklik, args.cekirdek))
    return args


def calisma_modu(args):
    from mediapipe.tasks.python import vision
    return vision.RunningMode.IMAGE if resim_kaynagi(args.kaynak) else vision.RunningMode.VIDEO


def _libcamera_vid():
    return shutil.which('rpicam-vid') or shutil.which('libcamera-vid')


def _pi_kamera_var():
    tool = shutil.which('rpicam-hello') or shutil.which('libcamera-hello')
    if not tool:
        return False
    try:
        p = subprocess.run([tool, '--list-cameras'], capture_output=True, text=True, timeout=10)
        import re
        return bool(re.search(r'^\s*\d+\s*:', p.stdout + '\n' + p.stderr, re.M))
    except (OSError, subprocess.TimeoutExpired):
        return False


class Kamera:
    def __init__(self, kaynak='otomatik', genislik=640, yukseklik=480, fps=10, denetim=None):
        self.genislik, self.yukseklik, self.fps = genislik, yukseklik, fps
        self.denetim = denetim or IsiDenetimi(fps)
        self.surec = self.cap = None
        self._stop = threading.Event()
        self._kosul = threading.Condition()
        self._kare, self._sira, self._hata, self._okuyucu = None, 0, '', None
        self.son_kaynak, self.sonuc, self.analiz_ms = kaynak, None, 0
        self._stderr = deque(maxlen=20)
        if kaynak == 'otomatik':
            kaynak = 'pi' if _libcamera_vid() and _pi_kamera_var() else 'usb'
        self.kaynak = kaynak
        self.resim_modu = resim_kaynagi(kaynak)
        self.canli = kaynak == 'pi' or kaynak == 'usb' or kaynak.startswith('usb:')
        if self.resim_modu:
            path = pathlib.Path(kaynak)
            self._dosyalar = sorted(p for p in path.iterdir() if p.is_file() and p.suffix.lower() in RESIM_UZANTILARI) if path.is_dir() else [path]
            if not self._dosyalar:
                raise RuntimeError('Klasörde desteklenen resim yok: ' + kaynak)
        elif kaynak == 'pi':
            self._pi_baslat()
        else:
            if self.canli:
                idx = int(kaynak.split(':')[1]) if ':' in kaynak else 0
                self.cap = cv2.VideoCapture(idx, cv2.CAP_V4L2)
                for prop, value in [(cv2.CAP_PROP_FRAME_WIDTH, genislik), (cv2.CAP_PROP_FRAME_HEIGHT, yukseklik),
                                    (cv2.CAP_PROP_FPS, fps), (cv2.CAP_PROP_BUFFERSIZE, 1)]:
                    self.cap.set(prop, value)
            else:
                if not os.path.isfile(kaynak):
                    raise RuntimeError('Kaynak bulunamadı: ' + kaynak)
                self.cap = cv2.VideoCapture(kaynak)
            if not self.cap.isOpened():
                self.cap.release()
                raise RuntimeError('Kamera veya video açılamadı: %s. Kamerayı kullanan diğer uygulamayı kapatın.' % kaynak)
        if self.canli:
            self._okuyucu = threading.Thread(target=self._oku, daemon=True)
            self._okuyucu.start()
        print('Kaynak:', kaynak)

    def _pi_baslat(self):
        tool = _libcamera_vid()
        if not tool:
            raise RuntimeError('libcamera-vid / rpicam-vid bulunamadı.')
        self.surec = subprocess.Popen([tool, '-t', '0', '-n', '--codec', 'mjpeg', '--width', str(self.genislik),
                                      '--height', str(self.yukseklik), '--framerate', str(self.fps), '--flush', '-o', '-'],
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self._tampon = bytearray()
        def drain():
            with self.surec.stderr as stderr:
                for line in stderr:
                    self._stderr.append(line.decode(errors='replace').strip()[-1000:])
        threading.Thread(target=drain, daemon=True).start()

    def _pi_kare(self):
        """Tampondaki en yeni tam JPEG'i çözer; analiz geride kaldıysa eski
        kareler çözülmeden atlanır (gecikme ve boşa işlemci harcanmaz)."""
        fd = self.surec.stdout.fileno()
        deadline = time.monotonic() + 10
        tampon = self._tampon
        while not self._stop.is_set():
            jpeg = None
            while True:
                bas = tampon.find(b'\xff\xd8')
                if bas < 0:
                    del tampon[:-1]
                    break
                son = tampon.find(b'\xff\xd9', bas + 2)
                if son < 0:
                    del tampon[:bas]
                    break
                jpeg = bytes(tampon[bas:son + 2])
                del tampon[:son + 2]
            if jpeg:
                return cv2.imdecode(np.frombuffer(jpeg, np.uint8), cv2.IMREAD_COLOR)
            if time.monotonic() > deadline:
                raise RuntimeError('Pi kamera 10 saniyedir kare göndermiyor.\n' + '\n'.join(self._stderr))
            if not select.select([fd], [], [], .5)[0]:
                continue
            parca = os.read(fd, 262144)
            if not parca:
                raise RuntimeError('Pi kamera akışı durdu.\n' + '\n'.join(self._stderr))
            tampon += parca
            if len(tampon) > 16 * 1024 * 1024:
                raise RuntimeError('Kamera JPEG tamponu sınırı aşıldı; çözünürlüğü düşürün.')
        return None

    def _boyutlandir(self, kare):
        h, w = kare.shape[:2]
        scale = min(self.genislik / w, self.yukseklik / h, 1)
        return cv2.resize(kare, (max(1, int(w * scale)), max(1, int(h * scale))), interpolation=cv2.INTER_AREA) if scale < 1 else kare

    def _oku(self):
        try:
            while not self._stop.is_set():
                if self.surec:
                    kare = self._pi_kare()
                else:
                    ok, kare = self.cap.read()
                    if not ok:
                        raise RuntimeError('USB kameradan görüntü alınamadı; bağlantıyı ve güç kaynağını kontrol edin.')
                if kare is None:
                    continue
                with self._kosul:
                    self._kare, self._sira = self._boyutlandir(kare), self._sira + 1
                    self._kosul.notify_all()
        except Exception as e:
            with self._kosul:
                self._hata = str(e)
                self._kosul.notify_all()
        finally:
            if self.cap:
                self.cap.release()
            if self.surec:
                self.surec.stdout.close()

    def __iter__(self):
        if self.resim_modu:
            for path in self._dosyalar:
                self.denetim.bekle()
                kare = cv2.imread(str(path))
                if kare is None:
                    raise RuntimeError('Resim okunamadı: ' + str(path))
                self.son_kaynak = str(path)
                yield self._boyutlandir(kare)
            return
        son = 0
        while not self._stop.is_set():
            self.denetim.bekle()
            if self.canli:
                with self._kosul:
                    ready = self._kosul.wait_for(lambda: self._sira != son or self._hata or self._stop.is_set(), timeout=12)
                    if self._hata:
                        raise RuntimeError(self._hata)
                    if not ready:
                        raise RuntimeError('Kameradan yeni kare alınamadı (12 saniye).')
                    if self._stop.is_set():
                        return
                    # Okuyucu her karede yeni dizi üretir ve eskisine dokunmaz;
                    # kopya gerekmez (çizimler yalnızca bu kareyi değiştirir).
                    kare, son = self._kare, self._sira
            else:
                ok, kare = self.cap.read()
                if not ok:
                    return
                kare = self._boyutlandir(kare)
            yield kare

    def kapat(self):
        self._stop.set()
        with self._kosul:
            self._kosul.notify_all()
        if self.surec:
            self.surec.terminate()
            try:
                self.surec.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.surec.kill()
                self.surec.wait()
        if self._okuyucu:
            self._okuyucu.join(timeout=3)
        elif self.cap:
            self.cap.release()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.kapat()


def _json_sonuc(value):
    if dataclasses.is_dataclass(value):
        return {f.name: _json_sonuc(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if isinstance(value, dict):
        return {k: _json_sonuc(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_sonuc(v) for v in value]
    if hasattr(value, 'numpy_view'):
        a = value.numpy_view()
        return {'shape': list(a.shape), 'min': float(a.min()), 'max': float(a.max())}
    if isinstance(value, np.generic):
        return value.item()
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def analiz(model, kare, kamera, tur='detect'):
    start = time.monotonic()
    method = getattr(model, tur if kamera.resim_modu else tur + '_for_video')
    image = mp_goruntu(kare)
    result = method(image) if kamera.resim_modu else method(image, zaman_ms())
    kamera.analiz_ms = round((time.monotonic() - start) * 1000, 1)
    kamera.sonuc = _json_sonuc(result) if kamera.resim_modu else None
    return result


class _Yayin(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        g = self.server.gosterici
        if self.path == '/durum':
            denetim = g.kamera.denetim if g.kamera else None
            body = json.dumps({'fps': round(g.fps, 1), 'hedef_fps': denetim.hedef_fps if denetim else None,
                               'sicaklik': denetim.sicaklik if denetim else None,
                               'durum': denetim.durum if denetim else 'normal'}, ensure_ascii=False).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path != '/akis':
            title = html.escape(g.baslik)
            body = ("<!doctype html><html lang=tr><meta name=viewport content='width=device-width,initial-scale=1'>"
                    "<meta charset=utf-8><title>%s</title><body style='margin:0;background:#111;color:#eee;font-family:sans-serif;text-align:center'>"
                    "<h3>%s</h3><img src='/akis' style='max-width:100%%;height:auto'><p id='durum'>Analiz hızı sıcaklığa göre otomatik azaltılır.</p>"
                    "<script>async function yenile(){try{const r=await fetch('/durum');const s=await r.json();document.getElementById('durum').textContent=s.durum+' · '+s.fps+' FPS · Hedef '+s.hedef_fps+(s.sicaklik==null?'':' · '+s.sicaklik+' °C');}catch{}setTimeout(yenile,2000)}yenile()</script></body></html>" % (title, title)).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if not g._izleyici.acquire(blocking=False):
            self.send_error(503, 'En fazla 4 izleyici')
            return
        self.connection.settimeout(10)
        with g.kosul:
            g.izleyici += 1
        try:
            self.send_response(200)
            self.send_header('Content-Type', 'multipart/x-mixed-replace; boundary=kare')
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            son = 0
            while not g.kapandi:
                with g.kosul:
                    ready = g.kosul.wait_for(lambda: g.sira != son or g.kapandi, timeout=5)
                    if g.kapandi:
                        break
                    if not ready:
                        continue
                    jpeg, son = g.jpeg, g.sira
                if jpeg:
                    self.wfile.write(b'--kare\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n' % len(jpeg) + jpeg + b'\r\n')
        except OSError:
            pass
        finally:
            with g.kosul:
                g.izleyici -= 1
            g._izleyici.release()


class Gosterici:
    def __init__(self, baslik, port=8000, pencere=True, kamera=None, cikti='sonuclar', kalite=65):
        self.baslik, self.kapandi, self.kamera = baslik, False, kamera
        self.cikti, self.kalite = cikti, kalite
        self.pencere = pencere and bool(os.environ.get('DISPLAY'))
        self.jpeg, self.sira, self.kosul = None, 0, threading.Condition()
        self.sunucu, self.izleyici, self._son_jpeg = None, 0, 0
        self._izleyici = threading.BoundedSemaphore(4)
        self._fps_t, self._fps_n, self.fps = time.monotonic(), 0, 0.
        if port and not (kamera and kamera.resim_modu):
            try:
                self.sunucu = ThreadingHTTPServer(('', port), _Yayin)
                self.sunucu.daemon_threads = True
                self.sunucu.gosterici = self
                threading.Thread(target=self.sunucu.serve_forever, daemon=True).start()
                print('Web: http://%s.local:%d/' % (socket.gethostname(), port))
            except OSError as e:
                raise RuntimeError('Web yayını açılamadı (port %s): %s. Başka port veya --port 0 kullanın.' % (port, e))

    def yaz(self, kare, metin, satir=0, renk=(255, 255, 255)):
        y = 30 + 30 * satir
        cv2.putText(kare, metin, (12, y), cv2.FONT_HERSHEY_SIMPLEX, .65, (0, 0, 0), 3, cv2.LINE_AA)
        cv2.putText(kare, metin, (12, y), cv2.FONT_HERSHEY_SIMPLEX, .65, renk, 1, cv2.LINE_AA)

    def goster(self, kare):
        now = time.monotonic()
        self._fps_n += 1
        if now - self._fps_t >= 1:
            self.fps = self._fps_n / (now - self._fps_t)
            self._fps_t, self._fps_n = now, 0
        denetim = self.kamera.denetim if self.kamera else None
        text = '%.1f FPS' % self.fps
        if self.kamera and self.kamera.resim_modu:
            text = 'Analiz: %.1f ms' % self.kamera.analiz_ms
        if denetim:
            text += ' / %.1f hedef' % denetim.hedef_fps
            if denetim.sicaklik is not None:
                text += ' | %.1f C' % denetim.sicaklik
        cv2.putText(kare, text, (12, kare.shape[0] - 12), cv2.FONT_HERSHEY_SIMPLEX, .45, (0, 255, 0), 1, cv2.LINE_AA)
        if self.kamera and self.kamera.resim_modu:
            os.makedirs(self.cikti, exist_ok=True)
            name = pathlib.Path(self.kamera.son_kaynak).stem + '-' + secrets.token_hex(4)
            path = os.path.join(self.cikti, name)
            if not cv2.imwrite(path + '.png', kare):
                raise RuntimeError('Analiz resmi kaydedilemedi: ' + path)
            with open(path + '.json', 'w', encoding='utf-8') as f:
                json.dump({'kaynak': self.kamera.son_kaynak, 'boyut': list(kare.shape[:2]),
                           'analiz_ms': self.kamera.analiz_ms, 'sonuc': self.kamera.sonuc}, f, ensure_ascii=False, indent=2)
            print('Analiz kaydedildi:', path + '.png', path + '.json')
        # İzleyici yoksa JPEG kodlama yapma; tüm izleyiciler aynı JPEG'i paylaşır.
        if self.sunucu and self.izleyici and now - self._son_jpeg >= .15:
            ok, buf = cv2.imencode('.jpg', kare, [cv2.IMWRITE_JPEG_QUALITY, self.kalite])
            if ok:
                with self.kosul:
                    self.jpeg, self.sira = buf.tobytes(), self.sira + 1
                    self.kosul.notify_all()
                self._son_jpeg = now
        if self.pencere:
            cv2.imshow(self.baslik, kare)
            if cv2.waitKey(1) & 0xFF in (ord('q'), 27):
                return False
        return True

    def kapat(self):
        self.kapandi = True
        with self.kosul:
            self.kosul.notify_all()
        if self.sunucu:
            self.sunucu.shutdown()
            self.sunucu.server_close()
        if self.pencere:
            cv2.destroyAllWindows()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.kapat()


def mp_goruntu(kare):
    import mediapipe as mp
    return mp.Image(image_format=mp.ImageFormat.SRGB, data=np.ascontiguousarray(cv2.cvtColor(kare, cv2.COLOR_BGR2RGB)))


_zaman_kilit = threading.Lock()
_son_zaman = 0


def zaman_ms():
    global _son_zaman
    with _zaman_kilit:
        _son_zaman = max(_son_zaman + 1, int(time.monotonic() * 1000))
        return _son_zaman
