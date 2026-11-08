# Rust Backend Services

[Türkçe](backend.md) · [English](backend.en.md)

MakerOS system services are written in Rust ([`makerd/`](../../makerd/)) around strict privilege separation, using Tokio, Axum, Serde, and zbus (D-Bus).

## Service Family

- **`makerd` ([`makerd/src/bin/makerd.rs`](../../makerd/src/bin/makerd.rs)):** The primary HTTP and WebSocket daemon running under the unprivileged `maker` service account. It serves the web shell, `/api/v1` endpoints, real-time `events`, system telemetry, and OTA orchestration.
- **`makerd-priv` ([`makerd/src/bin/makerd-priv.rs`](../../makerd/src/bin/makerd-priv.rs)):** A minimal root-privileged Unix socket helper restricted to account creation and password verification/updates.
- **`makerd-user` ([`makerd/src/bin/makerd-user.rs`](../../makerd/src/bin/makerd-user.rs)):** Per-user daemon (`makerd-user@<user>.service`) handling PTY terminal sessions, file operations, `uv` Python projects, Autostart/Cron schedules, camera streams, GPIO control, and plugins.
- **`makeros-ai` ([`makerd/src/bin/makeros-ai.rs`](../../makerd/src/bin/makeros-ai.rs)):** Dedicated local AI service managing `llama-server` lifecycle, conversational context, and command risk classification.
