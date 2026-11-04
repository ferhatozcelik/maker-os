<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os">
    <img src="shell/icons/logo.svg" alt="MakerOS logo" width="100">
  </a>
</p>

<h1 align="center">MakerOS</h1>

<p align="center">A maker operating system for the Raspberry Pi: students, makers and AI projects.</p>

<p align="center"><strong>Flash the SD card. Power on the Pi. Connect from a browser.</strong></p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os/releases/latest"><img src="https://img.shields.io/github/v/release/ferhatozcelik/maker-os?label=release&color=f07a1c" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Raspberry%20Pi-4%20%7C%20400-f07a1c" alt="Raspberry Pi 4, 400">
  <img src="https://img.shields.io/badge/base-Buildroot%20%2B%20Linux%206.18-f07a1c" alt="Base: Buildroot and Linux 6.18">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-f07a1c" alt="GPL-3.0 license"></a>
</p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os/releases/latest">Download</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#features">Features</a> ·
  <a href="#building-from-source">Building from source</a> ·
  <a href="docs/v2/api.md">API</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

<p align="center">
  <a href="README.md">Türkçe</a> ·
  <a href="README.en.md">English</a>
</p>

<br>

<table align="center">
  <tr><th align="center" colspan="2">Desktop: Chromium window (in the browser)</th></tr>
  <tr><td align="center" colspan="2"><img src="docs/v2/screenshots/desktop.png" alt="Chromium window on the MakerOS desktop" width="850"></td></tr>
  <tr>
    <th align="center">Web interface</th>
    <th align="center">Phone</th>
  </tr>
  <tr>
    <td align="center" valign="top"><img src="docs/v2/screenshots/web.png" alt="MakerOS web interface home" width="560"></td>
    <td align="center" valign="top"><img src="docs/v2/screenshots/mobile.png" alt="MakerOS web interface on a phone" width="200"></td>
  </tr>
  <tr><th align="center" colspan="2">Software update</th></tr>
  <tr><td align="center" colspan="2"><img src="docs/v2/screenshots/update.png" alt="Settings, General: software update" width="640"></td></tr>
</table>

Screenshots were taken from the current build in Docker; device data such as temperature, memory
and network are sample values (the interface is in Turkish by default, English is available).
Hardware tests of the camera, Wi-Fi and GPIO need a Raspberry Pi.

## Download

