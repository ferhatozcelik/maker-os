"""{{name}}: Pi kamerasıyla (veya USB kamera) foto.jpg çeker.

rpicam-still kullanılır (MakerOS'ta hazır). Kamera uygulaması açıksa önce onu kapatın: kamerayı aynı anda tek program kullanabilir.
"""
import subprocess
import sys

try:
    subprocess.run(["rpicam-still", "-n", "-t", "1500", "--width", "1920", "--height", "1080", "-o", "foto.jpg"],
                   check=True, capture_output=True, text=True, timeout=30)
except FileNotFoundError:
    sys.exit("rpicam-still bulunamadı.")
except subprocess.CalledProcessError as e:
    sys.exit("Fotoğraf çekilemedi. Kamera takılı mı, başka bir program (ör. Kamera uygulaması) kullanıyor mu?\n" + (e.stderr or "")[-400:])
except subprocess.TimeoutExpired:
    sys.exit("Kamera yanıt vermedi (zaman aşımı).")
print("Kaydedildi: foto.jpg")
