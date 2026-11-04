# MakerOS

[Türkçe](README.md)

**A maker operating system for the Raspberry Pi 4.** Use the Pi without a monitor from any
browser on the same network (computer, tablet, phone): files, terminal, Python projects, GPIO,
camera, AI agents and Chromium in one web desktop.

MakerOS 2 is an independent system built from scratch with Buildroot on the Linux kernel, with a
read-only root file system, A/B partitions and signed software updates. (MakerOS 1.x was based on
Pardus; this repository now contains MakerOS 2 only. The 1.x releases remain on the
[Releases](https://github.com/ferhatozcelik/maker-os/releases) page.)

![MakerOS desktop: Chromium window](docs/v2/screenshots/desktop.png)

| Web interface | On a phone | Software update |
|---|---|---|
| ![Web interface](docs/v2/screenshots/web.png) | ![Phone](docs/v2/screenshots/mobile.png) | ![Update](docs/v2/screenshots/update.png) |

> Screenshots are from the Docker preview; device data such as temperature, memory and network
> are mock values.

## Features

- **Two views, one address** (`http://makeros.local/`): the windowed **Screen** (desktop, dock,
  windows) and the phone-friendly **Web** interface, switched with *Web | Screen* at the top.
  Installable as a web app (PWA).
- **Chromium** opens as a normal window on the desktop. It is the real browser running on the Pi,
  drawn on a separate invisible display; only that window is streamed to you. It stops when closed.
- **Apps:** Files (click a `.py` file to run it), Terminal (shared session between the web and the
  Pi's screen), Python (uv projects with virtual environments and templates: LED, button, I2C,
  camera, AI object detection), AI Agents (Claude Code, Codex CLI, Gemini CLI, OpenCode, Ollama),
  Camera, GPIO (40-pin), Plugins (zip import/export), Activity Monitor, Settings.
- **Screen Mode** (off by default) shows the Pi's own screen in the browser. An administrator turns
  it on in Settings › Screen Mode; no extra port is opened and nothing runs in the background while
  it is off. Without a monitor the Pi's screen fits your browser window; on phones it turns landscape.
- **Software update:** Settings › General › Software update finds a new release on GitHub, downloads
  the signed package, installs it to the spare partition and restarts. If the new version fails to
  start, the Pi returns to the previous one automatically. Your files and settings are kept.
- **Headless setup:** with no network the Pi opens an open `MakerOS` Wi-Fi; connect your phone and
  the setup page appears.
- **Themes:** Maker, Hackerspace, Hardware, Cybersecurity, 3D printing; more via plugins.

## Installation

1. Download the latest `makeros-<version>-rpi4.img.xz` from
   [Releases](https://github.com/ferhatozcelik/maker-os/releases) (verify with `.sha256`).
2. Write it to an SD card with Raspberry Pi Imager (*Use custom*). Imager's OS customisation
   settings are not used by MakerOS.
3. *(Optional)* On the card's `MOSCFG` partition, copy `wifi-example.txt` to `wifi.txt` and fill in
   your network name and password. It is read on first boot and deleted for safety.
4. Power on the Pi and open **http://makeros.local/** in a browser. Without a cable or `wifi.txt`,
   join the `MakerOS` Wi-Fi with your phone; the setup page opens (or `http://10.42.0.1/kurulum`).
5. In the first-boot wizard choose region and keyboard, Wi-Fi if needed, then create your account
   and set the device name. There is no default password; the first account is the administrator.

**Hardware:** Raspberry Pi 4 Model B and Raspberry Pi 400 (aarch64). A/B updates need Pi bootloader
(EEPROM) 2025-03 or newer. Cards running MakerOS 1.x cannot be updated to MakerOS 2 over the air;
re-flash the card.

## Verification status

To be transparent: the MakerOS 2 interface, makerd and the Chromium window have been tested end to
end **in Docker** with the built root file system. There is no performance data measured on real
Raspberry Pi hardware yet; for the Docker comparison see
[docs/v2/BENCHMARK.md](docs/v2/BENCHMARK.md). Power-loss and corrupt-package tests of the signed
update on hardware are ongoing in the first releases. Please report problems on
[Issues](https://github.com/ferhatozcelik/maker-os/issues).

## Building from source

All builds and tests run in Docker; nothing is installed on the host.

```bash
./os/run-docker.sh                    # run the built root file system in Docker → http://localhost/
docker-compose up -d --build          # plain preview: makerd (mock data) + shell → http://localhost/
make -C makerd test                   # makerd tests
make -C shell smoke                   # shell smoke test (headless Chromium)
./os/build.sh build                   # Raspberry Pi image → os/out/makeros-<version>-rpi4.img.xz
```

The signed update package (`.raucb`) is produced only when the signing key is present
(`os/keys/rauc-key.pem`, never committed). Releases are published by GitHub Actions: bump
`os/VERSION` and push to `main`; the image and package are built and a Release is created
(see [.github/workflows/release.yml](.github/workflows/release.yml)).

## License and trademarks

The original source code, build scripts, configuration and examples developed in this repository
are provided under the **GNU General Public License version 3 or any later version**
(`GPL-3.0-or-later`); see [`LICENSE`](LICENSE). This does not mean all software in the image has
the same license: the Linux kernel, Raspberry Pi firmware, Chromium, WPE WebKit, Mesa and other
third-party components belong to their owners and licenses; see [`NOTICE.md`](NOTICE.md).

**MakerOS is an independent community project.** It is not developed on behalf of Raspberry Pi Ltd
and claims no partnership, sponsorship, certification, endorsement or official support.
**Raspberry Pi is a trademark of Raspberry Pi Ltd.** References to the trademark only identify the
target hardware; the software license grants no trademark or logo rights. See the
[Raspberry Pi trademark rules](https://www.raspberrypi.com/trademark-rules/). All rights to the
Konya HackerSpace name and mark used in the default plugin belong to Konya HackerSpace.

The software is provided **as is, without warranty**, to the extent permitted by applicable law and
as stated in the licenses.
