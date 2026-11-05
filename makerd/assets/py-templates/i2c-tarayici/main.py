"""{{name}}: I2C veri yolu 1'deki aygıtları tarar (i2cdetect benzeri)."""
from smbus2 import SMBus

found = []
with SMBus(1) as bus:
    for addr in range(0x03, 0x78):
        try:
            bus.read_byte(addr)
            found.append(addr)
        except OSError:
            pass
if found:
    print("Bulunan aygıtlar:", ", ".join(f"0x{a:02x}" for a in found))
else:
    print("Aygıt bulunamadı. Kabloları ve Ayarlar > Donanım'da I2C'yi kontrol edin.")
