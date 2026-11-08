# Build and Test Guide

[Türkçe](build-and-test.md) · [English](build-and-test.en.md)

All MakerOS build, preview, and smoke-testing workflows run inside Docker containers—no host toolchain installation is required.

## Rapid Development & Preview

```bash
# 1. Shell dev server (mock API for fast UI iteration)
make -C shell dev

# 2. Full Rust backend (makerd --mock) + web shell preview
make -C makerd preview

# 3. End-to-end headless Chromium smoke tests
make -C shell smoke
make -C shell smoke-makerd
```

## Building the Raspberry Pi Image (`os/`)

```bash
cd os
./build.sh defconfig
./build.sh build
# Output: os/out/makeros-1.0.0-rpi4.img.xz (+ .sha256)
```

To push live updates (`makerd` + `shell` + overlay scripts) to a Raspberry Pi connected over USB-C (`10.55.0.1`), run [`os/debug/push.sh`](../../os/debug/push.sh) (`os/debug/push.sh all`).
