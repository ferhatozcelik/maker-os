#!/bin/sh
# Paket deposunu hazirlar. Build'in internete ciktigi tek yer burasidir ve
# yalnizca kendi GitHub Release'imize gider: vendor/depo.lock'taki parcalar
# indirilir, sha256 ile dogrulanir, PIOS_DEPO'ya acilir. Ayni depo zaten
# aciksa hicbir sey yapilmaz. Senkron modunda (vendor/sync.sh) atlanir.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"

if is_sync; then
  echo "Senkron modu: paketler internetten toplanacak, depo indirilmiyor."
  exit 0
fi

LOCK="$VENDOR_DIR/depo.lock"
[ -f "$LOCK" ] || { echo "HATA: vendor/depo.lock yok - once vendor/sync.sh ile depo olusturun" >&2; exit 1; }
DEPO_ID="$(sed -n 's/^id=//p' "$LOCK")"
DEPO_URL="$(sed -n 's/^url=//p' "$LOCK")"

if [ "$(cat "$PIOS_DEPO/.id" 2>/dev/null)" = "$DEPO_ID" ] && [ -d "$PIOS_DEPO/apt" ]; then
  echo "Depo hazir: $DEPO_ID"
  exit 0
fi

DL="$PIOS_WORK/depo-indir"
mkdir -p "$DL"
echo "== depo $DEPO_ID indiriliyor =="
grep '^part=' "$LOCK" | cut -d= -f2- | while read -r sum name; do
  f="$DL/$name"
  if ! { [ -f "$f" ] && echo "$sum  $f" | sha256sum -c - >/dev/null 2>&1; }; then
    curl -fL --retry 5 --retry-all-errors --max-time 3600 "$DEPO_URL/$name" -o "$f.part"
    mv "$f.part" "$f"
    echo "$sum  $f" | sha256sum -c - >/dev/null || { echo "HATA: $name sha256 uyusmuyor" >&2; rm -f "$f"; exit 1; }
  fi
  echo "   $name dogrulandi"
done

echo "== depo aciliyor =="
rm -rf "$PIOS_DEPO.new"
mkdir -p "$PIOS_DEPO.new"
# shellcheck disable=SC2046
cat $(grep '^part=' "$LOCK" | awk '{print "'"$DL"'/" $2}') | xz -T0 -dc | tar -C "$PIOS_DEPO.new" -xf -
echo "$DEPO_ID" > "$PIOS_DEPO.new/.id"
rm -rf "$PIOS_DEPO"
mv "$PIOS_DEPO.new" "$PIOS_DEPO"
rm -rf "$DL"
echo "Depo hazir: $DEPO_ID ($(du -sh "$PIOS_DEPO" | cut -f1))"
