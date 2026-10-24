<p align="center">
  <a href="https://github.com/ferhatozcelik/pardus-maker">
    <img src="docs/assets/pardus-maker-logo.svg" alt="Pardus Maker logo" width="100">
  </a>
</p>

<h1 align="center">Pardus Maker</h1>

<p align="center">Ready-to-use Pardus for Raspberry Pi, no installation: for students, makers and AI projects.</p>

<p align="center"><strong>Flash the SD card. Power on the Pi. Connect from a browser.</strong></p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/pardus-maker/releases/tag/v1.0.0"><img src="https://img.shields.io/badge/release-v1.0.0-c8102e" alt="Release v1.0.0"></a>
  <img src="https://img.shields.io/badge/Raspberry%20Pi-4%20%7C%20400%20%7C%203B%2B-c8102e" alt="Raspberry Pi 4, 400, 3B+">
  <img src="https://img.shields.io/badge/base-Pardus%2021%20arm64-c8102e" alt="Base: Pardus 21 arm64">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-c8102e" alt="GPL-3.0 license"></a>
</p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/pardus-maker/releases/tag/v1.0.0">Download</a> ·
  <a href="#setup-guide">Setup guide</a> ·
  <a href="#features">Features</a> ·
  <a href="#building-from-source">Build</a> ·
  <a href="docs/README.md">Docs (Turkish)</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

<p align="center">
  <a href="README.md">Türkçe</a> ·
  <a href="README.en.md">English</a>
</p>

<br>

<table align="center">
  <tr>
    <th align="center">Pardus Maker Center</th>
    <th align="center">Phone · Wi-Fi</th>
    <th align="center">Phone · Terminal</th>
  </tr>
  <tr>
    <td align="center" valign="top"><img src="docs/screenshots/merkez.png" alt="Pardus Maker Center overview" width="420"></td>
    <td align="center" valign="top"><img src="docs/screenshots/telefon-wifi.png" alt="Wi-Fi and network settings on a phone" width="190"></td>
    <td align="center" valign="top"><img src="docs/screenshots/telefon-terminal.png" alt="Web terminal with extra key bar on a phone" width="190"></td>
  </tr>
  <tr><th align="center" colspan="3">Remote desktop (in the browser)</th></tr>
  <tr><td align="center" colspan="3"><img src="docs/screenshots/masaustu.png" alt="Pardus desktop opened from a browser" width="850"></td></tr>
</table>

<table align="center">
  <tr><th>Python and AI environment</th><th>MediaPipe project templates</th></tr>
  <tr>
    <td><img src="docs/screenshots/python.png" alt="Python versions and ready AI environment" width="420"></td>
    <td><img src="docs/screenshots/projeler.png" alt="Creating a hand tracking project from a MediaPipe template" width="420"></td>
  </tr>
</table>

Screenshots show the current build running in Docker, with desktop and phone viewports.
Camera, Wi-Fi and GPIO require a Raspberry Pi for hardware testing.
See the [screenshot gallery](docs/ekran-goruntuleri.md).

## Download

