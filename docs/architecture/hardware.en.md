# Hardware Support

[Türkçe](hardware.md) · [English](hardware.en.md)

MakerOS primarily targets **Pi 4 / 400 compatible** ARM64 boards (BCM2711 SoC), while keeping its service and shell architecture portable to other ARM64 single-board computers.

## Supported Interfaces

- **GPIO (40-pin, [`makerd/src/gpio.rs`](../../makerd/src/gpio.rs)):** Live pin state inspection and `HIGH` / `LOW` output control via `/dev/gpiochip0` (`gpiocdev` v2 character device).
- **I²C, SPI & UART:** Enabled by default in the kernel configuration, with ready-to-run `I2C Scanner`, `LED`, and `Button` templates in Python Manager.
- **Camera ([`makerd/src/camera.rs`](../../makerd/src/camera.rs)):** Live MJPEG streaming and still capture for CSI camera modules (`rpicam-vid` / `rpicam-still` / `libcamera`) and USB V4L2 webcams.
- **Graphics:** Wayland (`labwc`) and WPE WebKit (`cog`) running on the VC4/V3D KMS/DRM stack.
