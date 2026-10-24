#!/usr/bin/python3
# Pardus Maker Merkezi (port 80): Ayarlar, Python Yoneticisi, Agent
# Yoneticisi, GPIO Pinleri, Kamera, USB aygitlari, web terminali ve uzak
# ekran icin tek web uygulamasi.
#
# Erisim:
#  - Pi'nin masaustunden: pios-open betigi /run/pios/token'i URL ile verir,
#    sunucu bunu HttpOnly/SameSite=Strict cerezine cevirir (parola sorulmaz).
#  - Agdan: ozel giris sayfasi (/api/login) ile sistem kullanicisi ve parolasi
#    (/etc/shadow); basarili giriste ayni pios_t cerezi verilir. Istemci
#    araclarina Basic kimlik de sessizce kabul edilir; WWW-Authenticate
#    gonderilmez (tarayici diyalogu acilmasin diye).
# Durum degistiren tum istekler POST + "X-Pios: 1" basligi ister (CSRF).
# Yalnizca Python standart kutuphanesi kullanir (Pardus 21: Python 3.9).
import base64
import crypt
import grp
import gzip
import hmac
import json
import mimetypes
import os
import secrets
import sys
import time
import logging
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import agents  # noqa: E402
import activity  # noqa: E402
import fan  # noqa: E402
import network  # noqa: E402
import ota  # noqa: E402
import camera  # noqa: E402
import devices  # noqa: E402
import gpio  # noqa: E402
import pyman  # noqa: E402
import plugins  # noqa: E402
import system  # noqa: E402
import terminal  # noqa: E402
from common import USER, jobs  # noqa: E402

PORT = int(os.environ.get("PIOS_CENTER_PORT", "80"))
WEB_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "web")
NOVNC_DIR = "/usr/share/novnc"
XTERM_DIR = "/usr/share/pios/xterm"
TOKEN_FILE = "/run/pios/token"


def make_token():
    token = secrets.token_urlsafe(24)
    os.makedirs(os.path.dirname(TOKEN_FILE), exist_ok=True)
    with open(TOKEN_FILE + ".tmp", "w") as f:
        f.write(token)
    try:
        os.chown(TOKEN_FILE + ".tmp", 0, grp.getgrnam(USER).gr_gid)
    except KeyError:
        pass
    os.chmod(TOKEN_FILE + ".tmp", 0o640)
    os.replace(TOKEN_FILE + ".tmp", TOKEN_FILE)
    return token


TOKEN = make_token()

# Bu boyuttan kucuk yanitlar sikistirilmaz (kazanc baslik maliyetinden az).
GZIP_MIN = 1024
COMPRESSIBLE = ("text/", "application/json", "application/javascript", "image/svg+xml", "application/xml")
# Statik dosyalar: yol -> ((mtime_ns, boyut), etag, ham, gzip). Dosya
# degisince anahtar tutmaz ve yeniden okunur.
_static = {}


def _compressible(ctype):
    return ctype.startswith(COMPRESSIBLE)


def _load_static(path, ctype):
    st = os.stat(path)
    key = (st.st_mtime_ns, st.st_size)
    hit = _static.get(path)
    if hit and hit[0] == key:
        return hit
    with open(path, "rb") as f:
        raw = f.read()
    gz = gzip.compress(raw, 9) if _compressible(ctype) and len(raw) >= GZIP_MIN else None
    entry = (key, '"%x-%x"' % key, raw, gz)
    _static[path] = entry
    return entry


def check_password(user, password):
    if user != USER:
        return False
    try:
        with open("/etc/shadow") as f:
            for line in f:
                parts = line.split(":")
                if parts[0] == user:
                    stored = parts[1]
                    if not stored or stored[0] in "!*":
                        return False
                    return hmac.compare_digest(crypt.crypt(password, stored), stored)
    except OSError:
        pass
    return False


def _int(v):
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


