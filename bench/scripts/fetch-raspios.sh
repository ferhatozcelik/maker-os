#!/bin/sh
# Docker icinde (privileged, yalniz colima VM'inde loop mount): Raspberry Pi OS
# imajini indirir, sha256 dogrular, kok bolumunu tar'a doker ve olculer cikarir.
set -eu
V=2026-10-06
for kind in desktop lite; do
  case $kind in
    desktop) U=https://downloads.raspberrypi.com/raspios_arm64/images/raspios_arm64-$V/$V-raspios-trixie-arm64.img.xz ;;
    lite)    U=https://downloads.raspberrypi.com/raspios_lite_arm64/images/raspios_lite_arm64-$V/$V-raspios-trixie-arm64-lite.img.xz ;;
  esac
  F=/data/$(basename $U)
  [ -f $F ] || { curl -fsSL -o $F.part $U && mv $F.part $F; }
  curl -fsSL $U.sha256 | awk '{print $1"  '$F'"}' | sha256sum -c -
  IMG=/data/raspios-$kind.img
  [ -f $IMG ] || xz -dc -T0 $F > $IMG
  OFF=$(sfdisk -J $IMG | jq '.partitiontable.partitions[1].start * 512')
  mkdir -p /mnt/$kind
  mount -o loop,ro,offset=$OFF $IMG /mnt/$kind
  {
    echo "kind=$kind"
    echo "xz_bytes=$(stat -c %s $F)"
    echo "img_bytes=$(stat -c %s $IMG)"
    echo "rootfs_used_kb=$(df -k /mnt/$kind | awk 'NR==2{print $3}')"
    echo "packages=$(grep -c '^Package: ' /mnt/$kind/var/lib/dpkg/status)"
    echo "files=$(find /mnt/$kind -xdev -type f | wc -l)"
    echo "enabled_system_units=$(find /mnt/$kind/etc/systemd/system -name '*.wants' -type d -exec ls {} \; | wc -l)"
  } > /data/raspios-$kind.metrics
  [ -f /data/raspios-$kind.tar ] || tar -C /mnt/$kind -cf /data/raspios-$kind.tar --numeric-owner .
  umount /mnt/$kind
  cat /data/raspios-$kind.metrics
done
