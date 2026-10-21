#!/bin/sh
# overlay/ dosyalarini rootfs'e yerlestirir ve sistemi ayarlar:
# otomatik oturumlu macOS benzeri masaustu (ust cubuk + Plank dock, Pardus
# 100. Yil temasi), Pardus Maker Merkezi (port 80), VNC, ekransiz ag,
# ilk acilis isleri, SD kart/ARM optimizasyonlari.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
require_root
require_rootfs

trap chroot_umount EXIT
chroot_mount

echo "== overlay kopyalaniyor =="
# Eski build'den kalan dock ogeleri (or. 04-code.dockitem) kalmasin.
rm -rf "$ROOTFS_DIR/etc/skel/.config/plank"
cp -a "$OVERLAY_DIR/." "$ROOTFS_DIR/"
chown -R 0:0 "$ROOTFS_DIR/etc/systemd/system" "$ROOTFS_DIR/usr/lib/pios" "$ROOTFS_DIR/etc/skel" \
  "$ROOTFS_DIR/etc/lightdm" "$ROOTFS_DIR/etc/X11/xorg.conf.d" "$ROOTFS_DIR/etc/dconf" \
  "$ROOTFS_DIR/usr/share/applications" "$ROOTFS_DIR/usr/share/icons/hicolor" \
  "$ROOTFS_DIR/usr/local/bin/pios-open" "$ROOTFS_DIR/etc/udev/rules.d"
chmod -R a+rX "$ROOTFS_DIR/usr/lib/pios/templates"
sed -i "s/^autologin-user=.*/autologin-user=$PIOS_USER/" "$ROOTFS_DIR/etc/lightdm/lightdm.conf.d/50-pios.conf"

echo "== web terminali: xterm.js (Merkez > Terminal) =="
# npm kayit defterinden sabit surum + sha256; Pi agsizken de calissin diye
# imaja gomulur ve Merkez /xterm/ altinda sunar.
XTERM_DIR="$ROOTFS_DIR/usr/share/pios/xterm"
rm -rf "$XTERM_DIR"
mkdir -p "$XTERM_DIR" "$PIOS_CACHE"
for spec in \
  "@xterm/xterm/-/xterm-5.5.0.tgz bd954fa721872170188cc5d7e83e88db3c83c9a18a4e8d24c2783d26491f59d2 lib/xterm.js css/xterm.css" \
  "@xterm/addon-fit/-/addon-fit-0.10.0.tgz 917ac44972453d5eed52edc1e50260c76398ce48cf2290c2e60671102bba0b33 lib/addon-fit.js"; do
  # shellcheck disable=SC2086
  set -- $spec
  tgz="$PIOS_CACHE/$(basename "$1")"
  [ -s "$tgz" ] || curl -fL --retry 3 "https://registry.npmjs.org/$1" -o "$tgz"
  echo "$2  $tgz" | sha256sum -c - >/dev/null || { echo "HATA: $tgz sha256 uyusmuyor" >&2; rm -f "$tgz"; exit 1; }
  tgz_file="$tgz"; shift 2
  for f in "$@"; do
    tar -xzf "$tgz_file" -O "package/$f" > "$XTERM_DIR/$(basename "$f")"
  done
done

echo "== MediaPipe modelleri (Projeler > MediaPipe ornekleri, agsiz calisir) =="
MP_DIR="$ROOTFS_DIR/usr/share/pios/mediapipe"
mkdir -p "$MP_DIR" "$PIOS_CACHE/mediapipe"
while read -r sum path; do
  f="$PIOS_CACHE/mediapipe/$(basename "$path")"
  [ -s "$f" ] || curl -fL --retry 3 "https://storage.googleapis.com/mediapipe-models/$path" -o "$f"
  echo "$sum  $f" | sha256sum -c - >/dev/null || { echo "HATA: $f sha256 uyusmuyor" >&2; rm -f "$f"; exit 1; }
  cp "$f" "$MP_DIR/"
done <<'MODELS'
fbc2a30080c3c557093b5ddfc334698132eb341044ccee322ccf8bcf3607cde1 hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task
97952348cf6a6a4915c2ea1496b4b37ebabc50cbbf80571435643c455f2b0482 gesture_recognizer/gesture_recognizer/float16/1/gesture_recognizer.task
b4578f35940bf5a1a655214a1cce5cab13eba73c1297cd78e1a04c2380b0152f face_detector/blaze_face_short_range/float16/1/blaze_face_short_range.tflite
59929e1d1ee95287735ddd833b19cf4ac46d29bc7afddbbf6753c459690d574a pose_landmarker/pose_landmarker_lite/float16/1/pose_landmarker_lite.task
0720bf247bd76e6594ea28fa9c6f7c5242be774818997dbbeffc4da460c723bb object_detector/efficientdet_lite0/int8/1/efficientdet_lite0.tflite
191ac9529ae506ee0beefa6b2c945a172dab9d07d1e802a290a4e4038226658b image_segmenter/selfie_segmenter/float16/1/selfie_segmenter.tflite
64184e229b263107bc2b804c6625db1341ff2bb731874b0bcc2fe6544e0bc9ff face_landmarker/face_landmarker/float16/1/face_landmarker.task
MODELS

