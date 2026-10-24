#!/bin/sh
# Taban surumunu sabitler.
# Karar: Pardus 21 "yirmibir" arm64 (docs/decisions.md, 20.10.2026
# dogrulamasi: Release Architectures: amd64 arm64 i386; main/binary-arm64/
# Packages.gz HTTP 200 depo.pardus.org.tr uzerinden).
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
PIN_EXAMPLE="$IMAGE_DIR/pins/base.env.example"

cp "$PIN_EXAMPLE" "$PIN_FILE"
. "$PIN_FILE"

# Build modunda internet kullanilmaz (paketler vendor/ deposundan gelir);
# depo erisimi yalnizca senkron modunda (vendor/sync.sh) denetlenir.
if is_sync; then
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
fi

echo "Taban pinlendi: $PARDUS_SUITE/$PARDUS_ARCH -> $PIN_FILE"
