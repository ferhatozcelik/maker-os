# Desktop and Web Shell

[Türkçe](desktop.md) · [English](desktop.en.md)

The MakerOS shell ([`shell/`](../../shell/)) is built with standard, dependency-free ES modules and CSS without heavy frontend frameworks. It uses a flat rendering tier (`fx: "flat"`) to keep GPU and CPU usage minimal on Raspberry Pi 4.

## Two Interface Modes

- **Desktop View (`/`, [`shell/index.html`](../../shell/index.html)):** Provides the top menu bar, application Dock, multi-window manager ([`shell/js/wm/`](../../shell/js/wm/)), desktop AI Assistant widget, Find (`⌘K`), Control Center, Notification Center, and Lock Screen.
- **Web View (`/web`, [`shell/web/index.html`](../../shell/web/index.html)):** Designed for phones, tablets, and quick remote access with live system status cards and responsive navigation.

## Built-in Applications & Customization

Built-in applications include Files, Terminal, Python Manager, AI Agents, Camera, GPIO, Plugins, Activity Monitor, Browser (with integrated Google Search and `/api/v1/browse` proxy), and Settings. Under **Settings › Appearance**, users can customize themes, wallpapers, and individually toggle system UI elements (Menu Bar, Dock, AI Assistant, Temperature badge, and Toast notifications).
