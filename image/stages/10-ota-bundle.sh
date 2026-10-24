#!/bin/sh
# OTA paketi: kurulu cihazlarin SD karti yeniden yazmadan yukseltilmesi icin
# Pardus Maker katmani + cekirdek (bkz. overlay/usr/lib/pios/center/ota.py).
#   root/          overlay'deki tum yollar (06'nin isledigi halleriyle) ve
#                  build'de uretilenler (/usr/share/pios, surum damgasi)
#   kernel/boot/   cekirdek, DTB, overlay'ler, acilis yazilimi (config.txt ve
#                  cmdline.txt haric: cihaz ayarlari ezilmez)
#   kernel/modules/<surum>/
#   hooks/post-install.sh, enable.txt (pios birimleri), packages.txt
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
require_root
require_rootfs

VERSION="$(cat "$PIOS_WORK/build-version")"
KVER="$(cat "$PIOS_WORK/kernel-version")"
STAGE="$PIOS_WORK/ota"
OUT="$PIOS_OUT/pardus-maker-$VERSION-ota.tar.xz"
rm -rf "$STAGE"
mkdir -p "$STAGE/root" "$STAGE/kernel/boot" "$STAGE/kernel/modules" "$STAGE/hooks"

echo "== Pardus Maker katmani =="
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
tar -C "$STAGE" --numeric-owner --hard-dereference -cf - ota.json root kernel hooks enable.txt packages.txt | xz -T0 -6 > "$OUT"
( cd "$PIOS_OUT" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256" )
rm -rf "$STAGE"
ls -lh "$OUT"
