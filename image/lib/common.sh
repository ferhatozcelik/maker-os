# Stage betiklerinin ortak degiskenleri ve yardimcilari. Her stage
# betigi bu dosyayi ". lib/common.sh" ile yukler.

IMAGE_DIR="${IMAGE_DIR:-$(dirname "$(dirname "$(readlink -f "$0")")")}"
PIN_FILE="$IMAGE_DIR/pins/base.env"
OVERLAY_DIR="$IMAGE_DIR/overlay"

# Rootfs host (macOS) bind-mount'u uzerinde degil, Linux dosya sisteminde
# uretilmelidir: aygit dugumleri, sahiplik ve buyuk/kucuk harf duyarliligi.
PIOS_WORK="${PIOS_WORK:-$IMAGE_DIR/out/work}"
PIOS_OUT="${PIOS_OUT:-$IMAGE_DIR/out}"
PIOS_CACHE="${PIOS_CACHE:-$PIOS_WORK/cache}"
ROOTFS_DIR="${ROOTFS_DIR:-$PIOS_WORK/rootfs}"

PIOS_HOSTNAME="${PIOS_HOSTNAME:-pardus-maker}"
PIOS_USER="${PIOS_USER:-pardus}"
PIOS_PASSWORD="${PIOS_PASSWORD:-pardus}"
# Monitor yokken masaustunun calisacagi sanal ekran cozunurlugu.
PIOS_RESOLUTION="${PIOS_RESOLUTION:-1920x1080}"

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "HATA: bu adim root yetkisi gerektirir" >&2
    exit 1
  fi
}

require_rootfs() {
  if [ ! -f "$ROOTFS_DIR/etc/debian_version" ]; then
    echo "HATA: $ROOTFS_DIR bir rootfs gibi gorunmuyor - once 02-build-rootfs.sh calistirilmali" >&2
    exit 1
  fi
}

load_pins() {
  if [ ! -f "$PIN_FILE" ]; then
    echo "HATA: $PIN_FILE yok - once 01-pin-base.sh calistirilmali" >&2
    exit 1
  fi
  . "$PIN_FILE"
}

# chroot icin sanal dosya sistemlerini baglar; cikista trap ile cozulur.
chroot_mount() {
  mount -t proc proc "$ROOTFS_DIR/proc"
  mount -t sysfs sysfs "$ROOTFS_DIR/sys"
  mount -o bind /dev "$ROOTFS_DIR/dev"
  mount -t devpts devpts "$ROOTFS_DIR/dev/pts" 2>/dev/null || true
  rm -f "$ROOTFS_DIR/etc/resolv.conf"
  cp /etc/resolv.conf "$ROOTFS_DIR/etc/resolv.conf"
  # Paket kurulumu sirasinda servisler chroot icinde baslatilmasin.
  printf '#!/bin/sh\nexit 101\n' > "$ROOTFS_DIR/usr/sbin/policy-rc.d"
  chmod +x "$ROOTFS_DIR/usr/sbin/policy-rc.d"
}

chroot_umount() {
  rm -f "$ROOTFS_DIR/usr/sbin/policy-rc.d"
  umount "$ROOTFS_DIR/dev/pts" 2>/dev/null || true
  umount "$ROOTFS_DIR/dev" 2>/dev/null || true
  umount "$ROOTFS_DIR/sys" 2>/dev/null || true
  umount "$ROOTFS_DIR/proc" 2>/dev/null || true
}

in_chroot() {
  chroot "$ROOTFS_DIR" /usr/bin/env -i \
    PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin HOME=/root LANG=C.UTF-8 \
    DEBIAN_FRONTEND=noninteractive "$@"
}
