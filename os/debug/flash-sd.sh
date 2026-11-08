#!/usr/bin/env bash
# MakerOS imajını Mac'te SD karta yazar (yalnız MakerOS kartı: MOSCFG bölümü olan, harici/kart okuyucu).
#   sudo os/debug/flash-sd.sh [/dev/diskN] [imaj.img.xz]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DISK="${1:-}"
IMG="${2:-$(ls -t "$ROOT"/os/out/makeros-*-rpi4-debug.img.xz | head -n1)}"
if [ -z "$DISK" ]; then
	for d in $(diskutil list | awk '/^\/dev\/disk/ {print $1}'); do
		diskutil list "$d" | grep -q MOSCFG && DISK="$d" && break
	done
fi
[ -n "$DISK" ] || { echo "MakerOS SD kartı bulunamadı; diski elle verin: $0 /dev/diskN" >&2; exit 1; }
diskutil info "$DISK" | grep -qE "Removable Media: +Removable|Protocol: +Secure Digital|Device Location: +External" ||
	{ echo "$DISK çıkarılabilir bir kart değil; durduruldu." >&2; exit 1; }
[ "$DISK" != /dev/disk0 ] || exit 1
XZ="$(command -v xz || echo /opt/homebrew/bin/xz)"
echo "yazılacak: $IMG"
echo "hedef:     $DISK ($(diskutil info "$DISK" | awk -F: '/Media Name|Disk Size/ {gsub(/^ +/,"",$2); printf "%s; ", $2}'))"
diskutil unmountDisk "$DISK"
"$XZ" -dc "$IMG" | dd of="${DISK/disk/rdisk}" bs=4m status=progress
sync
diskutil eject "$DISK"
echo "tamam: kartı çıkarıp Pi'ye takın."
