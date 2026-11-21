#!/usr/bin/env python3
"""MakerOS Bench — tek dosyalık, bağımlılıksız sistem karşılaştırma testi.

macOS, Linux, Raspberry Pi OS ve MakerOS'ta çalışır (Python 3.8+; yalnız standart kitaplık).

    curl -fsSLO https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/benchmark/makeros-bench.py
    python3 makeros-bench.py                  # tam test (~2 dk), rapor: ./bench-results/
    python3 makeros-bench.py --quick          # hızlı (~40 sn)
    python3 makeros-bench.py --name "Pi 4 4GB · Raspberry Pi OS"   # listede görünecek ad

Ölçülenler
  CPU tek çekirdek  tamsayı, kayan nokta, SHA-256, zlib, JSON, düzenli ifade
  CPU çok çekirdek  aynı tamsayı/kayan nokta yükü tüm çekirdeklerde (ölçeklenme oranı)
  Bellek            büyük blok kopyalama (GB/s), nesne ayırma
  Disk              sıralı yazma/okuma (MB/s, fsync), rastgele 4K yazma (fsync) ve okuma (IOPS)

Puan: her test, sabit bir referansa göre ölçeklenir (REFERANS = 1000 puan; referans makine Apple M1 Pro,
macOS, Python 3.9 ilk ölçümüdür). Kategori puanı testlerin, genel puan kategorilerin geometrik ortalamasıdır.
Daha yüksek = daha iyi. Python ile ölçüldüğü için mutlak donanım değil, "bu sistemde Python iş yükü" başarımıdır;
aynı betik sürümüyle alınan sonuçlar birbiriyle karşılaştırılabilir.

Çıktı: bench-results/<tarih>-<sistem>.md (okunur rapor) ve .json (maker-os deposunda benchmark/results/ altına gönderilecek veri).
Kimlik bilgisi toplanmaz: makine adı, kullanıcı adı, IP, seri numarası rapora girmez.
"""
import argparse
import datetime as _dt
import hashlib
import json
import math
import multiprocessing as mp
import os
import platform
import random
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import zlib

VERSION = "1.0.0"
SCHEMA = 1

# Referans değerler (= 1000 puan): Apple M1 Pro · macOS · Python 3.9, betiğin ilk ölçümü (yuvarlanmış).
# Sürüm boyunca sabittir; değişirse VERSION'ın ana sürümü artar ve eski sonuçlarla karşılaştırma ayrılır.
REFERENCE = {
    "cpu.int": 5.9,        # milyon işlem/sn
    "cpu.float": 6.0,       # milyon piksel-yineleme/sn (Mandelbrot)
    "cpu.sha256": 2400.0,   # MB/s
    "cpu.zlib": 53.0,       # MB/s (seviye 6)
    "cpu.json": 11.3,       # bin döngü/sn
    "cpu.regex": 30.0,     # MB/s
    "multi.int": 34.0,     # milyon işlem/sn (tüm çekirdekler)
    "multi.float": 35.0,    # milyon piksel-yineleme/sn (tüm çekirdekler)
    "mem.copy": 44.0,       # GB/s
    "mem.alloc": 1.48,       # milyon nesne/sn
    "disk.seq_write": 3500.0,  # MB/s
    "disk.seq_read": 2500.0,   # MB/s
    "disk.rand_write": 205.0,  # IOPS (4K + fsync)
    "disk.rand_read": 12000.0,  # IOPS (4K)
}
CATEGORIES = {
    "cpu": ["cpu.int", "cpu.float", "cpu.sha256", "cpu.zlib", "cpu.json", "cpu.regex"],
    "multi": ["multi.int", "multi.float"],
    "mem": ["mem.copy", "mem.alloc"],
    "disk": ["disk.seq_write", "disk.seq_read", "disk.rand_write", "disk.rand_read"],
}
CAT_NAMES = {"cpu": "CPU tek çekirdek", "multi": "CPU çok çekirdek", "mem": "Bellek", "disk": "Disk"}
TESTS = {
    "cpu.int": ("Tamsayı", "M işlem/sn"),
    "cpu.float": ("Kayan nokta (Mandelbrot)", "M yineleme/sn"),
    "cpu.sha256": ("SHA-256", "MB/s"),
    "cpu.zlib": ("zlib sıkıştırma", "MB/s"),
    "cpu.json": ("JSON kodla/çöz", "bin/sn"),
    "cpu.regex": ("Düzenli ifade", "MB/s"),
    "multi.int": ("Tamsayı · tüm çekirdekler", "M işlem/sn"),
    "multi.float": ("Kayan nokta · tüm çekirdekler", "M yineleme/sn"),
    "mem.copy": ("Bellek kopyalama", "GB/s"),
    "mem.alloc": ("Nesne ayırma", "M nesne/sn"),
    "disk.seq_write": ("Sıralı yazma (fsync)", "MB/s"),
    "disk.seq_read": ("Sıralı okuma", "MB/s"),
    "disk.rand_write": ("Rastgele 4K yazma (fsync)", "IOPS"),
    "disk.rand_read": ("Rastgele 4K okuma", "IOPS"),
}


