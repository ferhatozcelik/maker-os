# Üçüncü taraf bileşenler ve lisanslar

MakerOS'un kendi kaynak kodu (makerd, web kabuğu, `os/` altındaki derleme betikleri ve
yapılandırmalar, örnek projeler) **GNU GPL v3 veya sonrası** ile lisanslanmıştır; bkz.
[`LICENSE`](LICENSE).

MakerOS 2 imajı Buildroot ile kaynaktan derlenir. Buildroot'un indirip derlediği paketlerin
lisansları ve kaynakları `./os/build.sh make legal-info` ile `output/legal-info/` altında
listelenir (paket başına lisans metinleri ve kaynak arşivleri). Aşağıdaki tablo başlıca
bileşenlerin özetidir; dağıtılan sürümün kendi lisans metinleri esastır.

## İmajdaki başlıca bileşenler

| Bileşen | Lisans | Kaynak |
|---|---|---|
| Buildroot 2026.02.3 (derleme sistemi) | GPL-2.0 | [buildroot.org](https://buildroot.org/) |
| Linux çekirdeği (Raspberry Pi) | GPL-2.0 | [github.com/raspberrypi/linux](https://github.com/raspberrypi/linux) |
| Raspberry Pi açılış yazılımı (`start*.elf`, `fixup*.dat`) | Broadcom yeniden dağıtım lisansı | [github.com/raspberrypi/firmware](https://github.com/raspberrypi/firmware) (`boot/LICENCE.broadcom`) |
| Wi-Fi/Bluetooth donanım yazılımı (brcmfmac) | Cypress/Broadcom yeniden dağıtım lisansı | [github.com/RPi-Distro/firmware-nonfree](https://github.com/RPi-Distro/firmware-nonfree) |
| glibc, GCC çalışma kütüphaneleri | LGPL-2.1+ / GPL-3.0 + çalışma zamanı istisnası | [gnu.org](https://www.gnu.org/) |
| systemd, polkit, dbus-broker | LGPL-2.1+ / LGPL-2.0+ / Apache-2.0 | [systemd](https://github.com/systemd/systemd) · [polkit](https://github.com/polkit-org/polkit) · [dbus-broker](https://github.com/bus1/dbus-broker) |
| RAUC (A/B güncelleme) | LGPL-2.1 | [github.com/rauc/rauc](https://github.com/rauc/rauc) |
| NetworkManager, wpa_supplicant, dnsmasq, nftables | GPL-2.0+ / BSD-3 / GPL-2.0 / GPL-2.0 | [NetworkManager](https://gitlab.freedesktop.org/NetworkManager/NetworkManager) · [hostap](https://w1.fi/) · [dnsmasq](https://thekelleys.org.uk/dnsmasq/) · [nftables](https://netfilter.org/) |
| labwc 0.9.8, wlroots | GPL-2.0 / MIT | [github.com/labwc/labwc](https://github.com/labwc/labwc) · [gitlab.freedesktop.org/wlroots](https://gitlab.freedesktop.org/wlroots/wlroots) |
| wayvnc 0.10.2, neatvnc 1.0.4, aml 1.0.0 | ISC | [github.com/any1](https://github.com/any1/wayvnc) |
| Mesa (V3D/VC4), Wayland, libinput | MIT | [mesa3d.org](https://mesa3d.org/) · [wayland.freedesktop.org](https://wayland.freedesktop.org/) |
| WPE WebKit, cog (Pi ekranındaki kabuk tarayıcısı) | BSD-2 / LGPL-2.1 (WebCore) / MIT (cog) | [wpewebkit.org](https://wpewebkit.org/) · [github.com/Igalia/cog](https://github.com/Igalia/cog) |
| Chromium 156 (Debian trixie arm64 derlemesi) ve birlikte gelen kütüphaneler | BSD-3 ve çeşitli; paket başına `/opt/chromium/share/doc/<paket>/copyright` | [chromium.org](https://www.chromium.org/) · [packages.debian.org/trixie/chromium](https://packages.debian.org/trixie/chromium) |
| libcamera, rpicam-apps | LGPL-2.1+ / BSD-2 | [github.com/raspberrypi/libcamera](https://github.com/raspberrypi/libcamera) · [rpicam-apps](https://github.com/raspberrypi/rpicam-apps) |
| libgpiod, i2c-tools | LGPL-2.1+ / GPL-2.0+ | [libgpiod](https://git.kernel.org/pub/scm/libs/libgpiod/libgpiod.git) · [i2c-tools](https://git.kernel.org/pub/scm/utils/i2c-tools/i2c-tools.git) |
| Python 3, uv | PSF-2.0 / MIT veya Apache-2.0 | [python.org](https://www.python.org/) · [github.com/astral-sh/uv](https://github.com/astral-sh/uv) |
| Node.js | MIT | [github.com/nodejs/node](https://github.com/nodejs/node) |
| Podman, crun (geliştirici kutusu `mos box`) | Apache-2.0 / GPL-2.0+ | [podman.io](https://podman.io/) · [github.com/containers/crun](https://github.com/containers/crun) |

## Web kabuğu ve makerd'ye gömülü bileşenler

| Bileşen | Lisans | Kaynak |
|---|---|---|
| noVNC 1.7.0 (pako dahil) | MPL-2.0 (pako: MIT) | [github.com/novnc/noVNC](https://github.com/novnc/noVNC) |
| xterm.js 5.5.0, @xterm/addon-fit 0.10.0 | MIT | [github.com/xtermjs/xterm.js](https://github.com/xtermjs/xterm.js) |
| Lucide simgeleri (alt küme) | ISC | [lucide.dev](https://lucide.dev/) |
| Inter, JetBrains Mono yazı tipleri | SIL OFL 1.1 | [rsms.me/inter](https://rsms.me/inter/) · [jetbrains.com/mono](https://www.jetbrains.com/lp/mono/) |
| Go modülleri: coder/websocket, creack/pty, godbus/dbus, go-gpiocdev, golang.org/x/sys | ISC / MIT / BSD-2 / MIT / BSD-3 | `makerd/go.mod` |

Sürümler ve dosya özetleri: [`shell/vendor/VERSIONS.md`](shell/vendor/VERSIONS.md).

AI ajanları (Claude Code, Codex CLI, Gemini CLI, OpenCode, Ollama) ve Python projelerinin
bağımlılıkları (ör. OpenCV, ONNX Runtime) imaja gömülmez; kullanıcı kurduğunda kendi lisans ve
kullanım koşulları geçerlidir.

## Marka ve dağıtım notları

MakerOS bağımsız bir topluluk projesidir; Raspberry Pi Ltd adına geliştirilmez ve onayını,
ortaklığını veya resmî desteğini iddia etmez. **Raspberry Pi is a trademark of Raspberry Pi Ltd.**
Chromium adı Google LLC'nin markasıdır; MakerOS, Debian'ın derlediği açık kaynak Chromium'u
değiştirmeden paketler ve Chromium logosunu kullanmaz. Konya HackerSpace adı ve işaretine
ilişkin haklar Konya HackerSpace'e aittir.

Bu tablodaki lisans adları bir envanter özetidir. Yazılım lisansları marka kullanım izni
sağlamaz. Bu dosyadaki kaynak bağlantıları, kaynak kodu sağlama yükümlülüklerinin yerine
getirildiğine dair bir beyan değildir; sürüme karşılık gelen kaynakların ve yeniden dağıtım
izinlerinin ayrıca doğrulanması gerekir.
