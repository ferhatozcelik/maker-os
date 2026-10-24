"""Sabit aralıklı /proc örneklemesi; istemci sayısından bağımsız CPU/ağ ölçümü.

Örnekleyici ilk istekte başlar ve IDLE_STOP_SECS boyunca istek gelmezse durur;
Etkinlik sayfası kapalıyken işlemci /proc taramasıyla meşgul edilmez."""
import glob
import os
import pwd
import threading
import time

from common import read_first

PROC = "/proc"
NET = "/sys/class/net"
_lock = threading.Lock()
_start_lock = threading.Lock()
_thread = None
_EMPTY = {"ready": False, "processes": [], "interfaces": [], "error": ""}
_snapshot = _EMPTY
IDLE_STOP_SECS = 60
_last_request = time.time()
_users = {}   # uid -> kullanıcı adı (her örnekte süreç başına NSS sorgusu yapılmasın)


def _user(uid):
    name = _users.get(uid)
    if name is None:
        try:
            name = pwd.getpwuid(uid).pw_name
        except KeyError:
            name = str(uid)
        _users[uid] = name
    return name


def _cpu():
    fields = read_first(PROC + "/stat").splitlines()[0].split()[1:9]
    ticks = [int(x) for x in fields]
    return sum(ticks), ticks[3] + (ticks[4] if len(ticks) > 4 else 0)


def _memory():
    mem = {}
    for line in read_first(PROC + "/meminfo").splitlines():
        key, value = line.split(":", 1)
        mem[key] = int(value.split()[0]) * 1024
    total = mem.get("MemTotal", 0)
    used = max(0, total - mem.get("MemAvailable", mem.get("MemFree", 0)))
    return {"total": total, "used": used, "percent": round(100 * used / total, 1) if total else 0,
            "swap_total": mem.get("SwapTotal", 0), "swap_used": mem.get("SwapTotal", 0) - mem.get("SwapFree", 0)}


def _processes():
    result = {}
    page = os.sysconf("SC_PAGE_SIZE")
    for path in glob.glob(PROC + "/[0-9]*/stat"):
        try:
            raw = read_first(path)
            end = raw.rindex(")")
            fields = raw[end + 2:].split()  # field 3: state
            pid = int(path.split("/")[-2])
            user = _user(os.stat(os.path.dirname(path)).st_uid)
            # Komut argümanları parola içerebilir; yalnızca işlem adı gösterilir.
            result[pid] = {"pid": pid, "name": raw[raw.index("(") + 1:end], "user": user,
                           "state": fields[0], "ticks": int(fields[11]) + int(fields[12]),
                           "start": fields[19], "memory": max(0, int(fields[21])) * page}
        except (OSError, ValueError, IndexError):
            continue  # Okunurken sonlanan süreç.
    return result


def _interfaces():
    result = {}
    for line in read_first(PROC + "/net/dev").splitlines()[2:]:
        name, values = line.split(":", 1)
        name, values = name.strip(), values.split()
        if name == "lo":
            continue
        speed = read_first(NET + "/" + name + "/speed")
        result[name] = {"name": name, "rx": int(values[0]), "tx": int(values[8]),
                        "rx_errors": int(values[2]), "tx_errors": int(values[10]),
                        "connected": read_first(NET + "/" + name + "/carrier") == "1",
                        "state": read_first(NET + "/" + name + "/operstate"),
                        "link_mbps": int(speed) if speed.isdigit() and int(speed) > 0 else None}
    return result


def _loop():
    global _snapshot, _thread
    previous, last_cpu, last_procs, last_nets = None, None, {}, {}
    hz = os.sysconf("SC_CLK_TCK")
    while True:
        started = time.monotonic()
        try:
            cpu, mem, procs, nets = _cpu(), _memory(), _processes(), _interfaces()
            dt = started - previous if previous is not None else 0
            total_delta = cpu[0] - last_cpu[0] if last_cpu else 0
            busy = max(0, min(100, 100 * (1 - (cpu[1] - last_cpu[1]) / total_delta))) if total_delta > 0 else 0
            for pid, p in procs.items():
                old = last_procs.get(pid)
                p["cpu"] = round(max(0, (p["ticks"] - old["ticks"]) / hz / dt * 100), 1) if dt and old and old["start"] == p["start"] else 0
                p["memory_percent"] = round(p["memory"] / mem["total"] * 100, 1) if mem["total"] else 0
            for name, n in nets.items():
                old = last_nets.get(name)
                for direction in ("rx", "tx"):
                    n[direction + "_bps"] = round(max(0, n[direction] - old[direction]) / dt) if dt and old else 0
            temp = read_first("/sys/class/thermal/thermal_zone0/temp")
            snapshot = {"ready": previous is not None, "sampled": time.time(), "interval": round(dt, 2),
                        "cpu_percent": round(busy, 1), "cores": os.cpu_count() or 1,
                        "memory": mem, "temp_c": round(int(temp) / 1000, 1) if temp.isdigit() else None,
                        "uptime": int(float(read_first(PROC + "/uptime", "0").split()[0])),
                        "processes": list(procs.values()), "interfaces": list(nets.values()), "error": ""}
            last_procs = {pid: dict(p) for pid, p in procs.items()}
            for p in snapshot["processes"]:
                p.pop("ticks", None)
                p.pop("start", None)
            # Önceki örnekleri yanıt nesnesinden ayrı tut.
            last_cpu, last_nets, previous = cpu, nets, started
            with _lock:
                _snapshot = snapshot
        except Exception as e:
            with _lock:
                # Yayımlanan nesne değiştirilmez; okuyucular kopyasız kullanır.
                _snapshot = dict(_snapshot, error="Kaynak kullanımı okunamadı: %s" % e)
        with _start_lock:
            if time.time() - _last_request > IDLE_STOP_SECS:
                with _lock:
                    _snapshot = _EMPTY
                _thread = None
                return
        time.sleep(max(.2, 2 - (time.monotonic() - started)))


def snapshot():
    """Son örnek. Yayımlanan sözlük sonradan değiştirilmez; salt okunur kullanın."""
    global _thread, _last_request
    with _start_lock:
        _last_request = time.time()
        if _thread is None:
            _thread = threading.Thread(target=_loop, daemon=True)
            _thread.start()
    with _lock:
        return _snapshot
