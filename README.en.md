<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os">
    <img src="shell/icons/pwa-512.png" alt="MakerOS" width="100">
  </a>
</p>

<h1 align="center">MakerOS</h1>

<p align="center">An independent, open-source Linux operating system for Raspberry Pi inspired by the usability of macOS.</p>

<p align="center"><strong>Make it yours. Build what matters.</strong></p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os/releases/tag/v1.0.0"><img src="https://img.shields.io/badge/release-v1.0.0-f07a1c" alt="Release v1.0.0"></a>
  <img src="https://img.shields.io/badge/platform-Raspberry%20Pi%204%20%7C%20400-f07a1c" alt="Platform: Raspberry Pi 4, 400">
  <img src="https://img.shields.io/badge/base-Buildroot%20%2B%20Linux%206.18-f07a1c" alt="Base: Buildroot and Linux 6.18">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-f07a1c" alt="GPL-3.0 license"></a>
</p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os/releases/latest">Download</a> ·
  <a href="#philosophy">Philosophy</a> ·
  <a href="#features">Features</a> ·
  <a href="#setup-guide">Setup Guide</a> ·
  <a href="#documentation">Documentation</a> ·
  <a href="docs/api.en.md">API</a> ·
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

<p align="center">
  <a href="README.md">Türkçe</a> ·
  <a href="README.en.md">English</a>
</p>

<p align="center">
  <strong>Support the project</strong><br>
  <a href="https://buymeacoffee.com/ferhatozcelik">Buy Me a Coffee</a> ·
  <a href="https://thanks.dev/donations">thanks.dev</a>
</p>

<br>

<table align="center">
  <tr>
    <th align="center" colspan="2">Desktop · Browser & Local AI Assistant</th>
  </tr>
  <tr>
    <td align="center" colspan="2"><img src="docs/screenshots/desktop.png" alt="MakerOS desktop view" width="850"></td>
  </tr>
  <tr>
    <th align="center">Web Interface</th>
    <th align="center">Mobile</th>
  </tr>
  <tr>
    <td align="center" valign="top"><img src="docs/screenshots/web.png" alt="MakerOS Web interface" width="560"></td>
    <td align="center" valign="top"><img src="docs/screenshots/mobile.png" alt="MakerOS Web interface on mobile" width="200"></td>
  </tr>
  <tr>
    <th align="center" colspan="2">Settings · General & Software Update</th>
  </tr>
  <tr>
    <td align="center" colspan="2"><img src="docs/screenshots/update.png" alt="Settings, General: software update" width="640"></td>
  </tr>
</table>

## What is MakerOS?

MakerOS is an independent Linux operating system running on accessible hardware like the Raspberry Pi, designed to give users full ownership and freedom over their computer and digital workspace. Instead of layering a theme onto a generic distribution, MakerOS builds its own image from the ground up—unifying its boot chain, Rust system services (`makerd`), web desktop shell, A/B OTA update system, and offline AI assistant into a single cohesive platform.

It draws inspiration from the clarity and ease of use of macOS, while maintaining its own identity, lightweight technical architecture, and open-source maker philosophy.

<a id="philosophy"></a>

## Philosophy, Story & Manifesto

**Computing should be personal again.** Modern operating systems increasingly lock users inside black-box ecosystems and opaque background services. MakerOS started on a Raspberry Pi with a simple question: *"Can we design a computer that truly belongs to us?"* Our goal is to make capable computing accessible without expensive hardware—providing a transparent system that you can understand, modify, and use to build what matters to you.

MakerOS is guided by five core principles: **Freedom** (you own your machine), **Simplicity** (powerful tools without unnecessary complexity), **Productivity** (built for creation, not just consumption), **Transparency** (clear visibility into services, hardware, and state), and **Accessibility** (engineered for affordable ARM64 devices). Artificial intelligence in MakerOS is designed to expand human capability rather than take control away from the user.

> **Manifesto:** We do not believe computers exist merely for consumption, nor that great design must come at the cost of user freedom. Simplicity and openness belong together. Technology should not tell you what to do—it should expand what you can build.
>
> **Make it yours. Build what matters.**

## Download

