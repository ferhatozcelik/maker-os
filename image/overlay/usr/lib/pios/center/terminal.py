# Web terminali: tarayicidaki xterm.js <-> WebSocket <-> PTY.
#
# Kabuk root olarak degil, masaustu kullanicisi (pardus) olarak giris
# kabugu (bash -l) acar; masaustu oturumunun DISPLAY/DBUS degiskenleri
# verildigi icin "code ." gibi komutlar Pi'nin ekraninda pencere acar.
#
# Guvenlik: WebSocket baglantisi icin once kimligi dogrulanmis bir istekle
# tek kullanimlik, 30 sn gecerli bilet alinir (/api/terminal/ticket);
# Origin basligi Host ile ayni olmalidir. Oturum sayisi ve bosta kalma
# suresi sinirlidir.
import base64
import fcntl
import hashlib
import json
import os
import pty
import secrets
import select
import signal
import socket
import struct
import sys
import termios
import threading
import time
from urllib.parse import urlparse

from common import USER, session_env, user_info

WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
MAX_SESSIONS = 4
IDLE_SECS = 2 * 3600
TICKET_SECS = 30

_lock = threading.Lock()
_tickets = {}
_sessions = 0


def log(msg):
    print("pios-terminal: " + msg, file=sys.stderr, flush=True)


def new_ticket():
    now = time.time()
    t = secrets.token_urlsafe(24)
    with _lock:
        for k in [k for k, exp in _tickets.items() if exp < now]:
            del _tickets[k]
        _tickets[t] = now + TICKET_SECS
    return t


def take_ticket(t):
    with _lock:
        exp = _tickets.pop(t, None) if t else None
    return exp is not None and exp > time.time()


# --- WebSocket (RFC 6455, yalnizca sunucu tarafi) ----------------------------

class WebSocket:
    def __init__(self, rfile, wfile):
        self.rfile, self.wfile = rfile, wfile
        self._wlock = threading.Lock()
        self.closed = False

    def _read(self, n):
        data = self.rfile.read(n)
        if len(data) != n:
            raise EOFError
        return data

    def send(self, data, opcode=2):
        n = len(data)
        if n < 126:
            head = struct.pack("!BB", 0x80 | opcode, n)
        elif n < 65536:
            head = struct.pack("!BBH", 0x80 | opcode, 126, n)
        else:
            head = struct.pack("!BBQ", 0x80 | opcode, 127, n)
        with self._wlock:
            if self.closed:
                return
            try:
                self.wfile.write(head + data)
            except OSError:
                self.closed = True

    def close(self):
        self.send(struct.pack("!H", 1000), opcode=8)
        self.closed = True

    def recv(self):
        """(opcode, veri) dondurur; baglanti kapaninca None."""
        msg, msg_op = b"", None
        try:
            while True:
                b0, b1 = self._read(2)
                fin, op, n = b0 & 0x80, b0 & 0x0F, b1 & 0x7F
                if n == 126:
                    n = struct.unpack("!H", self._read(2))[0]
                elif n == 127:
                    n = struct.unpack("!Q", self._read(8))[0]
                if n > 1 << 20:
                    return None
                mask = self._read(4) if b1 & 0x80 else None
                data = self._read(n) if n else b""
                if mask and n:
                    key = (mask * (n // 4 + 1))[:n]
                    data = (int.from_bytes(data, "big") ^ int.from_bytes(key, "big")).to_bytes(n, "big")
                if op == 8:
                    self.close()
                    return None
                if op == 9:
                    self.send(data, opcode=10)
                    continue
                if op == 10:
                    continue
                if op != 0:
                    msg_op = op
                msg += data
                if fin:
                    return msg_op, msg
        except (EOFError, OSError, ValueError):
            return None


def handshake(handler):
    """HTTP istegini WebSocket'e yukseltir; basarisizsa hata yanitini verir."""
    h = handler.headers
    key = h.get("Sec-WebSocket-Key", "")
    if h.get("Upgrade", "").lower() != "websocket" or not key:
        handler.send_json({"error": "WebSocket bekleniyor"}, 400)
        return False
    origin = h.get("Origin", "")
    if origin and urlparse(origin).netloc != h.get("Host", ""):
        handler.send_json({"error": "gecersiz Origin"}, 403)
        return False
    accept = base64.b64encode(hashlib.sha1((key + WS_GUID).encode()).digest()).decode()
    # BaseHTTPRequestHandler varsayilani HTTP/1.0; tarayicilar 101 yanitini
    # yalnizca HTTP/1.1 ile kabul eder.
    handler.protocol_version = "HTTP/1.1"
    handler.send_response(101)
    handler.send_header("Upgrade", "websocket")
    handler.send_header("Connection", "Upgrade")
    handler.send_header("Sec-WebSocket-Accept", accept)
    handler.end_headers()
    handler.wfile.flush()
    handler.close_connection = True
    return True


# --- PTY oturumu ---------------------------------------------------------------

def _spawn_shell():
    uid, gid, home = user_info()
    env = session_env()
    env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "SHELL": "/bin/bash",
                "PATH": "/usr/local/bin:/usr/bin:/bin:/usr/local/games:/usr/games"})
    pid, fd = pty.fork()
    if pid == 0:
        try:
            os.initgroups(USER, gid)
            os.setgid(gid)
            os.setuid(uid)
            os.chdir(home)
            os.execve("/bin/bash", ["-bash"], env)
        finally:
            os._exit(127)
    return pid, fd


