#!/bin/sh
# Raspberry Pi donanim destegini ekler: kernel, modul, bootloader, device
# tree, Wi-Fi/Bluetooth firmware'i. Pardus arm64
# deposunda RPi'ye ozel kernel yok; raspberrypi/firmware deposundaki
# onceden derlenmis kernel8.img + modulleri kullanilir.
#
# ONEMLI: raspberrypi/firmware/modules altinda birden cok kernel varyanti
# vardir (6.x+ = Pi1/Zero, -v7+, -v7l+, -v8+, -v8-16k+ = Pi5). kernel8.img
# "-v8+" varyantidir; baska bir dizin kopyalanirsa (onceki surumde oldugu
# gibi "6.18.57+") brcmfmac dahil hicbir modul yuklenemez ve Wi-Fi olmaz.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root
require_rootfs

mkdir -p "$PIOS_CACHE"
FW_TARBALL="$PIOS_CACHE/rpi-firmware-$RPI_FIRMWARE_REF.tar.gz"
if [ ! -s "$FW_TARBALL" ]; then
  echo "== raspberrypi/firmware ($RPI_FIRMWARE_REF) indiriliyor (buyuk dosya) =="
  curl -fL --retry 3 --max-time 1800 \
    "https://github.com/raspberrypi/firmware/archive/${RPI_FIRMWARE_REF}.tar.gz" \
    -o "$FW_TARBALL.part"
  mv "$FW_TARBALL.part" "$FW_TARBALL"
fi

WORKTMP="$(mktemp -d)"
trap 'rm -rf "$WORKTMP"' EXIT

echo "== yalnizca gerekli kisimlar aciliyor (boot/, modules/*-v8+) =="
tar -xzf "$FW_TARBALL" -C "$WORKTMP" --wildcards \
  '*/boot/bootcode.bin' '*/boot/start*.elf' '*/boot/fixup*.dat' \
  '*/boot/kernel8.img' '*/boot/bcm27*.dtb' '*/boot/overlays/*' \
  '*/modules/*-v8+/*'
FW_SRC="$(find "$WORKTMP" -mindepth 1 -maxdepth 1 -type d | head -n1)"

KVER_DIR="$(find "$FW_SRC/modules" -mindepth 1 -maxdepth 1 -type d -name '*-v8+' | head -n1)"
if [ -z "$KVER_DIR" ]; then
  echo "HATA: $FW_SRC/modules altinda -v8+ kernel modul dizini yok" >&2
  exit 1
fi
KVER="$(basename "$KVER_DIR")"
if ! grep -aq "Linux version $KVER " "$FW_SRC/boot/kernel8.img" 2>/dev/null; then
  # kernel8.img sikistirilmis olabilir; o durumda surum dizesi okunamaz.
  echo "Bilgi: kernel8.img icinden surum dogrulanamadi (sikistirilmis), modul dizini: $KVER"
fi

BOOT_DIR="$ROOTFS_DIR/boot/firmware"
rm -rf "$BOOT_DIR" "$ROOTFS_DIR/lib/modules"
mkdir -p "$BOOT_DIR/overlays" "$ROOTFS_DIR/lib/modules"

echo "== boot dosyalari kopyalaniyor =="
cp "$FW_SRC"/boot/bootcode.bin "$FW_SRC"/boot/start*.elf "$FW_SRC"/boot/fixup*.dat \
  "$FW_SRC"/boot/kernel8.img "$FW_SRC"/boot/bcm27*.dtb "$BOOT_DIR/"
cp -r "$FW_SRC"/boot/overlays/. "$BOOT_DIR/overlays/"

echo "== kernel modulleri kopyalaniyor ($KVER) =="
cp -a "$KVER_DIR" "$ROOTFS_DIR/lib/modules/"
depmod -b "$ROOTFS_DIR" "$KVER"
if ! grep -q 'brcmfmac.ko' "$ROOTFS_DIR/lib/modules/$KVER/modules.dep"; then
  echo "HATA: modules.dep icinde brcmfmac yok - Wi-Fi calismaz" >&2
  exit 1
fi
echo "$KVER" > "$PIOS_WORK/kernel-version"

