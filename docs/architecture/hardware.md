# Donanım Desteği

[Türkçe](hardware.md) · [English](hardware.en.md)

MakerOS'un birincil hedef platformu **Pi 4 / 400 uyumlu** ARM64 kartlardır (BCM2711 sistem yongası). Mimari, gelecekte farklı ARM64 tek kartlı bilgisayarlara taşınabilecek şekilde modüler tutulmuştur.

## Desteklenen Arayüzler

- **GPIO (40-pin, [`makerd/src/gpio.rs`](../../makerd/src/gpio.rs)):** `/dev/gpiochip0` (`gpiocdev` v2 karakter aygıtı) üzerinden tüm pinlerin durumunu canlı okuma, giriş/çıkış yönünü değiştirme ve `HIGH` / `LOW` sürme.
- **I²C, SPI ve UART:** Çekirdek yapılandırmasında varsayılan olarak etkindir; Python Yöneticisi içinde hazır `I2C Tarayıcı`, `LED` ve `Buton` şablonları bulunur.
- **Kamera ([`makerd/src/camera.rs`](../../makerd/src/camera.rs)):** CSI kamera modülleri (`rpicam-vid` / `rpicam-still` / `libcamera`) ve USB V4L2 kameralar için canlı MJPEG akışı ve fotoğraf çekimi.
- **Grafik:** VC4/V3D KMS/DRM sürücüsü üzerinde Wayland (`labwc`) ve WPE WebKit (`cog`).
