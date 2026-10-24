# GPIO Pinleri: 40 pinli baslik semasi, cikis ver / giris oku, I2C tarama.
# libgpiod v1 komut satiri araclari (gpiodetect/gpioget/gpioset) kullanilir.
#
# Not: gpioget bir hatti okurken onu GIRIS yapar. Bu yuzden yalnizca
# kullanicinin bu ekrandan giris olarak sectigi pinler okunur; I2C/SPI/UART
# gibi alternatif islevdeki pinlere dokunulmaz.
import re
import signal
import subprocess
import threading

from common import run

# Fiziksel pin -> (ad, BCM numarasi veya None, tur)
HEADER = {
    1: ("3V3", None, "power3"), 2: ("5V", None, "power5"),
    3: ("GPIO2 · SDA", 2, "i2c"), 4: ("5V", None, "power5"),
    5: ("GPIO3 · SCL", 3, "i2c"), 6: ("GND", None, "gnd"),
    7: ("GPIO4", 4, "gpio"), 8: ("GPIO14 · TX", 14, "uart"),
    9: ("GND", None, "gnd"), 10: ("GPIO15 · RX", 15, "uart"),
    11: ("GPIO17", 17, "gpio"), 12: ("GPIO18 · PWM", 18, "gpio"),
    13: ("GPIO27", 27, "gpio"), 14: ("GND", None, "gnd"),
    15: ("GPIO22", 22, "gpio"), 16: ("GPIO23", 23, "gpio"),
    17: ("3V3", None, "power3"), 18: ("GPIO24", 24, "gpio"),
    19: ("GPIO10 · MOSI", 10, "spi"), 20: ("GND", None, "gnd"),
    21: ("GPIO9 · MISO", 9, "spi"), 22: ("GPIO25", 25, "gpio"),
    23: ("GPIO11 · SCLK", 11, "spi"), 24: ("GPIO8 · CE0", 8, "spi"),
    25: ("GND", None, "gnd"), 26: ("GPIO7 · CE1", 7, "spi"),
    27: ("GPIO0 · ID_SD", 0, "id"), 28: ("GPIO1 · ID_SC", 1, "id"),
    29: ("GPIO5", 5, "gpio"), 30: ("GND", None, "gnd"),
    31: ("GPIO6", 6, "gpio"), 32: ("GPIO12 · PWM", 12, "gpio"),
    33: ("GPIO13 · PWM", 13, "gpio"), 34: ("GND", None, "gnd"),
    35: ("GPIO19", 19, "gpio"), 36: ("GPIO16", 16, "gpio"),
    37: ("GPIO26", 26, "gpio"), 38: ("GPIO20", 20, "gpio"),
    39: ("GND", None, "gnd"), 40: ("GPIO21", 21, "gpio"),
}
USER_PINS = {bcm for _, bcm, kind in HEADER.values() if bcm is not None and kind != "id"}

_lock = threading.Lock()
_outputs = {}   # bcm -> (Popen, value)
_inputs = {}    # bcm -> bias
_chip = []      # bulunan gpiochip; calisirken degismez, her yoklamada aranmaz


def chip():
    """Baslik pinlerini yoneten gpiochip (Pi4: pinctrl-bcm2711, Pi5: rp1)."""
    if _chip:
        return _chip[0]
    rc, out, _ = run(["gpiodetect"])
    if rc != 0:
        return None
    for line in out.splitlines():
        if re.search(r"pinctrl-bcm27|pinctrl-rp1|rp1", line):
            _chip.append(line.split()[0])
            return _chip[0]
    return None


def _release(bcm):
    p = _outputs.pop(bcm, None)
    if p:
        p[0].send_signal(signal.SIGTERM)
        try:
            p[0].wait(timeout=2)
        except subprocess.TimeoutExpired:
            p[0].kill()
    _inputs.pop(bcm, None)


def state():
    c = chip()
    values = {}
    if c:
        with _lock:
            for bcm, (_, val) in _outputs.items():
                values[bcm] = {"mode": "out", "value": val}
            for bcm, bias in _inputs.items():
                args = ["gpioget"] + (["-B", bias] if bias != "as-is" else []) + [c, str(bcm)]
                rc, out, _ = run(args, timeout=3)
                values[bcm] = {"mode": "in", "value": int(out) if rc == 0 and out.isdigit() else None,
                               "bias": bias}
    pins = []
    for phys in range(1, 41):
        name, bcm, kind = HEADER[phys]
        pins.append({"phys": phys, "name": name, "bcm": bcm, "kind": kind,
                     "usable": bcm in USER_PINS, **values.get(bcm, {})})
    return {"chip": c, "pins": pins}


def set_output(bcm, value):
    c = chip()
    if not c or bcm not in USER_PINS:
        return False, "GPIO donanimi bulunamadi" if not c else "gecersiz pin"
    with _lock:
        _release(bcm)
        # --mode=signal: surec yasadigi surece hat cikis olarak tutulur.
        p = subprocess.Popen(["gpioset", "--mode=signal", c, "%d=%d" % (bcm, 1 if value else 0)],
                             stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        try:
            p.wait(timeout=0.3)
            return False, (p.stderr.read() or b"").decode().strip() or "gpioset hatasi"
        except subprocess.TimeoutExpired:
            _outputs[bcm] = (p, 1 if value else 0)
    return True, ""


def set_input(bcm, bias="as-is"):
    if not chip() or bcm not in USER_PINS or bias not in ("as-is", "pull-up", "pull-down", "disable"):
        return False, "gecersiz istek"
    with _lock:
        _release(bcm)
        _inputs[bcm] = bias
    return True, ""


def release(bcm):
    with _lock:
        _release(bcm)
    return True


def i2c_scan(bus=1):
    rc, out, err = run(["i2cdetect", "-y", str(bus)], timeout=10)
    if rc != 0:
        return None, err or "I2C kapali (Ayarlar > Donanim)"
    found = []
    for line in out.splitlines()[1:]:
        for cell in line.split(":", 1)[-1].split():
            if re.fullmatch(r"[0-9a-f]{2}|UU", cell):
                found.append(cell if cell != "UU" else "UU")
    return [a for a in found if a != "UU"], ""