[Latest release](https://github.com/ferhatozcelik/maker-os/releases/latest): `makeros-<version>-rpi4.img.xz`
(Raspberry Pi 4 / 400, 64-bit). Verify with `sha256sum -c makeros-*.img.xz.sha256`.

## Features

| Area | What you get |
| --- | --- |
| Base | Independent system built with Buildroot on Linux 6.18 (Raspberry Pi); read-only EROFS root file system, A/B partitions, systemd |
| Two views | One address (`http://makeros.local/`): the windowed **Screen** (desktop, dock, windows) and the phone-friendly **Web** interface, switched with *Web \| Screen*. Installable as a web app (PWA) |
| Chromium | A normal window on the desktop: the real browser running on the Pi, drawn on a separate invisible display with only that window streamed; resizes with the window and stops when closed |
| Files and terminal | Upload/download; click a `.py` file to run it. The web terminal shares its session with the Pi's screen, with extra keys for phones |
| Python | uv projects with virtual environments and Python versions; templates: LED, button, I2C scanner, camera, web, AI object detection |
| AI agents | Claude Code, Codex CLI, Gemini CLI, OpenCode and Ollama (local models): install, update, run |
| Hardware | GPIO (40-pin), I2C, SPI, UART; Pi camera and USB cameras (libcamera) |
| Plugins | Python plugins, zip import/export, template; plugins can add themes, menu bar indicators and desktop cards |
| Software update | Settings › General finds a new release on GitHub, downloads and verifies the signed package and installs it to the spare partition. If the new version fails to start, the Pi returns to the previous one; files and settings are kept |
| Network | Ethernet, Wi-Fi, hidden networks, WPA2-Enterprise; with no network an open `MakerOS` setup Wi-Fi and setup page |
| Monitoring | Activity Monitor: CPU, memory, temperature, processes |
| Themes | Maker, Hackerspace, Hardware, Cybersecurity, 3D printing; light/dark appearance, accent colour |

## Installation

1. Download `makeros-<version>-rpi4.img.xz` from the
   [latest release](https://github.com/ferhatozcelik/maker-os/releases/latest).
2. Write it to an SD card with Raspberry Pi Imager (*Use custom*). Imager's OS customisation settings
   are not used by MakerOS.
3. *(Optional)* On the card's `MOSCFG` partition, copy `wifi-example.txt` to `wifi.txt` and fill in
   your network name and password. It is read on first boot and deleted for safety.
4. Power on the Pi and open **http://makeros.local/** in a browser. Without a cable or `wifi.txt`, join
   the `MakerOS` Wi-Fi with your phone; the setup page opens (or `http://10.42.0.1/kurulum`).
5. In the first-boot wizard choose region and keyboard, Wi-Fi if needed, then create your account and
   set the device name. There is no default password; the first account is the administrator.

**Hardware:** Raspberry Pi 4 Model B and Raspberry Pi 400. A/B updates need Pi bootloader (EEPROM)
2025-03 or newer.

## Verification status

The interface, makerd and the Chromium window have been tested end to end **in Docker** with the
built root file system. There is no performance data measured on real Raspberry Pi hardware yet; for
the Docker comparison see [docs/v2/BENCHMARK.md](docs/v2/BENCHMARK.md). Power-loss and corrupt-package
tests of the signed update on hardware are ongoing in the first releases. Please report problems on
[Issues](https://github.com/ferhatozcelik/maker-os/issues).

## Building from source

All builds and tests run in Docker; nothing is installed on the host.

```bash
./os/run-docker.sh                    # run the built root file system in Docker → http://localhost/
docker-compose up -d --build          # plain preview: makerd (sample data) + shell → http://localhost/
make -C makerd test                   # makerd tests
make -C shell smoke                   # shell smoke test (headless Chromium)
./os/build.sh build                   # Raspberry Pi image → os/out/makeros-<version>-rpi4.img.xz
```

The Chromium bundle is prepared once before building:
`docker run --rm -v "$PWD/os/chromium":/s:ro -v "$PWD/os/out":/out debian:trixie sh /s/build-bundle.sh`

The signed update package (`.raucb`) is produced only when the signing key is present
(`os/keys/rauc-key.pem`, never committed). Releases are published by GitHub Actions: bump `os/VERSION`
and push to `main`; the image and package are built and a Release is created
([.github/workflows/release.yml](.github/workflows/release.yml)).

## License and trademarks

The original source code, build scripts, configuration and examples developed in this repository are
provided under the **GNU General Public License version 3 or any later version** (`GPL-3.0-or-later`);
see [`LICENSE`](LICENSE). This does not mean all software in the image has the same license: the
Linux kernel, Raspberry Pi firmware, Chromium, WPE WebKit, Mesa and other third-party components
belong to their owners and licenses; see [`NOTICE.md`](NOTICE.md).

**MakerOS is an independent community project.** It is not developed on behalf of Raspberry Pi Ltd
and claims no partnership, sponsorship, certification, endorsement or official support.
**Raspberry Pi is a trademark of Raspberry Pi Ltd.** References to the trademark only identify the
target hardware; the software license grants no trademark or logo rights. See the
[Raspberry Pi trademark rules](https://www.raspberrypi.com/trademark-rules/). All rights to the
Konya HackerSpace name and mark used in the default plugin belong to Konya HackerSpace.

The software is provided **as is, without warranty**, to the extent permitted by applicable law and as
stated in the licenses.
