"""{{name}}: Buton (GPIO27, pin 13) basılıyken LED (GPIO17, pin 11) yanar.

Buton: GPIO27 ile GND (pin 14) arasına (dahili pull-up kullanılır). LED: GPIO17 --[330 Ω]--> LED --> GND.
gpiod ile kesme tabanlıdır: buton beklenirken işlemci boşta kalır. Çıkmak için Ctrl+C.
"""
from datetime import timedelta

import gpiod
from gpiod.line import Bias, Direction, Edge, Value

CHIP, BUTTON, LED = "/dev/gpiochip0", 27, 17  # GPIO2/3 I2C'ye ayrılı

config = {
    BUTTON: gpiod.LineSettings(direction=Direction.INPUT, bias=Bias.PULL_UP, edge_detection=Edge.BOTH, debounce_period=timedelta(milliseconds=20)),
    LED: gpiod.LineSettings(direction=Direction.OUTPUT, output_value=Value.INACTIVE),
}
with gpiod.request_lines(CHIP, consumer="{{name}}", config=config) as req:
    print("Butona basın; LED yanacak. Çıkmak için Ctrl+C.")
    try:
        while True:
            if req.wait_edge_events(timedelta(seconds=1)):
                req.read_edge_events()
                pressed = req.get_value(BUTTON) == Value.INACTIVE  # pull-up: basılıyken 0
                req.set_value(LED, Value.ACTIVE if pressed else Value.INACTIVE)
                print("basıldı" if pressed else "bırakıldı", flush=True)
    except KeyboardInterrupt:
        req.set_value(LED, Value.INACTIVE)
