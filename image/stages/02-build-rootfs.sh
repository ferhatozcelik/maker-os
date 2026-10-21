#!/bin/sh
# ARM64 root filesystem uretir.
# minbase varyanti: yalnizca Essential paketler + apt; gerisi 04'te
# --no-install-recommends ile tek tek secilir. arm64 host (Apple Silicon
# uzerinde colima) ikinci asamayi dogal calistirir; x86_64 host'ta
# qemu-aarch64-static + binfmt gerekir.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root

if ! command -v debootstrap >/dev/null 2>&1; then
  echo "HATA: debootstrap bulunamadi - image/Dockerfile.builder icinde calistirin" >&2
  exit 1
fi

HOST_ARCH="$(uname -m)"
NEED_QEMU=0
if [ "$HOST_ARCH" != "aarch64" ] && [ "$HOST_ARCH" != "arm64" ]; then
  NEED_QEMU=1
  if ! command -v qemu-aarch64-static >/dev/null 2>&1; then
    echo "HATA: $HOST_ARCH host'ta qemu-aarch64-static gerekli" >&2
    exit 1
  fi
fi

DEBOOTSTRAP_SCRIPTS_DIR="/usr/share/debootstrap/scripts"
SUITE_SCRIPT="$DEBOOTSTRAP_SCRIPTS_DIR/$PARDUS_SUITE"
if [ ! -e "$SUITE_SCRIPT" ]; then
  echo "Bilgi: $PARDUS_SUITE icin debootstrap scripti yok, $DEBOOTSTRAP_BASE_CODENAME kullaniliyor"
  ln -s "$DEBOOTSTRAP_BASE_CODENAME" "$SUITE_SCRIPT"
fi

PARDUS_KEYRING="$IMAGE_DIR/pins/pardus-archive-keyring.gpg"
if [ ! -f "$PARDUS_KEYRING" ]; then
  echo "== Pardus depo imza anahtari indiriliyor =="
  curl -sfL --max-time 20 "${PARDUS_MIRROR%/pardus}/Release.pub" -o /tmp/pardus-release.pub
  gpg --no-default-keyring --keyring "$PARDUS_KEYRING" --import /tmp/pardus-release.pub
fi

if [ -d "$ROOTFS_DIR" ]; then
  echo "== onceki rootfs siliniyor: $ROOTFS_DIR =="
  for m in dev/pts dev sys proc; do umount "$ROOTFS_DIR/$m" 2>/dev/null || true; done
  rm -rf "$ROOTFS_DIR"
fi
mkdir -p "$ROOTFS_DIR"

echo "== 1. asama (foreign, paket indirme) =="
debootstrap --arch="$PARDUS_ARCH" --variant=minbase --foreign \
  --keyring="$PARDUS_KEYRING" \
  --components="$(echo "$PARDUS_COMPONENTS" | tr ' ' ',')" \
  "$PARDUS_SUITE" "$ROOTFS_DIR" "$PARDUS_BUILD_MIRROR"

if [ "$NEED_QEMU" -eq 1 ]; then
  cp "$(command -v qemu-aarch64-static)" "$ROOTFS_DIR/usr/bin/"
fi

echo "== 2. asama (chroot icinde second-stage) =="
chroot "$ROOTFS_DIR" /debootstrap/debootstrap --second-stage

echo "== apt guveni: Pardus anahtari ve CA paketi =="
# pins/ altindaki anahtar GPG keybox bicimindedir; apt'nin trusted.gpg.d
# dizini duz OpenPGP paket bicimi bekler, bu yuzden disa aktarilir.
gpg --no-default-keyring --keyring "$PARDUS_KEYRING" --export \
  > "$ROOTFS_DIR/etc/apt/trusted.gpg.d/pardus-archive-keyring.gpg"
# Depo paket dosyalarini https://ulakbimdepo2.pardus.org.tr'ye yonlendirir;
# minbase'de CA paketi yok. ca-certificates kurulana kadar builder'in
# CA demeti kullanilir (paket kurulunca kendi demetiyle degistirilir).
mkdir -p "$ROOTFS_DIR/etc/ssl/certs"
cp /etc/ssl/certs/ca-certificates.crt "$ROOTFS_DIR/etc/ssl/certs/ca-certificates.crt"

echo "== dpkg: belge/man sayfasi/yabanci dil dosyalari kurulmasin =="
# SD kartta yer ve yazma tasarrufu; telif dosyalari ve tr/en cevirileri kalir.
cat > "$ROOTFS_DIR/etc/dpkg/dpkg.cfg.d/01-pios-nodoc" <<'CFG'
path-exclude=/usr/share/doc/*
path-include=/usr/share/doc/*/copyright
path-exclude=/usr/share/man/*
path-exclude=/usr/share/info/*
path-exclude=/usr/share/lintian/*
path-exclude=/usr/share/locale/*
path-include=/usr/share/locale/tr/*
path-include=/usr/share/locale/en*
path-include=/usr/share/locale/locale.alias
CFG

echo "== apt: oneri paketleri kurulmasin =="
cat > "$ROOTFS_DIR/etc/apt/apt.conf.d/90-pios" <<'CFG'
APT::Install-Recommends "false";
APT::Install-Suggests "false";
Acquire::Languages "none";
Acquire::Retries "5";
CFG

echo "Rootfs uretildi: $ROOTFS_DIR"
