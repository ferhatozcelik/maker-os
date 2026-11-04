#!/bin/sh
# Docker icinde: Buildroot EROFS kok dosya sistemini acar, tar'a doker, olculer cikarir.
set -eu
apt-get update -qq && apt-get install -y -qq erofs-utils xz-utils >/dev/null 2>&1
I=/build/output/images
rm -rf /x && mkdir /x
fsck.erofs --extract=/x $I/rootfs.erofs >/dev/null
XZ=$(ls $I/makeros-*-rpi4.img.xz | head -1)
{
  echo "kind=makeros"
  echo "xz_bytes=$(stat -c %s $XZ)"
  echo "img_bytes=$(xz --robot -l $XZ | awk '/^totals/{print $5}')"
  echo "rootfs_erofs_bytes=$(stat -c %s $I/rootfs.erofs)"
  echo "rootfs_used_kb=$(du -sk /x | cut -f1)"
  echo "packages=$(ls /build/output/build | grep -v '^host-\|^buildroot-\|^toolchain\|^skeleton\|\.stamp' | wc -l)"
  echo "files=$(find /x -type f | wc -l)"
  echo "enabled_system_units=$(find /x/etc/systemd/system -name '*.wants' -type d -exec ls {} \; 2>/dev/null | wc -l)"
} > /out/makeros.metrics
cat /out/makeros.metrics >&2
tar -C /x -cf - --numeric-owner .
