# MakerOS 2 — işletim sistemi ağacı (`os/`)

Bu dizin, MakerOS 2'nin Raspberry Pi 4 / 400 imajını üreten **Buildroot
`BR2_EXTERNAL` ağacıdır** (adı `MAKEROS`). Derleme tamamen Docker içinde
yapılır; Mac'e (veya Linux'a) Docker dışında hiçbir araç kurulmaz.

| | |
|---|---|
| Buildroot | **2026.02.3** (LTS, sha256 doğrulanır) |
| Toolchain | Buildroot iç toolchain: gcc 14, **glibc 2.42**, çekirdek başlıkları 6.18 |
| Çekirdek | `raspberrypi/linux` **rpi-6.18.y @ b3495ea401d7** (6.18.59), `bcm2711_defconfig` + `board/rpi4/linux.fragment` |
| Init | systemd 258 (logind, resolved, timesyncd, oomd, repart, sysext) |
| Kök FS | **EROFS (lz4hc)**, salt-okunur, A/B slot, initramfs yok |
| Ağ | NetworkManager + wpa_supplicant, resolved (mDNS) |
| Ekran | labwc 0.9.8 (wlroots 0.19) + cog (WPE WebKit 2.50) → `http://127.0.0.1/`; monitör yoksa sanal ekran (`LABWC_FALLBACK_OUTPUT`) |
| Uzak masaüstü | wayvnc 0.10.2 + neatvnc 1.0.4 (Tight/JPEG, libjpeg-turbo), yalnız `127.0.0.1:5900`; tarayıcıya makerd `/api/v1/vnc` + noVNC 1.7 |
| OTA | RAUC 1.15 + Pi firmware `tryboot` (özel arka uç betiği) |

## Derleme

Gereken: Docker (Mac'te colima, arm64 yerel; önerilen 6 CPU / 10 GB RAM / 80 GB disk).

```sh
cd os
./build.sh defconfig      # makeros_rpi4_defconfig'i uygula
./build.sh build          # tam derleme (ilk sefer ~1,5–3 saat)
# sonuç: os/out/makeros-2.0.0-rpi4.img.xz (+ .sha256)
```

Diğer komutlar:

| Komut | İş |
|---|---|
| `./build.sh menuconfig` | Buildroot ayarları (interaktif) |
| `./build.sh linux-menuconfig` | Çekirdek ayarları |
| `./build.sh savedefconfig` | `.config` → `os/out/makeros_rpi4_defconfig` (sonra `configs/` ile karşılaştırın) |
| `./build.sh make makerd-rebuild` | Tek paketi yeniden derle (her türlü make hedefi) |
| `./build.sh shell` | Derleyici kapta bash (`/build/output`) |
| `./build.sh clean` | `output/` silinir; indirmeler (`dl/`) ve ccache kalır |
| `./build.sh clean --all` | Docker volume'u tamamen silinir |

Ortam değişkenleri: `MAKEROS_JOBS` (paralellik, varsayılan cpu+1),
`MAKEROS_VOLUME` (varsayılan `makeros-build`).

Nasıl çalışır:

- `docker/Dockerfile` — Debian trixie arm64 derleyici imajı (Buildroot host bağımlılıkları).
- Büyük dizinler (`/build/output`, `/build/dl`, `/build/ccache`, açılmış Buildroot)
  **adlandırılmış Docker volume'unda** durur: büyük/küçük harf duyarlı ext4,
  macOS bind mount değil (hem doğruluk hem hız için).
- Depo kapta `/src` olarak **salt-okunur** bağlanır; yalnızca `os/out/` yazılabilir.
  `makerd` ve `makeros-shell` paketleri kaynaklarını `../makerd` ve `../shell`
  dizinlerinden (`local` site method) alır.

SD karta yazma: Raspberry Pi Imager → "Use custom" → `.img.xz`, ya da
`xz -dc makeros-*.img.xz | sudo dd of=/dev/rdiskN bs=4m`.
Gereken: **Pi 4 EEPROM ≥ 2025-03** (tryboot / `tryboot_a_b`).

## Bölüm düzeni (MBR, disk imzası `0x4d4f5332` = "MOS2")

| # | Etiket | FS | Boyut | İçerik |
|---|---|---|---|---|
| 1 | `MOSCFG` | FAT32 | 64 MiB | `autoboot.txt` (A/B seçici), `wifi.txt` (isteğe bağlı), `makeros-status.txt` |
| 2 | `BOOT_A` | FAT32 | 256 MiB | `start4.elf`, `fixup4.dat`, `kernel8.img`, DTB'ler, `overlays/`, `config.txt`, `cmdline.txt` (root=p5) |
| 3 | `BOOT_B` | FAT32 | 256 MiB | aynı, `cmdline.txt` root=p6 |
| 4 | (genişletilmiş) | | | |
| 5 | `ROOT_A` | EROFS | 2,5 GiB | kök dosya sistemi, slot A |
| 6 | `ROOT_B` | EROFS | 2,5 GiB | slot B (boş; ilk OTA yazar) |
| 7 | `DATA` | ext4 | 256 MiB → kartın sonu | `/data`: `/etc` üst katmanı, `/var`, `/home`, `/opt/data`, `/usr/local`, RAUC durumu |

PARTUUID'ler `4d4f5332-0N` biçimindedir; `cmdline.txt`, `fstab` ve RAUC
`system.conf` bunları kullanır. İmaj ham hâliyle ~5,9 GB'tır (boş ROOT_B
nedeniyle); `.xz` hâli çok daha küçüktür.

