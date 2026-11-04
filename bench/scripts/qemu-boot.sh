#!/bin/sh
# Docker icinde: QEMU raspi4b (BCM2711, 4x A72, 2 GB - QEMU siniri) ile tam acilis.
# Seri konsoldan zaman damgali satirlar; isaretler: cekirdek baslangici, kok FS,
# "Reached target" satirlari. Ayni QEMU, ayni makine, ayni sure siniri.
#   qemu-boot.sh <ad> <img> <kernel> <dtb> <initrd|-> <cmdline> [sure_sn]
set -u
NAME=$1 IMG=$2 K=$3 DTB=$4 INITRD=$5 CMD=$6 LIMIT=${7:-900}
W=/work/$NAME; mkdir -p $W
cp --sparse=always $IMG $W/sd.img
qemu-img resize -f raw $W/sd.img 8G >/dev/null 2>&1
INI=""; [ "$INITRD" != "-" ] && INI="-initrd $INITRD"
T0=$(date +%s.%N)
timeout $LIMIT qemu-system-aarch64 -M raspi4b -smp 4 -m 2G -nographic \
	-kernel $K -dtb $DTB $INI -append "$CMD" \
	-drive file=$W/sd.img,if=sd,format=raw -serial mon:stdio 2>&1 </dev/null | \
	while IFS= read -r l; do printf '%.3f %s\n' "$(echo "$(date +%s.%N) - $T0" | bc)" "$l"; done > /out/$NAME.serial.log
echo "bitti: $NAME"
