#!/usr/bin/env python3
"""MakerOS Bench — tek dosyalık, bağımlılıksız sistem karşılaştırma testi.

macOS, Linux, Raspberry Pi OS ve MakerOS'ta çalışır (Python 3.8+; yalnız standart kitaplık).

    curl -fsSLO https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/benchmark/makeros-bench.py
    python3 makeros-bench.py                  # tam test (~2 dk), rapor: ./bench-results/
    python3 makeros-bench.py --quick          # hızlı (~40 sn)
    python3 makeros-bench.py --name "Pi 4 4GB · Raspberry Pi OS"   # listede görünecek ad
    python3 makeros-bench.py --compare        # + yayımlanmış Raspberry Pi OS sonuçlarıyla aynı testler (~25 dk)
    python3 makeros-bench.py --compare --speedometer 17.4   # tarayıcıda ölçülen Speedometer 2.0 sonucunu da ekle
    python3 makeros-bench.py --compare --gb5 268,745        # Geekbench sayfasındaki puanları elle ekle

Ölçülenler
  CPU tek çekirdek  tamsayı, kayan nokta, SHA-256, zlib, JSON, düzenli ifade
  CPU çok çekirdek  aynı tamsayı/kayan nokta yükü tüm çekirdeklerde (ölçeklenme oranı)
  Bellek            büyük blok kopyalama (GB/s), nesne ayırma
  Disk              sıralı yazma/okuma (MB/s, fsync), rastgele 4K yazma (fsync) ve okuma (IOPS)

Puan: her test, sabit bir referansa göre ölçeklenir (REFERANS = 1000 puan; referans makine Apple M1 Pro,
macOS, Python 3.9 ilk ölçümüdür). Kategori puanı testlerin, genel puan kategorilerin geometrik ortalamasıdır.
Daha yüksek = daha iyi. Python ile ölçüldüğü için mutlak donanım değil, "bu sistemde Python iş yükü" başarımıdır;
aynı betik sürümüyle alınan sonuçlar birbiriyle karşılaştırılabilir.

Karşılaştırma (--compare; isteğe bağlı, internet gerekir)
  Kaynaklarda Raspberry Pi 4 için yayımlanmış testlerin aynısını çalıştırır ve yan yana raporlar:
  Geekbench 5.4.0 (ARM)  Primate Labs'in ikili dosyası indirilip çalıştırılır. Ücretsiz sürüm sonucu
                         browser.geekbench.com'a YÜKLER (işlemci, bellek, işletim sistemi bilgisi dahil).
  Zstd -19 (tek iş parçacığı)  OpenBenchmarking pts/compress-zstd ile aynı dosya (ubuntu-16.04.3-server-i386.img,
                         835 MB, SHA256 doğrulanır); sistemdeki zstd ile. ~1 GB boş alan gerekir.
  Speedometer 2.0        Elle: https://browserbench.org/Speedometer2.0/ tarayıcıda açılır, sonuç --speedometer ile girilir.
  --compare-only puanlı Python testlerini atlar. İndirmeler ~/.cache/makeros-bench altında saklanır.
  Kaynaklardaki ölçümler farklı yıllarda, çoğu 32 bit Raspbian ile alındı; koşullar raporda yazılıdır.

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

VERSION = "1.1.0"
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

def idle_load(seconds=3):
    """Testten önce tüm çekirdeklerde boşta yük (%); Linux dışında None."""
    def snap():
        v = [int(x) for x in read("/proc/stat").splitlines()[0].split()[1:]]
        return sum(v), v[3] + (v[4] if len(v) > 4 else 0)
    try:
        t1, i1 = snap()
        time.sleep(seconds)
        t2, i2 = snap()
        return round(100 * (1 - (i2 - i1) / max(1, t2 - t1)), 1)
    except (ValueError, IndexError, AttributeError):
        return None


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


# --- yayımlanmış sonuçlarla karşılaştırma (isteğe bağlı) ---------------------------------------

# Raspberry Pi 4 Model B için yayımlanmış değerler. MakerOS Bench puanına girmez; yalnız yan yana gösterilir.
PUBLISHED = {
    "gb5.single": {"name": "Geekbench 5 · tek çekirdek", "unit": "puan", "value": 233, "higher": True,
                   "env": "Pi 4 4 GB · Linux 5.10.17-v8+ aarch64 · Geekbench 5.4.0", "src": "https://browser.geekbench.com/v5/cpu/9697615"},
    "gb5.multi": {"name": "Geekbench 5 · çok çekirdek", "unit": "puan", "value": 686, "higher": True,
                  "env": "Pi 4 4 GB · Linux 5.10.17-v8+ aarch64 · Geekbench 5.4.0", "src": "https://browser.geekbench.com/v5/cpu/9697615"},
    "zstd19": {"name": "Zstd sıkıştırma (seviye 19)", "unit": "sn", "value": 291.48, "higher": False,
               "env": "Pi 4 4 GB · Raspbian 10 armv7l · Linux 4.19.50 · zstd 1.3.4", "src": "https://openbenchmarking.org/result/2202182-NE-2202156EI32"},
    "speedometer2": {"name": "Speedometer 2.0 (tarayıcı)", "unit": "çalışma/dk", "value": 16.91, "higher": True,
                     "env": "Pi 4 · Raspbian (Haziran 2019)", "src": "https://magazine.raspberrypi.com/articles/raspberry-pi-4-specs-benchmarks"},
}
GB5_URL = "https://cdn.geekbench.com/Geekbench-5.4.0-LinuxARMPreview.tar.gz"
ZSTD_IMG = ("http://old-releases.ubuntu.com/releases/16.04.3/ubuntu-16.04.3-server-i386.img",
            "2e763f7f3546be33ebc9a59311b2eee3b0b0c8e021fae65431c219b531d26d32", 834666496)


def _cache_dir():
    d = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache"), "makeros-bench")
    os.makedirs(d, exist_ok=True)
    return d


def _download(url, dest, sha256=None, size=None):
    """url → dest (yoksa); SHA256 verilmişse doğrular. Yarım indirmeler .part olarak kalır ve silinir."""
    import urllib.request
    if os.path.exists(dest) and (not sha256 or _sha256(dest) == sha256):
        return dest
    if size:
        free = shutil.disk_usage(os.path.dirname(dest)).free
        if free < size + (200 << 20):
            raise RuntimeError(f"yetersiz boş alan: {free >> 20} MB var, {(size >> 20) + 200} MB gerekir ({os.path.dirname(dest)})")
    part = dest + ".part"
    log(f"    indiriliyor: {url}")
    req = urllib.request.Request(url, headers={"User-Agent": f"makeros-bench/{VERSION}"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r, open(part, "wb") as f:
            total = int(r.headers.get("Content-Length") or size or 0)
            got, last = 0, 0.0
            while True:
                buf = r.read(1 << 20)
                if not buf:
                    break
                f.write(buf)
                got += len(buf)
                if total and time.time() - last > 2:
                    last = time.time()
                    log(f"\r    {got >> 20}/{total >> 20} MB", end="")
        if total:
            log("")
        if sha256 and _sha256(part) != sha256:
            raise RuntimeError("SHA256 tutmadı: " + url)
        os.replace(part, dest)
    finally:
        if os.path.exists(part):
            os.remove(part)
    return dest


def _sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def compare_geekbench():
    """Geekbench 5.4.0 ARM sürümü: tek ve çok çekirdek puanı (sonuç browser.geekbench.com'a yüklenir)."""
    if platform.system() != "Linux" or platform.machine() not in ("aarch64", "arm64"):
        return {"error": "yalnız Linux aarch64"}
    import tarfile
    cache = _cache_dir()
    tgz = _download(GB5_URL, os.path.join(cache, os.path.basename(GB5_URL)))
    root = os.path.join(cache, "gb5")
    exe = None
    if not os.path.isdir(root):
        with tarfile.open(tgz) as t:
            t.extractall(root, filter="data") if sys.version_info >= (3, 12) else t.extractall(root)
    for d, _, files in os.walk(root):
        if "geekbench5" in files:
            exe = os.path.join(d, "geekbench5")
    if not exe:
        return {"error": "geekbench5 bulunamadı"}
    os.chmod(exe, 0o755)
    log("    Geekbench 5 çalışıyor (5–10 dk; sonuç browser.geekbench.com'a yüklenir) …")
    try:
        out = subprocess.run([exe], capture_output=True, text=True, timeout=3600, cwd=os.path.dirname(exe)).stdout
    except (OSError, subprocess.SubprocessError) as e:
        return {"error": f"çalışmadı: {e}"}
    r = {}
    m = re.search(r"Single-Core Score\s+(\d+)", out)
    if m:
        r["gb5.single"] = int(m.group(1))
    m = re.search(r"Multi-Core Score\s+(\d+)", out)
    if m:
        r["gb5.multi"] = int(m.group(1))
    m = re.search(r"https://browser\.geekbench\.com/v5/cpu/\d+", out)
    if m:
        r["gb5.url"] = m.group(0)
    if "gb5.single" not in r:
        if "gb5.url" in r:
            # Linux Preview sürümü puanı ekrana yazmaz; yalnızca yüklediği sonucun bağlantısını verir.
            r["gb5.note"] = "puanlar " + r["gb5.url"] + " sayfasında; --gb5 TEK,ÇOK ile rapora eklenebilir"
        else:
            r["error"] = "puan okunamadı: " + " ".join(out.strip().splitlines()[-3:])[:200]
    return r


def compare_zstd(runs):
    """pts/compress-zstd ile aynı iş: ubuntu-16.04.3-server-i386.img → zstd -19, tek iş parçacığı; ortanca süre."""
    zstd = shutil.which("zstd")
    if not zstd:
        return {"error": "zstd komutu yok"}
    url, sha, size = ZSTD_IMG
    img = _download(url, os.path.join(_cache_dir(), os.path.basename(url)), sha, size)
    ver = run([zstd, "--version"])
    times = []
    for i in range(runs):
        log(f"    zstd -19 ({i + 1}/{runs}) …", end="")
        t = time.perf_counter()
        with open(os.devnull, "wb") as null:
            subprocess.run([zstd, "-19", "-T1", "-q", "-c", img], stdout=null, check=True)
        times.append(time.perf_counter() - t)
        log(f" {times[-1]:.0f} sn")
    return {"zstd19": round(statistics.median(times), 2), "zstd.runs": [round(x, 2) for x in times], "zstd.version": ver}


def compare_rows(cmp):
    """(anahtar, yayımlanmış, bu sistem, oran) — oran > 1 bu sistem daha iyi demektir."""
    rows = []
    for k, ref in PUBLISHED.items():
        v = cmp.get(k)
        ratio = None
        if isinstance(v, (int, float)) and v > 0:
            ratio = v / ref["value"] if ref["higher"] else ref["value"] / v
        rows.append((k, ref, v, ratio))
    return rows


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
    if res.get("compare"):
        cmp = res["compare"]
        lines += ["", "## Yayımlanmış Raspberry Pi 4 sonuçlarıyla", "",
                  "| Test | Raspberry Pi 4 (kaynak) | Bu sistem | Fark | |", "|---|---:|---:|---:|---|"]
        for k, ref, v, ratio in compare_rows(cmp):
            mine = f"{fmt(v)} {ref['unit']}" if isinstance(v, (int, float)) else "—"
            diff = f"{(ratio - 1) * 100:+.0f}%" if ratio else ""
            graph = f"`{bar(1000 * ratio)}`" if ratio else ""
            lines.append(f"| {ref['name']} | {fmt(ref['value'])} {ref['unit']} | {mine} | {diff} | {graph} |")
        lines += ["", "Fark: + bu sistem daha hızlı. Çubuk: kaynaktaki değer = yarım çubuk.", "", "Kaynaklar ve koşullar:", ""]
        for k, ref in PUBLISHED.items():
            lines.append(f"- {ref['name']}: {ref['env']} — {ref['src']}")
        if cmp.get("zstd.version"):
            lines.append(f"- Bu sistemde zstd: {cmp['zstd.version']} (`zstd -19 -T1`, {len(cmp.get('zstd.runs', []))} ölçümün ortancası)")
        if cmp.get("gb5.url"):
            lines.append(f"- Bu sistemin Geekbench sonucu: {cmp['gb5.url']}")
        for k in ("gb5.error", "zstd.error"):
            if cmp.get(k):
                lines.append(f"- {k.split('.')[0]} atlandı: {cmp[k]}")
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
    ap.add_argument("--compare", action="store_true",
                    help="yayımlanmış Raspberry Pi 4 sonuçlarıyla aynı testleri de çalıştır: Geekbench 5.4.0 (indirir; sonucu "
                         "browser.geekbench.com'a yükler) ve zstd -19 (835 MB imaj indirir). İnternet gerekir.")
    ap.add_argument("--compare-only", action="store_true", help="--compare, puanlı Python testleri olmadan")
    ap.add_argument("--no-geekbench", action="store_true", help="--compare içinde Geekbench'i atla (yükleme yapılmaz)")
    ap.add_argument("--no-zstd", action="store_true", help="--compare içinde zstd testini atla")
    ap.add_argument("--gb5", metavar="TEK,ÇOK", help="Geekbench 5 sayfasındaki tek ve çok çekirdek puanı (ör. 268,745)")
    ap.add_argument("--speedometer", type=float, help="tarayıcıda ölçtüğün Speedometer 2.0 sonucu (çalışma/dk)")
    a = ap.parse_args()

    scale, repeats = (0.4, 2) if a.quick else (1.0, 3)
    skip = {x.strip() for x in a.skip.split(",") if x.strip()}
    if a.compare_only:
        a.compare = True
        skip |= {"cpu", "multi", "mem", "disk"}
    os.makedirs(a.out, exist_ok=True)
    disk_dir = a.disk_dir or a.out
    sysi = system_info()
    name = a.name or default_name(sysi)
    log(f"MakerOS Bench {VERSION} · {name}")
    log(f"  {sysi.get('cpu')} · {sysi.get('cores')} çekirdek · {sysi.get('ram_gb', '?')} GB · Python {sysi['python']}")

    t0 = time.time()
    th_before = thermal()
    results = {}
    idle = idle_load()
    if idle is not None:
        sysi["idle_load_pct"] = idle
        if idle > 10:
            log(f"⚠ Boşta işlemci yükü %{idle:.0f}: arka planda çalışan bir şey var (eklenti, kapsayıcı, proje).")
            log("  Sonuçlar düşük çıkar; karşılaştırma için önce durdurun. En çok kullananlar:")
            log("  " + run(["sh", "-c", "ps -eo pcpu,comm --sort=-pcpu | sed -n 2,6p"]).replace("\n", "\n  "))
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
    cmp = {}
    if a.compare:
        if not a.no_geekbench:
            log("▶ Karşılaştırma: Geekbench 5.4.0")
            try:
                g = compare_geekbench()
            except Exception as e:  # ağ/indirme hatası tüm testi düşürmesin
                g = {"error": str(e)}
            if "error" in g:
                cmp["gb5.error"] = g.pop("error")
                log(f"    atlandı: {cmp['gb5.error']}")
            if "gb5.note" in g:
                log(f"    {g['gb5.note']}")
            cmp.update(g)
        if not a.no_zstd:
            log("▶ Karşılaştırma: zstd -19 (ubuntu-16.04.3-server-i386.img)")
            try:
                z = compare_zstd(1 if a.quick else 3)
            except Exception as e:
                z = {"error": str(e)}
            if "error" in z:
                cmp["zstd.error"] = z.pop("error")
                log(f"    atlandı: {cmp['zstd.error']}")
            cmp.update(z)
    if a.speedometer:
        cmp["speedometer2"] = a.speedometer
    if a.gb5:
        try:
            cmp["gb5.single"], cmp["gb5.multi"] = (int(x) for x in a.gb5.split(","))
        except ValueError:
            ap.error("--gb5 biçimi: TEK,ÇOK (ör. 268,745)")
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
    if cmp:
        res["compare"] = cmp
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
