"""{{name}}: Pi kamerasıyla foto.jpg çeker (picamera2).

picamera2 sistemde hazırdır (kamera takılınca MakerOS arka planda kurar).
"""
import time

from picamera2 import Picamera2

cam = Picamera2()
cam.configure(cam.create_still_configuration())
cam.start()
time.sleep(1.5)  # pozlama ve beyaz dengesi otursun
cam.capture_file("foto.jpg")
cam.stop()
print("Kaydedildi: foto.jpg")
