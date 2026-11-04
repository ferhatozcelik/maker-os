# MakerOS Next: Graphics and Web Shell Layer (Research Report 2)

Target: Raspberry Pi 4, 4 GB RAM, custom glibc OS built straight on the Linux kernel (Buildroot/Yocto-style, no Debian/Pardus).
Goal: a macOS-style desktop whose shell is written entirely in HTML/CSS/JS, that still runs native apps (terminal, code-server/VS Code, Chromium, camera preview, OpenCV/MediaPipe windows) and that you can also use from a phone or PC browser.
Date: 2026-10-31. RAM figures marked "est." are engineering estimates that must be measured on hardware (Phase 0 below). They are not published benchmarks.

---

## 0. TL;DR: firm recommendations

| Layer | Decision | Fallback |
|---|---|---|
| Kernel display | Upstream `vc4` (KMS) + `v3d` (render), `dtoverlay=vc4-kms-v3d`, full KMS only | none needed |
| GL/Vulkan | Mesa >= 25.0 (V3D: GLES 3.1 conformant, Vulkan 1.3 via V3DV). Shell renders through GLES2/3 only | — |
| Compositor | **A custom compositor on wlroots (0.19.x, C)**: "makeros-wm", seeded from tinywl and borrowing code from labwc (SSD, theme, menus) | labwc as a stop-gap during bring-up |
| Shell web engine | **WPE WebKit >= 2.52/2.54 (WPEPlatform API)** in a small "shell-host" process | Chromium `--ozone-platform=wayland --app` windows with a separate profile |
| User browser | Chromium (Ozone/Wayland) | Firefox ESR |
| Architecture | **Option (c), hybrid**: native compositor owns the windows. Shell surfaces are privileged web clients. JSON-RPC IPC over a Unix socket between the compositor and the shell (bridged to WebSocket for the web side) | — |
| Remote desktop | **wayvnc >= 0.9 on its native WebSocket listener -> noVNC >= 1.6 inside Center**. Pi 4 hardware H.264 (`--gpu`, CMA buffers) when the client supports it | WebRTC (GStreamer `v4l2h264enc` + `webrtcsink`) in v2 |
| Fonts/icons | Inter (OFL) + JetBrains Mono (OFL). Lucide (ISC) as the main icon set, Phosphor (MIT) for fill variants. No SF Pro or SF Symbols | — |
| Frontend | **No-build vanilla ES modules + Web Components + CSS custom-property tokens** for the shell. Preact (+htm, no build) is allowed only in heavy apps | Solid if a build step is accepted later |

Why, in one paragraph: on a Pi 4, wlroots is the ecosystem Raspberry Pi Ltd itself ships and funds. labwc is the default on Raspberry Pi OS, and wayvnc 0.9's Pi hardware-encoding work was sponsored by Raspberry Pi Ltd. wayvnc works only on wlroots compositors. A custom wlroots compositor gives us macOS behaviours that no stock compositor offers: Mission Control, Cmd+Tab, global shortcuts sent to a web shell, and a web-driven window list. Keeping native windows native (not streamed into a browser) is the only way to get acceptable camera, OpenCV and Chromium performance on V3D. WPE WebKit is the engine built for exactly this embedded case, and Igalia maintains both WPE and the Pi's Mesa V3D driver. It also has a much smaller footprint than a second Chromium instance and builds in Buildroot (`package/wpewebkit`, `package/cog`). Chromium does not.

---

## 1. Display stack on Raspberry Pi 4

### 1.1 Kernel / Mesa status (2025-2026)
- **DRM/KMS**: the `vc4` DRM driver drives the HVS/HDMI (KMS), and the `v3d` driver drives the 3D core (VideoCore VI). Use `dtoverlay=vc4-kms-v3d` (full KMS; fake-KMS is deprecated). MakerOS already uses this (docs/architecture.md, boot step 1).
- **OpenGL ES 3.1**: the Pi 4 V3D driver is OpenGL ES 3.1 conformant. The hardware is GLES 3.2-capable, but the driver does not expose it. Desktop GL is 3.1. https://blogs.igalia.com/itoral/page/2/ , https://www.phoronix.com/news/Raspberry-Pi-4-Almost-GLES-3.1
- **Vulkan 1.3**: V3DV exposes Vulkan 1.3 since Mesa 24.3, tested on Pi 4 and Pi 5. https://www.phoronix.com/news/Mesa-24.3-V3DV-Vulkan-1.3 , https://www.khronos.org/news/permalink/raspberry-pi-driver-updated-with-vulkan-1.3-support
- **Debian 13 / Mesa 25.0.7** brought "significant performance improvements" on Pi 4/5 (FOSDEM 2026 talk). https://fosdem.org/schedule/event/BMFQSE-raspberry-pi-gpu-drivers-from-bookworm-to-trixie
- **Practical implications**
  - Fill rate is the bottleneck. The V3D 4.2 GPU at 500 MHz manages only a few full-screen 1080p blended passes per frame at 60 fps. Every full-screen translucent layer and every blur pass costs a lot (see section 5.3).
  - Scanout planes: the HVS can composite several planes in hardware. wlroots supports direct scanout of a fullscreen client, so a fullscreen camera, video or game skips GPU composition entirely.
  - Video: the Pi 4 has an H.264 hardware **encoder and decoder** up to 1080p30/60 via V4L2 M2M (`bcm2835-codec`, `v4l2h264enc`, `h264_v4l2m2m`) and an HEVC **decoder** (`rpivid`, stateless). There is no HEVC encoder. Note: the Pi 5 has no H.264 encoder, so a future Pi 5 port needs software x264 for streaming.
  - Render with GLES2 in wlroots (`WLR_RENDERER=gles2`, the default). wlroots' Vulkan renderer works on V3DV but is less tested. Do not make it the default.

### 1.2 Compositor candidates

