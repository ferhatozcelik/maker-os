#!/bin/sh
# Ilk acilista bir kez, kullanici etkilesimi olmadan calisir:
#  - root bolumunu SD kartin tamamina genisletir
#  - cihaza ozel SSH host anahtarlarini uretir (imajda ortak anahtar yok)
set -u

STAMP=/var/lib/pios/firstboot-done
log() { echo "pios-firstboot: $*"; }

ROOT_PART="$(findmnt -n -o SOURCE /)"
ROOT_NAME="$(basename "$ROOT_PART")"
ROOT_DISK="/dev/$(lsblk -no PKNAME "$ROOT_PART" | head -n1)"
PART_NUM="$(cat "/sys/class/block/$ROOT_NAME/partition" 2>/dev/null || true)"

if [ -n "$PART_NUM" ] && [ -b "$ROOT_DISK" ]; then
  log "root bolumu genisletiliyor: $ROOT_DISK bolum $PART_NUM"
  if growpart "$ROOT_DISK" "$PART_NUM"; then
    partx -u "$ROOT_DISK" 2>/dev/null || true
    resize2fs "$ROOT_PART" && log "dosya sistemi genisletildi"
  else
    log "growpart degisiklik yapmadi (zaten tam boyut olabilir)"
  fi
fi

log "SSH host anahtarlari uretiliyor"
ssh-keygen -A

mkdir -p "$(dirname "$STAMP")"
touch "$STAMP"
log "tamamlandi"
