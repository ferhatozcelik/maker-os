#!/bin/sh
# OTA paketi: kurulu cihazlarin SD karti yeniden yazmadan yukseltilmesi icin
# MakerOS katmani + cekirdek (bkz. overlay/usr/lib/pios/center/ota.py).
#   root/          overlay'deki tum yollar (06'nin isledigi halleriyle) ve
#                  build'de uretilenler (/usr/share/pios, surum damgasi)
#   kernel/boot/   cekirdek, DTB, overlay'ler, acilis yazilimi (config.txt ve
#                  cmdline.txt haric: cihaz ayarlari ezilmez)
#   kernel/modules/<surum>/
#   debs/          ilk surumun kilidinden (vendor/packages.base.lock) bu yana
#                  degisen veya eklenen paketler; cihazin dis deposu yoktur,
#                  hangi surumden yukseltilirse yukseltilsin eksiksiz guncellenir
#   hooks/post-install.sh, enable.txt (pios birimleri), packages.txt
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
require_root
require_rootfs

VERSION="$(cat "$PIOS_WORK/build-version")"
KVER="$(cat "$PIOS_WORK/kernel-version")"
STAGE="$PIOS_WORK/ota"
OUT="$PIOS_OUT/maker-os-$VERSION-ota.tar.xz"
rm -rf "$STAGE"
mkdir -p "$STAGE/root" "$STAGE/kernel/boot" "$STAGE/kernel/modules" "$STAGE/hooks" "$STAGE/debs"

echo "== MakerOS katmani =="
LIST="$STAGE/files.txt"
( cd "$OVERLAY_DIR" && find . \( -type f -o -type l \) ! -name .DS_Store ! -name '*.pyc' ! -path '*/__pycache__/*' \
    | sed 's|^\./||' | grep -v '^boot/firmware/' )  > "$LIST"
( cd "$ROOTFS_DIR" && find usr/share/pios \( -type f -o -type l \) ) >> "$LIST"
echo etc/pios-build-version >> "$LIST"
tar -C "$ROOTFS_DIR" --hard-dereference -cf - -T "$LIST" | tar -C "$STAGE/root" -xpf -
echo "   $(wc -l < "$LIST") dosya"

echo "== cekirdek $KVER =="
( cd "$ROOTFS_DIR/boot/firmware" && cp -a kernel8.img bcm27*.dtb start*.elf fixup*.dat bootcode.bin overlays "$STAGE/kernel/boot/" )
cp -a "$ROOTFS_DIR/lib/modules/$KVER" "$STAGE/kernel/modules/"

echo "== degisen paketler =="
if [ -f "$VENDOR_DIR/packages.base.lock" ] && [ -d "$PIOS_DEPO/apt/pool" ]; then
  LC_ALL=C sort "$VENDOR_DIR/packages.base.lock" > "$STAGE/base.sorted"
  LC_ALL=C sort "$VENDOR_DIR/packages.lock" | LC_ALL=C comm -13 "$STAGE/base.sorted" - \
    | while IFS="$(printf '\t')" read -r pkg ver arch; do
        f="$PIOS_DEPO/apt/pool/${pkg}_$(echo "$ver" | sed 's/^[0-9]*://')_$arch.deb"
        [ -f "$f" ] || { echo "HATA: depoda yok: $f" >&2; exit 1; }
        cp "$f" "$STAGE/debs/"
      done
  rm -f "$STAGE/base.sorted"
fi
echo "   $(ls "$STAGE/debs" | wc -l) paket"

echo "== kanca ve listeler =="
cp "$IMAGE_DIR/ota/post-install.sh" "$STAGE/hooks/post-install.sh"
ls "$ROOTFS_DIR"/etc/systemd/system/*.wants/ | grep -E '^pios-.*\.(service|timer)$' | sort -u > "$STAGE/enable.txt"
# Yerel .deb/yer tutucu paketler depodan kurulamaz; listeye girmez.
cut -f1 "$PIOS_WORK/packages.tsv" | grep -vxE 'raspberrypi-kernel|pardus-yuzyil' | LC_ALL=C sort -u > "$STAGE/packages.txt"
printf '{"format": 1, "version": "%s", "kernel": "%s", "created": "%s"}\n' \
  "$VERSION" "$KVER" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$STAGE/ota.json"
rm -f "$LIST"

echo "== paketleniyor =="
rm -f "$OUT" "$OUT.sha256"
tar -C "$STAGE" --numeric-owner --hard-dereference -cf - ota.json root kernel hooks debs enable.txt packages.txt | xz -T0 -6 > "$OUT"
( cd "$PIOS_OUT" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256" )
rm -rf "$STAGE"
ls -lh "$OUT"
