# MakerOS (new OS, Pi 4 / 4 GB): system services, backend, performance

Research date: 2026-10-31. Scope: the move from Pardus 21 + XFCE + Python 3.9 "Merkez" to a
Buildroot-style glibc + systemd OS with a web desktop shell. Sections end in **Decision** lines.
Figures marked "~" are typical values from public reports or experience. Measure them on hardware
before treating them as targets (see §5.9).

---

## 0. What the current repo does (baseline to replace)

| Area | Today (repo) | Problem for the new OS |
|---|---|---|
| Backend | `pios-center.service`: **root**, `python3 -I server.py`, stdlib `ThreadingHTTPServer`, port 80, 4.5 kLoC in 14 modules | Runs as root, one thread per request, a shell-out (`nmcli`, `systemctl`, `gpioset`, `uv`, `lsblk`, `ffmpeg`) per call |
| Network | NetworkManager + `nmcli` text parsing, `shared` mode + dnsmasq `address=/#/10.42.0.1` captive portal, `network-fallback.sh` | NM is ~15-25 MB RSS. Parsing CLI output is fragile |
| OTA | tar.xz + `.sha256` from GitHub release, file backup, `tryboot` for kernel | **SHA-256 from the same release proves integrity only, not authenticity**, and the update is not atomic at image level |
| Camera | `rpicam-vid --codec mjpeg` / ffmpeg → MJPEG `multipart/x-mixed-replace` | Bandwidth-heavy. `decisions.md` notes the use of `bcm2835-unicam-legacy` with a 2023 libcamera |
| GPIO | `gpioset --mode=signal` subprocess per held line | That is **libgpiod v1 CLI syntax**. v2 tools changed the flags (`-c chip`, line held by default, `--daemonize`) |
| Plugins | `systemd-run --unit=pios-plugin-X -p User=… uv run …` with Memory/CPU accounting | Good pattern. Keep it and harden it |
| Tuning | zramswap zstd 50 %, `swappiness=100`, `page-cluster=0`, BFQ on mmc, ondemand (up_threshold 50, sampling_down_factor 50), journald 64 M persistent, `/tmp` tmpfs 25 %, ext4 `noatime,commit=60`, `arm_freq=2000 over_voltage=6 gpu_freq=750` | A good base. Changes are listed in §5 |

Files: `image/overlay/usr/lib/pios/center/*.py`, `image/overlay/etc/{sysctl.d,default/zramswap,udev/rules.d,systemd/system,NetworkManager}`, `image/stages/03-add-hw-packages.sh` (config.txt/cmdline/fstab).

---

## 1. Backend daemon `makerd`: language choice

### 1.1 Comparison (arm64, Pi 4, Cortex-A72)

| Criterion | **Go 1.25/1.26** | Rust (tokio+axum) | Python 3.12/3.13 (asyncio/aiohttp) | Zig 0.14/0.15 |
|---|---|---|---|---|
| Idle RSS, HTTP+WS server | ~8-15 MB (`GOMEMLIMIT`, `GOGC` tunable) | ~2-6 MB | ~25-45 MB (interpreter + imports). Each extra worker process adds the same again | ~1-3 MB |
| Stripped static binary | ~10-18 MB (`-ldflags="-s -w"`, `-trimpath`) | ~3-8 MB (LTO, `opt-level="s"`, `panic=abort`) | runtime ~30-50 MB on disk + stdlib | <2 MB |
| Cold start (SD card) | ~20-50 ms | ~5-20 ms | ~200-600 ms (pyc on SD, imports) | ~5 ms |
| Dev speed / team fit | High: simple, fast compiles, easy for contributors | Medium-low: borrow checker, slow builds (minutes for arm64 release) | Highest (existing code) | Low: pre-1.0, breaking changes each release |
| Cross-compile to arm64 | **Trivial**: `CGO_ENABLED=0 GOARCH=arm64 go build`; Buildroot has `golang-package` infra | Good: `cargo` + `aarch64-unknown-linux-gnu`; Buildroot `cargo-package` | N/A (needs target CPython + wheels; C-extension wheels for glibc/arm64) | Excellent (built-in) |
| WebSocket | `github.com/coder/websocket` (ex-nhooyr), gorilla/websocket | tokio-tungstenite, axum ws | aiohttp, websockets | DIY / immature |
| PTY | `github.com/creack/pty` (mature) | `portable-pty`, `nix::pty` | stdlib `pty` | libc |
| D-Bus | `github.com/godbus/dbus/v5`, `coreos/go-systemd/v22` (units, journal, sd_notify, socket activation) | `zbus` (pure Rust, excellent) | `dbus-next`/`sdbus` | none |
| GPIO | `github.com/warthog618/go-gpiocdev` (pure Go, gpio uAPI **v2**), `periph.io` (I2C/SPI) | `gpio-cdev`, `rppal`, `linux-embedded-hal` | `gpiod` 2.5 (PyPI), gpiozero 2.0.1 + lgpio 0.2.2 | none |
| Camera / WebRTC | **`pion/webrtc` v4** (pure Go). MediaMTX and go2rtc are Go, so code can be reused or embedded | `webrtc-rs` (less mature), gstreamer-rs | aiortc (CPU-heavy, needs PyAV) | none |
| Memory safety | GC, safe | Safe, no GC | Safe | Manual |

