#!/bin/sh
# Imaji sikistirir (.img.xz), sha256 ozetlerini ve build manifestini uretir.
# Raspberry Pi Imager ve balenaEtcher .img.xz dosyasini dogrudan yazar.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins

OUT_IMG="$(cat "$PIOS_WORK/image-path")"
VERSION="$(cat "$PIOS_WORK/build-version")"
BASE="$(basename "$OUT_IMG" .img)"

echo "== manifest =="
{
  echo "maker-os $VERSION"
  echo "taban      : Pardus $PARDUS_SUITE ($PARDUS_ARCH)"
  echo "kernel     : $(cat "$PIOS_WORK/kernel-version")"
  echo "rpi-fw     : raspberrypi/firmware@$(depo_version RPI_FIRMWARE_REF) ($RPI_FIRMWARE_BRANCH)"
  echo "paket depo : $(sed -n 's/^id=//p' "$VENDOR_DIR/depo.lock" 2>/dev/null)"
  echo "kullanici  : $PIOS_USER"
  echo "cozunurluk : $PIOS_RESOLUTION"
  echo "paket sayisi: $(wc -l < "$PIOS_WORK/packages.tsv")"
  echo
  echo "== gelistirme araclari =="
  cat "$PIOS_WORK/dev-tools.txt"
  echo
  echo "== AI ortami (/opt/pios/envs/ai) =="
  cat "$PIOS_WORK/ai-packages.txt"
  echo
  echo "== Debian paketleri =="
  cat "$PIOS_WORK/packages.tsv"
} > "$PIOS_OUT/$BASE.manifest.txt"

if [ "${PIOS_COMPRESS:-1}" = "1" ]; then
  echo "== xz ile sikistiriliyor =="
  rm -f "$OUT_IMG.xz"
  xz -T0 -6 -k "$OUT_IMG"
  ( cd "$PIOS_OUT" && sha256sum "$BASE.img.xz" > "$BASE.img.xz.sha256" )
fi
( cd "$PIOS_OUT" && sha256sum "$BASE.img" > "$BASE.img.sha256" )

ls -lh "$PIOS_OUT/$BASE".*
