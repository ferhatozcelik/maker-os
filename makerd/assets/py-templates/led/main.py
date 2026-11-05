"""{{name}}: GPIO17'deki (fiziksel pin 11) LED'i yakıp söndürür.

Bağlantı: GPIO17 --[330 Ω direnç]--> LED (+, uzun bacak) ; LED (-) --> GND (pin 9)
Pi 4 ve Pi 5'te aynı çalışır (gpiozero, lgpio arka ucu). Durdurmak için Ctrl+C.
"""
from time import sleep

from gpiozero import LED

led = LED(17)
print("LED GPIO17'de yanıp sönüyor. Durdurmak için Ctrl+C.")
try:
    while True:
        led.toggle()
        sleep(0.5)
except KeyboardInterrupt:
    led.off()
