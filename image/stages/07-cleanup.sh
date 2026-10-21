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

cat > "$ROOTFS_DIR/etc/apt/sources.list" <<CFG
deb $PARDUS_MIRROR $PARDUS_SUITE $PARDUS_COMPONENTS
deb $PARDUS_SECURITY_MIRROR $PARDUS_SECURITY_SUITE $PARDUS_COMPONENTS
CFG
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
