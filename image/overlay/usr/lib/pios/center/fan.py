"""GPIO kontrollü fan: kernel gpio-fan sürücüsü, yeniden başlatmada uygulanır."""
import glob
import re
import threading

from common import read_first
import system

_lock = threading.Lock()


def status():
    config = {"mode": "unmanaged", "pin": 14, "on_c": 60, "off_c": 50}
    for line in system._read_config():
        line = line.strip()
        if line.startswith("dtoverlay=gpio-fan") and line.split(",")[0] == "dtoverlay=gpio-fan":
            props = dict(p.split("=", 1) for p in line.split(",")[1:] if "=" in p)
            try:
                on = int(props.get("temp", "55000")) / 1000
                config = {"mode": "always" if on == 0 else "auto", "pin": int(props.get("gpiopin", "12")),
                          "on_c": on if on else 60, "off_c": on - int(props.get("hyst", "10000")) / 1000 if on else 50}
            except ValueError:
                pass
    active = []
    for path in glob.glob("/sys/class/thermal/cooling_device*"):
        if "fan" in read_first(path + "/type").lower():
            active.append({"driver": read_first(path + "/type"), "state": read_first(path + "/cur_state"),
                           "max": read_first(path + "/max_state")})
    temp = read_first("/sys/class/thermal/thermal_zone0/temp")
    return dict(config, active=active, temp_c=round(int(temp) / 1000, 1) if temp.isdigit() else None,
                note="GPIO kontrol girişli fan içindir. Doğrudan 5V’a bağlı iki kablolu fan yazılımla yönetilemez. Ayarlar yeniden başlatmada uygulanır.")


def save(data):
    mode = data.get("mode")
    if mode not in ("unmanaged", "auto", "always"):
        return False, "Geçersiz fan modu."
    try:
        pin = int(data.get("pin", 14))
        on, off = int(data.get("on_c", 60)), int(data.get("off_c", 50))
    except (ValueError, TypeError):
        return False, "GPIO ve sıcaklıklar tam sayı olmalıdır."
    if not 4 <= pin <= 27 or not 35 <= off < on <= 85:
        return False, "GPIO 4–27 olmalıdır. Kapanma sıcaklığı en az 35 °C ve açılma sıcaklığından düşük; açılma en fazla 85 °C olmalıdır."
    hw = system.hardware()
    conflicts = set()
    if hw.get("spi"):
        conflicts.update(range(7, 12))
    if hw.get("serial"):
        conflicts.update((14, 15))
    if mode != "unmanaged" and pin in conflicts:
        return False, "GPIO %s etkin SPI/UART arayüzü tarafından kullanılıyor. Başka pin seçin veya arayüzü kapatın." % pin
    with _lock:
        lines = [l for l in system._read_config() if not re.match(r"^\s*dtoverlay=gpio-fan(?:,|\s*$)", l)]
        # Eski yönetilen bloğu çıkar; ayarları koşulsuz [all] kapsamına ekle.
        try:
            start = lines.index("# MakerOS fan begin")
            end = lines.index("# MakerOS fan end", start)
            del lines[start:end + 1]
        except ValueError:
            pass
        if mode != "unmanaged":
            lines += ["# MakerOS fan begin", "[all]",
                      "dtoverlay=gpio-fan,gpiopin=%s,temp=%s,hyst=%s" % (pin, on * 1000 if mode == "auto" else 0,
                                                                        (on - off) * 1000 if mode == "auto" else 0),
                      "# MakerOS fan end"]
        system._write_config(lines)
    return True, "Fan ayarları kaydedildi. Uygulanması için Raspberry Pi’yi yeniden başlatın."