| Compositor | Lib/lang | Pi 4 status | Fit for MakerOS | Notes |
|---|---|---|---|---|
| **cage** | wlroots / C | Works; used by PiKiosk | Kiosk only (single fullscreen app) | Good for a "setup/recovery" mode. No windows. https://github.com/SailingGreg/pikiosk |
| **labwc** | wlroots / C | **Raspberry Pi OS default on all models since Oct 2024**; chosen over wayfire as "a much better fit for the Raspberry Pi graphics hardware" | Stacking WM, Openbox-like, SSD themes | No Mission Control and no IPC for a web shell. Best code donor and stop-gap. https://raspberrypi.com/news/a-new-release-of-raspberry-pi-os , https://lwn.net/Articles/996332 |
| **wayfire** | wlroots / C++ | Was Pi OS default (2023-24), dropped: "developing in a direction ... less compatible with our hardware" | Has an expo/scale plugin (Mission Control-like) and IPC | Heavier and effect-oriented. Pi Ltd stopped supporting it |
| **sway** | wlroots / C | Works | Tiling. IPC is good, but the model is wrong for macOS | Useful reference for IPC (i3-ipc JSON) |
| **Weston** | libweston / C | Works; reference compositor | desktop-shell/kiosk-shell. Privileged shell client pattern (good precedent) | wayvnc does not support it. Weston has its own VNC/RDP backends |
| **Mutter / KWin** | C / C++ | Run, but ~250-400 MB with the shell; GNOME on Pi 4 is sluggish | Too heavy for 4 GB with Chromium + code-server | Rejected |
| **Smithay (niri, cosmic-comp)** | Rust | niri reported failing on CM4 in early 2024 (fixed on main); nothing on Pi 4 is certified | Memory safety is attractive. You would have to implement the wlroots-only protocols wayvnc needs | Higher risk on V3D; niri's model is scrolling-tiling. https://www.github.com/YaLTeR/niri/issues/218 |
| **Mir / Ubuntu Frame** | C++ | Officially tested on Pi; WPE kiosk snap exists | Kiosk-style, snap-centric | Not suited to a non-Ubuntu custom OS. https://discourse.ubuntu.com/t/ubuntu-frame-with-wpe-on-raspberry-pi-4/28941 |
| **gamescope** | wlroots-derived / C++ | x86/AMD-centric (Vulkan, KMS planes) | Design inspiration only (nested, plane-optimised) | — |

**X11-free**: fully possible. Electron/Chromium, GTK3/4, Qt5/6, SDL2, VS Code (Electron) and OpenCV (GTK/Qt backend) all run natively on Wayland. **Ship Xwayland as an optional, lazily started package** (wlroots supports on-demand Xwayland) for legacy apps such as some Tk/OpenCV-built-against-X11 wheels. Note: `opencv-python` pip wheels bundle **Qt5 with xcb only**. `cv2.imshow` from pip wheels therefore needs Xwayland, or else an OpenCV built with `WITH_GTK=ON` (GTK3 on Wayland) or the Qt wayland plugin. This is a key decision for the camera/Python story: **ship Xwayland, started on demand, ~25-40 MB when running (est.)**.

**Decision: a custom wlroots compositor ("makeros-wm")**, because:
1. Pi Ltd's choice of labwc proves wlroots + V3D is well optimised (damage tracking, plane offload).
2. wayvnc needs `wlr-screencopy`/`ext-image-copy-capture`, `virtual-keyboard`, `wlr-virtual-pointer` and `wlr-output-management`. wlroots provides them for almost nothing.
3. macOS features need compositor-level control that labwc/sway don't expose: Exposé layouts, app-grouped Cmd+Tab, minimise-to-dock genie target rects, menu-bar app focus, global shortcut capture.
4. Effort: tinywl is ~1,000 LOC. A realistic MakerOS WM is 6-12k LOC of C. labwc (GPL-2.0) code can be reused if makeros-wm is GPL-2.0, which is fine for an OS component.

---

## 2. Web engine for the shell

### 2.1 Comparison (Pi 4, arm64, 1080p)

| Engine | GPU path on V3D | RAM for a simple shell page (est.) | Cold start (est.) | Embeddability / layer control | Buildroot/Yocto | Verdict |
|---|---|---|---|---|---|---|
| **WPE WebKit 2.52+ (WPEPlatform) + Cog or custom launcher** | GLES via Skia GPU (WebKit moved to Skia in 2.46), dmabuf to compositor | **70-130 MB** (UI + WebProcess + NetworkProcess) | 0.8-1.5 s | **Best**: WPEPlatform lets *your executable* define the platform (Wayland/DRM/headless) so you can pick the surface role, input regions, etc. | Buildroot `wpewebkit`, `cog`; Yocto `meta-webkit` | **Shell engine** |
| Chromium (Ozone/Wayland, `--app`/`--kiosk`) | GLES via ANGLE/Skia; needs `--ignore-gpu-blocklist` on some Pi builds | **200-320 MB** first window (browser + GPU + renderer + utility). +60-120 MB per extra renderer | 2-4 s | No wlr-layer-shell support in Ozone; the compositor must special-case windows by app_id | Not in Buildroot; Yocto `meta-browser` (very heavy build) | **User browser**; shell fallback |
| Electron | = Chromium + Node | 250-350 MB | 2-4 s | Same as Chromium (no layer-shell) | Painful to build | Rejected for the shell (VS Code desktop is Electron; prefer code-server in Chromium) |
| WebKitGTK (Tauri, or a GTK app + gtk4-layer-shell) | GLES (Skia) | 110-180 MB (adds GTK) | 1-2 s | gtk-layer-shell / gtk4-layer-shell 1.3 gives real layer surfaces | Buildroot `webkitgtk` | Viable alternative to WPE if a GTK dependency is acceptable. https://packages.fedoraproject.org/pkgs/gtk4-layer-shell/gtk4-layer-shell |
| Servo | wgpu/GL | — | — | Embedding API still young in 2025-26 | — | Watch, do not adopt |
| Ultralight | CPU/GPU | low | fast | **Proprietary licence**, limited CSS | — | Rejected |

