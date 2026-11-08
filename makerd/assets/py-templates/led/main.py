"""{{name}}: GPIO17'deki (fiziksel pin 11) LED'i yakıp söndürür.

Bağlantı: GPIO17 --[330 Ω direnç]--> LED (+, uzun bacak) ; LED (-) --> GND (pin 9)
gpiod (Linux GPIO karakter aygıtı, /dev/gpiochip0) kullanır; Pi 4 ve Pi 5'te çalışır. Durdurmak için Ctrl+C.
"""
import time

import gpiod
from gpiod.line import Direction, Value

CHIP, LED_PIN = "/dev/gpiochip0", 17

with gpiod.request_lines(CHIP, consumer="{{name}}", config={LED_PIN: gpiod.LineSettings(direction=Direction.OUTPUT, output_value=Value.INACTIVE)}) as req:
    print("LED GPIO17'de yanıp sönüyor. Durdurmak için Ctrl+C.")
    on = False
    try:
        while True:
            on = not on
            req.set_value(LED_PIN, Value.ACTIVE if on else Value.INACTIVE)
            time.sleep(0.5)
    except KeyboardInterrupt:
        req.set_value(LED_PIN, Value.INACTIVE)
