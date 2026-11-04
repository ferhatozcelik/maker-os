# MakerOS 2.x: Base OS Layer Research (Raspberry Pi 4/400, 4 GB)

Date: 2026-10-31. Scope: replace the Pardus 21 / Debian 11 base of MakerOS v1.0.2 with an
independent OS built directly on the Linux kernel. The UI is web-based (Center plus a new
macOS-like shell). Users install Python (uv), PyTorch CPU, ONNX Runtime, MediaPipe, OpenCV,
Node.js and AI coding CLIs. The build host is an Apple Silicon Mac running native arm64 Docker
under colima.

Facts checked live on 2026-10-31 are marked [verified]. Everything else is from documentation
or project knowledge.

---

## 0. Executive summary (firm recommendations)

| Item | Recommendation |
| --- | --- |
| Build system | **Yocto Project 6.0 "Wrynose" (LTS until Apr 2030), driven by `kas`, in a custom distro layer `meta-makeros`.** Buildroot 2026.02 is the fallback, but only if on-device compilers and a package feed are dropped. |
| libc | **glibc** (2.43 in Wrynose). musl is ruled out because torch, onnxruntime and mediapipe publish only `manylinux_2_28_aarch64` wheels. |
| Init | **systemd** (trimmed): journald (volatile+limited), logind, networkd *or* NetworkManager, timesyncd, oomd, repart, growfs, sysext, zram-generator. |
| Kernel | **raspberrypi/linux `rpi-6.18.y`** (current default branch, the same 6.18 LTS line as Yocto 6.0), `bcm2711_defconfig` plus a MakerOS config fragment. Mainline is not used: it lacks the unicam/ISP camera stack, `vcio` (needed for tryboot) and some firmware interfaces. |
| Display/UI | Wayland kiosk: **cage + cog (WPE WebKit)** showing the local web shell, from meta-webkit `wrynose`. Chromium (meta-browser) is an optional later package because of its build cost and RAM use. |
| Rootfs | **EROFS (lz4hc), read-only, no initramfs**, `/etc` as an overlay (Yocto `overlayfs-etc`), `/var` and `/home` on `/data`. |
| Updates | **RAUC 1.15.x with a custom bootloader backend on top of Pi firmware `tryboot` + `autoboot.txt` (`tryboot_a_b=1`)**. No U-Boot. Signed bundles, with verity format where possible. |
| User software | Toolchain baked into the base image (gcc, make, cmake, git, pkg-config, python3-dev). uv-managed CPython and venvs, plus official Node tarballs, under `/data`. Optional big extras ship as **systemd-sysext** images built from the same Yocto tree. **podman** is the "apt escape hatch" (a Debian/Ubuntu container) for advanced users. |
| Partitions | MBR: p1 `MOSCFG` FAT (autoboot.txt), p2/p3 `BOOT_A/B` FAT, extended → p5/p6 `ROOT_A/B` EROFS, p7 `DATA` ext4, grown on first boot. |

The main reason for choosing Yocto over Buildroot is that **Buildroot cannot put a native gcc on
the target.** It was removed and the Buildroot FAQ says a full distribution is better if you need
one. Buildroot also has **no package feed or package manager workflow** and **no Chromium**. For a
student/maker OS where people run `pip install` (sdists compile C), `npm install` (node-gyp) and
`make`, those gaps are blocking. Yocto provides native toolchains (`tools-sdk` / `packagegroup-core-buildessential`),
package feeds (ipk/rpm), sstate incremental builds, an LTS with 4 years of CVE backports, and
ready layers for RPi, RAUC and WPE.

---

## 1. Build system

### 1.1 Options matrix