echo "== uzak ekran parolasi =="
mkdir -p "$ROOTFS_DIR/etc/pios"
in_chroot x11vnc -storepasswd "$PIOS_PASSWORD" /etc/pios/vnc.pass >/dev/null
# Merkez > Ekran, girisi yapilmis kullaniciya VNC parolasini kendisi verir
# (tek giris). VNC protokolu en fazla 8 karakter kullanir.
printf '%s' "$PIOS_PASSWORD" | cut -c1-8 | tr -d '\n' > "$ROOTFS_DIR/etc/pios/vnc.secret"
chmod 0600 "$ROOTFS_DIR/etc/pios/vnc.pass" "$ROOTFS_DIR/etc/pios/vnc.secret"

echo "== build surum damgasi =="
PIOS_BUILD_VERSION="${PIOS_BUILD_VERSION:-$(date -u +%Y%m%dT%H%M%SZ)-$(head -c4 /dev/urandom | od -An -tx1 | tr -d ' \n')}"
echo "$PIOS_BUILD_VERSION" > "$ROOTFS_DIR/etc/pios-build-version"
echo "$PIOS_BUILD_VERSION" > "$PIOS_WORK/build-version"

echo "== donanim gruplari (sudo'suz GPIO/I2C/SPI) =="
for g in gpio spi i2c; do in_chroot groupadd -f "$g"; done
in_chroot usermod -aG gpio,spi,i2c "$PIOS_USER"

echo "== varsayilan tarayici, simge onbellegi, dconf =="
in_chroot update-alternatives --set x-www-browser /usr/bin/chromium 2>/dev/null || true
in_chroot gtk-update-icon-cache -f -q /usr/share/icons/hicolor || true
in_chroot dconf update

echo "== servisler =="
in_chroot systemctl set-default graphical.target
in_chroot systemctl enable \
  pios-firstboot.service pios-network-fallback.service pios-center.service \
  pios-vnc.service pios-novnc.service \
  NetworkManager.service ssh.service avahi-daemon.service \
  lightdm.service systemd-timesyncd.service fake-hwclock.service \
  zramswap.service bluetooth.service
rm -f "$ROOTFS_DIR/etc/systemd/system/multi-user.target.wants/pios-panel.service"

echo "== gereksiz servis/zamanlayicilar kapatiliyor (SD karti ve CPU'yu yormasin) =="
for unit in apt-daily.timer apt-daily-upgrade.timer man-db.timer e2scrub_all.timer \
            NetworkManager-wait-online.service rsync.service; do
  in_chroot systemctl disable "$unit" 2>/dev/null || true
done
in_chroot systemctl mask systemd-networkd-wait-online.service 2>/dev/null || true

echo "== kullanici masaustu ayarlari (/etc/skel -> /home/$PIOS_USER) =="
USER_HOME="$ROOTFS_DIR/home/$PIOS_USER"
rm -rf "$USER_HOME/.config/plank"
cp -a "$ROOTFS_DIR/etc/skel/.config" "$USER_HOME/"
# Terminal: renkli istem, git dali, alias'lar, "yardim" komutu, PATH.
for rc in "$ROOTFS_DIR/etc/skel/.bashrc" "$USER_HOME/.bashrc"; do
  grep -q '/usr/lib/pios/bashrc' "$rc" 2>/dev/null || \
    printf '\n# Pardus Maker\n[ -f /usr/lib/pios/bashrc ] && . /usr/lib/pios/bashrc\n' >> "$rc"
done
mkdir -p "$USER_HOME/.config/autostart" "$USER_HOME/Projeler"
# Ekran kilidi/ekran koruyucu yok (otomatik oturum + uzak ekran).
for app in light-locker xfce4-screensaver xscreensaver pardus-welcome blueman; do
  printf '[Desktop Entry]\nType=Application\nName=%s\nHidden=true\n' "$app" \
    > "$USER_HOME/.config/autostart/$app.desktop"
done
# VS Code: Python eklentisi ilk acilista onerilsin, telemetri kapali.
mkdir -p "$USER_HOME/.config/Code/User"
cat > "$USER_HOME/.config/Code/User/settings.json" <<'JSON'
{
  "telemetry.telemetryLevel": "off",
  "update.mode": "none",
  "workbench.startupEditor": "none",
  "python.defaultInterpreterPath": ".venv/bin/python",
  "window.titleBarStyle": "custom"
}
JSON
echo "== MediaPipe paketleri kullanici uv onbellegine (ornek projeler agsiz/hizli kurulsun) =="
# Projeler ayni dosya sisteminde oldugu icin uv onbellekten sert baglanti
# kullanir; her MediaPipe projesi diskte neredeyse yer kaplamaz.
in_chroot chown -R "$PIOS_USER:$PIOS_USER" "/home/$PIOS_USER"
in_chroot runuser -u "$PIOS_USER" -- env HOME="/home/$PIOS_USER" UV_PYTHON_INSTALL_DIR=/opt/pios/python UV_NO_PROGRESS=1 \
  sh -c 'set -e; uv venv -q --python 3.12 /tmp/mp-onbellek && uv pip install -q --python /tmp/mp-onbellek/bin/python mediapipe gpiozero RPi.GPIO && /tmp/mp-onbellek/bin/python -c "import mediapipe; print(\"   mediapipe\", mediapipe.__version__)"; rm -rf /tmp/mp-onbellek'
in_chroot chown -R "$PIOS_USER:$PIOS_USER" "/home/$PIOS_USER"

echo "== SSH: parola ile giris acik (ekransiz erisim icin) =="
mkdir -p "$ROOTFS_DIR/etc/ssh/sshd_config.d"
printf 'PermitRootLogin no\nPasswordAuthentication yes\n' > "$ROOTFS_DIR/etc/ssh/sshd_config.d/50-pios.conf"

echo "Sistem yapilandirildi (surum $PIOS_BUILD_VERSION)"