Sources and evidence:
- WPEPlatform introduced in 2.50 (2025), default in 2.54, libwpe deprecated: https://blogs.igalia.com/llepage/the-wpe-platform-api/ , https://sources.debian.org/src/wpewebkit/2.50.4-1/NEWS , https://lucaberton.com/blog/wpe-webkit-wpeplatform-api-migration-embedded/
- Memory-pressure tuning on low-RAM boards (kill thresholds, cache limits): https://bugs.webkit.org/show_bug.cgi?id=241012 , https://digi.com/resources/documentation/digidocs/embedded/dey/4.0/ccmp15/yocto-webkit-use_t
- Historic WPE Pi 4 performance regressions (60 fps to 24 fps after an upgrade): https://lists.webkit.org/pipermail/webkit-gtk/2021-August/003744.html . Pin versions and keep a perf regression test.
- Chromium on Pi needs `--ignore-gpu-blocklist` for GPU raster in kiosk mode, and it slowly fills RAM on low-memory Pis (PiKiosk restarts it with a watchdog): https://dietpi.com/forum/t/chromeium-kiosk-mode-gl-acceleration/19081 , https://github.com/SailingGreg/pikiosk
- Production kiosk users report bugs in wpe-webkit-mir-kiosk (an older Cog snap). This is a reason to pin a *recent* WPE and own the launcher: https://discourse.ubuntu.com/t/ubuntu-frame-with-wpe-on-raspberry-pi-4/28941

### 2.2 Why WPE for the shell and Chromium for the user
- The user needs Chromium anyway, for the web, code-server and the Center remote UI. The shell must **survive** the user closing or crashing Chromium, so it needs its own process tree. A second Chromium instance costs ~200+ MB private memory (shared .so text helps only partly). WPE costs ~70-130 MB.
- WPE has no browser chrome or sync and a small process model (one WebProcess can host several shell views from the same WebContext). It is designed for set-top boxes and digital signage at this exact SoC class.
- Risk: WebKit CSS/JS differences from Blink (minor in 2026; Safari-level features such as `backdrop-filter`, container queries, `:has()`, View Transitions in WebKit 2.48+). Mitigation: the shell's JS has no Blink-specific APIs, and the CI runs shell UI tests in both `cog` and Chromium.
- **Fallback trigger**: if WPE on V3D shows rendering or perf regressions in Phase 0, switch the shell-host to Chromium `--app` windows with `--user-data-dir=/run/makeros/shell` and accept about 150 MB more RAM. Because the shell is plain web plus the IPC protocol, this swap needs no shell code changes.

---

## 3. Architecture: "web shell + native windows"

### 3.1 Options evaluated

**(a) Native wlroots compositor + web shell as wlr-layer-shell clients.**
- Pros: standard protocols. The shell is "just a client", like waybar/eww.
- Cons: Chromium/Electron cannot create layer surfaces, so you would be locked to WebKitGTK+gtk-layer-shell or a custom WPE platform. layer-shell has no way for a client to drive Mission Control or window lists, so you end up adding private protocols anyway (foreign-toplevel gives only a list).
- Verdict: good but incomplete. It becomes (c).

**(b) A fully web desktop where one fullscreen browser hosts everything; native apps are embedded/streamed into the page.**
- Precedents: Greenfield (HTML5 Wayland compositor). Each native app is H.264-encoded with GStreamer per surface, sent over a WebRTC data channel and decoded in a WebWorker+WebGL. It is a proof of concept (FOSDEM 2019) with no maturity signals since. https://github.com/udevbe/greenfield , https://archive.fosdem.org/2019/schedule/event/greenfield/
- On a Pi 4: every native frame takes encode (one HW encoder, 1080p30 max, shared) + decode + GL upload, with 50-150 ms latency and lossy text. Camera/OpenCV at 30 fps plus a terminal plus VS Code at once would saturate the single encoder. Chromium-inside-Chromium is absurd.
- Verdict: **rejected for local use**. Useful idea only for *remote*, and VNC/WebRTC of the whole output already covers that.

**(c) Hybrid: native compositor + privileged web shell + IPC. RECOMMENDED.**
- Precedents:
  - **webOS** (LG): Luna Surface Manager is a Qt/QML Wayland compositor that renders the System UI itself. The Web App Manager (Blink, earlier WebKit) and native Qt/EGL apps are ordinary Wayland clients. https://www.webosose.org/docs/guides/core-topics/graphics-input/
  - **ChromeOS**: Ash is the system UI and window manager. Exo is the Wayland server used for ARC (Android) and Crostini (Linux apps through Sommelier). Lacros (browser split out over Wayland) was cancelled in 2024. Lesson: keep the shell and the user browser separate, but don't over-engineer the split. https://chromium.googlesource.com/chromium/src/+/89.0.4389.72/docs/lacros.md , https://9to5google.com/2024/07/12/chromeos-lacros-ending/
  - **Weston desktop-shell**: the compositor spawns a privileged shell client on a dedicated socket and trusts only it with private protocol. This is the exact trust model to copy.
  - **Phosh** (GNOME mobile shell = phoc wlroots compositor + GTK shell client over layer-shell/private protocols) and **Sxmo** (sway + scripts) show that "wlroots compositor + separate shell process" works on ARM phones with ≤3 GB RAM.
  - **KaiOS/B2G**: a Gecko "system app" (HTML) acts as the whole shell. This proves a web system UI is viable, but there web apps were the *only* apps.
  - **cosmic-comp / niri** (Smithay): modern compositors with IPC; design reference for workspaces and overview animations.

### 3.2 Recommended component diagram