class Handler(BaseHTTPRequestHandler):
    server_version = "PardusMakerMerkezi"
    _body_read = True
    # Kalici baglanti: sayfa ve yoklama istekleri ayni TCP baglantisini (ve
    # is parcacigini) kullanir. Bosta kalan baglanti 60 sn sonra kapanir.
    protocol_version = "HTTP/1.1"
    timeout = 60

    def log_message(self, fmt, *args):
        pass

    # --- yardimcilar ------------------------------------------------------

    def _cookie_token(self):
        for part in self.headers.get("Cookie", "").split(";"):
            k, _, v = part.strip().partition("=")
            if k == "pios_t":
                return v
        return ""

    def try_auth(self, query):
        """Kimligi dogrula; ?t= varsa cerez yazmak icin new_cookie ayarla. Yanit gondermez."""
        self.new_cookie = None
        self.clear_cookie = False
        if query.get("t") and hmac.compare_digest(query["t"][0], TOKEN):
            self.new_cookie = TOKEN
            return True
        cookie = self._cookie_token()
        if cookie and hmac.compare_digest(cookie, TOKEN):
            return True
        header = self.headers.get("Authorization", "")
        if header.startswith("Basic "):
            try:
                user, _, pw = base64.b64decode(header[6:]).decode().partition(":")
            except (ValueError, UnicodeDecodeError):
                user, pw = "", ""
            if check_password(user, pw):
                return True
            time.sleep(1)
        return False

    def deny(self):
        self.send_json({"error": "giris gerekli"}, 401)

    def _cookie_headers(self):
        if getattr(self, "new_cookie", None):
            self.send_header("Set-Cookie", "pios_t=%s; HttpOnly; SameSite=Strict; Path=/" % self.new_cookie)
        elif getattr(self, "clear_cookie", False):
            self.send_header("Set-Cookie", "pios_t=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0")

    def _accepts_gzip(self):
        return "gzip" in self.headers.get("Accept-Encoding", "")

    def send_body(self, body, ctype, code=200, cache=False, gz=None, etag=None):
        """cache: False (no-store), True (1 saat) veya Cache-Control metni.
        gz: onceden sikistirilmis govde (statik dosyalar)."""
        compressible = _compressible(ctype)
        if compressible and self._accepts_gzip():
            if gz is None and len(body) >= GZIP_MIN:
                gz = gzip.compress(body, 5)
        else:
            gz = None
        if gz is not None and len(gz) >= len(body):
            gz = None
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        if gz is not None:
            body = gz
            self.send_header("Content-Encoding", "gzip")
        if compressible:
            self.send_header("Vary", "Accept-Encoding")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", cache if isinstance(cache, str) else "max-age=3600" if cache else "no-store")
        if etag:
            self.send_header("ETag", etag)
        self._cookie_headers()
        # Govdesi okunmadan yanitlanan istekte (401, 403, 413) kalan baytlar
        # kalici baglantida sonraki istek gibi okunmasin: istemciye bildirip kapat.
        if not self._body_read and self.headers.get("Content-Length", "0") not in ("", "0"):
            self.close_connection = True
        if getattr(self, "close_connection", False):
            self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def _read_json(self, maximum=1048576):
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 <= length <= maximum:
                self.send_json({"error": "İstek boyutu sınırı aşıldı (%s MB)." % (maximum // 1048576)}, 413)
                return None
            self.connection.settimeout(30)
            raw = self.rfile.read(length)
            self._body_read = True
            data = json.loads(raw or b"{}")
            if not isinstance(data, dict):
                raise ValueError("JSON object required")
            return data
        except ValueError:
            self.send_json({"error": "gecersiz JSON"}, 400)
            return None

    def handle_login(self):
        if self.headers.get("X-Pios") != "1":
            return self.send_json({"error": "gecersiz istek"}, 403)
        data = self._read_json()
        if data is None:
            return
        user = str(data.get("username", "")).strip()
        password = str(data.get("password", ""))
        if check_password(user, password):
            self.new_cookie = TOKEN
            return self.send_json({"ok": True, "user": USER})
        time.sleep(1)
        return self.send_json({"ok": False, "message": "Kullanıcı adı veya parola hatalı."}, 401)

    def handle_logout(self):
        self.clear_cookie = True
        return self.send_json({"ok": True})

    def send_json(self, obj, code=200):
        self.send_body(json.dumps(obj).encode(), "application/json; charset=utf-8", code)

    def send_file(self, base, rel):
        path = os.path.realpath(os.path.join(base, rel.lstrip("/")))
        if not path.startswith(os.path.realpath(base) + os.sep) or not os.path.isfile(path):
            return self.send_json({"error": "bulunamadi"}, 404)
        ctype = mimetypes.guess_type(path)[0] or "application/octet-stream"
        if path.endswith(".js"):
            ctype = "text/javascript"
        _, etag, raw, gz = _load_static(path, ctype)
        # Merkez arayuzu her acilista dogrulanir (guncelleme hemen gorunur;
        # degismediyse 304 ile govde gonderilmez). noVNC/xterm sabit surumdur.
        cache = "max-age=3600" if base in (NOVNC_DIR, XTERM_DIR) else "no-cache"
        if etag in self.headers.get("If-None-Match", ""):
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Cache-Control", cache)
            self._cookie_headers()
            return self.end_headers()
        self.send_body(raw, ctype, cache=cache, gz=gz, etag=etag)

    def send_stream(self, frames):
        """MJPEG canli goruntu: her kare multipart'in bir parcasi."""
        self.connection.settimeout(20)
        self.send_response(200)
        self.send_header("Content-Type", "multipart/x-mixed-replace; boundary=piosframe")
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Accel-Buffering", "no")
        self.end_headers()
        self.close_connection = True
        try:
            for frame in frames:
                self.wfile.write(b"--piosframe\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n"
                                 % len(frame) + frame + b"\r\n")
        except OSError:
            pass  # tarayici sekmeyi kapatti
        finally:
            if hasattr(frames, "close"):
                frames.close()

    def job_reply(self, jid, err=""):
        if not jid:
            return self.send_json({"error": err or "baslatilamadi"}, 400)
        return self.send_json({"job": jid})

    def ota_action(self, what):
        """Pardus Maker surum guncellemesi. Kurulum ve geri alma Merkez'i
        yeniden baslattigi icin ayri surecte calisir; arayuz /api/ota'yi yoklar."""
        st = ota.status()["status"]
        try:
            if what == "check":
                return self.send_json(ota.check())
            if what == "start":
                if st != "available":
                    return self.result(False, "Kurulacak yeni sürüm yok.")
                ota.save_state(status="running", stage="Başlatılıyor…", progress=1, message="", started=time.time())
                ota.spawn("apply")
                return self.result(True, "Güncelleme başladı")
            if what == "confirm":
                ota.confirm()
                return self.result(True, "Güncelleme onaylandı; yedek ve geçici dosyalar silindi.")
            if what == "rollback":
                if st not in ("confirm", "reboot") or ota.load_state().get("rolling"):
                    return self.result(False, "Geri alınacak güncelleme yok.")
                ota.save_state(rolling=True, stage="Geri alınıyor…", started=time.time())
                ota.spawn("rollback")
                return self.result(True, "Geri alınıyor")
            if what == "reboot":
                ota.reboot_trial()
                return self.result(True, "Yeni çekirdekle yeniden başlatılıyor…")
        except ota.OtaError as e:
            return self.result(False, str(e))
        return self.send_json({"error": "bulunamadi"}, 404)

    def result(self, ok, msg=""):
        return self.send_json({"ok": bool(ok), "message": msg}, 200 if ok else 400)

    # --- GET --------------------------------------------------------------

    def do_GET(self):
        self._body_read = True
        self._guard(self._get)

    def do_POST(self):
        self._body_read = False
        self._guard(self._post)

    def _guard(self, action):
        try:
            action()
        except (BrokenPipeError, ConnectionResetError):
            self.close_connection = True
        except Exception as e:
            # Yanit yarida kalmis olabilir; baglanti yeniden kullanilmasin.
            self.close_connection = True
            ref = secrets.token_hex(4)
            logging.exception("Merkez istek hatası %s: %s", ref, urlparse(self.path).path)
            try:
                self.send_json({"error": "İstek tamamlanamadı.", "details": str(e),
                                "reference": ref, "path": urlparse(self.path).path}, 500)
            except OSError:
                pass

    def _get(self):
        url = urlparse(self.path)
        query = parse_qs(url.query)
        p = url.path
        # WebSocket: tarayicilar Basic kimligini WS istegine guvenilir sekilde
        # eklemedigi icin tek kullanimlik bilet (/api/terminal/ticket) kullanilir.
        if p == "/api/terminal/ws":
            if not terminal.take_ticket(query.get("ticket", [""])[0]):
                return self.send_json({"error": "gecersiz bilet"}, 403)
            return terminal.serve(self)
        # ?t= ile masaustu acilisi cerezi burada yazilir; sayfa herkese aciktir.
        authed = self.try_auth(query)
        if p in ("/", "/index.html"):
            return self.send_file(WEB_DIR, "index.html")
        if p.startswith("/static/"):
            return self.send_file(WEB_DIR, p[len("/static/"):])
        if not authed:
            return self.deny()
        if p.startswith("/novnc/"):
            return self.send_file(NOVNC_DIR, p[len("/novnc/"):])
        if p.startswith("/xterm/"):
            return self.send_file(XTERM_DIR, p[len("/xterm/"):])

        routes = {
            # client_local: istek Pi'nin kendisinden mi (uzak ekran gereksiz).
            "/api/status": lambda: dict(system.status(), user=USER,
                                        client_local=self.client_address[0] in ("127.0.0.1", "::1")),
            "/api/wifi": lambda: network.operation(),
            "/api/network/operation": lambda: network.operation(),
            "/api/network/diagnostics": lambda: network.diagnostics(),
            "/api/activity": lambda: activity.snapshot(),
            "/api/fan": lambda: fan.status(),
            "/api/ota": lambda: ota.status(),
            "/api/plugins": lambda: plugins.list_plugins(),
            "/api/settings": lambda: {"theme": system.get_theme(), "hardware": system.hardware(),
                                      "remote": system.remote_enabled()},
            "/api/time": lambda: system.time_status(),
            "/api/language": lambda: system.language_status(),
            "/api/vnc": lambda: {"password": system.vnc_password(), "port": 6080},
            "/api/python/versions": lambda: {"versions": pyman.versions(),
                                             "default": pyman.default_version()},
            "/api/python/projects": lambda: {"projects": pyman.projects(),
                                             "dir": pyman.projects_dir(),
                                             "templates": {k: v["title"] for k, v in pyman.TEMPLATES.items()}},
            "/api/python/ai": lambda: pyman.ai_env(),
            "/api/agents": lambda: {"agents": agents.list_agents(), "node": agents.node_version()},
            "/api/gpio": lambda: gpio.state(),
            "/api/network": lambda: dict(network.overview(), ethernet=system.ethernet(), internet=system.internet()),
            "/api/devices": lambda: {"usb": devices.usb_devices(), "storage": devices.storage(), "user": USER,
                                     "serial": devices.serial_ports()},
            "/api/displays": lambda: {"displays": devices.displays(), "current": devices.current_modes(),
                                      "audio": devices.audio()},
            "/api/camera": lambda: dict(camera.status(), photos=camera.photos()),
            "/api/terminal/ticket": lambda: {"ticket": terminal.new_ticket(),
                                             "ready": os.path.isfile(os.path.join(XTERM_DIR, "xterm.js"))},
        }
        if p in routes:
            return self.send_json(routes[p]())
        if p == "/api/plugins/template.zip":
            return self.send_body(plugins.template(), "application/zip")
        if p == "/api/plugins/logs":
            return self.send_json(plugins.logs(query.get("id", [""])[0]))
        if p.startswith("/api/jobs/"):
            job = jobs.get(p.rsplit("/", 1)[-1], _int(query.get("since", ["0"])[0]) or 0)
            return self.send_json(job) if job else self.send_json({"error": "is yok"}, 404)
        if p == "/api/camera/stream":
            cam = camera.find(query.get("cam", [""])[0])
            if not cam:
                return self.send_json({"error": "kamera bulunamadi"}, 404)
            return self.send_stream(camera.manager.frames(cam))
        if p.startswith("/api/camera/photo/"):
            path = camera.photo_path(p.rsplit("/", 1)[-1])
            if not path:
                return self.send_json({"error": "bulunamadi"}, 404)
            thumb = camera.thumbnail(path) if query.get("thumb") else None
            if thumb is None:
                with open(path, "rb") as f:
                    thumb = f.read()
            if query.get("download"):
                self.send_response(200)
                self.send_header("Content-Type", "image/jpeg")
                self.send_header("Content-Length", str(len(thumb)))
                self.send_header("Content-Disposition", 'attachment; filename="%s"' % os.path.basename(path))
                self.end_headers()
                return self.wfile.write(thumb)
            return self.send_body(thumb, "image/jpeg", cache=True)
        if p == "/api/gpio/i2c":
            found, err = gpio.i2c_scan()
            return self.send_json({"devices": found}) if found is not None else self.send_json({"error": err}, 400)
        self.send_json({"error": "bulunamadi"}, 404)

    # --- POST -------------------------------------------------------------

    def _post(self):
        url = urlparse(self.path)
        p = url.path
        if p == "/api/login":
            return self.handle_login()
        if not self.try_auth(parse_qs(url.query)):
            return self.deny()
        if self.headers.get("X-Pios") != "1":
            return self.send_json({"error": "gecersiz istek"}, 403)
        if p == "/api/logout":
            return self.handle_logout()
        maximum = 22 * 1024 * 1024 if p == "/api/plugins/import" else 1048576
        d = self._read_json(maximum)
        if d is None:
            return
        s = lambda k, default="": str(d.get(k, default)).strip()  # noqa: E731

        # Ayarlar
        if p == "/api/plugins/import":
            return self.result(*plugins.import_zip(d.get("archive", "")))
        if p == "/api/plugins/action":
            return self.result(*plugins.action(s("id"), s("action")))
        if p == "/api/wifi/connect":
            return self.result(*network.start("connect", d))
        if p == "/api/wifi/scan":
            return self.result(*network.start("scan", d))
        if p == "/api/wifi/forget":
            return self.result(*network.start("forget", d))
        if p == "/api/hotspot":
            return self.result(*network.start("hotspot", d))
        if p == "/api/fan":
            return self.result(*fan.save(d))
        if p == "/api/settings/theme":
            return self.result(*system.set_theme(s("mode")))
        if p == "/api/language":
            return self.result(*system.set_language(s("locale")))
        if p == "/api/keyboard":
            return self.result(*system.set_keyboard(s("keyboard"), bool(d.get("switch"))))
        if p == "/api/time/timezone":
            return self.result(*system.set_timezone(s("timezone")))
        if p == "/api/time/ntp":
            return self.result(*system.set_ntp(bool(d.get("enabled"))))
        if p == "/api/time/sync":
            return self.result(*system.sync_time())
        if p == "/api/time/set":
            return self.result(*system.set_time(d.get("epoch")))
        if p == "/api/settings/hostname":
            return self.result(*system.set_hostname(s("name")))
        if p == "/api/settings/password":
            return self.result(*system.set_password(str(d.get("password", ""))))
        if p == "/api/settings/resolution":
            return self.result(*system.set_resolution(s("resolution")))
        if p == "/api/settings/interface":
            return self.result(*system.set_interface(s("key"), bool(d.get("enabled"))))
        if p == "/api/settings/remote":
            return self.result(*system.set_remote(bool(d.get("enabled"))))
        if p == "/api/updates/check":
            return self.job_reply(system.updates_check())
        if p == "/api/updates/install":
            return self.job_reply(system.updates_install())
        if p.startswith("/api/ota/"):
            return self.ota_action(p[len("/api/ota/"):])
        if p == "/api/power":
            return self.result(system.power(s("action")))
        if p == "/api/desktop/restart":
            return self.result(*system.restart_desktop())

        # Aygitlar ve kamera
        if p == "/api/devices/mount":
            return self.result(*devices.mount(s("path")))
        if p == "/api/devices/eject":
            return self.result(*devices.eject(s("path")))
        if p == "/api/devices/open":
            return self.result(*devices.open_folder(s("path")))
        if p == "/api/camera/photo":
            ok, msg, name = camera.take_photo(s("cam"), s("name"))
            return self.send_json({"ok": ok, "message": msg, "name": name}, 200 if ok else 400)
        if p == "/api/camera/delete":
            return self.result(*camera.delete_photo(s("name")))
        if p == "/api/camera/settings":
            return self.result(*camera.save_settings(s("size"), bool(d.get("rotate"))))
        if p == "/api/camera/model":
            return self.result(*camera.set_model(s("model")))

        # Python
        if p == "/api/python/install":
            return self.job_reply(pyman.install(s("minor")))
        if p == "/api/python/uninstall":
            return self.job_reply(*pyman.uninstall(s("minor")))
        if p == "/api/python/default":
            return self.result(*pyman.set_default(s("minor")))
        if p == "/api/python/projects/create":
            return self.job_reply(*pyman.create_project(s("name"), s("python"), s("template", "bos")))
        if p == "/api/python/projects/add":
            return self.job_reply(*pyman.add_package(s("name"), s("package")))
        if p == "/api/python/projects/open":
            return self.result(pyman.open_project(s("name"), s("how")))
        if p == "/api/python/projects/delete":
            return self.result(pyman.delete_project(s("name")))
        if p == "/api/python/ai/open":
            return self.result(pyman.ai_open(s("how")))

        # Agent'lar
        if p == "/api/agents/action":
            return self.job_reply(*agents.action(s("id"), s("action")))
        if p == "/api/agents/launch":
            proj = pyman._project_path(s("project")) if s("project") else None
            return self.result(agents.launch(s("id"), proj))

        # GPIO
        if p == "/api/gpio/set":
            bcm = _int(d.get("bcm"))
            mode = s("mode")
            if mode == "out":
                return self.result(*gpio.set_output(bcm, bool(d.get("value"))))
            if mode == "in":
                return self.result(*gpio.set_input(bcm, s("bias", "as-is")))
            return self.result(gpio.release(bcm))

        self.send_json({"error": "bulunamadi"}, 404)


if __name__ == "__main__":
    ThreadingHTTPServer.allow_reuse_address = True
    ThreadingHTTPServer.daemon_threads = True
    ThreadingHTTPServer(("", PORT), Handler).serve_forever()