echo "== Wi-Fi/Bluetooth firmware'i (Raspberry Pi OS paketleri) =="
# Debian bullseye'daki firmware-brcm80211 (2021) yerine RPi'nin guncel
# paketleri dpkg ile kurulur; boylece paket veritabaninda da izlenir ve
# Pardus deposundaki eski surum (epoch'suz) bunu asagi indiremez.
RPI_PKGS_GZ="$WORKTMP/rpi-Packages.gz"
curl -sfL --max-time 120 "$RPI_APT_MIRROR/dists/$RPI_APT_SUITE/main/binary-arm64/Packages.gz" -o "$RPI_PKGS_GZ"
mkdir -p "$ROOTFS_DIR/var/cache/pios-debs"
for pkg in firmware-brcm80211 bluez-firmware; do
  path=$(zcat "$RPI_PKGS_GZ" | awk -v p="$pkg" '$1=="Package:"{n=$2} $1=="Filename:" && n==p {print $2; exit}')
  if [ -z "$path" ]; then
    echo "HATA: $pkg RPi deposunda bulunamadi" >&2
    exit 1
  fi
  echo "   $pkg <- $path"
  curl -sfL --max-time 300 "$RPI_APT_MIRROR/$path" -o "$ROOTFS_DIR/var/cache/pios-debs/$(basename "$path")"
done

echo "== config.txt / cmdline.txt yaziliyor =="
cat > "$BOOT_DIR/config.txt" <<'CFG'
# Pardus Maker - Raspberry Pi 4 / 400 / 3B+ (arm64)
arm_64bit=1
kernel=kernel8.img

# Tam KMS grafik surucusu (Xorg modesetting + Mesa v3d hizlandirma).
dtoverlay=vc4-kms-v3d
max_framebuffers=2
disable_overscan=1
disable_splash=1
# Daha hizli acilis: cekirdek yuklenmeden once 1 sn bekleme yok; ilk 30 sn
# islemci tam hizda (ondemand henuz devrede degilken servisler hizli baslar).
boot_delay=0
initial_turbo=30
# Monitor takili olmasa da HDMI etkin kabul edilir (ekransiz masaustu).
hdmi_force_hotplug=1

dtparam=audio=on
camera_auto_detect=1

# GPIO baslik arayuzleri (DIY projeleri icin varsayilan acik; Ayarlar >
# Donanim'dan degistirilebilir).
dtparam=i2c_arm=on
dtparam=spi=on
display_auto_detect=1

# Seri konsol (sorun giderme icin GPIO14/15, 115200).
enable_uart=1

[pi4]
# Pi 4 hiz asirtma: CPU 1.5/1.8 -> 2.0 GHz, GPU (v3d, masaustu/Chromium)
# 500 -> 750 MHz. force_turbo yok: bostayken frekans duser, garanti bozulmaz.
# Sogutucu veya fan gerekir; sogutmasiz Pi 80 C'de kendini yavaslatir.
# Kararsizlik (donma, SD hatasi) olursa bu dort satiri silip yeniden baslatin.
arm_boost=1
arm_freq=2000
gpu_freq=750
over_voltage=6

[all]
CFG

# PARTUUID'ler 08-package-image.sh tarafindan gercek degerle degistirilir.
# video=...D : monitor bagli olmasa da HDMI-A-1 cikisini bu cozunurlukte
# zorla etkinlestirir; boylece masaustu calisir ve VNC ile erisilir.
# zswap.enabled=0: takas zram'da; zswap acik kalirsa sayfalar iki kez
# sikistirilir. logo.nologo: acilista cekirdek logosu cizilmez.
cat > "$BOOT_DIR/cmdline.txt" <<CFG
console=serial0,115200 console=tty1 root=PARTUUID=00000000-02 rootfstype=ext4 fsck.repair=yes rootwait quiet loglevel=3 logo.nologo zswap.enabled=0 video=HDMI-A-1:${PIOS_RESOLUTION}M@60D cfg80211.ieee80211_regdom=TR
CFG

echo "== fstab yaziliyor =="
cat > "$ROOTFS_DIR/etc/fstab" <<'CFG'
proc                  /proc           proc    defaults                              0  0
PARTUUID=00000000-01  /boot/firmware  vfat    defaults,noatime                      0  2
PARTUUID=00000000-02  /               ext4    defaults,noatime,commit=60            0  1
tmpfs                 /tmp            tmpfs   defaults,noatime,nosuid,nodev,size=25%  0  0
CFG

echo "Donanim destegi eklendi: kernel $KVER"