```
 ┌───────────────────────────── Pi 4 HDMI (KMS planes) ─────────────────────────────┐
 │                                                                                   │
 │  ┌─────────────────────────── makeros-wm (wlroots, C) ───────────────────────┐  │
 │  │ scene graph · xdg-shell · xdg-decoration(SSD) · layer-shell · dmabuf      │  │
 │  │ seat/input (libinput) · global shortcuts · Exposé/Cmd+Tab layout engine   │  │
 │  │ screencopy + ext-image-copy-capture · virtual kbd/pointer · output-mgmt   │  │
 │  │ foreign-toplevel · idle/lock (ext-session-lock) · Xwayland (on demand)    │  │
 │  │ SSD renderer: traffic lights/title (cairo+pango, design tokens from JSON) │  │
 │  └──────▲───────────────▲──────────────────▲───────────────────▲──────────────┘  │
 │   private socket   wayland            wayland              wayland               │
 │   (trusted)          │                  │                    │                   │
 │  ┌──────┴─────────┐ ┌┴──────────────┐ ┌─┴────────────────┐ ┌─┴───────────────┐   │
 │  │ shell-host     │ │ web apps      │ │ native apps      │ │ Chromium (user) │   │
 │  │ (WPE WebKit)   │ │ (WPE windows, │ │ foot/terminal,   │ │ code-server UI, │   │
 │  │ views:         │ │ app_id=       │ │ GTK/Qt, OpenCV,  │ │ web browsing    │   │
 │  │ • menubar      │ │ makeros.app.*)│ │ camera preview   │ │                 │   │
 │  │ • dock         │ │ Settings,Files│ │ (GStreamer       │ │                 │   │
 │  │ • overlay:     │ │ Activity,GPIO │ │ waylandsink/     │ │                 │   │
 │  │  Spotlight,    │ │ Camera UI ... │ │ dmabuf → plane)  │ │                 │   │
 │  │  Cmd+Tab, MC   │ └───────▲───────┘ └──────────────────┘ └─────────────────┘   │
 │  │  labels, notif │         │ HTTP/WS (localhost)                                 │
 │  │ • desktop/wall │         │                                                     │
 │  └──────▲─────────┘         │                                                     │
 │         │ JSON-RPC (WebSocket ws://127.0.0.1/shell, token)                        │
 │  ┌──────┴─────────────────────────┴──────────────────────────────────────────┐   │
 │  │ makeros-center (port 80) — existing Center evolved: REST + WS APIs,        │   │
 │  │ app registry (.desktop), notifications, settings, terminal pty, camera,    │   │
 │  │ GPIO; bridges WM IPC (Unix socket /run/makeros/wm.sock) ⇄ WebSocket       │   │
 │  └──────▲───────────────────────────────────────────────▲────────────────────┘   │
 └─────────┼───────────────────────────────────────────────┼────────────────────────┘
           │ /api/vnc/ws (proxied)                          │ HTTPS/WS
     ┌─────┴──────┐                                  ┌──────┴────────────────────┐
     │ wayvnc     │  WebSocket RFB, H.264 (HW)       │ Phone / PC browser:        │
     │ (headless/ │ ───────────────────────────────► │ same web apps (Center UI)  │
     │  mirror)   │                                  │ + noVNC "Screen" view      │
     └────────────┘                                  └────────────────────────────┘
```

### 3.3 Key mechanisms
1. **Trust and roles without layer-shell**: makeros-wm spawns `shell-host` itself with `WAYLAND_SOCKET=<fd>`, the Weston pattern. Surfaces from that client get roles from their app_id/title (`makeros.shell.menubar`, `.dock`, `.overlay`, `.desktop`), and the compositor places them in fixed scene layers (background < desktop < windows < dock/menubar < overlay < lock). The same rule supports a Chromium fallback. If WPEPlatform or WebKitGTK is used, real `zwlr_layer_surface_v1` can be used instead. Both paths are kept behind one compositor function.
2. **Input regions**: the overlay view is mapped only while Spotlight, Cmd+Tab or Mission Control is shown. Menubar and dock are small surfaces. A full-screen transparent web layer is never kept mapped, because it costs a 1080p blend per frame and steals input.
3. **IPC protocol** (JSON-RPC 2.0, newline-delimited on `/run/makeros/wm.sock`, bridged to WS by Center, auth by a per-boot token in `/run/makeros/shell-token`):
   - WM to shell events: `window.opened/closed/focused/title/app_id/icon`, `workspace.changed`, `output.changed`, `shortcut.pressed {id}`, `exposé.layout {windows:[{id,rect}]}`, `idle`, `lock`.
   - Shell to WM requests: `window.focus/close/minimize/zoom/move/resize`, `app.launch {desktop_id}`, `expose.enter/exit`, `switcher.select`, `workspace.goto`, `shortcuts.set [...]`, `layout.reserve {top:28,bottom:72}`, `cursor.theme`.
   - Precedent: sway/i3 IPC (JSON over a Unix socket), Hyprland IPC, cosmic-comp.
4. **Mission Control / Exposé**: the *compositor* animates scaled window textures, because transforms in the scene graph are cheap on the GPU. The *web overlay* draws only labels, the Spaces bar and the background dim. Window thumbnails never travel to the web side. That would mean copying 1080p buffers into the browser, which the Pi can't afford at 60 fps.
5. **Cmd+Tab / Spotlight**: the compositor grabs Super-combos even when a native app has focus, then sends `shortcut.pressed`. The overlay shows the UI and gets keyboard focus while it is shown. On release, the shell sends `window.focus`.
6. **Window decorations**:
   - Native apps that request SSD (Qt, foot, Chromium with `--enable-features=WaylandWindowDecorations` off, Xwayland): the compositor draws a macOS-style title bar (traffic lights left, centred title, 28 px, colours from `tokens.json`).
   - GTK3/4 apps always draw CSD headerbars. Ship a GTK theme with matching tokens (derive from a permissively licensed theme, check the licence; do not copy Apple bitmaps).
   - Web apps (WPE windows) are frameless with an HTML title bar ("unified toolbar"). Window actions go over IPC; drag uses `xdg_toplevel.move` triggered through the shell-host's native bridge.
7. **Global menu bar**: macOS's per-app menus aren't portable (GTK4/Qt6 dropped dbusmenu in many places). The menubar shows the app name + a standard App/File/Edit/Window menu that maps to keystrokes sent through `virtual-keyboard` (Cmd+C to Ctrl+C, etc.). Web apps register real menus over IPC (`menu.set`).
8. **Camera preview fast path**: the camera app uses GStreamer `libcamerasrc ! ... ! waylandsink` with dmabuf, which can be promoted to a KMS overlay plane. Web UI for controls only. Never route live preview through a `<video>` MJPEG stream locally.

### 3.4 RAM budget (4 GB, est.; verify in Phase 0)

