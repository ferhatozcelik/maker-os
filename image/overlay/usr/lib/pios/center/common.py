# Pardus Maker Merkezi ortak yardimcilari: komut calistirma, masaustu
# oturumunda kullanici olarak calistirma, arka plan isleri (jobs).
import itertools
import os
import pwd
import shutil
import subprocess
import threading
import time

USER = os.environ.get("PIOS_USER", "pardus")
# Alt surecler kullanici ortamiyla (PATH'inde sbin yok) baslatildigi icin
# runuser tam yoluyla cagrilir.
RUNUSER = shutil.which("runuser", path="/usr/sbin:/sbin:/usr/bin:/bin") or "/sbin/runuser"


def run(cmd, timeout=30, env=None, input=None):
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                           env=env, input=input)
        return p.returncode, p.stdout.strip(), p.stderr.strip()
    except subprocess.TimeoutExpired:
        # TimeoutExpired metni komut argümanlarını (Wi-Fi parolası dahil)
        # içerir. Kimlik bilgilerini hata yanıtına taşımayın.
        return 124, "", "İşlem %s saniyede tamamlanamadı (%s)." % (timeout, os.path.basename(cmd[0]))
    except OSError as e:
        return 1, "", "%s başlatılamadı: %s" % (os.path.basename(cmd[0]), e.strerror)


def read_first(path, default=""):
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return default


def user_info():
    pw = pwd.getpwnam(USER)
    return pw.pw_uid, pw.pw_gid, pw.pw_dir


_session_pid = [None]


def session_env():
    """Kullanicinin calisan masaustu oturumunun ortam degiskenleri.

    xfce4-session surecinin /proc/<pid>/environ dosyasindan DISPLAY,
    XAUTHORITY ve DBUS_SESSION_BUS_ADDRESS alinir; boylece hem Pi'deki
    LightDM oturumunda hem de Docker test konteynerinde calisir. Bulunan
    PID once denenir; /proc yalnizca oturum yeniden baslayinca taranir.
    """
    uid, _, home = user_info()
    env = {"HOME": home, "USER": USER, "LOGNAME": USER, "LANG": "tr_TR.UTF-8",
           "PATH": "/opt/pios/node/bin:%s/.local/bin:/usr/local/bin:/usr/bin:/bin" % home,
           "DISPLAY": ":0", "XDG_RUNTIME_DIR": "/run/user/%d" % uid}
    cached = _session_pid[0]
    for pid in itertools.chain([cached] if cached else [], os.listdir("/proc")):
        if not pid.isdigit():
            continue
        try:
            if os.stat("/proc/" + pid).st_uid != uid:
                continue
            if read_first("/proc/%s/comm" % pid) != "xfce4-session":
                continue
            try:
                with open("/proc/%s/environ" % pid, "rb") as f:
                    raw = f.read()
            except PermissionError:
                # CAP_SYS_PTRACE olmayan ortamlarda (Docker) ayni kullanici okur.
                raw = subprocess.run([RUNUSER, "-u", USER, "--", "cat", "/proc/%s/environ" % pid],
                                     capture_output=True, timeout=5).stdout
            for item in raw.split(b"\0"):
                k, _, v = item.decode(errors="replace").partition("=")
                if k in ("DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"):
                    env[k] = v
            _session_pid[0] = pid
            break
        except (OSError, subprocess.TimeoutExpired):
            continue
    bus = "/run/user/%d/bus" % uid
    if "DBUS_SESSION_BUS_ADDRESS" not in env and os.path.exists(bus):
        env["DBUS_SESSION_BUS_ADDRESS"] = "unix:path=" + bus
    return env


def as_user(cmd):
    """Komutu masaustu kullanicisi olarak calistirmak icin sarar."""
    return [RUNUSER, "-u", USER, "--"] + cmd


def run_user(cmd, timeout=30):
    return run(as_user(cmd), timeout=timeout, env=session_env())


def spawn_user(cmd):
    """Masaustunde kullanici olarak bir uygulama baslatir (beklemez)."""
    subprocess.Popen(as_user(cmd), env=session_env(), stdout=subprocess.DEVNULL,
                     stderr=subprocess.DEVNULL, start_new_session=True)


def open_terminal(command, title="Terminal", cwd=None):
    """Masaustunde komutu calistiran bir terminal acar; komut bitince
    kabuk acik kalir."""
    script = "%s; exec bash" % command if command else "exec bash"
    cmd = ["xfce4-terminal", "--title", title]
    if cwd:
        cmd += ["--working-directory", cwd]
    spawn_user(cmd + ["-x", "bash", "-lc", script])


class Jobs:
    """Uzun suren islemler (Python/agent kurulumu, guncelleme) icin arka
    plan isleri. Arayuz /api/jobs/<id> ile ciktiyi izler."""

    def __init__(self):
        self._jobs = {}
        self._ids = itertools.count(1)
        self._lock = threading.Lock()

    def start(self, title, cmd, env=None, user=False, after=None):
        jid = str(next(self._ids))
        job = {"id": jid, "title": title, "lines": [], "status": "running",
               "started": time.time(), "offset": 0}
        with self._lock:
            self._jobs[jid] = job
            # Eski bitmis isleri at.
            for k in [k for k, j in self._jobs.items()
                      if j["status"] != "running" and time.time() - j["started"] > 3600]:
                del self._jobs[k]
            finished = [k for k, j in self._jobs.items() if j["status"] != "running"]
            for k in finished[:-100]:
                del self._jobs[k]
        if user:
            cmd = as_user(cmd)
            env = dict(session_env(), **(env or {}))
        threading.Thread(target=self._run, args=(job, cmd, env, after), daemon=True).start()
        return jid

    def _run(self, job, cmd, env, after):
        try:
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                 text=True, env=env, bufsize=1)
            for line in p.stdout:
                with self._lock:
                    job["lines"].append(line.rstrip("\n")[-2000:])
                    if len(job["lines"]) > 2000:
                        del job["lines"][:500]
                        job["offset"] += 500
            rc = p.wait()
            if rc == 0 and after:
                after()
            with self._lock:
                job["status"] = "ok" if rc == 0 else "error"
        except Exception as e:
            with self._lock:
                job["lines"].append(str(e))
                job["status"] = "error"

    def get(self, jid, since=0):
        with self._lock:
            job = self._jobs.get(jid)
            if not job:
                return None
            offset = job["offset"]
            return {"id": job["id"], "title": job["title"], "status": job["status"],
                    "lines": job["lines"][max(0, since - offset):], "next": offset + len(job["lines"])}


jobs = Jobs()
