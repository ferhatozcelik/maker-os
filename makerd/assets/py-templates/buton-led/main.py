"""{{name}}: Buton (GPIO2, pin 3) basılıyken LED (GPIO17, pin 11) yanar.

Buton: GPIO2 ile GND arasına (dahili pull-up kullanılır).
"""
from signal import pause

from gpiozero import LED, Button

led = LED(17)
button = Button(2)
button.when_pressed = led.on
button.when_released = led.off
print("Butona basın; LED yanacak. Çıkmak için Ctrl+C.")
pause()