| Component | Idle | Typical session |
|---|---|---|
| Kernel + CMA (`cma=256M` for camera/HW codec/scanout) | 300 MB | 300 MB |
| systemd, NetworkManager/iwd, udev, avahi, sshd | 40 MB | 45 MB |
| makeros-center (Python, existing) | 35 MB | 60 MB |
| makeros-wm (wlroots) + GPU buffers | 30 MB | 60 MB |
| shell-host WPE (UI + WebProcess + NetworkProcess, 4 views) | 110 MB | 150 MB |
| wayvnc (only when a remote client is connected) | 0 | 25 MB |
| Xwayland (on demand) | 0 | 35 MB |
| Two web apps (Settings + Files, shared WebProcess pool) | 0 | 120 MB |
| Terminal (foot) | 0 | 15 MB |
| Chromium, 4 tabs | 0 | 700 MB |
| code-server (Node server side) + its tab in Chromium | 0 | 450 MB |
| Python + OpenCV + MediaPipe demo + camera | 0 | 450 MB |
| **Sum** | **~515 MB** | **~2.4 GB** |
| Free + page cache headroom (zram 1-2 GB zstd as safety) | ~3.4 GB | ~1.5 GB |

Rules: run Chromium with `--renderer-process-limit=4` and memory-saver/tab discarding. Use systemd-oomd or earlyoom with priorities: the shell and the WM are protected (`OOMScoreAdjust=-900`), and Chromium renderers are killed first. Set WPE memory-pressure limits via its settings, so a shell page leak can't take the box down.

---

## 4. Remote access to the same desktop

| Option | Pi 4 HW accel | Browser client | Latency | Effort | Verdict |
|---|---|---|---|---|---|
| **wayvnc + noVNC** | wayvnc 0.9 adds CMA buffers "required for hardware encoding on Raspberry Pi" (sponsored by Raspberry Pi Ltd) and `ext-image-copy-capture`. H.264 via Open H.264 RFB extension (`--gpu`) | noVNC 1.6 added H.264 decoding (WebCodecs) | 30-80 ms LAN | Low. wayvnc has a native **WebSocket** listener since 0.7, so websockify can be dropped | **Recommended v1** |
| WebRTC (GStreamer: `pipewiresrc`/screencopy ! `v4l2h264enc` ! `webrtcsink`) | Yes (v4l2h264enc, 1080p30) | Any browser | 100-200 ms, fine over the internet with STUN/TURN | Medium-high (signalling, input channel, NAT) | v2, for over-the-internet access. Raspberry Pi Connect uses WebRTC P2P with a DTLS relay fallback |
| RDP (weston rdp-backend, gnome-remote-desktop) | No HW on Pi; needs Weston/GNOME | Needs a native RDP client or Guacamole | Good | Doesn't match the wlroots choice | Rejected |
| Greenfield-style per-window streaming | Shares one encoder | Browser | Higher | Very high | Rejected (section 3.1 b) |

Details:
- wayvnc releases: 0.7 websocket option, 0.8 headless output auto-resize and detached mode, 0.9.0 ext-image-copy-capture + CMA for Pi HW encoding. https://github.com/any1/wayvnc/releases . Supports wlroots compositors only (not GNOME/KDE/Weston). https://github.com/any1/wayvnc
- noVNC: H.264 in 1.6.0, improved detection in 1.7.0 beta. https://github.com/novnc/noVNC/releases , https://github.com/novnc/novnc/releases/tag/v1.7.0-beta
- Pi 4 hardware H.264 latency evidence: v4l2h264enc pipelines ~0.5-0.7 s over SRT (buffer-bound), ~130 ms glass-to-glass over UDP on a Zero 2 W. https://c3voc.de/wiki/hardware:video-pis , https://redlib.neco.lol/r/raspberry_pi/comments/1geq4ym/any_tips_for_low_latency_video_streaming_using . Known stall issue with v4l2h264enc in an RTSP server: https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2450
- Raspberry Pi Connect architecture (WebRTC P2P, relay fallback, Wayland-only, Pi 4/5): https://cnx-software.com/2024/05/08/raspberry-pi-connect-software-makes-remote-access-to-raspberry-pi-boards-easier , https://heise.de/-9711895
- **Plan**:
  - v1: wayvnc on `127.0.0.1:5901 --websocket`, proxied by Center at `/api/vnc/ws`. This keeps today's auth and URL, so `vncproxy.py` becomes simpler and websockify is gone. Use Tight/ZRLE by default and H.264 when the client advertises it.
  - Headless mode: makeros-wm creates a wlroots headless output when no HDMI is connected, replacing today's `video=HDMI-A-1:...D` trick. Phone sessions get a phone-sized virtual output (wayvnc 0.8 auto-resize).
  - Important product insight: because the shell and the system apps are web apps, the **phone gets those apps natively from Center** (responsive), with no pixels streamed. VNC is needed only for native windows. Build every MakerOS web app to run both as a WPE window on the Pi and as a route in the remote Center UI. One codebase, two hosts.
  - v2: a WebRTC path (`webrtcsink` from gst-plugins-rs, input over a data channel) for internet access.

---

## 5. macOS-inspired design system for the web

### 5.1 Typography
- **Inter** (SIL OFL 1.1), variable font with optical sizing ("Inter Display" for ≥20 px). It is the closest open substitute for SF Pro: neo-grotesque with tabular figures (`font-feature-settings: "tnum","cv11"`). https://rsms.me/inter/
- Monospace: **JetBrains Mono** (OFL) or **Iosevka** (OFL) for the terminal and code (SF Mono substitute).
- Alternatives: Geist (OFL), Manrope (OFL). Turkish glyph coverage (ğ, ş, ı, İ) is fine in Inter and JetBrains Mono.
- Never bundle SF Pro, SF Mono or New York. The licence limits them to mock-ups for Apple platforms. https://uxplanet.org/ui-ux-design-a-warning-about-san-francisco-cf2cfe9d7b0f
- Type scale (px, at 1x): 11 caption, 12 footnote, 13 body (macOS default), 15 title3, 17 title2, 22 title1, 26 large title. Line-height 1.3.

