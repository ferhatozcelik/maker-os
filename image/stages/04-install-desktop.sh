#!/bin/sh
# Taban sistemi, temel araclari ve hafif Pardus XFCE masaustunu kurar.
# Tum paketler --no-install-recommends ile tek tek secilir; Pardus'un
# tam masaustu meta paketi (pardus-xfce-desktop: LibreOffice, GIMP, CUPS,
# Evolution...) bilincli olarak kurulmaz.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root
require_rootfs

trap chroot_umount EXIT
chroot_mount

echo "== apt kaynaklari (build aynasi; 06 resmi adresleri yazar) =="
cat > "$ROOTFS_DIR/etc/apt/sources.list" <<CFG
deb $PARDUS_BUILD_MIRROR $PARDUS_SUITE $PARDUS_COMPONENTS
deb $PARDUS_BUILD_SECURITY_MIRROR $PARDUS_SECURITY_SUITE $PARDUS_COMPONENTS
CFG

BASE_PKGS="
  base-files pardus-archive-keyring lsb-release
  systemd-sysv udev kmod dbus dbus-user-session libpam-systemd
  sudo locales tzdata keyboard-configuration console-setup
  procps psmisc iproute2 iputils-ping less nano vim-tiny bash-completion
  ca-certificates curl wget git htop file lsof tree jq gnupg
  unzip zip xz-utils rsync usbutils pciutils dnsutils
  openssh-server avahi-daemon libnss-mdns
  network-manager dnsmasq-base wpasupplicant iw wireless-regdb rfkill
  systemd-timesyncd fake-hwclock zram-tools cloud-guest-utils
  e2fsprogs dosfstools fdisk
  python3 python3-venv
  build-essential pkg-config
  i2c-tools gpiod
  bluez
  x11vnc novnc websockify
  alsa-utils v4l-utils ffmpeg
  udisks2 ntfs-3g exfatprogs
"

# Bilincli olarak YOK: tarayici disinda ofis/goruntuleyici/hesap makinesi
# gibi uygulamalar. Hedef: ogrenci/DIY/AI/Python icin hizli, sade sistem.
DESKTOP_PKGS="
  xserver-xorg-core xserver-xorg-input-libinput xserver-xorg-video-fbdev
  x11-xserver-utils xauth libgl1-mesa-dri libgl1 libglib2.0-0
  lightdm lightdm-gtk-greeter
  xfce4-session xfwm4 xfce4-panel xfdesktop4 xfce4-settings xfconf
  xfce4-notifyd xfce4-pulseaudio-plugin
  dbus-x11 dconf-gsettings-backend dconf-cli policykit-1 policykit-1-gnome
  xdg-user-dirs xdg-utils gvfs
  pardus-xfce-settings pardus-gtk-theme pardus-icon-theme pardus-locales
  adwaita-icon-theme fonts-dejavu-core fonts-noto-ui-core fonts-inter
  thunar thunar-volman xfce4-terminal plank
  chromium chromium-l10n chromium-sandbox
  network-manager-gnome
  pulseaudio
"

echo "== paket listeleri guncelleniyor ve taban yukseltiliyor =="
# debootstrap base-files'i Debian surumuyle (11.x) getirir; dist-upgrade
# Pardus surumune (12pardusXX) gecirir ve /etc/os-release Pardus olur.
in_chroot apt-get update --error-on=any
in_chroot apt-get -y dist-upgrade

echo "== taban paketler =="
# shellcheck disable=SC2086
in_chroot apt-get install -y $BASE_PKGS