[Release 1.0.0](https://github.com/ferhatozcelik/maker-os/releases/latest): `makeros-1.0.0-rpi4.img.xz` (Raspberry Pi 4 / 400, 64-bit). Verify after download with `sha256sum -c makeros-*.img.xz.sha256`.

<a id="features"></a>

## Features

| Area | Overview |
| --- | --- |
| **Base System** | Custom Buildroot + Linux 6.18 (ARM64) distribution; read-only EROFS rootfs, persistent ext4 `/data` partition, RAUC + `tryboot` A/B updates, systemd |
| **Desktop & Web** | Shared experience on local display (Cog/WPE WebKit) and remote browsers: windowed **Desktop** (`/`) and mobile-ready **Web** (`/web`) interface |
| **Rust Backend** | `makerd` (HTTP/WS API), `makerd-priv` (privileged operations), `makerd-user` (user processes & PTY terminal), `makeros-ai` (local AI daemon) |
| **Local AI Assistant** | Offline assistant powered by Qwen2.5-0.5B-Instruct GGUF + `llama.cpp`; conversational mode, MakerOS knowledge cards, context reset, risk classification, and Full Bypass / Ask execution modes |
| **Python & Automation** | `uv`-based virtual environment and project manager with ready-made GPIO/Camera/Web/AI templates; **Autostart** and **Cron** scheduler for boot and periodic execution |
| **Hardware & Camera** | Live 40-pin GPIO control, I²C, SPI, UART; live MJPEG streaming and photo capture for CSI and USB cameras |
| **Plugins & Themes** | Python plugin architecture, Plugin Store (`makeros-plugins`), `.zip` import/export; Maker (default), Hardware, Cyber, 3D Print, and Konya HackerSpace themes, flat low-GPU mode, and granular System UI toggles |

<a id="documentation"></a>

## Documentation Tree

All technical architecture, verification, and API documents are maintained side by side in Turkish and English under [`docs/`](docs/README.en.md):

```text
maker-os/
├── README.md · README.en.md
├── docs/
│   ├── README.md · README.en.md
│   ├── architecture/
│   │   ├── overview.md · overview.en.md
│   │   ├── boot-and-storage.md · boot-and-storage.en.md
│   │   ├── desktop.md · desktop.en.md
│   │   ├── backend.md · backend.en.md
│   │   ├── ai-assistant.md · ai-assistant.en.md
│   │   ├── network-security.md · network-security.en.md
│   │   └── hardware.md · hardware.en.md
│   ├── development/
│   │   └── build-and-test.md · build-and-test.en.md
│   ├── verification/
│   │   └── validation.md · validation.en.md
│   ├── project/
│   │   └── branding-and-licensing.md · branding-and-licensing.en.md
│   └── api.md · api.en.md
├── os/         # Buildroot external tree, kernel, packages, and image builder
├── makerd/     # Rust system services and local AI assistant
└── shell/      # Web desktop shell and built-in applications
```

<a id="setup-guide"></a>

## Setup Guide

1. **Flash the Image:** Download `makeros-1.0.0-rpi4.img.xz` from [Releases](https://github.com/ferhatozcelik/maker-os/releases/latest) and flash it to an SD card using Raspberry Pi Imager (*Use custom*). Optionally copy `wifi-example.txt` to `wifi.txt` on the `MOSCFG` partition to preconfigure Wi-Fi.
2. **Boot & Connect:** Insert the SD card into your Raspberry Pi 4 / 400 and power it on. Open `http://maker-os.local/` on your local network, `http://10.55.0.1/` over USB-C, or connect to the open `MakerOS` setup Wi-Fi hotspot (`http://10.42.0.1/kurulum`).
3. **Sign In:** Default credentials are **`makeros` / `makeros`**. Change your password anytime under **Settings › User**.
4. **OTA Updates:** Check and install signed `.raucb` updates from **Settings › General › Software Update** with automatic `tryboot` rollback protection.

<a id="build-from-source"></a>

## Build from Source & Quick Commands

```bash
make -C makerd preview     # local preview: makerd (mock) + shell → http://localhost:8080/
make -C shell smoke        # headless Chromium smoke test in Docker
./os/build.sh build        # build Raspberry Pi image → os/out/makeros-1.0.0-rpi4.img.xz
```

See [`docs/development/build-and-test.en.md`](docs/development/build-and-test.en.md) for full build instructions.

## Validation Status

The desktop shell (`shell/`), `makerd` service family, local AI assistant (`makeros-ai`), terminal sessions, Python Manager, Autostart / Cron scheduler, and OTA update flow have been verified via automated Docker smoke tests and on real Raspberry Pi 4 hardware. See [`docs/verification/validation.en.md`](docs/verification/validation.en.md) for verified features versus future roadmap items.

## License & Trademark Notice

[GPL-3.0-or-later](LICENSE) — See [`NOTICE.md`](NOTICE.md) and [`docs/project/branding-and-licensing.en.md`](docs/project/branding-and-licensing.en.md) for third-party notices.

**MakerOS is an independent open-source project.** It is not affiliated with, endorsed by, or sponsored by Raspberry Pi Ltd. **Raspberry Pi is a trademark of Raspberry Pi Ltd.**