### 5.2 Icons
- **Lucide** (ISC; tree-shakeable SVG, 1.5-2 px stroke, close to SF Symbols "regular") for UI glyphs. **Phosphor** (MIT; 6 weights incl. fill/duotone) where filled states are needed (dock running indicators, toggles).
- App icons: own squircle-mask icon set (superellipse via SVG path, *not* Apple's exact template), drawn in-house or adapted from Papirus (GPL-3, keep as a separate package).
- **SF Symbols are forbidden**: they count as Apple "system-provided images" under the Xcode/SDK licence, and glyphs "substantially or confusingly similar" are also restricted. https://developer.apple.com/forums/thread/661582 , https://origin-devforums.apple.com/forums/thread/757407

### 5.3 Vibrancy, blur and the V3D GPU budget
- Cost model: `backdrop-filter: blur(r)` = read back what's behind the element, then two separable Gaussian passes (per-pixel cost scales with radius, Skia downsamples for large r), redone **every frame the backdrop changes**. Animating blur is "not really an option, very slow" even on desktops. https://developer.chrome.com/blog/animated-blur/ . On CPU raster it is extremely laggy. https://bugzilla.redhat.com/show_bug.cgi?id=2395768
- Budget at 1080p60 on Pi 4 (est., verify): ~16.6 ms per frame. The compositor and one full-screen blend use ~4-6 ms. A 1920x28 menubar blur is cheap (≈0.3 ms). A 900x500 Spotlight panel blur is ~2-4 ms. **Full-screen blur (Mission Control background) is a frame-killer, so don't do it live.**
- Tiered "material" tokens (`--material-tier` chosen at boot from a quick GPU benchmark, and user-overridable):
  1. **Tier A (live)**: `backdrop-filter: blur(20px) saturate(1.6)` only on small, static surfaces (menubar, dock, popovers, Spotlight), and never on animating elements. Animate `opacity`/`transform` of a container whose blur stays constant.
  2. **Tier B (pre-blurred wallpaper, default on Pi 4)**: at wallpaper change, the shell renders a **pre-blurred, downscaled copy** of the wallpaper (canvas/OffscreenCanvas once, cached as a PNG). Shell surfaces use it as `background-image` with `background-attachment: fixed` + `background-position` matching the screen coordinates. This gives "fake vibrancy", correct over the desktop and zero per-frame cost. Over windows it is a translucent tint.
  3. **Tier C (solid)**: `rgba` surfaces with `color-mix()` tint, 1 px inner highlight, soft shadow. Use for remote/phone and `prefers-reduced-transparency`.
- The Mission Control backdrop is compositor-drawn: a dimmed, pre-blurred wallpaper texture, never a live blur.
- Shadows: avoid large animated `box-shadow`. Use 9-slice shadow images or compositor-drawn shadows for windows (labwc-style SSD shadow texture).

### 5.4 Animation budget (60 fps at 1080p)
- Animate only `transform` and `opacity` (compositor-thread friendly in both Blink and WebKit). Never animate `width/height/top/left`, `filter`, `backdrop-filter` or `box-shadow`.
- Durations: 120-180 ms for menus and popovers, 220-280 ms for Spotlight, Cmd+Tab and window open, 350 ms for Mission Control (compositor side). Easing `cubic-bezier(.2,.8,.2,1)` (spring-like; `linear()` springs are supported in WebKit 2.44+ and Chromium 113+).
- Dock magnification: transform-scale icons rendered as SVGs/bitmaps pre-rasterised at 2x. Cap at 9 visible icons scaled at once. Default "magnification off" on Tier B/C.
- Window minimise "genie": the compositor does a scale+translate to the dock rect (given by the shell over IPC). No mesh deformation in v1.
- `will-change` only during the animation, then removed (it costs memory per layer). Honour `prefers-reduced-motion`.
- Measure with WPE `WEBKIT_SHOW_FPS=1`, Chromium `--show-fps-counter`, and wlroots frame timings in the compositor debug HUD.

### 5.5 Keyboard shortcut map (Super = Cmd)

Principle: **Super acts as Cmd for system and shell actions**. Inside apps, the compositor's optional "Mac keys" mode **remaps Super+letter to Ctrl+letter** through a per-app policy (on by default for GUI apps, off for terminals, where Super+C/V is turned into Ctrl+Shift+C/V). This is the approach of Kinto/Toshy-style remappers, built into the WM's key handler. On remote noVNC from a Mac, the Cmd key arrives as Super/Meta, so it maps 1:1.

| macOS | Action | MakerOS (PC keyboard) | Handled by |
|---|---|---|---|
| Cmd+Space | Spotlight | **Super+Space** (also tap Super alone) | WM to shell overlay |
| Cmd+Tab / Cmd+Shift+Tab | App switcher fwd/back | **Super+Tab** / Super+Shift+Tab (Alt+Tab also works) | WM + overlay |
| Cmd+` | Next window of same app | Super+` | WM |
| Cmd+Q | Quit app | Super+Q (sends xdg close to all app windows, kills after 5 s with a prompt) | WM |
| Cmd+W | Close window/tab | Super+W, remapped to Ctrl+W for apps with tabs and xdg close otherwise (per-app policy) | WM |
| Cmd+H / Cmd+Opt+H | Hide app / hide others | Super+H / Super+Alt+H | WM |
| Cmd+M | Minimise | Super+M | WM |
| Cmd+N / T / O / S / P / F / Z / Shift+Z / A / C / X / V | App commands | Super+same, remapped to Ctrl+same | WM remap |
| Cmd+, | App settings | Super+, (sends Ctrl+, ; web apps get IPC `app.settings`) | WM / app |
| Ctrl+Up (F3) | Mission Control | **Super+Up** or F3 | WM |
| Ctrl+Down | App Exposé | Super+Down | WM |
| Ctrl+Left/Right | Previous/next Space | Ctrl+Super+Left/Right | WM |
| Cmd+Ctrl+F | Fullscreen | Super+Ctrl+F (F11 passthrough) | WM |
| Opt+Cmd+Esc | Force Quit dialog | **Super+Alt+Esc** (Ctrl+Alt+Del also opens it) | WM + overlay |
| Cmd+Shift+3 / 4 / 5 | Screenshot full / area / panel | Super+Shift+3/4/5 + PrtSc | WM (screencopy) + overlay |
| Ctrl+Cmd+Q | Lock screen | Super+Ctrl+Q and Super+L | WM (ext-session-lock) |
| Cmd+Opt+D | Show/hide dock | Super+Alt+D | shell |
| Fn+F11 / Cmd+F3 | Show desktop | Super+D | WM |
| Cmd+Shift+. | Toggle hidden files (Files) | Super+Shift+. | Files app |
| Ctrl+Cmd+Space | Emoji picker | Super+Ctrl+Space | overlay |
| Cmd+Opt+Ctrl+T | (MakerOS) Terminal | Super+Enter, Ctrl+Alt+T | WM |
| — | Notification centre | Super+N (Super+Shift+N when an app owns Super+N) | shell |
| Cmd+Shift+/ | Help / shortcut sheet | Super+/ | overlay |

Configured in `/etc/makeros/shortcuts.json` (defaults) and `~/.config/makeros/shortcuts.json`, editable in Settings. The shell pushes them to the WM with `shortcuts.set`.

### 5.6 Legal note (trademarks and trade dress)
- Do not use: the Apple logo, "macOS", "Mac", "Finder", "Spotlight", "Mission Control", "Dock" (the word is generic, but avoid styled marks), "Launchpad", "Aqua", "Liquid Glass", SF fonts, SF Symbols, Apple wallpapers, system sounds, cursors or icon bitmaps.
- Use your own names: "Bul" (search), "Genel Bakış"/"Overview", "Raf"/"Dock bar", "Uygulama Değiştirici". Say "macOS-inspired" in docs only as a descriptive phrase, never in branding.
- Trade dress: macOS-like *layout and interaction* (menu bar on top, dock at bottom, three window controls) is common practice (elementary OS, Deepin, Zorin). Avoid pixel-identical reproduction: use your own colours for the window controls (or the same hue family with a different shape/gloss), your own corner radii (e.g. 12 px windows, 8 px controls), and your own icon art.
- Keep a `THIRD_PARTY_NOTICES` file for Inter/JetBrains Mono (OFL), Lucide (ISC), Phosphor (MIT), noVNC (MPL-2.0), wlroots (MIT), labwc-derived code (GPL-2.0) and WPE WebKit (LGPL-2.1/BSD). LGPL means shipping relinkable WPE shared libraries, which is the default.

---

## 6. Frontend technology for shell and apps

### 6.1 Options

| Option | Runtime (min+gz) | Build step | Memory/CPU on Pi | Fit |
|---|---|---|---|---|
| **Vanilla ES modules + Web Components (custom elements, shadow DOM optional)** | 0 KB | None (matches the current Center: `app.js` ~92 KB, `app.css` ~25 KB, dynamic `import()` for noVNC) | Lowest. No VDOM | **Shell (menubar, dock, overlay) + shared component library** |
| Preact 10 + htm (via import map, no build) | ~4 KB + ~1 KB | None | Low | Heavier apps (Files, Settings, Activity, Package manager) |
| Solid | ~7 KB | Required (JSX compile) | Lowest of the frameworks (fine-grained signals) | Option if a build step is accepted |
| Svelte 5 | ~3-10 KB (compiled) | Required | Low | Same as Solid; tooling heavier |
| React 19 + ReactDOM | ~45-50 KB | Required | Higher (VDOM, GC churn on V3D-era CPU) | **Not recommended** |
| Lit 3 | ~5 KB | None | Low | Good alternative for the component layer if a declarative template is wanted |

Recommendation:
- **The shell uses zero-dependency vanilla**: four small pages, a long-lived process, and every MB and GC pause matters at 60 fps. A tiny `signal()` helper (≈40 lines, or `@preact/signals-core` ~1.5 KB) gives reactivity.
- **Shared UI kit as custom elements**: `<mk-window>`, `<mk-toolbar>`, `<mk-button>`, `<mk-segmented>`, `<mk-switch>`, `<mk-popover>`, `<mk-menu>`, `<mk-list>`, `<mk-sidebar>`, `<mk-sheet>`, `<mk-toast>`, `<mk-search-field>`. They work inside Preact apps, the Center remote UI and the shell alike.
- **Budget per app**: ≤150 KB JS (uncompressed) for the initial route, ≤60 KB CSS, all served locally (no CDN; the OS works offline). Lazy-`import()` heavy modules (xterm.js, noVNC, Monaco are not needed because code-server brings its own).
- Keep JS ≥ ES2022 and test in WPE + Chromium. Avoid Blink-only APIs (File System Access, WebUSB). WebSerial/WebUSB-style hardware access goes through Center REST/WS APIs instead.

### 6.2 Design tokens and file structure

```
/usr/share/makeros/ui/
  tokens/
    tokens.json          # source of truth (also read by makeros-wm SSD + GTK theme generator)
    tokens.css           # generated: :root custom properties, light/dark, material tiers
  kit/                   # <mk-*> custom elements (ES modules, no build)
    mk-button.js  mk-window.js  mk-menu.js  ...  index.js
  shell/
    menubar.html  dock.html  overlay.html  desktop.html
    lib/ipc.js           # JSON-RPC over WS, reconnect, typed events
    lib/shortcuts.js  lib/apps.js  lib/material.js (tier detection, pre-blur)
  apps/<id>/             # settings, files, activity, camera, gpio, python, agents ...
    manifest.json        # id, name(tr/en), icon, window size, menus, remote:true
    index.html  main.js  style.css
  fonts/  Inter-Variable.woff2  JetBrainsMono-Variable.woff2
  icons/  lucide/*.svg (subset)  apps/*.svg
```

Token layers:
1. **Primitive**: `--gray-0..13`, `--blue-500`, `--radius-4/6/8/12`, `--space-2..32` (4 px grid), `--dur-fast/med/slow`, `--ease-out-spring`.
2. **Semantic**: `--color-bg-window`, `--color-bg-sidebar`, `--color-text-primary/secondary/tertiary`, `--color-accent` (user-selectable, default blue), `--color-separator`, `--control-height-sm/md/lg` (20/24/28), `--focus-ring`.
3. **Material**: `--material-menubar`, `--material-popover`, `--material-hud`, with values switched by `[data-material-tier="a|b|c"]`.
4. **Component**: `--mk-button-bg`, `--mk-window-radius`, `--mk-titlebar-height: 28px`, `--mk-traffic-size: 12px`, `--mk-traffic-gap: 8px`.
- Dark mode via `prefers-color-scheme` + `[data-theme]`. The same JSON generates the GTK `gtk.css` colours and the compositor SSD palette, so native and web chrome match.
- Accessibility: `prefers-reduced-motion`, `prefers-reduced-transparency` (forces Tier C), `prefers-contrast`, full keyboard navigation, ARIA roles on menus and lists, Turkish and English strings in a `i18n/*.json` per app.

---

## 7. Phased plan and validation

**Phase 0, hardware spike (2-3 weeks)**: measure everything marked est. on a Pi 4 4 GB.
- Build: Buildroot 2025.x/2026.02 with Mesa 25.x, wlroots 0.19, labwc, wpewebkit 2.52+/cog, wayvnc 0.9, noVNC 1.6. Chromium from Yocto meta-browser, or cross-built separately; flag this as the single largest build-system risk of the whole OS.
- Measure: PSS per process (`smem`), cold start, menubar/dock/Spotlight fps with Tier A/B/C, Chromium + WPE simultaneously, `cv2.imshow` via Xwayland vs GTK-Wayland, camera `waylandsink` plane promotion, and wayvnc H.264 vs Tight latency to a phone.
- Exit criteria: shell idle ≤150 MB PSS, Spotlight opens in under 150 ms, 60 fps dock hover on Tier B, a full session (section 3.4) fits in under 2.8 GB without swap.

**Phase 1 (MVP)**: labwc + WPE shell surfaces (layer-shell via a custom WPEPlatform or WebKitGTK) + foreign-toplevel for the window list. This gives a menubar, dock and Spotlight quickly while makeros-wm is written.

**Phase 2**: makeros-wm (tinywl base + labwc SSD code), private IPC, Exposé, Cmd+Tab, Super remap, headless output, wayvnc integration in Center.

**Phase 3**: WebRTC remote, multi-Spaces, notifications protocol (freedesktop notifications D-Bus bridge to shell), polish and accessibility.

Open risks: (1) the Chromium build on a custom OS; (2) WPE regressions on V3D (keep the Chromium-shell fallback); (3) the pip OpenCV wheel needs Xwayland; (4) GTK CSD visuals vs SSD consistency; (5) the GPL implications of reusing labwc code (fine if makeros-wm is GPL-2.0).

---

## 8. Source index
- Mesa V3D GLES 3.1: https://blogs.igalia.com/itoral/page/2/ , https://www.phoronix.com/news/Raspberry-Pi-4-Almost-GLES-3.1
- V3DV Vulkan 1.3: https://www.phoronix.com/news/Mesa-24.3-V3DV-Vulkan-1.3 , https://www.khronos.org/news/permalink/raspberry-pi-driver-updated-with-vulkan-1.3-support
- Pi graphics stack Bookworm to Trixie (FOSDEM 2026): https://fosdem.org/schedule/event/BMFQSE-raspberry-pi-gpu-drivers-from-bookworm-to-trixie
- Raspberry Pi OS moves to labwc: https://raspberrypi.com/news/a-new-release-of-raspberry-pi-os , https://lwn.net/Articles/996332 , https://alternativeto.net/news/2024/10/raspberry-pi-os-now-comes-with-wayland-support-by-default-across-all-raspberry-pi-models
- niri on CM4: https://www.github.com/YaLTeR/niri/issues/218
- Ubuntu Frame + WPE on Pi 4: https://discourse.ubuntu.com/t/ubuntu-frame-with-wpe-on-raspberry-pi-4/28941 , https://ubuntu.com/frame/docs/24/how-to/running-ubuntu-frame/on-your-device/
- WPEPlatform: https://blogs.igalia.com/llepage/the-wpe-platform-api/ , https://sources.debian.org/src/wpewebkit/2.50.4-1/NEWS , https://lucaberton.com/blog/wpe-webkit-wpeplatform-api-migration-embedded/
- WPE memory/perf on Pi: https://bugs.webkit.org/show_bug.cgi?id=241012 , https://github.com/igalia/cog/issues/65 , https://lists.webkit.org/pipermail/webkit-gtk/2021-August/003744.html , https://digi.com/resources/documentation/digidocs/embedded/dey/4.0/ccmp15/yocto-webkit-use_t
- Chromium kiosk on Pi: https://github.com/SailingGreg/pikiosk , https://dietpi.com/forum/t/chromeium-kiosk-mode-gl-acceleration/19081 , https://www.linuxuprising.com/2021/04/how-to-enable-hardware-acceleration-in.html
- gtk4-layer-shell: https://packages.fedoraproject.org/pkgs/gtk4-layer-shell/gtk4-layer-shell
- Greenfield: https://github.com/udevbe/greenfield , https://archive.fosdem.org/2019/schedule/event/greenfield/ , https://www.phoronix.com/news/Greenfield-Run-Apps-Browser
- webOS LSM: https://www.webosose.org/docs/guides/core-topics/graphics-input/ , https://github.com/webOS-ports/luna-next
- ChromeOS exo/Lacros: https://chromium.googlesource.com/chromium/src/+/89.0.4389.72/docs/lacros.md , https://9to5google.com/2024/07/12/chromeos-lacros-ending/
- wayvnc: https://github.com/any1/wayvnc , https://github.com/any1/wayvnc/releases
- noVNC H.264: https://github.com/novnc/noVNC/releases , https://github.com/novnc/novnc/releases/tag/v1.7.0-beta
- Pi H.264 streaming latency: https://c3voc.de/wiki/hardware:video-pis , https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2450
- Raspberry Pi Connect (WebRTC): https://cnx-software.com/2024/05/08/raspberry-pi-connect-software-makes-remote-access-to-raspberry-pi-boards-easier , https://heise.de/-9711895
- Blur cost: https://developer.chrome.com/blog/animated-blur/ , https://bugzilla.redhat.com/show_bug.cgi?id=2395768
- Apple font/symbol licensing: https://uxplanet.org/ui-ux-design-a-warning-about-san-francisco-cf2cfe9d7b0f , https://developer.apple.com/forums/thread/661582 , https://origin-devforums.apple.com/forums/thread/757407
- Fonts/icons: https://rsms.me/inter/ , https://lucide.dev/license , https://github.com/phosphor-icons/homepage/blob/master/LICENSE
