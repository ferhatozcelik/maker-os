#!/bin/sh
# Imaja girmemesi gereken her seyi temizler: apt onbellegi, cihaza ozel
# kimlikler (SSH host anahtarlari, machine-id), build artiklari.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root
require_rootfs

trap chroot_umount EXIT
chroot_mount

echo "== paket listesi kaydediliyor =="
in_chroot dpkg-query -W -f='${Package}\t${Version}\t${Architecture}\n' > "$PIOS_WORK/packages.tsv"

echo "== apt temizligi =="
in_chroot apt-get -y autoremove --purge
in_chroot apt-get clean
chroot_umount
trap - EXIT

# Cihaz disaridaki depolara (Pardus, Raspberry Pi, Microsoft) baglanmaz:
# paketler yalnizca MakerOS surumleriyle (OTA) gelir. Bkz. vendor/README.md
cat > "$ROOTFS_DIR/etc/apt/sources.list" <<'CFG'
# MakerOS: sistem paketleri MakerOS surumleriyle (Merkez >
# Guncellemeler, OTA) guncellenir. Bu cihaz dis depolara baglanmaz.
CFG
rm -f "$ROOTFS_DIR"/etc/apt/sources.list.d/* "$ROOTFS_DIR/etc/apt/preferences.d/50-pios-raspberrypi" \
  "$ROOTFS_DIR/etc/apt/apt.conf.d/89-pios-sync"
rm -rf "$ROOTFS_DIR"/var/lib/apt/lists/* "$ROOTFS_DIR"/var/cache/apt/*.bin
rm -rf "$ROOTFS_DIR"/var/cache/debconf/*-old "$ROOTFS_DIR"/var/lib/dpkg/*-old
rm -rf "$ROOTFS_DIR"/tmp/* "$ROOTFS_DIR"/var/tmp/* "$ROOTFS_DIR"/root/.cache
find "$ROOTFS_DIR/var/log" -type f -exec truncate -s 0 {} \;

echo "== cihaza ozel kimlikler siliniyor (ilk acilista uretilir) =="
rm -f "$ROOTFS_DIR"/etc/ssh/ssh_host_*
: > "$ROOTFS_DIR/etc/machine-id"
rm -f "$ROOTFS_DIR/var/lib/dbus/machine-id"
ln -s /etc/machine-id "$ROOTFS_DIR/var/lib/dbus/machine-id"
rm -f "$ROOTFS_DIR/var/lib/systemd/random-seed"
rm -f "$ROOTFS_DIR/var/lib/pios/firstboot-done"

echo "== build artiklari =="
rm -f "$ROOTFS_DIR/usr/bin/qemu-aarch64-static"
rm -rf "$ROOTFS_DIR/debootstrap"
# NetworkManager /etc/resolv.conf'u calisma aninda kendisi yazar.
rm -f "$ROOTFS_DIR/etc/resolv.conf"

echo "Temizlik tamam: $(du -sh "$ROOTFS_DIR" | cut -f1)"