def _resize(fd, cols, rows):
    if 0 < cols < 1000 and 0 < rows < 1000:
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def serve(handler):
    global _sessions
    with _lock:
        if _sessions >= MAX_SESSIONS:
            full = True
        else:
            full = False
            _sessions += 1
    if full:
        return handler.send_json({"error": "En fazla %d terminal oturumu acilabilir" % MAX_SESSIONS}, 429)
    try:
        if not handshake(handler):
            return
        # HTTP bosta kalma zaman asimi terminali kesmesin; bosta kalma
        # IDLE_SECS ile ayrica denetlenir.
        handler.connection.settimeout(None)
        ws = WebSocket(handler.rfile, handler.wfile)
        client = handler.client_address[0]
        pid, fd = _spawn_shell()
        log("oturum acildi (%s, pid %d)" % (client, pid))
        last_input = [time.time()]

        def pump():
            # PTY -> tarayici. Kabuk cikinca veya bosta kalinca baglanti kapanir.
            while not ws.closed:
                if time.time() - last_input[0] > IDLE_SECS:
                    ws.send(b"\r\n[Bosta kalma suresi doldu, oturum kapatildi]\r\n")
                    break
                r, _, _ = select.select([fd], [], [], 5)
                if not r:
                    continue
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    data = b""
                if not data:
                    break
                ws.send(data)
            ws.close()
            # Tarayici kapanis cercevesine yanit vermese de recv() donsun.
            time.sleep(1)
            try:
                handler.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

        t = threading.Thread(target=pump, daemon=True)
        t.start()
        while True:
            msg = ws.recv()
            if msg is None:
                break
            op, data = msg
            last_input[0] = time.time()
            if op == 2:
                os.write(fd, data)
            elif op == 1:
                try:
                    ctl = json.loads(data)
                except ValueError:
                    continue
                if ctl.get("type") == "resize":
                    _resize(fd, int(ctl.get("cols", 0)), int(ctl.get("rows", 0)))
        ws.closed = True
        try:
            os.killpg(os.getpgid(pid), signal.SIGHUP)
        except OSError:
            pass
        t.join(timeout=6)
        os.close(fd)
        for _ in range(20):
            if os.waitpid(pid, os.WNOHANG)[0]:
                break
            time.sleep(0.1)
        else:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        log("oturum kapandi (%s, pid %d)" % (client, pid))
    finally:
        with _lock:
            _sessions -= 1