Notes:
- The Go arm64 scavenger bug that "keeps idle heap" ([golang/go#78962](https://github.com/golang/go/issues/78962)) affects **64 KB-page** kernels. The Pi 4 `kernel8.img` uses 4 KB pages, so it does not apply. The Pi 5 `kernel_2712` uses 16 KB pages, so recheck there.
- Public Go-vs-Rust RSS numbers are anecdotal ([alestaweb](https://alestaweb.com/haber-go-vs-rust-backend-performance-2026-hangisi-daha-hzl-benchmark-karlatrmas-119) quotes 12 MB vs 4 MB). The difference is about 5-10 MB, which is about 0.2 % of 4 GB. It is not decisive. The decisive costs are subprocess churn and Chromium.

**Decision: Go** for `makerd`. Reasons: (1) a static binary with no runtime to ship in Buildroot, (2) every lib needed (PTY, WS, D-Bus, systemd, gpio-cdev v2, WebRTC) exists in pure Go, so there is **no cgo** and cross-builds stay simple, (3) ~10 MB RSS is fine, (4) the team keeps iterating fast. Set `GOMEMLIMIT=48MiB`, `GOGC=50` and `GOMAXPROCS=2` in the unit. Rust is reserved for an optional hot-path component later; none is needed today. Python stays **only for user workloads** (uv, AI), never for system services, because a 30-45 MB interpreter in the base image costs more than its convenience is worth.

### 1.2 Process and privilege model

```
                    ┌──────────────── browser (shell UI / phone) ─────────────────┐
                    │ HTTPS :443 (h2) · WS /api/v1/events · WS /api/v1/term/{id}  │
                    └───────────────────────────────┬─────────────────────────────┘
 makerd.socket (:443, :80 redirect+captive)        │  systemd socket activation (fast boot)
 ┌─────────────────────────────────────────────────▼─────────────────────────────┐
 │ makerd (User=makerd, groups: video gpio i2c spi dialout netdev bluetooth)    │
 │  HTTP router · auth · events bus · camera (WebRTC/MJPEG) · gpio · activity   │
 │  network (iwd/networkd D-Bus) · devices (udev netlink, udisks2) · ota (stage) │
 └───────┬─────────────────────┬────────────────────────────┬────────────────────┘
         │ varlink/JSON over   │ unix socket, SO_PEERCRED    │ D-Bus + polkit rules
         ▼ /run/makerd/priv.sock (socket-activated)          ▼ (group makerd allowed)
 ┌──────────────────────┐   ┌────────────────────────────┐   systemd-hostnamed, -timedated,
 │ makerd-priv (root,   │   │ makerd-user@UID (per user, │   -localed, logind (reboot),
 │ ~1 MB, exits idle 30s)│   │ User=$USER, systemd --user)│   iwd, networkd, resolved,
 │ fixed verbs only:    │   │ terminal PTY · pyman (uv)  │   udisks2 (mount), timesyncd
 │ ota.apply/rollback,  │   │ agents · plugins (systemd- │
 │ passwd, users, boot  │   │ run --user) · files        │
 │ config (config.txt), │   └────────────────────────────┘
 │ fan/overclock, ssh   │
 └──────────────────────┘
```

Rules:
1. `makerd` **never runs as root**. It gets device access through groups set by udev (§4). Network, time, hostname, reboot and mounts go through **the existing systemd and iwd D-Bus APIs**, authorised by a polkit rule for `unix-group:makerd`. That removes most root needs without writing any helper code. Example `/etc/polkit-1/rules.d/50-makerd.rules`: allow `org.freedesktop.hostname1.*`, `org.freedesktop.timedate1.*`, `org.freedesktop.login1.reboot*/power-off*`, `org.freedesktop.udisks2.filesystem-mount*`, `org.freedesktop.network1.*`, and the `net.connman.iwd` policy file group `netdev`.
2. **`makerd-priv`** is the same Go module as a separate tiny `cmd/`. It is socket-activated (`Accept=no`, `ExitOnIdle`) and serves a closed list of verbs with JSON-schema'd arguments. It checks the peer UID with `SO_PEERCRED`, logs every call to the journal, and contains no shell interpolation. This replaces all the `subprocess` calls to `systemctl`, `chpasswd` and file edits that today run as root.
3. **Per-user work runs as the user.** Terminal, Python manager, agents installer and plugins run in `makerd-user@.service` (or `systemd --user`). `makerd` proxies the WebSocket to `/run/user/UID/makerd.sock`. A web terminal shell is then spawned by the user's own manager, not `fork` from root plus `setuid`, and inherits the user's cgroup and limits.
4. Hardening for `makerd.service`: `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateTmp=yes`, `ReadWritePaths=/var/lib/makerd /run/makerd`, `ProtectKernelTunables/Modules/Logs=yes`, `ProtectControlGroups=yes`, `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK`, `SystemCallFilter=@system-service`, `CapabilityBoundingSet=CAP_NET_BIND_SERVICE` (or none when socket-activated), `DeviceAllow=char-gpiochip rw`, `char-video4linux rw`, `char-i2c rw`, `char-spidev rw`, `char-ttyS rw`, `/dev/dma_heap rw`, `MemoryHigh=64M`, `MemoryMax=128M`, `OOMScoreAdjust=-500`. Check the result with `systemd-analyze security makerd`; target is below 3.0.

### 1.3 Module and API mapping

Go layout: `cmd/makerd`, `cmd/makerd-priv`, `cmd/makerd-user`, `internal/<module>`, `api/openapi.yaml` (generate the TS client for the shell with oapi-codegen / openapi-typescript).
Transport: HTTP/2 over TLS. REST under `/api/v1`. **One multiplexed event WebSocket** `/api/v1/events` (topics: activity, jobs, net, devices, ota, notifications) replaces polling. Separate binary WS for terminal. WebRTC WHEP for camera.

| Current module | New package | Runs in | API (v1) | Implementation change |
|---|---|---|---|---|
| `server.py`, `common.py` (auth, token, jobs) | `internal/httpd`, `auth`, `jobs`, `events` | makerd | `POST /auth/login`, `/auth/logout`, `GET /session`; `GET /jobs`, WS events `jobs.*` | Password check through PAM (or libxcrypt `crypt_r` yescrypt with cgo **only in makerd-priv**); session cookie `Secure; HttpOnly; SameSite=Strict`; CSRF header kept; local shell login through a token-file handshake as today |
| `system.py` | `internal/system` | makerd + priv | `GET/PUT /system/{hostname,time,locale,users}`, `POST /system/reboot` | hostnamed/timedated/localed/logind over D-Bus; users/passwd → priv |
| `network.py` | `internal/network` | makerd | `GET /net/wifi/scan`, `POST /net/wifi/connect`, `GET/PUT /net/ifaces`, `POST /net/ap` | iwd D-Bus (`net.connman.iwd.Station`, `.AccessPoint`, `.KnownNetwork`) + networkd D-Bus; captive DNS in-process (`miekg/dns`) |
| `ota.py` | `internal/ota` | makerd (download, verify) + priv (apply) | `GET /ota`, `POST /ota/{check,apply,confirm,rollback}` | A/B image slots + **signed** bundles (RAUC or own ed25519/minisign); see §5.6 |
| `camera.py` | `internal/camera` | makerd | `GET /camera`, `POST /camera/{id}/whep` (WebRTC), `GET /camera/{id}/mjpeg`, `POST /camera/{id}/photo`, `/photos` | One `rpicam-vid --codec h264` producer per camera, fanned out with pion; UVC passthrough |
| `devices.py` | `internal/devices` | makerd | `GET /devices/{usb,serial,storage,displays,audio}`, `POST /storage/{mount,eject}` | udev netlink (`pilebones/go-udev`) for hotplug events instead of `lsblk`; udisks2 D-Bus for mount; displays via DRM (`/sys/class/drm`) or the compositor |
| `gpio.py` | `internal/gpio` | makerd | `GET /gpio`, `PUT /gpio/{line}`, WS `gpio.edge`, `GET /i2c/{bus}/scan` | `go-gpiocdev` holds lines in-process (no `gpioset` processes). The I2C scan uses `/dev/i2c-N` ioctl (SMBus quick/read like i2cdetect) |
| `pyman.py` | `internal/pyman` | makerd-user | `GET /python/versions`, `POST /python/{install,remove,default}`, `POST /projects` | Keep **uv**. Run it as the user, stream logs through jobs |
| `agents.py` | `internal/agents` | makerd-user | `GET /agents`, `POST /agents/{id}/{install,update,launch}` | Same. Node and agent CLIs installed per user under `~/.local` |
| `plugins.py` | `internal/plugins` | makerd-user | `GET /plugins`, `POST /plugins/import`, `/plugins/{id}/{start,stop,logs}` | `systemd-run --user --slice=apps.slice` with hardening properties (§8) and an optional bubblewrap wrapper |
| `terminal.py` | `internal/term` | makerd-user (PTY), makerd (WS proxy) | WS `/term/{id}` (binary frames, resize msg) | `creack/pty`. Sessions survive WS reconnect (ring buffer of 256 KB) |
| `fan.py` | `internal/fan` | makerd + priv | `GET/PUT /fan` | Live thermal reading from `/sys/class/thermal`. The persisted change (`dtoverlay=gpio-fan,temp=`) goes through priv into config.txt, or the PWM fan is driven live via `/sys/class/hwmon` |
| `activity.py` | `internal/activity` | makerd | WS topic `activity` (1 Hz only when subscribed), `GET /processes`, `POST /processes/{pid}/signal` (user's own only) | `/proc` parsing in Go, plus cgroup `memory.current`/`memory.pressure` per slice. Signals to other users' processes → priv |
| `vncproxy.py` | drop | – | – | No X11 desktop any more. Remote access is the web shell itself. A Wayland screen-share (wayvnc) can be an optional plugin |

---

## 2. Networking, mDNS, time, Bluetooth

### 2.1 Wi-Fi/IP stack

| | NetworkManager 1.5x (+wpa_supplicant) | **iwd 3.x + systemd-networkd + resolved** | ConnMan 1.4x (+wpa_s or iwd) |
|---|---|---|---|
| RSS (idle) | NM ~15-25 MB + wpa_supplicant ~4-8 MB | iwd ~3-6 MB + networkd ~3-5 MB + resolved ~4-6 MB | ~3-5 MB + supplicant |
| AP / fallback hotspot | `shared` mode + dnsmasq (current repo) | iwd AP mode (`iwctl device wlan0 set-property Mode ap`, `/var/lib/iwd/ap/MakerOS.ap` with `[IPv4] Address=` and built-in DHCP server) | tethering, works |
| WPA2/3-Enterprise, eduroam | Full | Full: PEAP, TTLS, TLS, PWD; profiles in `/var/lib/iwd/eduroam.8021x` ([iwd network config](https://iwd.wiki.kernel.org/networkconfigurationsettings), [eduroam example](https://bbs.archlinux.org/viewtopic.php?id=264726)) | Via supplicant, weaker UI story |
| Hidden SSIDs | Yes | Yes (`Station.ConnectHiddenNetwork`) | Yes |
| D-Bus API quality | Large but stable | Small and clean | OK |
| Buildroot | `network-manager` | `iwd`, systemd networkd/resolved options | `connman` |
| Risk on Pi brcmfmac | Lowest (what Pi OS ships) | brcmfmac SAE/FWSUP offload bugs were seen ([iwd offloading](https://iwd.wiki.kernel.org/offloading), [RPi firmware #34](https://github.com/RPi-Distro/firmware-nonfree/issues/34)); WPA3 confirmed working with correct firmware ([LKML 2023](https://lkml.iu.edu/hypermail/linux/kernel/2312.2/04634.html)). brcmfmac AP mode under iwd is less tested ([Infineon thread](https://community.infineon.com/t5/Wi-Fi-Bluetooth-for-Linux/Anyone-working-on-IWD-AP-mode-for-fmac-driver/td-p/264565)) | Medium |

**Decision: iwd + systemd-networkd + systemd-resolved**, behind a **go/no-go hardware gate** in the first sprint. Run on Pi 4 brcmfmac with the RPi `firmware-nonfree` blobs:
(a) WPA2-PSK, WPA3-SAE and mixed-mode APs, (b) PEAP-MSCHAPv2 eduroam, (c) a hidden SSID, (d) AP mode + DHCP + phone captive-portal detection, (e) AP → STA switch without reboot. Pass gives ~20 MB saved and a cleaner API. If (a) or (d) fails, set `brcmfmac feature_disable=0x82000` (per the iwd wiki) and retest. If it still fails, fall back to **NetworkManager with `wifi.backend=iwd`**, or NM + wpa_supplicant. Hide the backend behind a Go interface `network.Backend` so the switch costs about a day.

Captive portal without dnsmasq: in AP mode `makerd` runs (1) a DNS responder on `10.42.0.1:53` (bound to `wlan0` only) answering every A query with `10.42.0.1`, and (2) HTTP handlers for `/generate_204`, `/hotspot-detect.html`, `/connecttest.txt`, `/ncsi.txt` and `/canonical.html` that redirect to `/setup`. iwd's built-in DHCP server sets DNS to its own address. Keep the current flow: `wifi.txt` on the boot partition, 45 s with no default route → scan → AP.

Other settings: Wi-Fi power save off (`iwd`: `[DriverQuirks] PowerSaveDisable=brcmfmac`, the equivalent of today's `wifi.powersave=2`). Regulatory domain on the cmdline (`cfg80211.ieee80211_regdom=TR`, already done) or `[General] Country=` in iwd. Disable MAC randomisation for known networks (`AddressRandomization=disabled`).

### 2.2 mDNS / DNS-SD
- **Decision: systemd-resolved with `MulticastDNS=yes`** and a DNS-SD service file `/etc/systemd/dnssd/makeros-https.dnssd` (`Type=_https._tcp`, `Port=443`). This drops avahi (~2-4 MB plus libdaemon and expat). Hostname `makeros-XXXX` (last 4 hex digits of the MAC, set at first boot) avoids `.local` collisions in classrooms.
- Fallback: avahi only if a user app needs avahi's D-Bus browse API. It can be offered as an optional package.

### 2.3 Time
- **systemd-timesyncd** (~1-2 MB). It stores the last time in `/var/lib/systemd/timesync/clock` and moves the clock forward at boot, so **fake-hwclock is not needed**. The Pi 4 has no RTC. TLS validity checks before the first sync must tolerate a clock that is up to a day off: issue the device certificate with `notBefore` set to the build time minus one day.
- Add `systemd-time-wait-sync.service` only for units that need correct time, such as the OTA check. Never put it on the boot path.

### 2.4 Bluetooth
- **BlueZ 5.8x `bluetoothd`** (~2-4 MB). The Pi 4's BT chip sits on UART0. Attach it with `btattach -B /dev/serial1 -P bcm -S 3000000` (an udev or service equivalent of Pi OS `hciuart`/`bthelper`), plus the `BCM4345C0.hcd` firmware from `RPi-Distro/bluez-firmware`.
- Start on demand: `bluetooth.service` gets `ConditionPathExists=/var/lib/makerd/bt-enabled`, toggled from Settings. When disabled, the user can choose `dtoverlay=disable-bt`, which also frees PL011 UART0 for GPIO14/15. That matters for makers.

---

## 3. Camera on Pi 4

### 3.1 Stack
- Kernel: `rpi-6.12.y` and `rpi-6.18.y` contain **both** the upstream `drivers/media/platform/broadcom/bcm2835-unicam.c` (media-controller based) and the legacy `drivers/media/platform/bcm2835/bcm2835-unicam.c` (checked via the GitHub API). Sensor overlays (`imx219`, `imx708`, `ov5647` …) default to `media-controller=on` ([overlays README](https://github.com/raspberrypi/linux/blob/rpi-6.12.y/arch/arm/boot/dts/overlays/README)). The legacy workaround in `decisions.md` only existed because of the 2023 libcamera. **With a current libcamera, use the standard (non-legacy) unicam driver.**
- libcamera: the Raspberry Pi fork is at **v0.7.2+rpt20261009** (Oct 2026) and upstream is **v0.7.1**. The Buildroot `libcamera` package tracks upstream v0.7.1 with `BR2_PACKAGE_LIBCAMERA_PIPELINE_RPI_VC4` ([Buildroot pkg](https://gitlab.com/buildroot.org/buildroot/-/tree/master/package/libcamera)). **Decision: use the raspberrypi/libcamera fork** (newest tuning files, sensors and PiSP fixes) as a `BR2_EXTERNAL` override of the package.
- rpicam-apps: latest **v1.14.0** (Oct 2026). Buildroot's `libcamera-apps` is at v1.7.0 → bump it in the external tree. Build with `-Denable_libav=disabled -Denable_drm=enabled -Denable_egl=disabled -Denable_qt=disabled -Denable_opencv=disabled` to cut Boost, Qt and ffmpeg pulls. Keep `libav` only if the shell needs recording to MP4 in-process.

### 3.2 Streaming to the browser (low CPU)
- The Pi 4 has a **hardware H.264 encoder** (`bcm2835-codec`, V4L2 M2M at `/dev/video11`) and a hardware JPEG encoder (`/dev/video31`). Gotcha: the driver defaults to H.264 level 1.0 (176x144), so set the level explicitly (rpicam-vid `--level 4.1/4.2`) ([shiguredo_v4l2 docs](https://docs.rs/shiguredo_v4l2), [lalitm on HW encoding](https://lalitm.com/hw-encoding-raspi/)). The Pi **5 has no HW encoder**, so any future Pi 5 port falls back to software encoding.

| Method | Latency | CPU (1080p30, Pi 4) | Bandwidth | Notes |
|---|---|---|---|---|
| **WebRTC (WHEP), H.264 HW** | ~150-300 ms | ~5-10 % (encode on VPU, pion packetises) | 2-6 Mbit/s | Best. Every browser decodes H.264 baseline/constrained-high |
| MJPEG (current) | ~100-200 ms | low (HW JPEG) but high per-viewer send cost | 15-40 Mbit/s at 1080p | Simple fallback, snapshots, `<img>` embeds |
| HLS / LL-HLS | 1-6 s | low | low | Only for recording/playback, not live |

**Decision**: `makerd/internal/camera` runs one producer, `rpicam-vid -t 0 -n --codec h264 --profile high --level 4.2 --inline --intra 30 --flush --low-latency -o -`, parses Annex-B NAL units, and fans them out to N WebRTC peers with **pion/webrtc v4** (WHEP endpoint, a browser `RTCPeerConnection` in about 40 lines of JS). Keep MJPEG at 640x480 as the fallback (`--codec mjpeg`). Photo capture uses `rpicam-still` (or the libcamera still stream) while the H.264 stream pauses, as today. UVC cameras: pass through H.264 if the camera offers it (`v4l2-ctl --list-formats`), else MJPEG passthrough; avoid ffmpeg transcodes on Pi 4 software paths.
Alternative considered: run **MediaMTX** (Go, v1.21.x) as a sidecar with its native `rpiCamera` source ([MediaMTX](https://pkg.go.dev/github.com/bluenviron/mediamtx)). It is mature, but adds ~20-30 MB and has stall reports with long browser sessions ([issue #5093](https://github.com/bluenviron/mediamtx/issues/5093)). go2rtc (v1.9.x) is similar (~50 MB per stream quoted, [korben](https://korben.info/en/go2rtc-swiss-army-knife-video-streaming.html)). Use either as a plugin for RTSP/NVR users, not in the base image.

### 3.3 picamera2 outside Raspberry Pi OS
- `picamera2` 0.3.38 on PyPI is pure Python, but it needs the **libcamera Python bindings** (`libcamera` built with `-Dpycamera=enabled`, tied to one exact Python ABI) plus `python-prctl`, `simplejpeg`, `av`, `numpy`, `videodev2`, `PiDNG`, `piexif`. The `rpi-libcamera` PyPI package (0.1a10) builds bindings from source but is alpha, so don't rely on it. Buildroot has no `python-picamera2`.
- **Decision**: build libcamera's pycamera against the **one system CPython** shipped for the AI env (3.12 or 3.13), install it in `/usr/lib/python3.X/site-packages`, and create the uv "ai-env" with `--system-site-packages` (or a `.pth` file pointing to `/usr/lib/makeros/pylibcamera`). uv-managed Pythons of *other* minor versions cannot import it; document this in pyman. Run the OpenCV and MediaPipe wheels (manylinux aarch64, glibc ≥ 2.28) on top.

---

## 4. GPIO, I2C, SPI, UART

- **libgpiod v2** (2.2.x C lib; Python `gpiod` 2.5.0 on PyPI). In Buildroot select `libgpiod2` (not `libgpiod` v1). The v2 CLI breaks the current `gpio.py` (`--mode=signal` was removed; `gpioset` now holds lines until killed, use `-c gpiochip0` and `--daemonize`). In makerd, `go-gpiocdev` replaces the tools entirely.
- Python for users: **gpiozero 2.0.1 + lgpio 0.2.2** (lgpio works on any recent glibc through `/dev/gpiochip*`). Ship `rpi-lgpio` as a drop-in for `RPi.GPIO` code. Pi 4 header = `gpiochip0` (pinctrl-bcm2711).
- PWM: `dtoverlay=pwm-2chan` → `/sys/class/pwm/pwmchip0`. A udev rule `chgrp gpio` on export is needed for non-root use.
- udev rules (`/usr/lib/udev/rules.d/60-makeros-devices.rules`), extending the current ones:
```
SUBSYSTEM=="gpio", KERNEL=="gpiochip*", GROUP="gpio", MODE="0660"
KERNEL=="gpiomem", GROUP="gpio", MODE="0660"
SUBSYSTEM=="i2c-dev", GROUP="i2c", MODE="0660"
SUBSYSTEM=="spidev", GROUP="spi", MODE="0660"
SUBSYSTEM=="pwm", ACTION=="add|change", RUN+="/bin/sh -c 'chgrp -R gpio /sys%p && chmod -R g+w /sys%p'"
# Pi serial aliases (as Pi OS 99-com.rules): serial0 = header UART, serial1 = BT UART
KERNEL=="ttyAMA0", PROGRAM="/bin/sh -c 'test -e /proc/device-tree/aliases/serial0 && [ \"$(readlink -f /dev/ttyAMA0)\" ]'", SYMLINK+="serial%n"
SUBSYSTEM=="tty", KERNEL=="ttyS0|ttyAMA[0-9]*|ttyUSB[0-9]*|ttyACM[0-9]*", GROUP="dialout", MODE="0660"
SUBSYSTEM=="dma_heap", GROUP="video", MODE="0660"
SUBSYSTEM=="media|video4linux", GROUP="video", MODE="0660"
KERNEL=="vchiq|vcsm-cma", GROUP="video", MODE="0660"
```
(Copy the exact `serial0/serial1` logic from Pi OS `raspberrypi-sys-mods/99-com.rules`. The line above is only a placeholder for it.)
- Default user groups: `video gpio i2c spi dialout audio render input plugdev bluetooth`.

---

## 5. Memory and performance tuning (Pi 4, 4 GB)

### 5.1 Swap: zram
- Pi OS Trixie moved to `rpi-swap` = zram plus an optional file, zram sized to RAM and **capped at 2 GB** ([raspberry.tips](https://raspberry.tips/en/raspberrypi-tutorials/raspberry-pi-swap-file), [peppe8o](https://peppe8o.com/?p=3806)). HAOS uses zram with **LZ4** for swap, /tmp and /var ([HA OS docs](https://developers.home-assistant.io/docs/operating-system)), and later moved toward zswap ([commit #2420](https://git.homebeck.duckdns.org/d-two/operating-system/src/commit/75dcb932f8b511bee5556a40ef15b50a92dd2d65)).
- **Decision: one zram device, `disksize = 50 % RAM (~1.9 GB)`, algorithm `zstd`** (about 3:1 on browser and JS heaps vs about 2.2:1 for lz4; the A72 decompresses zstd at 500+ MB/s per core, fast enough). Optional on kernel ≥ 6.2: `lz4` primary with `recomp_algorithm=algo=zstd` for idle pages; adopt it only if benchmarks show input lag. Keep `zswap.enabled=0`. No swap on SD. On a USB SSD, add a 1 GB swapfile with lower priority as a last resort.
- Buildroot has no `zram-generator` package. Use the small unit in §9.2.

### 5.2 VM sysctls, MGLRU, OOM
- `vm.swappiness=180` (kernel range 0-200; zram is cheaper than evicting page cache from SD; Fedora and Pop!_OS use 180 with zram). Keep `page-cluster=0`. `vfs_cache_pressure=50` stays. `watermark_boost_factor=0`. `watermark_scale_factor=125` (earlier background reclaim gives fewer stalls). `dirty_*` in bytes (SD-friendly; §9.1).
- **MGLRU**: build with `CONFIG_LRU_GEN=y`, `CONFIG_LRU_GEN_ENABLED=y`, and set `/sys/kernel/mm/lru_gen/min_ttl_ms=1000`. The patch author's guidance is that ~1000 ms "usually eliminates intolerable lags due to thrashing" ([MGLRU thrashing-prevention patch](https://android-kvm.googlesource.com/linux/+/1332a809d95a4fc763cabe5ecb6d4fb6a6d941b2)).
- **OOM policy: systemd-oomd** (already in systemd, PSI-based, per-cgroup, which matches the slice design) with MGLRU `min_ttl_ms` as the kernel backstop. Some users report earlyoom reacts faster on browser-heavy desktops ([gist](https://gist.github.com/davidar/243ca325716c4cca0baf14a7b3069857)), so tune oomd aggressively (§9.4) and validate with a stress test (open tabs + AI process until pressure). Use **one** userspace killer, never both. earlyoom (also in Buildroot) is the fallback if oomd proves slow on zram.
- **THP**: `transparent_hugepage=madvise` (default `never` on small systems is also fine). Never `always`: memory bloat on 4 GB with many processes.
- `CONFIG_PSI=y` is required for oomd and for per-slice pressure display in Activity.

### 5.3 cgroup v2 slice layout

| Slice | Contents | Settings |
|---|---|---|
| `system.slice` | makerd, iwd, networkd, resolved, journald, bluetoothd | `MemoryMin=96M` (protected), oomd: never kill |
| `shell.slice` | compositor + shell browser (UI process, GPU process, shell renderer) | `MemoryLow=700M`, `CPUWeight=200`, `IOWeight=200`, `ManagedOOMPreference=avoid`, `OOMScoreAdjust=-300` |
| `apps.slice` | user browser tabs/windows, code-server, plugins, terminals | `MemoryHigh=2200M`, `ManagedOOMMemoryPressure=kill`, `ManagedOOMMemoryPressureLimit=50%` |
| `ai.slice` | Python AI / MediaPipe / LLM runners | `MemoryHigh=1400M`, `MemoryMax=1800M`, `MemorySwapMax=1G`, `CPUWeight=50`, `CPUQuota=300%` (leave 1 core for UI), `ManagedOOMPreference=omit` (kill first) |
| `build.slice` | uv installs, pip builds, OTA staging | `CPUWeight=20`, `IOWeight=20`, `MemoryHigh=800M` |

Each shell tab or app maps to its own transient scope (`systemd-run --user --scope --slice=apps.slice`). Activity shows `memory.current` and `memory.pressure` per app. For Chromium this needs per-renderer process handling: start Chromium's browser process in `shell.slice` and accept that its renderers share it, or run user apps in a separate Chromium profile/instance in `apps.slice`. That is a trade-off for the shell research. One shared instance is ~200 MB cheaper.

### 5.4 CPU governor and clocks
- Pi OS still uses **ondemand** with exactly the tunables the repo sets (`up_threshold=50`, `sampling_rate=100000`, `sampling_down_factor=50`). Firmware-mediated clock changes on the Pi 4 are slow, which makes schedutil less responsive. **Decision: keep ondemand**, but build it as the kernel default (`CONFIG_CPU_FREQ_DEFAULT_GOV_ONDEMAND=y`) and set the tunables with a udev rule or tmpfiles entry rather than an early service.
- Overclock: `over_voltage` range is −16..8 (0.95-1.55 V); >6 requires `force_turbo=1`, and `force_turbo` + over_voltage can set the warranty bit. `arm_boost=1` gives 1.8 GHz on newer Pi 4 revisions and all Pi 400s. `initial_turbo` defaults to 60 s since the Nov 2024 firmware. `temp_limit` defaults to 85 °C ([config.txt docs](https://www.raspberrypi.com/documentation/computers/config_txt.html)).
  **Decision: ship stock (`arm_boost=1` → 1.8 GHz), and offer 2.0 GHz (`arm_freq=2000`, `over_voltage=6`, `gpu_freq=750`) as an opt-in "Performance mode"** in Settings. It must require a heatsink or fan acknowledgement, then run a 2-minute stress plus `vcgencmd get_throttled` check through priv, with automatic revert on a failed boot (use `tryboot`, as OTA already does). Reason: an unstable overclock corrupts SD cards, which is the #1 support cost. The current repo enables 2.0 GHz by default. Change that.

### 5.5 Storage
- **Root filesystem: read-only EROFS (lz4hc) A/B image slots** + a writable **ext4 data partition** (`/var`, `/home`, `/etc` overlay). EROFS reads faster than ext4 on SD (fewer blocks), cannot be corrupted by power loss, and gives atomic OTA. This is the HAOS and balenaOS pattern. Mount the data partition with `noatime,lazytime,commit=60,errors=remount-ro`, plus `fsck.repair=yes` on the cmdline.
- **ext4 vs f2fs**: f2fs gives better random write on raw flash, but its power-loss recovery and fsck are weaker, and Pi OS and HAOS stay on ext4. **Decision: ext4** for data. Revisit f2fs only for a dedicated `/home` on SD after power-cut tests.
- `fstrim.timer` weekly (mmc and most UASP SSDs support discard; do not use the `discard` mount option). I/O scheduler: keep **BFQ for mmc** (current rule); use `mq-deadline`/`none` for USB SSD.
- journald: `Storage=volatile`, `RuntimeMaxUse=32M`, `ForwardToSyslog=no`, `Compress=yes`. Offer "persistent logs" as a toggle (writes `SystemMaxUse=64M`).
- tmpfs `/tmp` (size 25 %, already), `/var/tmp` on data. Browser cache: point `--disk-cache-dir` to `/run/user/UID/chromium-cache` (tmpfs, size-capped at 64-128 MB) to save SD writes, at the cost of RAM.
- **USB-SSD boot: strongly recommended.** EEPROM `BOOT_ORDER=0xf14` (USB first, then SD). It cuts boot I/O and app start times severalfold. Document the UAS quirk (`usb-storage.quirks=VID:PID:u`) for bad adapters.
- Readahead and preload: none (systemd-readahead was removed long ago). Instead make the *shell* files small and contiguous (EROFS packs them), and warm the browser binary with `vmtouch`-like `posix_fadvise(WILLNEED)` from makerd at boot. That is optional and should be measured first.

### 5.6 OTA (performance and security)
- Use **RAUC** (in Buildroot; used by HAOS) or SWUpdate with A/B slots + `tryboot` (`autoboot.txt` with `[tryboot] boot_partition=N`) and **ed25519/X.509-signed bundles**. Optional delta via casync/adaptive (RAUC `block-hash-index`) to save bandwidth. This replaces the file-copy and backup logic in `ota.py`.

### 5.7 Boot time: target < 10 s to an interactive shell

| Stage | Typical Pi 4 | Target | Levers |
|---|---|---|---|
| Boot ROM + EEPROM + `start4.elf` | ~2.5-4 s | ~2 s | `boot_delay=0`, `disable_splash=1`, EEPROM `BOOT_UART=0`, `NET_INSTALL_AT_POWER_ON=0`, `DISABLE_HDMI=0` (keep), no `initramfs`, small boot partition files; USB boot via SD-less order adds ~0.5 s for USB enumeration |
| Kernel to `/sbin/init` | ~1.5-3 s | ~1.2 s | `quiet loglevel=3`, drop `console=serial0` in production (UART printk is slow), built-in (`=y`) mmc/ext4/erofs/vc4 drivers, no initramfs, `rootwait` kept, trimmed modules (only `brcmfmac`, `bcm2835-codec`, camera as modules) |
| systemd to `graphical.target` | ~4-15 s (Debian-like) | ~2 s | Only ~15 units, socket activation for makerd, **nothing waits for `network-online`**, `systemd-networkd-wait-online` disabled, firstboot work only on first boot, no `cloud-init` (Trixie's cloud-init made boot slow, per [community report](https://forum.armbian.com/profile/218058-anttix/content/)) |
| Compositor + shell browser first paint | ~2-4 s (Chromium cold on SD) | ~2.5 s | Start the compositor (cage/labwc) from `getty`-less autologin unit early (`After=systemd-user-sessions`), shell served as precompressed static files from `/usr/share` by makerd, Chromium flags `--disable-features=Translate,MediaRouter --no-first-run --disable-background-networking`, SSD |
| **Total** | | **~7.5-9 s** | |

Measure: `systemd-analyze` (kernel + userspace; "firmware" is not reported on a Pi), `systemd-analyze critical-chain graphical.target`, `systemd-analyze blame`, `systemd-analyze plot > boot.svg`. `systemd-bootchart` (`init=/usr/lib/systemd/systemd-bootchart`) gives CPU/IO graphs. `dmesg --time-format=reltime` shows the kernel. For the firmware stage, use a GPIO toggle from the earliest unit plus a logic analyser, or `BOOT_UART=1` timestamps in a debug build. Have the shell call `POST /api/v1/boot/ready` on first paint, so `makerd` logs "time to shell" as `CLOCK_BOOTTIME` and CI on a test Pi tracks it per build.

### 5.8 Firmware features to disable (config.txt)
- `enable_uart=1` locks the VPU/core clock on Pi 4 when the mini UART is used. Default it **off** in production and make it a Settings toggle (serial console for debug builds only).
- `dtparam=audio=on` loads the 3.5 mm PWM audio driver. Keep it (makers use it) but it can be a toggle.
- Bluetooth: optional `disable-bt` (§2.4). `hdmi_enable_4kp60` off. `max_framebuffers=2`. `camera_auto_detect=1` and `display_auto_detect=1` stay (cheap).
- `disable_fw_kms_setup=1` (the kernel owns KMS; slightly faster). `gpu_mem` is not set (Pi 4 default 76 MB is enough with KMS + libcamera; the H.264 codec imports buffers via vcsm-cma). Raise it to 128 only if the encoder fails at 1080p.
- CMA: the overlay supports `cma-64…cma-512` ([overlays README](https://github.com/raspberrypi/linux/blob/rpi-6.12.y/arch/arm/boot/dts/overlays/README)). **Use `dtoverlay=vc4-kms-v3d,cma-256`**: enough for two 1080p scanout buffers, the camera pipeline (ISP + 1080p H.264) and the codec. CMA pages are reusable for movable allocations, so this is not "lost" RAM, but large CMA increases allocation latency under pressure. Verify with `grep -i cma /proc/meminfo` while streaming and running the shell.

### 5.9 Benchmark protocol (do before freezing defaults)
1. Idle after boot: `smem -t -k -c "name uss pss rss"` and `/sys/fs/cgroup/*/memory.current` per slice.
2. Workload A: shell + 5 Chromium tabs (docs, YouTube 720p, GitHub, a heavy SPA) + code-server + camera WebRTC.
3. Workload B: A + MediaPipe pose at 640x480 in ai.slice.
4. Record `memory.pressure` some/full avg10, zram `mm_stat` (compression ratio), oomd kills, input latency (the shell measures frame time), and temperature/throttling. Compare zstd vs lz4 and swappiness 100/150/180.

---

## 6. Reference OSes: what to copy

| OS | Copy | Avoid |
|---|---|---|
| **Home Assistant OS** (Buildroot, v18.x) | Buildroot `BR2_EXTERNAL` structure, **RAUC A/B + signed bundles**, read-only squashfs/EROFS root + overlay data partition, zram LZ4 for /tmp and /var, OS-Agent (small Go D-Bus service), hardware-specific defconfigs ([dev docs](https://developers.home-assistant.io/docs/operating-system)) | Docker-in-the-base (~100+ MB) |
| **DietPi** | Aggressive service minimisation, RAMlog (volatile logs + periodic sync), clear per-board config.txt presets, interactive first-boot setup with forced password change | Debian base size, bash-heavy tooling |
| **Raspberry Pi OS Lite (Bookworm/Trixie)** | Tuned ondemand values, `99-com.rules` serial/GPIO udev rules, `rpi-swap` (zram capped at 2 GB), `rpi-eeprom` and `tryboot`, `firmware-nonfree`/`bluez-firmware` blobs, `raspberrypi-sys-mods` | cloud-init (slow boot), dpkg triggers on every update |
| **Ubuntu Core** | Strict confinement thinking (per-app sandbox with declared interfaces → map to plugin manifest permissions), transactional updates, model assertions / signed images | snapd overhead (~100 MB + squashfs mounts per snap) |
| **balenaOS** (Yocto) | Read-only host + A/B ("hostapp"), robust delta updates, `balena-supervisor` health check → rollback, NetworkManager config injection through the boot partition | Container engine for a single-user desktop |
| **LibreELEC** (own build system, ~150 MB image) | Single read-only squashfs `SYSTEM` file + `KERNEL` file update (atomic swap), very fast boot (~5-7 s to UI on Pi 4), minimal userspace, Kodi started directly by systemd without a display manager | Kodi-specific choices |
| **Batocera** (Buildroot) | Buildroot packaging for a large app set, a userdata partition separate from the system image, a `batocera.conf`-style single config file applied at boot | X11 legacy parts |
| **FydeOS / ChromiumOS** | Web-first shell over a minimal Linux, verified boot concept, Chromium on Ozone/Wayland with GPU, zram + aggressive tab discarding (Chromium's memory-pressure tab discard) | Chromium build cost (Buildroot has **no Chromium package**; `cog`/`wpewebkit` exist. Shipping Chromium needs a custom package or prebuilt arm64 binaries; this is a key input for the shell research) |

---

## 7. RAM budget (Pi 4 4 GB, all active, steady state)

Linux sees ~3.8-3.9 GB after firmware reservations. The table counts PSS, so shared libraries are split.

| Component | MB (typ.) | MB (high) | Notes |
|---|---:|---:|---|
| VideoCore firmware (`gpu_mem` 76) + reserved | 80 | 80 | Not visible to Linux |
| Kernel text/data, slab, page tables, buffers | 150 | 250 | Page tables grow with Chromium |
| CMA (`cma-256`) actually pinned by display + camera | 60 | 200 | The rest is reusable by movable pages |
| systemd PID1, journald (volatile 32 M cap), udevd, logind, dbus-broker, oomd | 30 | 60 | dbus-broker instead of dbus-daemon |
| iwd + networkd + resolved + timesyncd | 15 | 25 | NM path: +15-20 |
| bluetoothd (when on) | 3 | 6 | |
| makerd + makerd-user (+ priv when active) | 20 | 45 | `GOMEMLIMIT` caps |
| Camera (rpicam-vid H.264 1080p30 + pion fan-out) | 40 | 70 | Only while streaming |
| Wayland compositor (cage/labwc) | 30 | 60 | |
| Shell browser: browser proc + GPU proc + shell renderer | 250 | 380 | Shared instance with user tabs |
| One Chromium tab (typical site) | 150 | 450 | YouTube or a heavy SPA hits the high end |
| code-server (Node, one workspace, TS server) | 300 | 600 | Extensions add 50-200 each |
| Python AI process (MediaPipe pose / small ONNX) | 450 | 900 | LLM runners exceed this, so cap them in ai.slice |
| Terminal / uv / misc user processes | 30 | 100 | |
| **Sum** | **~1,610** | **~3,230** | |
| **Headroom of ~3,850 usable** | **~2,240 (page cache)** | **~620** + zram (~1.9 GB at ~3:1 = ~4-5 GB of cold pages) | |

Interpretation: the typical scenario is comfortable. In the high case the browser, code-server and AI compete. `ai.slice` `MemoryHigh` stops a model from evicting the UI, oomd kills from `ai.slice`/`apps.slice` first, and the shell slice is protected by `MemoryLow`. The two biggest levers are (1) one shared Chromium instance (saves ~150-250 MB versus a separate shell engine) and (2) Chromium memory-saver / tab discarding (`--enable-features=HighEfficiencyModeAvailable`, memory-pressure signals from PSI).

---

## 8. Security baseline

1. **First boot**: no default password. The setup flow (local screen or captive portal) creates the user and password, sets hostname and timezone, and generates SSH host keys and the device CA. Until it completes, sshd is disabled and the web API only exposes `/setup`. Password hashes use **yescrypt** (libxcrypt; glibc 2.38+ removed `crypt` from libc). Login rate limit: 5 tries per minute per IP, with exponential backoff.
2. **HTTPS**: at first boot `makerd-priv` creates a **per-device local CA** (ECDSA P-256, 10 years, name-constrained to `makeros-XXXX.local`, `*.local` and private IP ranges) and a leaf certificate (397 days, auto-renewed by makerd) for `makeros-XXXX.local`, `localhost`, the current IPs and `10.42.0.1`. Port 80 only serves redirects and the captive portal. The shell offers "Trust this device" downloads (`/ca.crt`, iOS `.mobileconfig`, Android and Windows instructions). The local shell uses `https://localhost` with the CA preinstalled in the shell browser's NSS store, so there is no warning on the device. Cookies are `Secure`, the WebSocket `Origin` is checked, and CSRF headers are kept. Optional remote access later via a Tailscale/WireGuard plugin, never port-forwarding.
3. **OTA authenticity**: sign bundles (RAUC X.509 or minisign ed25519) and **pin the public key in the image**. The current sha256-from-same-release scheme does not stop a compromised release or a MITM.
4. **Firewall**: nftables default-drop inbound, except 443/80 (LAN), 22 (after setup, key auth preferred), 5353/udp, DHCP/DNS on the AP interface. Plugin web ports are reachable only through makerd's reverse proxy (`/apps/{id}/`) with auth, never bound on 0.0.0.0.
5. **App and plugin sandboxing** (defence in depth, cheapest first):
   - The systemd transient unit for every plugin or user service gets `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=tmpfs` + `BindPaths=<plugin dir> <declared project dirs>`, `PrivateTmp=yes`, `PrivateDevices=yes` unless the manifest declares `gpio/camera/serial` (then `DeviceAllow=` for exactly those), `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`, `SystemCallFilter=@system-service`, `MemoryHigh`/`CPUWeight` from the slice. These work under `systemd-run --user` for unprivileged options.
   - **bubblewrap** (setuid-less with user namespaces, `kernel.unprivileged_userns_clone=1`) for untrusted imported plugins: `bwrap --ro-bind /usr /usr --bind $PLUGIN /app --dev-bind-try /dev/gpiochip0 … --unshare-all --share-net`.
   - **Landlock** (kernel ≥ 6.7 for the network ABI v4; Pi 6.12/6.18 kernels qualify; `github.com/landlock-lsm/go-landlock`). makerd restricts its own filesystem view at startup, and the plugin launcher applies Landlock rules from the manifest.
   - The manifest declares permissions (`gpio`, `camera`, `serial`, `network`, `files:~/Projects`). The shell shows them at install, like Ubuntu Core interfaces.
6. **Kernel and sysctl hardening** (cheap): `kernel.kptr_restrict=2`, `kernel.dmesg_restrict=1`, `fs.protected_*`, `net.ipv4.conf.*.rp_filter=2`, `kernel.yama.ptrace_scope=1`. Keep the BPF JIT hardening default.
7. **AI agents**: run agent CLIs only in `apps.slice`/`ai.slice` as the user, never with access to `makerd-priv`. The privileged socket accepts only UID `makerd`.

---

## 9. Ready-to-use configuration proposal

### 9.1 `/usr/lib/sysctl.d/60-makeros.conf`
```ini
# --- memory / zram ---
vm.swappiness = 180
vm.page-cluster = 0
vm.vfs_cache_pressure = 50
vm.watermark_boost_factor = 0
vm.watermark_scale_factor = 125
# SD-friendly writeback (bytes, not ratios; 4 GB RAM)
vm.dirty_background_bytes = 33554432
vm.dirty_bytes = 134217728
vm.dirty_writeback_centisecs = 1500
vm.dirty_expire_centisecs = 3000
vm.min_free_kbytes = 32768
# --- hardening ---
kernel.kptr_restrict = 2
kernel.dmesg_restrict = 1
kernel.yama.ptrace_scope = 1
kernel.unprivileged_userns_clone = 1
fs.protected_hardlinks = 1
fs.protected_symlinks = 1
fs.protected_fifos = 2
fs.protected_regular = 2
net.ipv4.conf.all.rp_filter = 2
net.ipv4.conf.default.rp_filter = 2
# --- network (Wi-Fi + many WS connections) ---
net.core.default_qdisc = fq_codel
net.ipv4.tcp_fastopen = 3
fs.inotify.max_user_watches = 524288   # code-server / file watchers
```

### 9.2 zram (no zram-generator in Buildroot)
`/usr/lib/systemd/system/makeros-zram.service`
```ini
[Unit]
Description=MakerOS zram swap
DefaultDependencies=no
Before=swap.target
After=systemd-modules-load.service
[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/bin/sh -c 'modprobe zram num_devices=1; \
  echo zstd > /sys/block/zram0/comp_algorithm; \
  echo $(( $(awk "/MemTotal/{print \\$2}" /proc/meminfo) * 512 )) > /sys/block/zram0/disksize; \
  mkswap -q /dev/zram0 && swapon -p 100 -d /dev/zram0'
ExecStart=-/bin/sh -c 'echo 1000 > /sys/kernel/mm/lru_gen/min_ttl_ms'
ExecStop=/bin/sh -c 'swapoff /dev/zram0; echo 1 > /sys/block/zram0/reset'
[Install]
WantedBy=swap.target
```
(`MemTotal kB × 512` = 50 % of RAM in bytes. Use `CONFIG_ZRAM=y` built-in to skip modprobe.)

### 9.3 Kernel config fragment (Pi 4, on top of `bcm2711_defconfig`)
```
CONFIG_LRU_GEN=y
CONFIG_LRU_GEN_ENABLED=y
CONFIG_PSI=y
CONFIG_ZRAM=y
CONFIG_ZRAM_DEF_COMP_ZSTD=y
CONFIG_ZRAM_MULTI_COMP=y
CONFIG_ZSWAP=n
CONFIG_TRANSPARENT_HUGEPAGE=y
CONFIG_TRANSPARENT_HUGEPAGE_MADVISE=y
CONFIG_CPU_FREQ_DEFAULT_GOV_ONDEMAND=y
CONFIG_EROFS_FS=y
CONFIG_EROFS_FS_ZIP=y
CONFIG_EXT4_FS=y
CONFIG_MMC_BCM2835=y
CONFIG_MMC_SDHCI_IPROC=y
CONFIG_SECURITY_LANDLOCK=y
CONFIG_LSM="landlock,lockdown,yama,bpf"
CONFIG_IKCONFIG_PROC=y
# keep as modules: brcmfmac, bcm2835-codec, bcm2835-isp, unicam, sensor drivers, btbcm/hci_uart
```

### 9.4 systemd-oomd and slices
`/usr/lib/systemd/oomd.conf.d/60-makeros.conf`
```ini
[OOM]
SwapUsedLimit=85%
DefaultMemoryPressureLimit=60%
DefaultMemoryPressureDurationSec=10s
```
`/usr/lib/systemd/system/-.slice.d/60-makeros.conf` → `[Slice] ManagedOOMSwap=kill`
`/usr/lib/systemd/user/apps.slice`
```ini
[Slice]
MemoryHigh=2200M
ManagedOOMMemoryPressure=kill
ManagedOOMMemoryPressureLimit=50%
```
`/usr/lib/systemd/user/ai.slice`
```ini
[Slice]
MemoryHigh=1400M
MemoryMax=1800M
MemorySwapMax=1G
CPUWeight=50
CPUQuota=300%
ManagedOOMMemoryPressure=kill
ManagedOOMMemoryPressureLimit=40%
```
`/usr/lib/systemd/user/shell.slice` → `MemoryLow=700M`, `CPUWeight=200`, `IOWeight=200`, `ManagedOOMPreference=avoid`.
(Enable `user@.service` delegation of `memory cpu io`; systemd does this by default.)

### 9.5 `/boot/config.txt`
```ini
# MakerOS - Raspberry Pi 4 / 400 (arm64)
arm_64bit=1
kernel=kernel8.img
auto_initramfs=0
disable_splash=1
boot_delay=0
initial_turbo=60
# Graphics: kernel KMS, modest CMA (display + camera + H.264 codec)
dtoverlay=vc4-kms-v3d,cma-256
max_framebuffers=2
disable_fw_kms_setup=1
disable_overscan=1
hdmi_enable_4kp60=0
# Hardware detection
camera_auto_detect=1
display_auto_detect=1
# Maker interfaces (toggled from Settings via makerd-priv)
dtparam=i2c_arm=on
dtparam=spi=on
dtparam=audio=on
enable_uart=0
# dtoverlay=disable-bt      # Settings > Bluetooth off (frees PL011 on GPIO14/15)
# dtoverlay=gpio-fan,gpiopin=14,temp=65000
dtparam=act_led_trigger=mmc0

[pi4]
arm_boost=1
# Performance mode (opt-in, written by makerd-priv after stress test + tryboot):
# arm_freq=2000
# gpu_freq=750
# over_voltage=6

[all]
include makeros-user.txt
```
`autoboot.txt` (A/B with tryboot):
```ini
[all]
tryboot_a_b=1
boot_partition=2
[tryboot]
boot_partition=3
```
EEPROM (`rpi-eeprom-config`): `BOOT_UART=0`, `BOOT_ORDER=0xf14` (USB → SD → loop), `NET_INSTALL_AT_POWER_ON=0`, `POWER_OFF_ON_HALT=1` (lowest halt power).

### 9.6 `/boot/cmdline.txt`
Production:
```
console=tty3 root=/dev/mmcblk0p2 rootfstype=erofs ro rootwait quiet loglevel=3 logo.nologo vt.global_cursor_default=0 systemd.show_status=auto rd.udev.log_level=3 zswap.enabled=0 transparent_hugepage=madvise cfg80211.ieee80211_regdom=TR fsck.repair=yes
```
(Use `root=PARTUUID=` per slot, rewritten by the OTA tool. The debug image adds `console=serial0,115200` and `enable_uart=1`. If the shell's compositor runs headless for remote use, add `video=HDMI-A-1:1920x1080M@60D` as today.)

### 9.7 fstab / data mounts
```
PARTUUID=xxxx-01  /boot        vfat  noatime,ro,fmask=0133,dmask=0022           0 2
PARTUUID=xxxx-04  /data        ext4  noatime,lazytime,commit=60,errors=remount-ro 0 2
tmpfs             /tmp         tmpfs noatime,nosuid,nodev,size=25%,mode=1777      0 0
# /etc, /var, /home via overlay or bind from /data (systemd mount units)
```
`/usr/lib/systemd/journald.conf.d/60-makeros.conf`: `Storage=volatile`, `RuntimeMaxUse=32M`, `Compress=yes`, `ForwardToSyslog=no`.
udev: keep `ATTR{queue/scheduler}="bfq"` for `mmcblk*`. Add `KERNEL=="sd[a-z]", ATTR{queue/rotational}=="0", ATTR{queue/scheduler}="mq-deadline"`.
ondemand via udev or tmpfiles: `w /sys/devices/system/cpu/cpufreq/ondemand/up_threshold - - - - 50`, `… sampling_down_factor 50`, `… sampling_rate 100000`.

---

## 10. Firm recommendations (summary)
1. **makerd in Go** (static, no cgo) + `makerd-priv` (root, socket-activated, fixed verbs) + `makerd-user@` (PTY, uv, agents, plugins). Use systemd/iwd D-Bus with polkit instead of shelling out.
2. **iwd + networkd + resolved (mDNS + DNS-SD)**, captive DNS inside makerd. Hardware gate in sprint 1, with NM+iwd as the fallback. timesyncd (no fake-hwclock). BlueZ on demand.
3. **Camera**: RPi libcamera fork v0.7.x + rpicam-apps 1.14 on **non-legacy unicam**. HW H.264 → **WebRTC (WHEP, pion)** with MJPEG fallback. picamera2 via system pycamera + `--system-site-packages`.
4. **GPIO**: libgpiod v2 / go-gpiocdev in makerd; gpiozero + lgpio for users; udev groups; fix the v1-only `gpioset` usage.
5. **Memory**: zram zstd 50 %, swappiness 180, MGLRU `min_ttl_ms=1000`, systemd-oomd, slices shell/apps/ai/build, THP madvise, `cma-256`.
6. **Storage and updates**: EROFS A/B root + ext4 data, RAUC-signed OTA with tryboot, volatile journald, fstrim weekly, USB-SSD boot recommended.
7. **Clocks**: stock 1.8 GHz by default. 2.0 GHz is opt-in with a stress test + tryboot revert. ondemand with the Pi OS tunables.
8. **Boot <10 s**: no initramfs, built-in drivers, ~15 units, nothing on `network-online`, socket-activated makerd, shell first paint measured by makerd.
9. **Security**: forced first-boot credentials, per-device name-constrained CA + HTTPS, signed OTA, nftables default-drop, systemd-hardened and bubblewrap/Landlock-sandboxed plugins.
10. **Flag for the shell team**: Buildroot ships no Chromium. Decide between a custom Chromium package, prebuilt arm64 binaries, or WPE/cog. This choice dominates the RAM budget.

## Sources
- Raspberry Pi config.txt docs: https://www.raspberrypi.com/documentation/computers/config_txt.html
- Pi kernel overlays README (vc4-kms-v3d cma, disable-bt, sensors): https://github.com/raspberrypi/linux/blob/rpi-6.12.y/arch/arm/boot/dts/overlays/README
- Pi kernel unicam drivers (upstream + legacy): https://github.com/raspberrypi/linux/tree/rpi-6.18.y/drivers/media/platform/broadcom
- raspberrypi/libcamera releases (v0.7.2+rpt20261009): https://github.com/raspberrypi/libcamera/releases
- rpicam-apps releases (v1.14.0): https://github.com/raspberrypi/rpicam-apps/releases ; meson build: https://github.com/raspberrypi/libcamera-apps/blob/main/meson.build
- Buildroot packages (libcamera v0.7.1, libcamera-apps v1.7.0, iwd, libgpiod2, earlyoom, cog, wpewebkit; no chromium/zram-generator): https://gitlab.com/buildroot.org/buildroot/-/tree/master/package
- picamera2 on PyPI: https://pypi.org/project/picamera2/ ; rpi-libcamera: https://pypi.org/project/rpi-libcamera/
- gpiod (libgpiod v2 Python) https://pypi.org/project/gpiod/ ; gpiozero https://pypi.org/project/gpiozero/ ; lgpio https://pypi.org/project/lgpio/
- Pi 4 V4L2 H.264 encoder notes: https://docs.rs/shiguredo_v4l2 ; https://lalitm.com/hw-encoding-raspi/ ; WebRTC example: https://github.com/trankin/RaspberryPi_WebRTC
- MediaMTX: https://pkg.go.dev/github.com/bluenviron/mediamtx ; stall report: https://github.com/bluenviron/mediamtx/issues/5093 ; go2rtc: https://github.com/AlexxIT/go2rtc , https://korben.info/en/go2rtc-swiss-army-knife-video-streaming.html
- iwd config and offloading: https://iwd.wiki.kernel.org/networkconfigurationsettings , https://iwd.wiki.kernel.org/offloading ; brcmfmac issues: https://github.com/RPi-Distro/firmware-nonfree/issues/34 , https://lkml.iu.edu/hypermail/linux/kernel/2312.2/04634.html , https://community.infineon.com/t5/Wi-Fi-Bluetooth-for-Linux/Anyone-working-on-IWD-AP-mode-for-fmac-driver/td-p/264565 ; eduroam iwd profile: https://bbs.archlinux.org/viewtopic.php?id=264726 ; iwd vs NM discussion: https://discourse.ubuntu.com/t/call-for-testing-improved-wifi-via-iwd/17795
- Pi OS Trixie rpi-swap: https://raspberry.tips/en/raspberrypi-tutorials/raspberry-pi-swap-file , https://peppe8o.com/?p=3806 ; Trixie tmpfs /tmp: https://feedbagel.com/post/debian-13-trixie-moves-tmp-directory-to-memory-based-tmpfs-filesystem
- Home Assistant OS: https://developers.home-assistant.io/docs/operating-system ; zswap change: https://git.homebeck.duckdns.org/d-two/operating-system/src/commit/75dcb932f8b511bee5556a40ef15b50a92dd2d65
- MGLRU thrashing prevention (min_ttl_ms): https://android-kvm.googlesource.com/linux/+/1332a809d95a4fc763cabe5ecb6d4fb6a6d941b2 , https://lkml.iu.edu/2203.2/06379.html
- earlyoom vs oomd experiences: https://gist.github.com/davidar/243ca325716c4cca0baf14a7b3069857 , https://hn.svelte.dev/item/45661253
- Go arm64 64K-page scavenger issue: https://github.com/golang/go/issues/78962 ; Go vs Rust figures (anecdotal): https://alestaweb.com/haber-go-vs-rust-backend-performance-2026-hangisi-daha-hzl-benchmark-karlatrmas-119 , https://www.danilchenko.dev/posts/rust-vs-go/
- Trixie cloud-init slow boot report: https://forum.armbian.com/profile/218058-anttix/content/
