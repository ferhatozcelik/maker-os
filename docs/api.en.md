# makerd HTTP & WebSocket API Reference (`/api/v1`)

[Türkçe](api.md) · [English](api.en.md)

This document specifies the `/api/v1` contract between the [`makerd/`](../makerd/) backend and the [`shell/`](../shell/) desktop/web shell.

## General Conventions

- **Base Path:** `/api/v1` (JSON, UTF-8).
- **Authentication:** `POST /api/v1/auth/login` sets the `mk_session` cookie (`HttpOnly; SameSite=Strict; Path=/`).
- **CSRF Protection:** All state-changing `POST`, `PUT`, and `DELETE` requests (except `/api/v1/browse`) require the `X-Maker: 1` header.
- **Error Envelope:** `{"error": {"code": "code", "message": "description"}}` (`unauthorized`, `forbidden`, `not_found`, `invalid`, `conflict`, `rate_limited`, `internal`, `unavailable`).

## Core Endpoints

| Group | Endpoints | Description |
| --- | --- | --- |
| **Session & Setup** | `POST /auth/login`, `POST /auth/logout`, `GET /session`, `GET /setup`, `POST /setup/complete` | Authentication and first-boot setup wizard |
| **System & OTA** | `GET /system`, `PUT /system/hostname`, `GET\|PUT /system/time`, `GET\|PUT /system/locale`, `PUT /system/password`, `POST /system/power`, `GET /system/update`, `POST /system/update/check`, `POST /system/update/apply` | System telemetry, configuration, power, and RAUC OTA updates |
| **Network** | `GET /net`, `PUT /net/wifi`, `POST /net/wifi/scan`, `POST /net/wifi/connect`, `POST /net/wifi/forget` | Ethernet, Wi-Fi, and Hotspot management |
| **Shell & Preferences** | `GET\|PUT /prefs`, `GET\|PUT\|DELETE /wallpaper/{name}`, `GET /apps`, `POST /apps/{id}/launch`, `GET /processes`, `POST /processes/{pid}/signal`, `GET\|DELETE /notifications` | User preferences, wallpapers, apps, processes, and notifications |
| **Terminal & Events** | `GET\|POST /term`, `WS /term/{id}?ticket=`, `POST /term/{id}/ticket`, `DELETE /term/{id}`, `WS /events` | PTY terminal sessions and real-time WebSocket event stream |
| **Files & Python** | `GET /files`, `GET\|PUT /files/raw`, `POST /files/op`, `POST /files/run`, `GET /python`, `POST /python/{op}`, `GET\|POST /projects`, `GET /projects/templates`, `POST /projects/{name}/{run\|start\|stop\|packages}` | File manager and `uv`-based Python project manager |
| **Autostart & Cron** | `GET\|POST /autostart`, `POST /autostart/{id}/{toggle\|run}`, `DELETE /autostart/{id}` | Boot-time (`boot`) and Cron-scheduled app/Python/script tasks |
| **Hardware & Plugins** | `GET /gpio`, `POST /gpio/{n}`, `GET /camera`, `GET /camera/stream`, `POST /camera/photo`, `GET /plugins`, `GET /plugins/catalog`, `POST /plugins/catalog/{id}/install`, `POST /plugins/import` | 40-pin GPIO, Camera MJPEG/photo, and Plugin Store |
| **AI & Browser** | `POST /asistan/calistir`, `ANY /ai/{*rest}` (`POST /v1/chat/completions`, `GET /v1/makeros/durum`, `POST /v1/makeros/temizle`), `POST /browse/ticket`, `GET\|POST /browse` | Local AI assistant, command execution, and Browser proxy |