echo "== RPi Wi-Fi/Bluetooth firmware paketleri =="
# 03 paketleri indirir; build 04'ten yeniden baslatildiysa zaten kuruludur.
if ls "$ROOTFS_DIR"/var/cache/pios-debs/*.deb >/dev/null 2>&1; then
  in_chroot sh -c 'apt-get install -y /var/cache/pios-debs/*.deb'
  rm -rf "$ROOTFS_DIR/var/cache/pios-debs"
else
  for pkg in firmware-brcm80211 bluez-firmware; do
    status=$(in_chroot dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null) || status=
    [ "$status" = "install ok installed" ] \
      || { echo "HATA: $pkg kurulu degil - build'i 03'ten baslatin" >&2; exit 1; }
  done
  echo "   zaten kurulu"
fi

echo "== masaustu paketleri =="
# shellcheck disable=SC2086
in_chroot apt-get install -y $DESKTOP_PKGS

echo "== Raspberry Pi kamera: libcamera, libcamera-apps, picamera2 =="
# Pardus/Debian bullseye'da RPi kamera yigini yok; Raspberry Pi'nin bullseye
# deposu yalnizca bu paketler icin eklenir. apt tercihleri depodaki diger
# her seyi (kernel, firmware, chromium, mesa...) -1 ile engeller.
RPI_CAMERA_SUITE="${RPI_CAMERA_SUITE:-bullseye}"
curl -fsSL "$RPI_APT_MIRROR/raspberrypi.gpg.key" | gpg --dearmor > "$ROOTFS_DIR/usr/share/keyrings/raspberrypi-archive-keyring.gpg"
[ -s "$ROOTFS_DIR/usr/share/keyrings/raspberrypi-archive-keyring.gpg" ] || { echo "HATA: RPi depo anahtari yazilamadi" >&2; exit 1; }
echo "deb [signed-by=/usr/share/keyrings/raspberrypi-archive-keyring.gpg] $RPI_APT_MIRROR $RPI_CAMERA_SUITE main" \
  > "$ROOTFS_DIR/etc/apt/sources.list.d/raspberrypi-kamera.list"
mkdir -p "$ROOTFS_DIR/etc/apt/preferences.d"
cat > "$ROOTFS_DIR/etc/apt/preferences.d/50-pios-raspberrypi" <<'CFG'
# Pardus Maker: RPi deposundan yalnizca kamera yigini kurulur.
Package: libcamera* python3-libcamera python3-picamera2 python3-kms++ libkms++* python3-pidng python3-simplejpeg python3-v4l2 python3-piexif
Pin: origin archive.raspberrypi.com
Pin-Priority: 500

Package: *
Pin: origin archive.raspberrypi.com
Pin-Priority: -1
CFG
# libcamera0 "raspberrypi-kernel" paketine bagimli; kernel burada 03'te
# raspberrypi/firmware'den kurulur. Gercek paket /boot'u ezeceginden yerine
# sahte bir paket kurulur ve tutulur.
mkdir -p "$ROOTFS_DIR/tmp/rpk/DEBIAN"
cat > "$ROOTFS_DIR/tmp/rpk/DEBIAN/control" <<'CFG'
Package: raspberrypi-kernel
Version: 1:1.20990101-pios1
Architecture: all
Maintainer: Pardus Maker <pardus-maker@localhost>
Description: Pardus Maker - kernel raspberrypi/firmware ile ayrica kurulur (yer tutucu)
CFG
in_chroot dpkg-deb --build /tmp/rpk /tmp/rpk.deb >/dev/null
in_chroot dpkg -i /tmp/rpk.deb
in_chroot apt-mark hold raspberrypi-kernel
rm -rf "$ROOTFS_DIR/tmp/rpk" "$ROOTFS_DIR/tmp/rpk.deb"
in_chroot apt-get update --error-on=any
in_chroot apt-get install -y libcamera-apps python3-picamera2
in_chroot sh -c 'command -v libcamera-hello && libcamera-hello --version' || { echo "HATA: libcamera-apps calismiyor" >&2; exit 1; }

echo "== Pardus 100. Yil (Yuzyil) temasi =="
# Pardus 23 deposundaki mimariden bagimsiz paket (ikon temasi, duvar
# kagidi, baslat simgesi); bagimliligi yalnizca librsvg2-common.
mkdir -p "$PIOS_CACHE"
YUZYIL_DEB="$PIOS_CACHE/$(basename "$PARDUS_YUZYIL_DEB")"
[ -s "$YUZYIL_DEB" ] || curl -fL --retry 3 "$PARDUS_YUZYIL_DEB" -o "$YUZYIL_DEB"
cp "$YUZYIL_DEB" "$ROOTFS_DIR/tmp/yuzyil.deb"
in_chroot apt-get install -y /tmp/yuzyil.deb
rm -f "$ROOTFS_DIR/tmp/yuzyil.deb"

echo "== yerel ayarlar: Turkce, Europe/Istanbul, TR-Q klavye =="
sed -i -e 's/^# *\(tr_TR.UTF-8 UTF-8\)/\1/' -e 's/^# *\(en_US.UTF-8 UTF-8\)/\1/' "$ROOTFS_DIR/etc/locale.gen"
in_chroot locale-gen
printf 'LANG=tr_TR.UTF-8\n' > "$ROOTFS_DIR/etc/default/locale"
ln -sf /usr/share/zoneinfo/Europe/Istanbul "$ROOTFS_DIR/etc/localtime"
echo "Europe/Istanbul" > "$ROOTFS_DIR/etc/timezone"
cat > "$ROOTFS_DIR/etc/default/keyboard" <<'CFG'
XKBMODEL="pc105"
XKBLAYOUT="tr"
XKBVARIANT=""
XKBOPTIONS=""
BACKSPACE="guess"
CFG

echo "== hostname =="
echo "$PIOS_HOSTNAME" > "$ROOTFS_DIR/etc/hostname"
printf '127.0.0.1\tlocalhost\n127.0.1.1\t%s\n::1\t\tlocalhost ip6-localhost ip6-loopback\n' \
  "$PIOS_HOSTNAME" > "$ROOTFS_DIR/etc/hosts"

echo "== kullanici: $PIOS_USER (otomatik oturum, sudo) =="
in_chroot groupadd -f i2c
in_chroot groupadd -f autologin
if ! in_chroot id "$PIOS_USER" >/dev/null 2>&1; then
  in_chroot useradd -m -s /bin/bash -c "Pardus" "$PIOS_USER"
fi
in_chroot usermod -aG sudo,adm,audio,video,render,input,plugdev,netdev,bluetooth,dialout,i2c,autologin "$PIOS_USER"
echo "$PIOS_USER:$PIOS_PASSWORD" | in_chroot chpasswd
# root parolasi kilitli; yonetim sudo ile.
in_chroot passwd -l root

echo "Masaustu ve araclar kuruldu"
