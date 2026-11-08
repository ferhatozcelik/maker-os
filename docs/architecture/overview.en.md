# Architecture Overview

[Türkçe](overview.md) · [English](overview.en.md)

MakerOS is built from the ground up using Buildroot rather than layering packages onto an existing Linux distribution. It is organized into three tightly integrated layers: the operating system image tree ([`os/`](../../os/)), Rust system services ([`makerd/`](../../makerd/)), and the web desktop shell ([`shell/`](../../shell/)).

## Core Components

| Component | Technology | Source |
| --- | --- | --- |
| Build system | Buildroot 2026.02 LTS | [`os/`](../../os/) |
| Kernel | Linux 6.18 (ARM64) | [`os/board/rpi4/`](../../os/board/rpi4/) |
| Init & supervision | systemd | [`os/board/rpi4/rootfs-overlay/`](../../os/board/rpi4/rootfs-overlay/) |
| Filesystems | Read-only EROFS rootfs + writable ext4 (`/data`) | [`os/board/rpi4/genimage.cfg.in`](../../os/board/rpi4/genimage.cfg.in) |
| OTA updates | RAUC + bootloader `tryboot` | [`makerd/src/ota.rs`](../../makerd/src/ota.rs) |
| Backend services | Rust (`makerd`, `makerd-priv`, `makerd-user`, `makeros-ai`) | [`makerd/src/`](../../makerd/src/) |
| Local display | Wayland (`labwc`) + Cog (WPE WebKit) | [`os/package/labwc/`](../../os/package/labwc/) |
| Desktop & Web shell | Dependency-free ES modules and CSS | [`shell/`](../../shell/) |

Both the local HDMI display session and remote browsers connecting over Wi-Fi, Ethernet, or USB-C share the exact same `makerd` API and `shell/` experience.