## Açılış akışı

1. **EEPROM** → `MOSCFG/autoboot.txt` okunur: normalde `[all] boot_partition=2`
   (BOOT_A); `reboot "0 tryboot"` ile tek seferlik `[tryboot] boot_partition=3`.
2. **start4.elf** → `config.txt` (`vc4-kms-v3d,cma-256`, kamera/ekran otomatik
   algılama, i2c/spi açık, `arm_boost=1`, hız aşırtma yok) → `kernel8.img`.
3. **Çekirdek** (initramfs yok) → `root=PARTUUID=4d4f5332-05 rootfstype=erofs ro`
   → `init=/usr/lib/makeros/preinit`.
4. **preinit** (PID 1): DATA'yı `e2fsck -p` ile denetler ve `/data`'ya bağlar,
   `/etc` üzerine overlayfs kurar (üst katman `/data/overlay/etc`), sonra
   systemd'yi `exec` eder. DATA yoksa/bozuksa `/data` tmpfs olur ("geçici mod",
   `/run/makeros/volatile`).
5. **systemd**: fstab ile `/var` (overlay), `/home`, `/opt/data`, `/usr/local`
   (bind) bağlanır; `makeros-growdata` DATA'yı kart sonuna kadar büyütür;
   `makeros-zram` (zstd, RAM'in %50'si); `makeros-firstboot` (ana makine adı
   `makeros-XXXX`, SSH anahtarları); NetworkManager; `makeros-network-fallback`
   (45 sn içinde ağ yoksa açık "MakerOS" erişim noktası + captive portal,
   10.42.0.1); `makerd` (:80/:443) + `makerd-priv.socket`.
6. **makeros-kiosk.service**: `maker` hizmet hesabı, logind oturumu (PAM,
   seat0/tty1), `shell.slice` içinde `labwc -C /usr/share/makeros/labwc`; autostart
   `cog http://127.0.0.1/` (yerel bilet varsa `/api/v1/auth/local?token=…`) ve
   kullanıcı hizmeti `makeros-vnc` (wayvnc, 127.0.0.1:5900) başlatır. Monitör
   yoksa labwc 1280x720 sanal ekran açar. Kurulum bitmeden de açılır;
   makerd `/` adresinde kurulum sihirbazını sunar.
7. **boot-complete.target** (`makeros-healthcheck`) → `makeros-mark-good` →
   `rauc status mark-good`; tryboot ile açılmışsak arka uç `autoboot.txt`'i
   atomik olarak yeni slota çevirir. Takılan açılışı bcm2835 bekçi köpeği
   (systemd `RuntimeWatchdogSec=15s`) sıfırlar; firmware `[all]` slotuna döner.

## Ağaç düzeni

