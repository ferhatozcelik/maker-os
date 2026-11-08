# Local AI Assistant

[Türkçe](ai-assistant.md) · [English](ai-assistant.en.md)

MakerOS includes an integrated offline AI assistant ([`makerd/src/ai/`](../../makerd/src/ai/) and [`shell/apps/ai-widget/`](../../shell/apps/ai-widget/)) powered by **Qwen2.5-0.5B-Instruct (GGUF Q4_K_M)** and `llama.cpp` (`llama-server`).

## Modes and Capabilities

- **Conversational Mode & MakerOS Knowledge Base ([`prompt.rs`](../../makerd/src/ai/prompt.rs)):** Answers questions about MakerOS apps, Python, electronics, and general topics in natural conversational language without forcing rigid command templates.
- **Command Suggestion & Risk Classification ([`risk.rs`](../../makerd/src/ai/risk.rs)):** Terminal commands are classified as `SAFE` (read-only), `WARNING` (system-modifying), or `DANGEROUS` (destructive).
- **Execution Modes ([`exec.rs`](../../makerd/src/ai/exec.rs)):** Supports **Full Bypass** (direct execution) and **Ask Permission** modes from the desktop widget, complete with an active progress bar and a **Stop** button to cancel in-flight generation or commands.
- **History & Context Reset:** The **Clear** (`Temizle`) button wipes local chat history and resets server context (`POST /v1/makeros/temizle`).
