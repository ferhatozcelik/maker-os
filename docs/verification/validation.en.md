# Validation Processes

[Türkçe](validation.md) · [English](validation.en.md)

This document describes how MakerOS components are tested and verified. It focuses strictly on functional verification and clearly separates tested capabilities from future roadmap items.

## Verified Components

1. **Unit & Integration Tests (`makerd/`):** Session authentication, CSRF/Origin guards, PTY terminal tickets, file operations, `uv` Python project lifecycle, Autostart / Cron scheduling, RAUC version comparison, AI command risk classification, and browser proxy (`browse.rs`) are verified via automated Rust test suites.
2. **Shell Smoke Tests (`shell/tools/smoke.mjs`):** First-boot setup wizard, login and server-enforced screen lock, desktop window management, `/web` mobile layout, and all built-in apps are tested end-to-end using headless Chromium in Docker.
3. **Hardware Verification (Pi 4 compatible ARM64):** The Buildroot image, read-only EROFS + `/data` overlay, USB-C (`10.42.0.1`) and Wi-Fi networking, `makerd` service family, `makeros-ai` (Qwen2.5-0.5B + `llama.cpp`), GPIO pin control, and Camera streaming have been validated on a physical Pi 4 compatible ARM64 board.

## Future Roadmap (Not Yet Implemented)

- Portable board profiles for additional ARM64 single-board computers (including Pi 5 compatible boards).
- SSH toggle and key management directly inside the Settings UI.
- Direct Wayland compositor window bridging into the web window manager.