# --- yardımcılar -------------------------------------------------------------

def log(msg, end="\n"):
    sys.stderr.write(msg + end)
    sys.stderr.flush()


def run(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=5).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def read(path):
    try:
        with open(path, "rb") as f:
            return f.read().decode(errors="replace").strip("\x00\n ")
    except OSError:
        return ""


def best_of(fn, repeats):
    """fn() → (iş miktarı, süre sn); en iyi hızı döner (ısınma/gürültü etkisini azaltır)."""
    rates = []
    for _ in range(repeats):
        work, dt = fn()
        rates.append(work / dt if dt > 0 else 0.0)
    return max(rates)


def geomean(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else 0.0


# --- sistem bilgisi (kimlik bilgisi yok) ------------------------------------------

def system_info():
    info = {"python": platform.python_version(), "python_impl": platform.python_implementation(),
            "arch": platform.machine(), "kernel": platform.release(), "cores": os.cpu_count() or 1}
    sysname = platform.system()
    if sysname == "Darwin":
        info["os"] = f"macOS {platform.mac_ver()[0]}"
        info["os_id"] = "macos"
        info["cpu"] = run(["sysctl", "-n", "machdep.cpu.brand_string"]) or platform.processor()
        info["model"] = run(["sysctl", "-n", "hw.model"])
        try:
            info["ram_gb"] = round(int(run(["sysctl", "-n", "hw.memsize"])) / 2**30, 1)
        except ValueError:
            pass
        perf = run(["sysctl", "-n", "hw.perflevel0.physicalcpu"])
        eff = run(["sysctl", "-n", "hw.perflevel1.physicalcpu"])
        if perf.isdigit():
            info["core_layout"] = f"{perf} performans + {eff or 0} verimlilik"
    elif sysname == "Linux":
        osr = {}
        for line in read("/etc/os-release").splitlines():
            k, _, v = line.partition("=")
            osr[k] = v.strip('"')
        info["os"] = osr.get("PRETTY_NAME") or "Linux"
        info["os_id"] = osr.get("ID", "linux")
        cpuinfo = read("/proc/cpuinfo")
        m = re.search(r"^(?:model name|Hardware|Processor)\s*:\s*(.+)$", cpuinfo, re.M)
        info["cpu"] = m.group(1).strip() if m else platform.processor() or platform.machine()
        model = read("/proc/device-tree/model") or read("/sys/firmware/devicetree/base/model")
        if model:
            info["model"] = model  # ör. "Raspberry Pi 4 Model B Rev 1.5"
        m = re.search(r"^MemTotal:\s*(\d+)", read("/proc/meminfo"), re.M)
        if m:
            info["ram_gb"] = round(int(m.group(1)) / 2**20, 1)
        freq = read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        if freq.isdigit():
            info["cpu_max_mhz"] = int(freq) // 1000
        gov = read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        if gov:
            info["governor"] = gov
    else:
        info["os"] = f"{sysname} {platform.release()}"
        info["os_id"] = sysname.lower()
        info["cpu"] = platform.processor()
    return info


def thermal():
    """Sıcaklık (°C) ve Raspberry Pi kısma durumu; yoksa boş."""
    out = {}
    t = read("/sys/class/thermal/thermal_zone0/temp")
    if t.lstrip("-").isdigit():
        out["temp_c"] = round(int(t) / 1000, 1)
    if shutil.which("vcgencmd"):
        m = re.search(r"0x([0-9a-fA-F]+)", run(["vcgencmd", "get_throttled"]))
        if m:
            out["throttled"] = "0x" + m.group(1)
    return out


# --- CPU iş yükleri (alt süreçlerde de kullanılır: modül düzeyinde) -----------------

def work_int(n):
    """Tamsayı: LCG + bit işlemleri (yorumlayıcı + ALU)."""
    x, acc = 12345, 0
    for i in range(n):
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF
        acc ^= (x >> 3) + (i & 0xFF)
    return acc


def work_float(size):
    """Kayan nokta: Mandelbrot (size × size, 64 yineleme) → toplam yineleme."""
    total = 0
    for py in range(size):
        ci = py * 2.4 / size - 1.2
        for px in range(size):
            cr = px * 3.0 / size - 2.1
            zr = zi = 0.0
            for _ in range(64):
                zr2, zi2 = zr * zr, zi * zi
                if zr2 + zi2 > 4.0:
                    break
                zi = 2.0 * zr * zi + ci
                zr = zr2 - zi2 + cr
                total += 1
    return total


def _timed_int(n):
    t = time.perf_counter()
    work_int(n)
    return n, time.perf_counter() - t


def _timed_float(size):
    t = time.perf_counter()
    it = work_float(size)
    return it, time.perf_counter() - t


def bench_cpu(scale, repeats):
    r = {}
    n = int(4_000_000 * scale)
    r["cpu.int"] = best_of(lambda: _timed_int(n), repeats) / 1e6
    size = int(260 * math.sqrt(scale))
    r["cpu.float"] = best_of(lambda: _timed_float(size), repeats) / 1e6

    blob = os.urandom(1 << 20) * int(64 * scale)  # ~64 MB
    def sha():
        t = time.perf_counter()
        hashlib.sha256(blob).digest()
        return len(blob) / 1e6, time.perf_counter() - t
    r["cpu.sha256"] = best_of(sha, repeats)

    rnd = random.Random(1)
    words = [("".join(rnd.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(rnd.randint(3, 9)))) for _ in range(5000)]
    text = (" ".join(rnd.choice(words) for _ in range(int(1_500_000 * scale)))).encode()  # ~9 MB yarı-yinelenen metin
    def zl():
        t = time.perf_counter()
        zlib.compress(text, 6)
        return len(text) / 1e6, time.perf_counter() - t
    r["cpu.zlib"] = best_of(zl, repeats)

    doc = {"cihazlar": [{"id": i, "ad": f"sensör-{i}", "değer": i * 1.5, "etiketler": ["a", "b", str(i)], "açık": i % 2 == 0} for i in range(50)]}
    loops = int(4000 * scale)
    def js():
        t = time.perf_counter()
        for _ in range(loops):
            json.loads(json.dumps(doc))
        return loops / 1e3, time.perf_counter() - t
    r["cpu.json"] = best_of(js, repeats)

    s = text.decode()
    pat = re.compile(r"\b(\w{3})(\w*)\1\b|\b[aeiou]\w{4,}\b")
    def rx():
        t = time.perf_counter()
        sum(1 for _ in pat.finditer(s))
        return len(s) / 1e6, time.perf_counter() - t
    r["cpu.regex"] = best_of(rx, repeats)
    return r


def bench_multi(scale, repeats, cores):
    """Tüm çekirdekler: çekirdek başına bir süreç, aynı yük; toplam iş / duvar saati süresi."""
    r = {}
    n = int(4_000_000 * scale)
    size = int(260 * math.sqrt(scale))
    ctx = mp.get_context("spawn")  # macOS/Linux aynı davranış
    with ctx.Pool(cores) as pool:
        pool.map(work_int, [1000] * cores)  # süreçleri ısıt

        def multi_int():
            t = time.perf_counter()
            pool.map(work_int, [n] * cores)
            return n * cores, time.perf_counter() - t

        def multi_float():
            t = time.perf_counter()
            its = pool.map(work_float, [size] * cores)
            return sum(its), time.perf_counter() - t

        r["multi.int"] = best_of(multi_int, repeats) / 1e6
        r["multi.float"] = best_of(multi_float, repeats) / 1e6
    return r


# --- bellek ----------------------------------------------------------------------

def bench_mem(scale, repeats):
    r = {}
    size = int(64 * scale) << 20  # 64 MB
    src = bytearray(os.urandom(1 << 20)) * (size >> 20)
    dst = bytearray(size)
    mv_src, mv_dst = memoryview(src), memoryview(dst)

    def copy():
        t = time.perf_counter()
        for _ in range(32):
            mv_dst[:] = mv_src
        return 32 * size / 1e9, time.perf_counter() - t
    r["mem.copy"] = best_of(copy, repeats)

    count = int(1_000_000 * scale)
    def alloc():
        t = time.perf_counter()
        objs = [(i, str(i), [i]) for i in range(count)]
        del objs
        return count / 1e6, time.perf_counter() - t
    r["mem.alloc"] = best_of(alloc, repeats)
    return r


# --- disk ------------------------------------------------------------------------

def _drop_cache(fd):
    """Okuma önbelleğini atlamaya çalışır (Linux: fadvise DONTNEED; macOS: F_NOCACHE). Kök yetkisi gerekmez."""
    if hasattr(os, "posix_fadvise"):
        try:
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        except OSError:
            pass
    if sys.platform == "darwin":
        try:
            import fcntl
            fcntl.fcntl(fd, 48, 1)  # F_NOCACHE
        except OSError:
            pass


def _sync(fd):
    """Kalıcı yazma. macOS'ta fsync diske tam yazmaz; Linux fsync'iyle eşdeğeri F_FULLFSYNC'tir."""
    if sys.platform == "darwin":
        try:
            import fcntl
            fcntl.fcntl(fd, 51)  # F_FULLFSYNC
            return
        except OSError:
            pass
    os.fsync(fd)


def bench_disk(scale, repeats, directory):
    r = {}
    size = int(256 * scale) << 20  # 256 MB
    chunk = os.urandom(1 << 20)
    path = os.path.join(directory, f".makeros-bench-{os.getpid()}.tmp")
    try:
        def seq_write():
            t = time.perf_counter()
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            try:
                _drop_cache(fd)  # macOS: F_NOCACHE yazarken de (okuma testi önbellekten gelmesin)
                for _ in range(size >> 20):
                    os.write(fd, chunk)
                _sync(fd)
                _drop_cache(fd)  # Linux: yazılan sayfaları önbellekten at
            finally:
                os.close(fd)
            return size / 1e6, time.perf_counter() - t
        r["disk.seq_write"] = best_of(seq_write, max(1, repeats - 1))

        def seq_read():
            fd = os.open(path, os.O_RDONLY)
            try:
                _drop_cache(fd)
                t = time.perf_counter()
                while os.read(fd, 1 << 20):
                    pass
                return size / 1e6, time.perf_counter() - t
            finally:
                os.close(fd)
        r["disk.seq_read"] = best_of(seq_read, max(1, repeats - 1))

        blocks = size // 4096
        rnd = random.Random(7)
        block = os.urandom(4096)
        nw = int(200 * max(scale, 0.5))
        def rand_write():
            fd = os.open(path, os.O_WRONLY)
            try:
                t = time.perf_counter()
                for _ in range(nw):
                    os.pwrite(fd, block, rnd.randrange(blocks) * 4096)
                    _sync(fd)  # her yazma kalıcı: SD kartlarda belirleyici
                return nw, time.perf_counter() - t
            finally:
                os.close(fd)
        r["disk.rand_write"] = best_of(rand_write, max(1, repeats - 1))

        nr = int(5000 * scale)
        def rand_read():
            fd = os.open(path, os.O_RDONLY)
            try:
                _drop_cache(fd)
                t = time.perf_counter()
                for _ in range(nr):
                    os.pread(fd, 4096, rnd.randrange(blocks) * 4096)
                return nr, time.perf_counter() - t
            finally:
                os.close(fd)
        r["disk.rand_read"] = best_of(rand_read, max(1, repeats - 1))
    finally:
        try:
            os.remove(path)
        except OSError:
            pass
    return r


# --- puan ve rapor ----------------------------------------------------------------

def scores(results):
    per = {k: round(1000 * v / REFERENCE[k]) for k, v in results.items() if k in REFERENCE and v > 0}
    cats = {c: round(geomean([per[k] for k in ks if k in per])) for c, ks in CATEGORIES.items() if any(k in per for k in ks)}
    total = round(geomean(list(cats.values())))
    return per, cats, total


def fmt(v):
    return f"{v:,.0f}" if v >= 100 else f"{v:,.1f}" if v >= 10 else f"{v:,.2f}"


def bar(score, width=20):
    n = max(0, min(width, round(width * score / 2000)))  # 2000 puan = tam çubuk
    return "█" * n + "░" * (width - n)


def markdown(res):
    s, sysi = res["scores"], res["system"]
    lines = [
        f"# MakerOS Bench · {res['name']}",
        "",
        f"**Genel puan: {s['total']:,}**  ·  referans (Apple M1 Pro · macOS) = 1000  ·  {res['date'][:16].replace('T', ' ')} UTC  ·  MakerOS Bench {res['version']}",
        "",
        "| Kategori | Puan | |",
        "|---|---:|---|",
    ]
    for c, v in s["categories"].items():
        lines.append(f"| {CAT_NAMES[c]} | {v:,} | `{bar(v)}` |")
    lines += ["", "## Sistem", "", "| | |", "|---|---|"]
    rows = [("İşletim sistemi", sysi.get("os")), ("Model", sysi.get("model")), ("İşlemci", sysi.get("cpu")),
            ("Çekirdek", f"{sysi.get('cores')}" + (f" ({sysi['core_layout']})" if sysi.get("core_layout") else "")),
            ("En yüksek frekans", f"{sysi['cpu_max_mhz']} MHz" if sysi.get("cpu_max_mhz") else None),
            ("Bellek", f"{sysi['ram_gb']} GB" if sysi.get("ram_gb") else None), ("Mimari", sysi.get("arch")),
            ("Çekirdek (kernel)", sysi.get("kernel")), ("Python", f"{sysi.get('python')} ({sysi.get('python_impl')})"),
            ("CPU yöneticisi", sysi.get("governor"))]
    lines += [f"| {k} | {v} |" for k, v in rows if v]
    th = res.get("thermal", {})
    if th.get("before") or th.get("after"):
        b, a = th.get("before", {}), th.get("after", {})
        if "temp_c" in b or "temp_c" in a:
            lines.append(f"| Sıcaklık (başta → sonda) | {b.get('temp_c', '?')} °C → {a.get('temp_c', '?')} °C |")
        if a.get("throttled"):
            lines.append(f"| Kısma (vcgencmd) | {a['throttled']}{' ⚠ kısma oldu' if a['throttled'] != '0x0' else ' (yok)'} |")
    lines += ["", "## Testler", "", "| Test | Sonuç | Puan |", "|---|---:|---:|"]
    for c, ks in CATEGORIES.items():
        for k in ks:
            if k in res["results"]:
                name, unit = TESTS[k]
                lines.append(f"| {name} | {fmt(res['results'][k])} {unit} | {s['tests'].get(k, 0):,} |")
    if "multi.int" in res["results"] and "cpu.int" in res["results"]:
        sc = res["results"]["multi.int"] / res["results"]["cpu.int"]
        lines += ["", f"Çok çekirdek ölçeklenmesi: **{sc:.1f}×** ({sysi.get('cores')} çekirdekte, tamsayı)."]
    lines += ["", f"Süre: {res['duration_s']:.0f} sn · kip: {res['mode']} · disk testi: {res.get('disk_fs', '?')}",
              "", "_Python iş yükü başarımıdır; aynı MakerOS Bench sürümüyle alınan sonuçlar karşılaştırılabilir._", ""]
    return "\n".join(lines)


def slug(s):
    s = s.lower()
    for a, b in (("ı", "i"), ("ş", "s"), ("ğ", "g"), ("ü", "u"), ("ö", "o"), ("ç", "c")):
        s = s.replace(a, b)
    return re.sub(r"[^a-z0-9]+", "-", s).strip("-")[:60] or "sistem"


def default_name(sysi):
    parts = []
    if sysi.get("model") and not sysi["model"].startswith("Mac"):
        parts.append(sysi["model"].replace(" Model B", "").replace("Raspberry Pi", "Raspberry Pi"))
    else:
        parts.append(sysi.get("cpu", "?"))
    if sysi.get("ram_gb"):
        parts[-1] += f" {round(sysi['ram_gb'])} GB"
    parts.append(sysi.get("os", "?"))
    return " · ".join(parts)


def main():
    ap = argparse.ArgumentParser(description="MakerOS Bench: CPU, bellek ve disk karşılaştırma testi (Markdown + JSON rapor).")
    ap.add_argument("--quick", action="store_true", help="kısa test (~40 sn; puanlar daha oynak)")
    ap.add_argument("--name", help='listede görünecek ad (varsayılan: model/işlemci · işletim sistemi)')
    ap.add_argument("--out", default="bench-results", help="rapor klasörü (varsayılan ./bench-results)")
    ap.add_argument("--disk-dir", default=None, help="disk testi klasörü (varsayılan: rapor klasörü; test edilecek diskte olmalı)")
    ap.add_argument("--skip", default="", help="atlanacak kategoriler, virgülle: cpu,multi,mem,disk")
    ap.add_argument("--json", action="store_true", help="yalnız JSON'u standart çıktıya yaz")
    a = ap.parse_args()

    scale, repeats = (0.4, 2) if a.quick else (1.0, 3)
    skip = {x.strip() for x in a.skip.split(",") if x.strip()}
    os.makedirs(a.out, exist_ok=True)
    disk_dir = a.disk_dir or a.out
    sysi = system_info()
    name = a.name or default_name(sysi)
    log(f"MakerOS Bench {VERSION} · {name}")
    log(f"  {sysi.get('cpu')} · {sysi.get('cores')} çekirdek · {sysi.get('ram_gb', '?')} GB · Python {sysi['python']}")

    t0 = time.time()
    th_before = thermal()
    results = {}
    steps = [("cpu", "CPU tek çekirdek", lambda: bench_cpu(scale, repeats)),
             ("multi", f"CPU çok çekirdek ({sysi['cores']})", lambda: bench_multi(scale, repeats, sysi["cores"])),
             ("mem", "Bellek", lambda: bench_mem(scale, repeats)),
             ("disk", f"Disk ({os.path.abspath(disk_dir)})", lambda: bench_disk(scale, repeats, disk_dir))]
    for key, label, fn in steps:
        if key in skip:
            continue
        log(f"▶ {label} …", end="")
        t = time.time()
        part = fn()
        results.update(part)
        log(f" {time.time() - t:.0f} sn")
    th_after = thermal()

    per, cats, total = scores(results)
    now = _dt.datetime.now(_dt.timezone.utc).replace(microsecond=0)
    fs = ""
    try:
        st = os.statvfs(disk_dir)
        fs = f"{round(st.f_blocks * st.f_frsize / 1e9)} GB birim"
    except (OSError, AttributeError):
        pass
    res = {
        "schema": SCHEMA, "version": VERSION, "name": name, "date": now.isoformat().replace("+00:00", "Z"),
        "mode": "hızlı" if a.quick else "tam", "duration_s": round(time.time() - t0, 1), "disk_fs": fs,
        "system": sysi, "thermal": {"before": th_before, "after": th_after},
        "results": {k: round(v, 3) for k, v in results.items()},
        "scores": {"total": total, "categories": cats, "tests": per},
    }
    base = os.path.join(a.out, f"{now:%Y%m%d-%H%M}-{slug(sysi.get('os_id', ''))}-{slug(name)}")
    with open(base + ".json", "w", encoding="utf-8") as f:
        json.dump(res, f, ensure_ascii=False, indent=1)
    md = markdown(res)
    with open(base + ".md", "w", encoding="utf-8") as f:
        f.write(md)
    if a.json:
        print(json.dumps(res, ensure_ascii=False, indent=1))
    else:
        print(md)
    log(f"\nGenel puan: {total:,}  ·  rapor: {base}.md  ·  veri: {base}.json")
    log("Listeye eklemek için: .json dosyasını github.com/ferhatozcelik/maker-os deposunda benchmark/results/ klasörüne PR ile gönderin.")


if __name__ == "__main__":
    main()