```
os/
  build.sh  docker/Dockerfile  VERSION (2.0.0)
  external.desc  external.mk  Config.in
  configs/makeros_rpi4_defconfig
  patches/linux/linux.hash               sabit çekirdek tarball'unun sha256'sı
  board/rpi4/
    linux.fragment  config.txt  cmdline.txt  autoboot.txt  makeros-user.txt
    genimage.cfg.in  post-build.sh  post-image.sh  users.txt  wifi-example.txt
    rootfs-overlay/
      usr/lib/makeros/      preinit, growdata, zram-setup, cpu-tune,
                            network-fallback, kiosk-browser, browser-session
      usr/lib/systemd/      makeros-*.service, shell/apps/ai/build.slice
                            (sistem + kullanıcı), oomd, journald (volatile),
                            watchdog, resolved (mDNS), 70-makeros.preset
      usr/lib/sysctl.d/     60-makeros.conf (swappiness 180, sertleştirme…)
      usr/lib/udev/rules.d/ gpio/i2c/spi/video grupları, BFQ, Pi OS seri takma adları
      usr/lib/rauc/rpi-tryboot-backend
      usr/bin/mos, usr/bin/docker (→ podman)
      etc/NetworkManager/   powersave kapalı, captive dnsmasq
      etc/rauc/system.conf  etc/pam.d/makeros-kiosk  etc/subuid|subgid …
  package/
    makerd/            cargo-package (Rust), ../makerd → /usr/bin/makerd{,-priv,-user}; birimler makerd/dist'ten + polkit kuralı
    makeros-shell/     ../shell → /usr/share/makeros/shell (+ .gz ön-sıkıştırma)
    makeros-firstboot/ ilk açılış betiği + birimi
    chromium-bin/      Debian trixie arm64 Chromium paketi + başlatıcı
    labwc/ wayvnc/ neatvnc/ aml/  Wayland pencere yöneticisi ve VNC bileşenleri
    uv-bin/            uv 0.13.6 aarch64-gnu (sha256 sabit)
    makeros-node/      Node.js v24.22.2 linux-arm64 → /opt/node (sha256 sabit)
    font-inter/        Inter 4.1 (değişken TTF)
```

## Kullanıcılar ve kurulum

- `maker` (UID 999, `/sbin/nologin`, ev `/var/lib/makerd`) **hizmet hesabıdır**:
  makerd ve kiosk bununla çalışır; wheel'de değildir.
- İnsan kullanıcı (UID ≥ 1000, `wheel` + donanım grupları) ilk açılış
  sihirbazında `makerd-priv user.create` ile oluşturulur; terminali
  `makerd-user@<ad>.service` sağlar. Kurulum bitince `makerd-priv`
  `/var/lib/makerd/setup-done` yazar (firstboot bu dosyaya dokunmaz).
- `/etc` (passwd/group/shadow/hostname dahil) `/data` üzerindeki overlay ile,
  `/home` ise `/data/home` bind ile çalışma anında yazılabilir.
- Parolalar **SHA-512** (`/etc/login.defs ENCRYPT_METHOD SHA512`; makerd-priv
  yescrypt doğrulamaz). `sudo`: `/etc/sudoers.d/wheel`.
- SSH varsayılan kapalı (TODO M3: makerd-priv `ssh.enable`).

## Yazılım notları

- **Kapsayıcı / "apt kaçış kapısı"**: podman 5.7 (crun, netavark, passt,
  catatonit). `docker` komutu podman'a yönlenir. `mos box` Debian trixie arm64
  kutusu açar: `$HOME` paylaşılır, `/dev/gpiochip*`, i2c, spi, video, seri
  aygıtlar geçirilir, kutu içinde parolasız `sudo apt install …`.
- **Python**: sistem python3 (3.14, salt-okunur, PEP 668 işaretli) + `uv`.
  Projeler `uv venv` ile `/data/home/maker` altında.
- **Bellek**: zram zstd %50, `vm.swappiness=180`, MGLRU `min_ttl_ms=1000`,
  systemd-oomd (PSI), dilimler: `shell.slice` MemoryLow=256M,
  `apps.slice` MemoryHigh=2.2G, `ai.slice` MemoryHigh=1.6G.
- **CPU**: çekirdek varsayılanı `schedutil`; `/etc/makeros/cpu-governor`
  içine `ondemand` yazılırsa Pi OS'teki ayarlar uygulanır.

## Buildroot'ta olmayan / farklı olanlar

| İstenen | Durum |
|---|---|
| Bootlin hazır glibc toolchain | Yalnızca x86_64 host destekli (`depends on BR2_HOSTARCH = "x86_64"`). Arm toolchain'in başlıkları 4.20 (systemd ≥5.4 ister). → **Buildroot iç glibc toolchain'i** |
| `rpicam-apps` | Buildroot adı `libcamera-apps` (v1.7.0) — seçildi |
| `wayvnc` / `neatvnc` / `aml` / `labwc` | `package/{labwc,wayvnc,neatvnc,aml}` altında özel paketler |
| `chromium-bin` | `os/chromium/build-bundle.sh` + `package/chromium-bin` |
| `zram-generator` | `makeros-zram.service` + `zram-setup` |
| Inter yazı tipi | `package/font-inter` |
| `nodejs-bin` adı | Buildroot'taki `host-nodejs-bin` ile çakışıyor → paket adı **`makeros-node`** |
| systemd-repart ile DATA büyütme | repart yalnızca GPT destekler; MBR için `makeros-growdata` (sfdisk + resize2fs, çevrimiçi) |
