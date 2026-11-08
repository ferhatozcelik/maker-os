# MakerOS Documentation

[Türkçe](README.md) · [English](README.en.md)

The MakerOS documentation covers the operating system architecture, Rust backend services, web desktop shell, development workflow, and verification methodology in a clean, modular structure.

## Contents

### Architecture (`architecture/`)
- [Architecture Overview](architecture/overview.en.md) — System layers, core components, and runtime model
- [Boot and Storage](architecture/boot-and-storage.en.md) — Read-only EROFS rootfs, ext4 `/data`, and RAUC + `tryboot` A/B updates
- [Desktop and Web Shell](architecture/desktop.en.md) — `/` desktop and `/web` mobile interface, window manager, and themes
- [Rust Backend Services](architecture/backend.en.md) — `makerd`, `makerd-priv`, `makerd-user`, and privilege separation
- [Local AI Assistant](architecture/ai-assistant.en.md) — `makeros-ai`, Qwen2.5-0.5B-Instruct, conversational and command execution modes
- [Network and Security](architecture/network-security.en.md) — Session auth, CSRF, WebSocket Origin validation, Hotspot, and USB-C networking
- [Hardware Support](architecture/hardware.en.md) — Raspberry Pi 4 / 400, GPIO, I²C, SPI, UART, and Camera stack

### Development, Verification & Project
- [Build and Test](development/build-and-test.en.md) — Docker-based image build, local preview, and smoke tests
- [Validation Processes](verification/validation.en.md) — Verified capabilities, test environments, and future roadmap
- [Branding and Licensing](project/branding-and-licensing.en.md) — GPL-3.0-or-later licensing and third-party trademark policy
- [HTTP & WebSocket API Reference](api.en.md) — `/api/v1` endpoints and real-time event stream
