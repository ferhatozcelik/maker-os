# Üçüncü taraf bileşenler ve lisanslar

MakerOS'un kendi kaynak kodu (build betikleri, `image/overlay/` altındaki
yapılandırma, MakerOS Merkezi ve örnek projeler) **GNU GPL v3 veya
sonrası** ile lisanslanmıştır; bkz. [`LICENSE`](LICENSE).

İmaj oluşturulurken aşağıdaki bileşenler kendi lisanslarıyla indirilip imaja
eklenir. Bu depo onların kaynak kodunu içermez; kaynaklarına aşağıdaki
bağlantılardan ulaşılır. Debian/Pardus paketlerinin lisansları kurulu sistemde
`/usr/share/doc/<paket>/copyright` altındadır.

| Bileşen | Lisans | Kaynak |
|---|---|---|
| Pardus 21 (Yirmibir) paket deposu | Paket başına (çoğunlukla GPL/LGPL/MIT/BSD) | [pardus.org.tr](https://www.pardus.org.tr/) · [github.com/pardus](https://github.com/pardus) |
| Pardus GTK/simge temaları, Pardus 100. Yıl (Yüzyıl) teması | GPL-3.0 | [github.com/pardus](https://github.com/pardus) |
| Linux çekirdeği (Raspberry Pi) | GPL-2.0 | [github.com/raspberrypi/linux](https://github.com/raspberrypi/linux) |
| Raspberry Pi açılış yazılımı (`start*.elf`, `fixup*.dat`, `bootcode.bin`) | Broadcom yeniden dağıtım lisansı | [github.com/raspberrypi/firmware](https://github.com/raspberrypi/firmware) (`boot/LICENCE.broadcom`) |
| Wi-Fi/Bluetooth donanım yazılımı (`firmware-brcm80211`, `bluez-firmware`) | Cypress/Broadcom yeniden dağıtım lisansı | [archive.raspberrypi.com](https://archive.raspberrypi.com/debian/) |
| libcamera, libcamera-apps, picamera2 | LGPL-2.1+ / BSD-2-Clause / BSD-2-Clause | [github.com/raspberrypi/libcamera](https://github.com/raspberrypi/libcamera) · [rpicam-apps](https://github.com/raspberrypi/rpicam-apps) · [picamera2](https://github.com/raspberrypi/picamera2) |
| xterm.js 5.5.0, @xterm/addon-fit 0.10.0 | MIT | [github.com/xtermjs/xterm.js](https://github.com/xtermjs/xterm.js) |
| noVNC, websockify | MPL-2.0 / LGPL-3.0 | [github.com/novnc/noVNC](https://github.com/novnc/noVNC) · [websockify](https://github.com/novnc/websockify) |
| x11vnc | GPL-2.0 | [github.com/LibVNC/x11vnc](https://github.com/LibVNC/x11vnc) |
| MediaPipe (Python paketi, örnek projelerde) | Apache-2.0 | [github.com/google-ai-edge/mediapipe](https://github.com/google-ai-edge/mediapipe) |
| MediaPipe modelleri (el, jest, yüz, poz, nesne, segmentasyon) | Apache-2.0 (model kartlarına bakın) | [ai.google.dev/edge/mediapipe](https://ai.google.dev/edge/mediapipe/solutions/guide) |
| uv, CPython | MIT/Apache-2.0, PSF | [github.com/astral-sh/uv](https://github.com/astral-sh/uv) · [python.org](https://www.python.org/) |
| PyTorch, ONNX Runtime, LiteRT, OpenCV, Ultralytics | BSD-3 / MIT / Apache-2.0 / Apache-2.0 / **AGPL-3.0** | [pytorch](https://github.com/pytorch/pytorch) · [onnxruntime](https://github.com/microsoft/onnxruntime) · [LiteRT](https://github.com/google-ai-edge/LiteRT) · [opencv](https://github.com/opencv/opencv) · [ultralytics](https://github.com/ultralytics/ultralytics) |
| Node.js | MIT | [github.com/nodejs/node](https://github.com/nodejs/node) |
| Visual Studio Code (Microsoft derlemesi) | Microsoft yazılım lisansı (kaynak: MIT) | [code.visualstudio.com](https://code.visualstudio.com/License/) · [github.com/microsoft/vscode](https://github.com/microsoft/vscode) |

Agent CLI'ları (Claude Code, Codex CLI, Gemini CLI, OpenCode, Antigravity,
Ollama) imaja gömülmez; kullanıcı Merkez'den kurduğunda kendi lisans ve
kullanım koşulları geçerlidir.

## Marka ve dağıtım notları

MakerOS bağımsız bir topluluk projesidir; TÜBİTAK, Pardus projesi veya
Raspberry Pi Ltd adına geliştirilmez ve bu kuruluşların onayını, ortaklığını
veya resmî desteğini iddia etmez. Pardus adı ve görsel kimliğine ilişkin
haklar ilgili hak sahiplerine aittir. Raspberry Pi is a trademark of
Raspberry Pi Ltd.

Bu tablodaki lisans adları bir envanter özetidir; dağıtılan sürümün kendi
lisans metinleri esastır. Yazılım lisansları marka kullanım izni sağlamaz.
Bu dosyadaki kaynak bağlantıları da kaynak kodu sağlama yükümlülüklerinin
yerine getirildiğine dair bir beyan değildir. Sürüme karşılık gelen
kaynakların ve yeniden dağıtım izinlerinin ayrıca doğrulanması gerekir.

Ayrıntılar ve resmî kaynaklar: [Lisans ve dağıtım notları](docs/license.md).