| Criterion | Buildroot 2026.02 | Yocto/OE 6.0 Wrynose | From-scratch (LFS-like scripts) | mkosi |
| --- | --- | --- | --- | --- |
| What it is | Make/Kconfig, builds one firmware image | BitBake + layers, builds a distro (packages + images + SDK) | Your own shell scripts | Assembles images from an existing distro's packages (Debian/Fedora/Arch/openSUSE) |
| "No Debian base" requirement | Yes | Yes | Yes | **No**, it always inherits a distro |
| Learning curve | Low (days) | High (2–4 weeks to be productive) | Low to start, very high to maintain | Low |
| First full build, 6-core arm64 VM | ~1–1.5 h for base, +~1 h for WPE | ~3–5 h without Chromium (Rust/LLVM avoided). **Chromium adds 2–4 h and needs ≥32 GB RAM** | Days of iteration | Minutes (downloads binaries) |
| Incremental rebuild | Per-package, but toolchain/global option changes force a full rebuild | **sstate cache**: only changed tasks rebuild, cache can be shared/mirrored | Manual | Fast |
| Reproducibility | Good (pinned hashes, `BR2_REPRODUCIBLE`) | **Very good** (reproducible builds tested on oe-core, SPDX SBOM, `kas` lock files) | Poor unless heavily engineered | Depends on distro snapshots |
| Packages available | ~3,000. **No target gcc, no Chromium, no Firefox** [verified: `package/gcc` is host-only, `chromium`/`firefox` 404]. Has nodejs, python3 3.14, libcamera 0.7.0, libcamera-apps, rauc 1.15.2, swupdate, podman, network-manager, wpewebkit, cog, cage, erofs-utils [verified] | ~10,000+ recipes (oe-core + meta-oe/python/networking + community). Native gcc, Chromium (meta-browser `wrynose`), WPE (meta-webkit `wrynose`), RAUC (meta-rauc `wrynose`), RPi BSP (meta-raspberrypi `wrynose`, kernels 6.1/6.6/6.12/**6.18**, libcamera-apps 1.9.0) [verified] | Whatever you package | Whole upstream distro |
| On-device package manager | None (opkg exists but Buildroot does not produce feeds) | **ipk (opkg), rpm (dnf) or deb (apt) feeds** generated as part of the build | DIY | Upstream (apt/dnf/pacman) |
| Security maintenance | LTS (Feb release, 1 yr) | **LTS 4 yrs (to Apr 2030)**, CVE checker class | You alone | Upstream distro |
| Host requirements | Modest | Wrynose docs: 32 GB RAM / 140 GB disk (mostly because of LLVM). Workable with 16–24 GB if LLVM/Chromium are avoided | – | – |

### 1.2 How comparable projects do it

| Project | Base | Update model | Notes |
| --- | --- | --- | --- |
| Home Assistant OS | **Buildroot** (fork, git submodule) + BR2_EXTERNAL | **RAUC** A/B, squashfs root, overlay data partition, apps in Docker | Closest architectural twin. Apps are containers, so it never needs an on-device gcc |
| Batocera | **Buildroot** | Whole-image update | Appliance with no dev tools on target |
| LibreELEC | Custom build scripts (OpenELEC "packages" system) | squashfs `SYSTEM` file swapped in `/storage/.update` | From-scratch style. Works for a single-purpose media appliance with a large volunteer team |
| balenaOS | **Yocto** (meta-balena) | Host OS A/B "hostapp" updates, apps in containers | Same pattern: immutable host, containers for user software |
| Torizon OS | **Yocto** + OSTree | OSTree A/B | Embedded dev-board OS with docker workloads |
| SteamOS 3 | Arch packages | **RAUC** A/B rootfs + casync | Read-only root, user software via Flatpak |
| ChromeOS | Gentoo Portage (from source) | A/B partitions + dm-verity | Linux dev environment (Crostini) is a **container/VM**, which is the escape-hatch pattern |

What these have in common: every immutable consumer OS keeps the host small and pushes user
software into a separate layer (containers, Flatpak, or user-space toolchains). MakerOS differs in
that students expect `gcc`/`make` on the host. Yocto is the only option here that provides that
natively.

### 1.3 Recommendation

**Yocto 6.0 Wrynose with `kas`.** Layers:

- `poky` (oe-core + bitbake) `wrynose`
- `meta-openembedded` (meta-oe, meta-python, meta-networking, meta-multimedia, meta-filesystems) `wrynose`
- `meta-raspberrypi` `wrynose`, MACHINE = `raspberrypi4-64`
- `meta-rauc` `wrynose` (meta-rauc-community only has `scarthgap`/`master` and uses U-Boot, so use it for reference only)
- `meta-webkit` `wrynose` (cog + wpewebkit)
- optional, later: `meta-browser` + `meta-clang` (`wrynose`) for Chromium, `meta-virtualization` for podman
- `meta-makeros` (ours): distro config, images, kernel fragment, Center, RAUC backend, sysext images

Running it on the Mac:

- Use the `ghcr.io/siemens/kas/kas` image, which is multi-arch with **arm64 available** [verified].
  (The `crops/poky` images are amd64-only [verified], so do not use them.)
- Put `build/`, `sstate-cache/` and `downloads/` in a **Docker volume inside the colima VM (ext4,
  case-sensitive)**, never on a macOS bind mount: BitBake needs a case-sensitive filesystem and
  virtiofs is slow.
- colima: `--cpu 6 --memory 20 --disk 300` (minimum 16 GB). `BB_NUMBER_THREADS=6`,
  `PARALLEL_MAKE=-j6`, `BB_PRESSURE_MAX_MEMORY` to avoid OOM.
- Avoid pulling in LLVM/clang and Rust where possible (Mesa: `PACKAGECONFIG:remove = "llvm"`
  works for v3d/vc4. Check `DEPENDS` on python3-cryptography etc. or accept Rust once, since it is cached by sstate).
- Publish `sstate-cache` and `downloads` as a build mirror (`SSTATE_MIRRORS`) so CI and other
  developers rebuild in minutes.

When the Buildroot fallback makes sense: if the project decides the host is an appliance and *all*
dev work happens in a podman container (the HAOS/balena model). Then Buildroot 2026.02 plus a
BR2_EXTERNAL tree is faster to learn and builds in about half the time. The cost is no native
compiler on the host and no package feed. A layout is sketched in section 8.2.

Not recommended:
- From-scratch/LFS: you become the security team for ~500 packages.
- mkosi: violates the "no distro base" goal. It would be the right tool if that goal were relaxed
  (e.g. mkosi + Debian trixie or Fedora).

---

## 2. libc

PyPI wheel tags for the latest releases, checked against the PyPI JSON API [verified 2026-10-31]:

| Package | Latest | aarch64 tag | glibc needed |
| --- | --- | --- | --- |
| torch | 2.15.0 | `manylinux_2_28_aarch64` (cp311–cp315) | ≥ 2.28 |
| onnxruntime | 1.31.1 | `manylinux_2_28_aarch64` (cp311–cp314) | ≥ 2.28 |
| mediapipe | 1.2.0 | `manylinux_2_28_aarch64` (py3) | ≥ 2.28 |
| opencv-python-headless | 5.0.0.93 | `manylinux_2_17` + `manylinux_2_28` | ≥ 2.17 |

None of these ship `musllinux` aarch64 wheels, so musl would force source builds of torch and
mediapipe, which is not realistic on a Pi. Official Node.js arm64 binaries (v18+) also need glibc ≥ 2.28,
and uv's python-build-standalone CPython builds target glibc.

**Decision: glibc**, using the version Wrynose ships (**2.43**, with gcc 15.2). Any glibc ≥ 2.34 also
satisfies future `manylinux_2_34` wheels. Buildroot 2026.02 would give glibc 2.42 [verified: `glibc.mk`].
Keep `GLIBC_GENERATE_LOCALES` small (en_US, tr_TR UTF-8) to save space.

Note: the gcc that students use must be the target glibc's native gcc (same sysroot), so that
locally built C extensions link against the system glibc.

---

## 3. Init and service manager

| | systemd | s6 / s6-rc | runit | OpenRC | busybox init |
| --- | --- | --- | --- | --- | --- |
| Boot time on Pi 4 (userspace) | ~2–4 s trimmed | ~1–2 s | ~1–2 s | ~2–3 s | ~1 s |
| RAM (PID1 + core daemons) | ~25–40 MB (journald, udevd, logind, networkd/resolved) | ~3–5 MB | ~2 MB | ~5 MB | ~1 MB |
| NetworkManager | Native | Works, but needs elogind/D-Bus wiring | Same | Works | Painful |
| udisks2 (USB automount in Center) | Native (needs logind/polkit) | Needs elogind | Needs elogind | Needs elogind | No |
| bluez | Works with any | | | | |
| Immutable-OS tooling | **repart, growfs, sysext/confext, tmpfiles, zram-generator, oomd, sysupdate, volatile modes, watchdog, boot-complete targets** | None | None | None | None |
| Yocto support | First-class (Wrynose sets systemd as the default init in its distro config) | Not well supported in OE | meta-oe | meta-oe | Yes |

**Decision: systemd.** On 4 GB of RAM the ~30 MB overhead does not matter, and the immutable/update
features (sysext, repart, growfs, `boot-complete.target` used by the RAUC "mark good" step, oomd,
watchdog) would otherwise have to be rebuilt by hand. Trimming list:

- journald: `Storage=volatile` (or persistent at `/data/log` with `SystemMaxUse=64M`), `RateLimitBurst`
- drop: `systemd-homed`, `machined` (unless podman), `importd`, `portabled`, `userdbd`, `hwdb` online updates (build hwdb at image time)
- `systemd-oomd` + PSI enabled in the kernel: PyTorch jobs can push the 4 GB board into thrashing
- `zram-generator`: `zram-size = min(ram / 2, 2048)`, `compression-algorithm = zstd`
- Networking: **NetworkManager** (Center already uses nmcli-style flows, and handles Wi-Fi AP/hotspot,
  captive portal and Ethernet sharing) with `systemd-resolved`. NM 1.56 is in meta-oe Wrynose [verified].

---

## 4. Raspberry Pi 4 kernel, firmware and boot config

### 4.1 Kernel source

- **raspberrypi/linux `rpi-6.18.y`.** It is the default branch of the repo [verified] and Raspberry Pi OS moved
  to 6.18 LTS in 2026. meta-raspberrypi Wrynose ships `linux-raspberrypi_6.18.bb` [verified].
  Pin a commit SHA (that branch is rebased/force-updated, so never track the branch head).
- Mainline (6.18/6.19) boots a Pi 4 and has vc4/v3d KMS, but is missing or behind on: the
  **unicam + ISP (bcm2835-isp) camera pipeline needed by libcamera's rpi/vc4 pipeline handler**, the
  `/dev/vcio` mailbox (used to set the **tryboot** flag), some overlays, and vendor fixes. The camera
  is a core MakerOS feature, so the downstream kernel is required.
- Defconfig: `bcm2711_defconfig` + `meta-makeros/recipes-kernel/linux/files/makeros.cfg` fragment.

### 4.2 Kernel config fragment (performance and immutability)

```
# Scheduling / latency (desktop-like responsiveness, low overhead)
CONFIG_PREEMPT_DYNAMIC=y        # boot-selectable; default preempt=voluntary, try preempt=full
CONFIG_HZ_250=y                 # keep 250; 1000 adds overhead for little UI gain on A72
CONFIG_NO_HZ_IDLE=y
CONFIG_CPU_FREQ_DEFAULT_GOV_SCHEDUTIL=y
CONFIG_SCHED_AUTOGROUP=y
# Memory (4 GB, ML workloads)
CONFIG_LRU_GEN=y
CONFIG_LRU_GEN_ENABLED=y        # MGLRU: better behavior under memory pressure
CONFIG_PSI=y                    # for systemd-oomd
CONFIG_ZRAM=y
CONFIG_ZRAM_BACKEND_ZSTD=y
CONFIG_ZRAM_BACKEND_LZ4=y
CONFIG_ZSMALLOC=y
CONFIG_TRANSPARENT_HUGEPAGE=y
CONFIG_TRANSPARENT_HUGEPAGE_MADVISE=y
# Filesystems (built in, not modules: no initramfs)
CONFIG_EROFS_FS=y
CONFIG_EROFS_FS_ZIP=y
CONFIG_EROFS_FS_ZIP_ZSTD=y
CONFIG_SQUASHFS=y
CONFIG_SQUASHFS_ZSTD=y          # for RAUC bundles if not using erofs bundles
CONFIG_OVERLAY_FS=y
CONFIG_EXT4_FS=y
CONFIG_VFAT_FS=y
CONFIG_DM_VERITY=y              # RAUC verity bundles; optional rootfs verity later
CONFIG_BLK_DEV_LOOP=y
# Containers (podman escape hatch)
CONFIG_USER_NS=y
CONFIG_CGROUPS=y                # plus memory, cpu, pids; cgroup v2 only
CONFIG_VETH=m
CONFIG_BRIDGE=m
CONFIG_NETFILTER_XT_MATCH_ADDRTYPE=m
CONFIG_NF_TABLES=m
# Size / speed
# CONFIG_DEBUG_INFO is not set (keep a separate -dbg build)
CONFIG_MODULE_COMPRESS_ZSTD=y
CONFIG_WATCHDOG=y
CONFIG_BCM2835_WDT=y
```

Notes:
- Pi 4 uses 4 K pages (`kernel8.img`). The 16 K page kernel is Pi 5 only (`kernel_2712.img`).
- Compare the fragment against the pinned `bcm2711_defconfig` (`make savedefconfig` diff in CI):
  several of these options are already on downstream.
- Use the `performance` governor only for benchmarks. `schedutil` with `arm_freq=1800` (Pi 4 rev 1.4+
  default) is the sane default. Overclocking (`arm_freq=2000`, `over_voltage=6`) should be opt-in from Center
  and requires cooling.

### 4.3 Firmware and boot files (BOOT_A / BOOT_B FAT)

From `raspberrypi/firmware` (pinned SHA, as v1.0.2 already does) via meta-raspberrypi `rpi-bootfiles`:
`start4.elf`, `fixup4.dat` (plus `start4x.elf`/`fixup4x.dat` only if needed; `start4cd` is the
minimal variant, but it breaks the camera), `bcm2711-rpi-4-b.dtb`, `bcm2711-rpi-400.dtb`,
`bcm2711-rpi-cm4.dtb`, `overlays/*.dtbo`, `kernel8.img`, `config.txt`, `cmdline.txt`.
The Pi 4 has no `bootcode.bin` (the second stage is in EEPROM). Require **EEPROM ≥ 2025-03** (tryboot,
`tryboot_a_b`, `boot_partition` conditionals). Center already flashes EEPROM via `rpi-eeprom`,
so ship `rpi-eeprom` + firmware images.

Wi-Fi/BT: `linux-firmware-rpidistro` (brcmfmac43455-sdio.bin/.clm_blob/.txt for Pi 4/400,
CYW43455) and `bluez-firmware-rpidistro` (BCM4345C0.hcd) from meta-raspberrypi [verified recipes].
Also add `regulatory.db` (wireless-regdb) and set the country via NM/Center.

`config.txt` (per slot):

```
arm_64bit=1
kernel=kernel8.img
dtoverlay=vc4-kms-v3d,cma-384   # KMS; CMA big enough for camera + GPU + WPE
max_framebuffers=2
disable_overscan=1
camera_auto_detect=1
display_auto_detect=1
dtparam=audio=on
dtparam=i2c_arm=on
dtparam=spi=on
enable_uart=1
disable_splash=1                # faster boot; draw our own splash via plymouth-free KMS
boot_delay=0
arm_boost=1
[pi400]
# keyboard-specific bits if any
```

`cmdline.txt` (slot A example, slot B has `root=PARTLABEL/PARTUUID` of ROOT_B):

```
console=tty3 root=PARTUUID=<uuid>-05 rootfstype=erofs ro rootwait quiet loglevel=3
fsck.mode=skip systemd.show_status=auto cgroup_enable=memory video=HDMI-A-1:1920x1080@60D
```

Camera: libcamera with the RPi pipeline handler (`libcamera_%.bbappend` in meta-raspberrypi enables
`raspberrypi` pipeline/IPA), **rpicam-apps** (`libcamera-apps_1.9.0.bb` [verified]) and
**picamera2** (pip install in venv, or package it in meta-makeros). Unicam/ISP drivers come from the
downstream kernel. `camera_auto_detect=1` handles the OV5647/IMX219/IMX477/IMX708 overlays.

GPU/Display: Mesa with `gallium vc4 v3d` (no llvmpipe), Wayland `cage` (kiosk compositor) running `cog`
with the WPE `wl` platform, or the direct `drm` platform for lowest RAM.

### 4.4 Boot time targets

With firmware ~2.5–3.5 s (EEPROM SD boot), kernel without initramfs ~1.5 s, and trimmed systemd
to `graphical.target` ~3 s, the Center UI should be on screen in **~9–12 s** (Pi OS desktop takes
~20–30 s). Measure with `systemd-analyze` and `BOOT_UART`. Use `systemd-analyze critical-chain` in CI on hardware.

---

## 5. Partitions, read-only root, A/B OTA and user software

### 5.1 Read-only root: EROFS vs squashfs

| | EROFS (lz4hc) | squashfs (zstd/xz) | ext4 ro |
| --- | --- | --- | --- |
| Random read / app start | **Best** (fixed-size output blocks, in-place decompression; Android/ChromeOS-class) | Good | Best uncompressed |
| Compression ratio | ~0.5–0.55 (lz4hc), ~0.45 with zstd/lzma | ~0.4–0.45 | 1.0 |
| CPU cost on A72 | Low | Medium (zstd) to high (xz) | None |
| Tooling | erofs-utils in OE/Buildroot [verified] | Mature | Mature |

**Decision: EROFS lz4hc** (switch to zstd/lzma per-file later if slot size gets tight). The
rootfs with toolchain, python3, WPE, NM, bluez and libcamera is estimated at ~1.1–1.4 GB
uncompressed, or ~600–750 MB as EROFS.

Writable state:
- `/etc`: Yocto `IMAGE_FEATURES += "read-only-rootfs overlayfs-etc"`. overlayfs-etc mounts
  `/data/overlay/etc/{upper,work}` over `/etc` from a tiny pre-init before systemd starts, with no initramfs.
- `/var` → bind `/data/var`. `/home` → `/data/home`. `/opt/makeros` (user-installed toolchains) →
  `/data/opt`. `/tmp` tmpfs.
- `/data/extensions` → systemd-sysext images (`/var/lib/extensions` symlink).
- `/usr/local` → overlay or bind from `/data/usr-local` so `make install` works for students.
  Document that `make install` goes there.

### 5.2 Partition table (MBR, 32 GB card example)

| # | Label | FS | Size | Contents |
| --- | --- | --- | --- | --- |
| 1 | `MOSCFG` | FAT32 | 64 MiB | `autoboot.txt` only (+ `wifi.txt`/`makeros-setup.txt` user-editable first-boot config, as in v1) |
| 2 | `BOOT_A` | FAT32 | 256 MiB | firmware, kernel8.img, DTBs, overlays, config.txt, cmdline.txt (root=p5) |
| 3 | `BOOT_B` | FAT32 | 256 MiB | same, cmdline.txt root=p6 |
| 4 | (extended) | – | rest | – |
| 5 | `ROOT_A` | EROFS | 2.5 GiB | rootfs slot A |
| 6 | `ROOT_B` | EROFS | 2.5 GiB | rootfs slot B |
| 7 | `DATA` | ext4 (`noatime,commit=60`) | rest (grown on 1st boot by `systemd-repart`/`growfs`) | /home, /var, /etc overlay, /opt, uv/node, sysext, RAUC status |

Shipped image size is ~1.5 GB (`.img.xz` ~700 MB). DATA starts at ~512 MB and grows. MBR+extended is
the well-tested path for `autoboot.txt` partition numbers. GPT is supported by the Pi 4 EEPROM and
is an option later, but test tryboot with it first. Alternative to consider: f2fs for DATA, which
is gentler on cheap SD cards but has weaker fsck. Default to ext4.

`MOSCFG/autoboot.txt`:

```
[all]
tryboot_a_b=1
boot_partition=2
[tryboot]
boot_partition=3
```

### 5.3 A/B update options

| | RAUC 1.15.2 | SWUpdate | Mender | systemd-sysupdate |
| --- | --- | --- | --- | --- |
| Model | A/B slots, signed bundles (verity, casync/adaptive delta, HTTP streaming) | Very flexible handlers, Lua, sw-description | A/B + hosted management server (open-core) | Pulls versioned partition/file images and matches them to partitions |
| RPi tryboot | **Custom backend script** (native backend PR rauc#1599 "bootchooser: add Raspberry Pi firmware initial support" is **still open** [verified]). Bootlin has a working Pi 5 recipe that carries over to Pi 4 | Possible with a custom handler/script | Needs U-Boot or GRUB integration | Has no bootloader logic. You must write tryboot + boot-counting glue |
| Used by | HAOS, SteamOS, many Yocto products | Many industrial products | Commercial fleets | Newer systemd-based images (ParticleOS, etc.) |
| Yocto/Buildroot support | meta-rauc `wrynose`, Buildroot package | meta-swupdate, Buildroot | meta-mender (Buildroot package missing [verified 404]) | systemd only |
| Fit | **Best** | Good, more DIY | Overkill, server lock-in | Too DIY for A/B + rollback |

**Decision: RAUC + custom tryboot backend** (no U-Boot, which saves ~1 s of boot and a component):

- `system.conf`: `bootloader=custom`, slot groups `boot.0`(p2)+`rootfs.0`(p5) and `boot.1`(p3)+`rootfs.1`(p6),
  with `data-directory=/data/rauc`.
- Backend (`/usr/lib/rauc/rpi-tryboot-backend`, following Bootlin's design):
  - `get-primary`: parse the `[all]` `boot_partition` in autoboot.txt
  - `set-primary`: set the one-shot tryboot flag via `vcmailbox` (`/dev/vcio`, downstream kernel) or `reboot "0 tryboot"`
  - `get-state` / `get-current`: read `/proc/device-tree/chosen/bootloader/partition` and tryboot status
  - `set-state good`: swap the `[all]`/`[tryboot]` sections (an atomic write of the FAT file: write a temp file, fsync, rename)
- The "mark good" service runs after `boot-complete.target`, requiring Center healthy + network-online + no
  failed critical units. If tryboot fails, the next cold boot falls back to `[all]` automatically.
- Bundles: `format=verity`, signed with a project key. Keep the CA in the image and the private key offline/CI-secret.
  Optionally use adaptive/casync for delta downloads.
- Caveats: the firmware has no boot counter, so a hang after the kernel starts needs the **hardware
  watchdog** (`bcm2835_wdt`, systemd `RuntimeWatchdogSec=15`) so a reboot falls back. Never point `[all]` at an
  unproven slot.
- The Center OTA page replaces v1's `10-ota-bundle.sh` flow: `rauc install https://.../makeros-2.x.raucb`.

### 5.4 User-installable software on an immutable OS

What makers expect, and what is realistic:

1. **Built into the base image (always present):** gcc/g++ 15, binutils, make, cmake, ninja, pkg-config,
   autoconf/automake/libtool, git, git-lfs, python3 + python3-dev headers + pip + venv, openssh, curl,
   htop, nano/vim, i2c-tools, libgpiod tools, `-dev` packages for libcamera, OpenSSL, zlib, libffi,
   libjpeg, libpng, alsa, v4l, libusb, libgpiod. Yocto: `IMAGE_FEATURES += "tools-sdk dev-pkgs"` gives all `-dev`.
   That is too heavy, so list them explicitly in `packagegroup-makeros-dev`. Expect +300–400 MB uncompressed.
2. **Python:** `uv` (pinned binary, as in v1) in the image. `uv python install 3.12` and venvs go to
   `/data/home/<user>/.local/share/uv`. The default `ai-env` venv is preinstalled on DATA at image build
   time (from v1's `vendor/ai-requirements.txt`), so torch, onnxruntime, mediapipe and opencv install as
   manylinux_2_28 wheels with no compiling. The system python3 is only for Center and OS tools (PEP 668
   `EXTERNALLY-MANAGED` marker to steer users to uv/venv).
3. **Node.js:** ship the **official nodejs.org arm64 tarball** (v24 LTS, as in v1) in `/usr/lib/makeros/node`
   rather than compiling meta-oe nodejs 22.23 [verified version], which saves ~45 min of build. Let users
   add versions under `/data` with `fnm`. npm global prefix is `~/.local`. node-gyp works because gcc + python3 are present.
4. **AI CLIs** (Claude Code etc.): user-space npm or native installers into `~/.local/bin`. glibc arm64 is
   fully supported. Center keeps v1's agent install UI.
5. **Optional extras as systemd-sysext images** (`makeros-ext-chromium`, `makeros-ext-vscode-server`,
   `makeros-ext-ros2`, `makeros-ext-arduino`, …): built by the same Yocto tree, signed, downloaded by Center
   to `/data/extensions` and activated with `systemd-sysext refresh`. They are versioned against the OS release
   (`extension-release` with `ID=makeros VERSION_ID=…`), so they cannot break across A/B updates.
6. **"apt" escape hatch:** podman (rootless) + a `mos box` helper (distrobox-like) that starts a
   Debian trixie or Ubuntu 24.04 arm64 container with `$HOME` shared. Students who need `apt install foo`
   get it without touching the host. This is ChromeOS's Crostini pattern.
7. **Optional package feed (opkg/ipk)** from the Yocto build, for advanced users only, installing into a
   writable `/usr` overlay. **Not recommended as the default**, because overlaid `/usr` drifts from the A/B
   slot and breaks clean updates. If offered, the overlay is reset on each RAUC update and the user re-installs
   from a recorded list. Prefer sysext.

Rejected: Nix (store size and learning curve for students, and builds on the Pi), Flatpak
(desktop-app oriented, not CLI/dev), a writable rootfs (loses atomic updates and rollback).

---

## 6. On-device toolchain availability

| Tool | Yocto 6.0 | Buildroot 2026.02 |
| --- | --- | --- |
| gcc/g++ native | **Yes** (gcc 15.2, `packagegroup-core-buildessential`) | **No** (removed; only out-of-tree hacks such as the UC Davis `gcc-target` package from 2021) |
| make, cmake, ninja, autotools | Yes | make yes, cmake target yes, autotools partial |
| git | Yes | Yes |
| python3 + headers | Yes (3.13/3.14 + `python3-dev`) | python3 yes; headers on target not supported (no staging on target) |
| Node.js | meta-oe 22.x or official tarball | 22.22 or official tarball |
| `-dev` headers for libs | Yes (`dev-pkgs`) | Not designed for it |
| gdb, strace, perf, valgrind | Yes | gdb/strace yes |

So students can `pip install` packages without wheels (e.g. `RPi.GPIO`, `smbus2`, `spidev`,
`lgpio`, `evdev`), build Arduino-cli cores, use `node-gyp` (serialport, onoff) and run `make` for C
coursework only with Yocto (or Buildroot + container).

---

## 7. RAM budget on a 4 GB Pi 4 (idle, estimate)

| Component | RSS |
| --- | --- |
| Kernel + CMA reserve (384 MB is reserved but usable by movable pages) | ~120 MB |
| systemd, journald, udevd, logind, resolved, dbus | ~35 MB |
| NetworkManager + wpa_supplicant + bluetoothd + avahi | ~30 MB |
| Center (Python stdlib) | ~30–50 MB |
| cage + cog/WPE (shell page, GPU composited) | ~150–250 MB |
| **Total idle** | **~400–500 MB** (Pardus + XFCE + Plank + Chromium is ~900 MB–1.2 GB) |

That leaves ~3.3 GB plus zram (~2 GB compressed swap) for PyTorch/MediaPipe/OpenCV and AI CLIs. Chromium as
the kiosk would cost an extra ~300–500 MB, another reason to use WPE for the shell.

---

## 8. Proposed repository layout

### 8.1 Yocto (recommended): `maker-os/os/`

```
os/
├── kas/
│   ├── base.yml                 # repos + pinned commits (poky, meta-oe, meta-raspberrypi,
│   │                            #   meta-rauc, meta-webkit) all on 'wrynose'
│   ├── makeros-rpi4.yml         # machine: raspberrypi4-64, distro: makeros, target images
│   ├── dev.yml                  # debug-tweaks, ssh root, -dbg kernel (overlay for dev builds)
│   ├── chromium.yml             # optional: meta-browser + meta-clang
│   └── makeros-rpi4.lock.yml    # kas lock file (exact SHAs) -> reproducibility
├── meta-makeros/
│   ├── conf/
│   │   ├── layer.conf
│   │   ├── distro/makeros.conf  # glibc, systemd, usrmerge, wayland, opengl, no x11,
│   │   │                        #   PACKAGE_CLASSES=package_ipk, read-only-rootfs, overlayfs-etc,
│   │   │                        #   reproducible, SDE, locales en_US/tr_TR, security flags
│   │   └── machine/include/makeros-rpi.inc   # RPI_KERNEL_DEVICETREE trim, config.txt vars
│   ├── classes/makeros-sysext.bbclass        # builds signed sysext images from packagegroups
│   ├── recipes-bsp/
│   │   ├── rpi-config/rpi-config_%.bbappend  # config.txt above
│   │   ├── rpi-cmdline/                       # per-slot cmdline.txt
│   │   ├── makeros-autoboot/                  # autoboot.txt for MOSCFG partition
│   │   └── rpi-eeprom/                        # EEPROM images + rpi-eeprom-update
│   ├── recipes-kernel/linux/
│   │   ├── linux-raspberrypi_6.18.bbappend   # SRCREV pin, fragment
│   │   └── files/makeros.cfg                  # section 4.2
│   ├── recipes-core/
│   │   ├── images/
│   │   │   ├── makeros-image.bb               # main rootfs (EROFS)
│   │   │   ├── makeros-image-dev.bb           # + debug tools
│   │   │   └── makeros-ext-*.bb               # sysext images
│   │   ├── packagegroups/
│   │   │   ├── packagegroup-makeros-base.bb   # systemd, NM, bluez, avahi, zram, rauc, podman
│   │   │   ├── packagegroup-makeros-dev.bb    # gcc, make, cmake, git, python3-dev, -dev libs
│   │   │   ├── packagegroup-makeros-camera.bb # libcamera, rpicam-apps, v4l-utils, ffmpeg
│   │   │   └── packagegroup-makeros-ui.bb     # cage, cog, wpewebkit, mesa v3d, fonts
│   │   ├── systemd/systemd_%.bbappend         # trimmed PACKAGECONFIG, journald, oomd, repart
│   │   ├── systemd-conf/                      # zram-generator, repart.d (DATA grow), tmpfiles
│   │   ├── base-files/                        # fstab, os-release (ID=makeros), motd
│   │   └── makeros-firstboot/                 # SSH keys, hostname, user setup, wifi.txt import
│   ├── recipes-ota/
│   │   ├── rauc/rauc-conf.bbappend            # system.conf, keyring (CA)
│   │   ├── rauc/files/rpi-tryboot-backend     # custom bootloader backend (section 5.3)
│   │   ├── makeros-mark-good/                 # boot-complete -> rauc status mark-good
│   │   └── bundles/makeros-bundle.bb          # .raucb (verity, signed)
│   ├── recipes-makeros/
│   │   ├── makeros-center/                    # existing Center (Python stdlib) port 80
│   │   ├── makeros-shell/                     # new macOS-like web shell (static assets)
│   │   ├── makeros-kiosk/                     # cage+cog systemd unit on tty1, autologin user
│   │   ├── uv-bin/                            # pinned uv binary
│   │   ├── nodejs-bin/                        # official Node v24 tarball -> /usr/lib/makeros/node
│   │   └── makeros-ai-env/                    # prebuilt venv on DATA at image time (wheels lock)
│   └── wic/makeros-rpi4-ab.wks.in             # partition table, section 5.2
├── keys/README                                # how to create/rotate RAUC signing keys (no secrets)
├── scripts/
│   ├── build.sh                               # colima check -> kas-container build
│   ├── flash-sd.sh                            # reused from v1
│   └── publish.sh                             # img.xz + raucb + sysext + sha256 + SBOM
└── docker/colima.md                           # VM sizing, volumes (case-sensitive)
```

Build command:
`kas-container --runtime-args "-v makeros-build:/build" build os/kas/makeros-rpi4.yml`.
Outputs: `makeros-rpi4-<ver>.wic.xz`, `makeros-<ver>.raucb`, `makeros-ext-*.raw`, SPDX SBOM.

### 8.2 Buildroot fallback: BR2_EXTERNAL `maker-os/os-br/`

```
os-br/
├── external.desc            # name: MAKEROS
├── external.mk / Config.in  # include package/*/*.mk, Config.in
├── configs/makeros_rpi4_defconfig   # Bootlin aarch64 glibc toolchain, systemd, rpi-6.18 pinned tarball,
│                                    # BR2_LINUX_KERNEL_DEFCONFIG=bcm2711 + fragment, erofs rootfs,
│                                    # rauc, network-manager, libcamera(+apps), wpewebkit, cog, cage, podman
├── board/rpi4/
│   ├── linux.fragment
│   ├── config.txt, cmdline-a.txt, cmdline-b.txt, autoboot.txt
│   ├── genimage.cfg         # partition table 5.2
│   ├── post-build.sh        # /etc overlay mounts, os-release, symlinks to /data
│   ├── post-image.sh        # genimage + rauc bundle
│   └── rootfs-overlay/      # systemd units, rauc system.conf, tryboot backend, zram, repart.d
├── package/
│   ├── makeros-center/  makeros-shell/  makeros-kiosk/
│   ├── uv-bin/  nodejs-bin/
│   └── makeros-firstboot/
└── patches/                 # BR2_GLOBAL_PATCH_DIR
```

With Buildroot, development tooling must live in a podman dev container (Debian/Ubuntu arm64) that
`mos box` starts. That is the HAOS/balena model.

---

## 9. Risks and mitigations

| # | Risk | Impact | Mitigation |
| --- | --- | --- | --- |
| 1 | Yocto learning curve and first-build time (3–5 h) for a solo maintainer | Slow start | kas + sstate mirror. Start from meta-raspberrypi `core-image-base`, then add layers one at a time. Buildroot prototype is acceptable for the first 2 weeks |
| 2 | Mac host RAM (Yocto 6.0 recommends 32 GB) | OOM, failed builds | Avoid LLVM/Chromium. `BB_PRESSURE_MAX_MEMORY`, `PARALLEL_MAKE=-j4` for heavy recipes. Optional cloud arm64 runner (e.g. 16-core Graviton/Ampere) for CI |
| 3 | No native RAUC tryboot backend (PR #1599 open) | Custom code in the critical update path | Small, well-tested script based on Bootlin's design. Hardware-in-the-loop test: power-cut during install, bad bundle, kernel panic → must fall back. Track upstream PR |
| 4 | Firmware has no boot counter | A slot that hangs after the kernel starts never falls back | bcm2835 watchdog + systemd RuntimeWatchdog. `[all]` only changes after mark-good |
| 5 | Downstream kernel branch rebases | Broken pins | Pin SHAs (never the branch), mirror tarballs in `downloads/` |
| 6 | WPE WebKit compatibility of the new web shell | UI bugs vs Chrome | Test the shell in WPE/Epiphany during development. Avoid Chrome-only APIs. Chromium sysext as a fallback |
| 7 | User installs in `/usr` (opkg/make install) diverge from A/B slots | Broken updates | `/usr/local` on DATA, sysext for extras, podman for apt, PEP 668 marker. No writable `/usr` by default |
| 8 | Losing Debian's ecosystem (random `apt install` instructions in tutorials) | Student friction | `mos box` Debian container + docs. Center "install" catalog maps common tutorial packages to sysext/uv/npm |
| 9 | Security maintenance of an own distro | CVE debt | Yocto LTS (to 2030), `cve-check`/SBOM in CI, monthly point-release rebuilds |
| 10 | SD card wear and corruption on DATA | Data loss | ext4 `commit=60`, volatile journal, zram swap (no swap on SD), A/B root is read-only. Recommend A2 cards. Optional USB SSD boot (Pi 4 EEPROM supports it; same layout) |
| 11 | MediaPipe/torch wheel tags move to `manylinux_2_34+` | Install failures | glibc 2.43 already covers future tags. Check wheel tags in CI against the image's glibc |
| 12 | Camera stack changes (libcamera 0.7, rpicam-apps 1.9) vs picamera2 version | Camera broken | Pin libcamera/rpicam/picamera2 as a tested set, plus a CI smoke test on a real Pi with a camera |

---

## 10. Suggested first milestones

1. Week 1–2: kas + Wrynose + meta-raspberrypi builds `core-image-base` with systemd. It boots on Pi 4 and 400, with Wi-Fi, BT, HDMI KMS and camera (`rpicam-hello`).
2. Week 3: EROFS read-only root, overlayfs-etc, DATA with repart/growfs, zram, `packagegroup-makeros-dev`, uv + Node. Check torch/mediapipe wheel installs and imports.
3. Week 4: cage + cog kiosk + Center port. Measure boot time and idle RAM against the targets in sections 4.4 and 7.
4. Week 5–6: A/B wic layout, RAUC custom tryboot backend, signed bundles, watchdog, rollback HIL tests.
5. Week 7+: sysext extras, `mos box`, CI with sstate mirror, SBOM/CVE reporting.

---

## Sources

- Buildroot 2026.02 release (LWN): https://lwn.net/Articles/1061543 , https://noise.getoto.net/2026/03/05/buildroot-2026-02-released/
- Buildroot glibc.mk (2026.02.x, glibc 2.42): https://gitlab.com/buildroot.org/buildroot/-/raw/2026.02.x/package/glibc/glibc.mk
- Buildroot raspberrypi4_64_defconfig: https://gitlab.com/buildroot.org/buildroot/-/raw/2026.02.x/configs/raspberrypi4_64_defconfig
- Buildroot FAQ (no compiler on target): https://bootlin.com/~thomas/site/buildroot/faq-troubleshooting.html
- Buildroot target gcc workaround (UC Davis): https://luplab.cs.ucdavis.edu/2022/01/06/buildroot-and-compiler-on-target.html
- Yocto 6.0 Wrynose release notes: https://docs.yoctoproject.org/migration-guides/release-notes-6.0.html
- CNX Software, Yocto 6.0 with Linux 6.18 LTS: https://www.cnx-software.com/2026/05/14/yocto-project-6-0-wrynose-released-with-linux-6-18-lts/
- meta-raspberrypi: https://github.com/agherzan/meta-raspberrypi
- meta-rauc / meta-rauc-community: https://github.com/rauc/meta-rauc , https://github.com/rauc/meta-rauc-community
- meta-webkit (WPE/cog): https://github.com/Igalia/meta-webkit
- meta-browser (Chromium): https://github.com/OSSystems/meta-browser
- TI Chromium-on-Yocto build cost notes: https://software-dl.ti.com/jacinto7/esd/processor-sdk-linux-am67/latest/exports/docs/system/Demo_User_Guides/Chromium_Browser.html
- kas: https://github.com/siemens/kas (container ghcr.io/siemens/kas/kas, amd64+arm64)
- raspberrypi/linux (default branch rpi-6.18.y): https://github.com/raspberrypi/linux
- Raspberry Pi OS on 6.18 LTS: https://www.linux.org/threads/phoronix-raspberry-pi-os-moves-to-linux-6-18-lts-kernel-updated-labwc-compositor.67977/
- Raspberry Pi config.txt / autoboot.txt / tryboot: https://www.raspberrypi.com/documentation/computers/config_txt.html
- Bootlin, Safe updates using RAUC on Raspberry Pi 5 (tryboot backend design): https://bootlin.com/?p=19769
- RAUC PR #1599 (RPi firmware bootchooser, open): https://github.com/rauc/rauc/pull/1599
- RAUC releases (v1.15.2, 2026-03-27): https://github.com/rauc/rauc/releases
- Ubuntu Pi A/B boot (tryboot): https://canonical-ubuntu-hardware-support.readthedocs-hosted.com/boards/explanations/piboot-ab
- HAOS developer docs (Buildroot, RAUC): https://developers.home-assistant.io/docs/operating-system , https://developers.home-assistant.io/docs/operating-system/update-system
- HAOS 18.0 (Pi 4 KMS switch, RAUC keyring): https://github.com/home-assistant/operating-system/releases/tag/18.0.rc1
- PyTorch manylinux_2_28 switch: https://dev-discuss.pytorch.org/t/pytorch-linux-wheels-switching-to-new-wheel-build-platform-manylinux-2-28-on-november-12-2024/2581
- PyPI JSON API (wheel tags checked): https://pypi.org/pypi/torch/json , https://pypi.org/pypi/onnxruntime/json , https://pypi.org/pypi/mediapipe/json , https://pypi.org/pypi/opencv-python-headless/json
- MediaPipe wheel build docs: https://developers.google.com/edge/mediapipe/solutions/build_python
