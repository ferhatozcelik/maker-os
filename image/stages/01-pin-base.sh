#!/bin/sh
# Taban surumunu sabitler.
# Karar: Pardus 21 "yirmibir" arm64 (docs/kararlar.md, 20.10.2026
# dogrulamasi: Release Architectures: amd64 arm64 i386; main/binary-arm64/
# Packages.gz HTTP 200 depo.pardus.org.tr uzerinden).
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
PIN_EXAMPLE="$IMAGE_DIR/pins/base.env.example"

cp "$PIN_EXAMPLE" "$PIN_FILE"
. "$PIN_FILE"

echo "== depo erisim kontrolu =="
TMP_RELEASE="$(mktemp)"
trap 'rm -f "$TMP_RELEASE"' EXIT
# depo.pardus.org.tr her istekte rastgele bir aynaya yonlendirir; secilen ayna
# o an yanit vermeyebilir. Yeniden denemek yeni bir ayna secer.
RETRY="--retry 6 --retry-delay 10 --retry-all-errors"
# shellcheck disable=SC2086
if ! curl -sfL $RETRY --max-time 30 "$PARDUS_MIRROR/dists/$PARDUS_SUITE/Release" -o "$TMP_RELEASE"; then
  echo "HATA: $PARDUS_MIRROR/dists/$PARDUS_SUITE/Release alinamadi" >&2
  exit 1
fi
if ! grep -q "^Architectures:.*\b$PARDUS_ARCH\b" "$TMP_RELEASE"; then
  echo "HATA: Release dosyasinda $PARDUS_ARCH mimarisi listelenmiyor" >&2
  exit 1
fi
# shellcheck disable=SC2086
if ! curl -sfIL $RETRY --max-time 30 "$PARDUS_MIRROR/dists/$PARDUS_SUITE/main/binary-$PARDUS_ARCH/Packages.gz" >/dev/null; then
  echo "HATA: main/binary-$PARDUS_ARCH/Packages.gz erisilemiyor" >&2
  exit 1
fi

if [ -z "$RPI_FIRMWARE_REF" ]; then
  echo "== raspberrypi/firmware master commit'i cozuluyor =="
  RPI_FIRMWARE_REF=$(git ls-remote https://github.com/raspberrypi/firmware refs/heads/master | cut -f1)
  if [ -z "$RPI_FIRMWARE_REF" ]; then
    echo "HATA: raspberrypi/firmware commit'i cozulemedi" >&2
    exit 1
  fi
  sed -i "s/^RPI_FIRMWARE_REF=.*/RPI_FIRMWARE_REF=$RPI_FIRMWARE_REF/" "$PIN_FILE"
fi

echo "Taban pinlendi: $PARDUS_SUITE/$PARDUS_ARCH, firmware $RPI_FIRMWARE_REF -> $PIN_FILE"
