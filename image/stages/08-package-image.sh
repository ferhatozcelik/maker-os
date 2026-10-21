#!/bin/sh
# Rootfs'i boot (FAT32) + root (ext4) bolumlu bir .img dosyasina paketler
#. Loop aygiti/mount KULLANMAZ: dosya sistemleri
# "mkfs.ext4 -d" ve mtools ile dosya olarak uretilip imaja yazilir. Bu
# sayede Docker/colima icinde (yeni /dev dugumleri gorunmeyen ortamlar)
# guvenilir calisir. Root bolumu ilk acilista SD kartin tamamina buyur.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
require_root
require_rootfs

for bin in sfdisk mkfs.vfat mcopy mkfs.ext4; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "HATA: $bin bulunamadi" >&2
    exit 1
  fi
done

BOOT_SIZE_MIB="${PIOS_BOOT_SIZE_MIB:-512}"
BOOT_START_MIB=4
VERSION="$(cat "$PIOS_WORK/build-version")"
OUT_IMG="$PIOS_OUT/pardus-maker-$VERSION.img"
PART_DIR="$PIOS_WORK/parts"
mkdir -p "$PIOS_OUT" "$PART_DIR"

DISK_ID="$(head -c4 /dev/urandom | od -An -tx1 | tr -d ' \n')"
BOOT_PARTUUID="$DISK_ID-01"
ROOT_PARTUUID="$DISK_ID-02"

echo "== PARTUUID'ler yaziliyor (boot=$BOOT_PARTUUID root=$ROOT_PARTUUID) =="
sed -i "s/root=PARTUUID=[0-9a-fA-F-]*/root=PARTUUID=$ROOT_PARTUUID/" "$ROOTFS_DIR/boot/firmware/cmdline.txt"
sed -i -e "s|^PARTUUID=[0-9a-fA-F-]*\( *\)/boot/firmware|PARTUUID=$BOOT_PARTUUID\1/boot/firmware|" \
       -e "s|^PARTUUID=[0-9a-fA-F-]*\( *\)/ |PARTUUID=$ROOT_PARTUUID\1/ |" "$ROOTFS_DIR/etc/fstab"
grep -q "$ROOT_PARTUUID" "$ROOTFS_DIR/etc/fstab" || { echo "HATA: fstab guncellenemedi" >&2; exit 1; }

echo "== boot bolumu (FAT32, ${BOOT_SIZE_MIB} MiB) =="
BOOT_IMG="$PART_DIR/boot.vfat"
rm -f "$BOOT_IMG"
mkfs.vfat -F 32 -n PARDUSBOOT -C "$BOOT_IMG" $((BOOT_SIZE_MIB * 1024)) >/dev/null
mcopy -s -i "$BOOT_IMG" "$ROOTFS_DIR"/boot/firmware/* ::/

echo "== root bolumu (ext4, mkfs.ext4 -d) =="
ROOT_IMG="$PART_DIR/root.ext4"
rm -f "$ROOT_IMG"
# boot/firmware ayri bolumde; root icinde yalnizca bos mount noktasi kalir.
FW_STASH="$PART_DIR/firmware"
rm -rf "$FW_STASH"
mv "$ROOTFS_DIR/boot/firmware" "$FW_STASH"
mkdir "$ROOTFS_DIR/boot/firmware"
trap 'rmdir "$ROOTFS_DIR/boot/firmware" 2>/dev/null && mv "$FW_STASH" "$ROOTFS_DIR/boot/firmware"' EXIT
USED_MIB=$(du -s -B1M -x "$ROOTFS_DIR" | cut -f1)
# %20 + 400 MiB bos alan; asil buyume ilk acilista growpart ile olur.
ROOT_SIZE_MIB=$(( USED_MIB * 12 / 10 + 400 ))
truncate -s "${ROOT_SIZE_MIB}M" "$ROOT_IMG"
# Hedefteki e2fsprogs 1.46 (bullseye): 1.47'nin varsayilan orphan_file
# ozelligi acik kalirsa acilistaki fsck ve ilk acilistaki resize2fs
# dosya sistemini reddeder.
mkfs.ext4 -q -L pardusroot -O ^orphan_file,^metadata_csum_seed -i 8192 \
  -E root_owner=0:0 -d "$ROOTFS_DIR" "$ROOT_IMG"
rmdir "$ROOTFS_DIR/boot/firmware"
mv "$FW_STASH" "$ROOTFS_DIR/boot/firmware"
trap - EXIT
e2fsck -fy "$ROOT_IMG" >/dev/null || [ $? -le 1 ]

echo "== imaj birlestiriliyor =="
ROOT_START_MIB=$(( BOOT_START_MIB + BOOT_SIZE_MIB ))
TOTAL_MIB=$(( ROOT_START_MIB + ROOT_SIZE_MIB ))
rm -f "$OUT_IMG"
truncate -s "${TOTAL_MIB}M" "$OUT_IMG"
sfdisk -q "$OUT_IMG" <<SFDISK
label: dos
label-id: 0x$DISK_ID
start=$((BOOT_START_MIB * 2048)), size=$((BOOT_SIZE_MIB * 2048)), type=c, bootable
start=$((ROOT_START_MIB * 2048)), type=83
SFDISK
dd if="$BOOT_IMG" of="$OUT_IMG" bs=1M seek="$BOOT_START_MIB" conv=notrunc,sparse status=none
dd if="$ROOT_IMG" of="$OUT_IMG" bs=1M seek="$ROOT_START_MIB" conv=notrunc,sparse status=none
rm -f "$BOOT_IMG" "$ROOT_IMG"

echo "$OUT_IMG" > "$PIOS_WORK/image-path"
echo "Imaj uretildi: $OUT_IMG (${TOTAL_MIB} MiB)"