[Release 1.0.0](https://github.com/ferhatozcelik/pardus-maker/releases/tag/v1.0.0): `pardus-maker-<version>.img.xz` (Raspberry Pi 4 / 400 / 3B+, 64-bit).
Verify it with `sha256sum -c pardus-maker-*.img.xz.sha256`.

The user interface is in Turkish; the desktop language and keyboard layout can
be changed under **Center → Dil ve Klavye** (Language and Keyboard).

## Features

| Area | Contents |
| --- | --- |
| Base | Pardus 21 "Yirmibir" (Debian 11) arm64 + Pardus security repository, current Raspberry Pi kernel (`-v8+`) |
| Desktop | XFCE in a macOS-like layout (top bar + Plank dock), Pardus **100th Year** theme (light/dark), Chromium, VS Code, Files, Terminal |
| Pardus Maker Center | One phone-friendly web app (port 80): settings, network, date/time, language/keyboard, Python, projects, agents, terminal, camera, USB, GPIO, remote desktop |
| Network | Ethernet (status, speed), Wi-Fi scan, hidden networks, WPA2-Enterprise (eduroam), saved networks; falls back to a `Pardus-Maker-XXXX` hotspot |
| Web terminal | A real shell in the browser (xterm.js) with Esc/Tab/Ctrl/arrow keys for phones; runs as `maker`, not root |
| Camera | Raspberry Pi camera modules (libcamera, picamera2) and USB webcams: live view, full-resolution photos, gallery |
| MediaPipe | 7 ready-made camera projects: hand tracking, gesture-controlled LED (GPIO), faces, pose/exercise counter, object detection, background segmentation, face mesh. Models are bundled, no internet needed |
| Python and AI | Any Python version in one click with uv; ready AI environment (`ai-env`): PyTorch, ONNX Runtime, LiteRT, OpenCV, Ultralytics, Jupyter Lab |
| Hardware | GPIO without sudo, I2C, SPI, UART; mount/safely eject USB drives, Arduino/ESP32 serial ports; HDMI 0/1, DSI touch display, HDMI/3.5 mm audio |
| Agents | Claude Code, Codex CLI, Gemini CLI, OpenCode, Antigravity, Ollama: install, update and launch from the Center |
| Remote access | Web (desktop + all settings), VNC (5900), SSH, mDNS (`pardus-maker.local`) |

## Setup guide

1. Download the `.img.xz` from the [release page](https://github.com/ferhatozcelik/pardus-maker/releases/tag/v1.0.0) (16 GB+ card recommended) and write it with [Raspberry Pi Imager](https://www.raspberrypi.com/software/) → **Use custom**. Do **not** use Imager's OS customisation. balenaEtcher also works.
2. Optional: copy `wifi-ornek.txt` on the `bootfs` partition to `wifi.txt` and fill in `SSID` and `PAROLA` (password).
3. Power on the Pi. On first boot the root partition grows to fill the card and SSH host keys are generated (~1 min).
4. Connect:
   - Ethernet or `wifi.txt`: `http://pardus-maker.local/`
   - No network: after ~45 s join the `Pardus-Maker-XXXX` Wi-Fi (password `pardus1234`) → `http://10.42.0.1/`
5. Log in as `maker` / `maker` and change the password first under **Center → Kullanıcı** (User). Web and VNC traffic is not encrypted; use them on a trusted local network only.

### Camera and MediaPipe

Plug the camera into the **CAMERA** connector while the Pi is off, then create a
project under **Center → Projeler** with one of the **MediaPipe** templates and
run it:

```bash
uv run main.py                       # Pi camera or USB camera, detected automatically
uv run main.py --kaynak video.mp4    # use a video file instead
```

The result is shown in a window on the Pi and live at
`http://pardus-maker.local:8000/` on your phone.

### Hardware support

| Model | Status |
| --- | --- |
| Raspberry Pi 4 Model B | Target platform |
| Raspberry Pi 400 | Supported |
| Raspberry Pi 3B+ | Works; AI and MediaPipe are slow |
| Raspberry Pi 5 | Not supported yet |

## Building from source

On an Apple Silicon Mac (arm64, no qemu) or arm64 Linux with Docker:

```bash
colima start --arch aarch64 --cpu 6 --memory 8 --disk 60   # macOS only
cd image
./build-docker.sh          # all stages → image/out/pardus-maker-<version>.img.xz
./build-docker.sh 06       # resume at configuration; also runs stages 07–09
./test/run-test.sh         # run the image's root filesystem in Docker → http://localhost:8080/
```

See the [Turkish README](README.md#kaynaktan-derleme) for the stage list.

## License and trademarks

Original code, build scripts, configuration and examples developed in this
repository are licensed under the **GNU General Public License, version 3
or any later version** (`GPL-3.0-or-later`). See [`LICENSE`](LICENSE) for the
permissions and conditions for use, modification and redistribution.

Third-party packages, the kernel, firmware, themes and models retain their
own licenses and ownership. See [`NOTICE.md`](NOTICE.md) for components and
source references. Redistributors must preserve the relevant notices and
meet the source provision requirements of each applicable license. A link
to this repository or an upstream homepage alone may not meet those requirements.

**Pardus Maker is an independent community project.** It is not an official
TÜBİTAK or Pardus release, product or service, and does not claim affiliation,
sponsorship, certification, approval or official support. Project-specific
issues belong in this repository's
[issue tracker](https://github.com/ferhatozcelik/pardus-maker/issues).

Rights in the Pardus name and visual identity remain with their respective
holders. **Raspberry Pi is a trademark of Raspberry Pi Ltd.** Brand references
identify the software base and target hardware. Software licenses do not
grant trademark permissions, and this notice does not replace any required
permission. Names, logos and branded themes remain subject to the relevant
holders' terms, including the
[Raspberry Pi trademark rules](https://www.raspberrypi.com/trademark-rules/).

Software is provided **as is, without warranty**, to the extent permitted by
applicable law and under the applicable licenses. This notice does not
limit mandatory statutory rights or certify legal compliance. See the
[detailed licensing notes (Turkish)](docs/lisans.md).
