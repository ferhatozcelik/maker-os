#!/bin/sh
# OTA kurulum kancasi: dosyalar yerine konduktan sonra cihazda (root) calisir.
# Idempotent olmali; basarisiz olursa (cikis kodu != 0) guncelleme geri alinir.
# Ortam: PIOS_OTA_DIR (acilmis paket), OTA_FROM, OTA_TO, PIOS_USER.
set -u
export LC_ALL=C
DIR="${PIOS_OTA_DIR:?}"
DEVICE_USER="${PIOS_USER:-maker}"

# Paket build kullanicisini (maker) tasir; otomatik oturum cihazdaki
# kullanicida kalsin (ilk on surum imajlarinda: pardus).
sed -i "s/^autologin-user=.*/autologin-user=$DEVICE_USER/" /etc/lightdm/lightdm.conf.d/50-pios.conf

# Varsayilan arka plani hala kullanan kullanicida yenisine gec (kendi
# resmini secen kullanicinin ayarina dokunulmaz; oturum yenilenince gorunur).
DESKTOP_XML="$(getent passwd "$DEVICE_USER" | cut -d: -f6)/.config/xfce4/xfconf/xfce-perchannel-xml/xfce4-desktop.xml"
if [ -f /usr/share/backgrounds/pardus-maker.jpg ] && [ -f "$DESKTOP_XML" ]; then
  sed -i 's|/usr/share/backgrounds/pardus-yuzyil.svg|/usr/share/backgrounds/pardus-maker.jpg|g' "$DESKTOP_XML"
fi

if [ -d /run/systemd/system ]; then
  systemctl daemon-reload
  while read -r unit; do
    [ -n "$unit" ] && systemctl enable "$unit" >/dev/null 2>&1 && echo "etkin: $unit"
  done < "$DIR/enable.txt"
  # Yeni frekans ayari hemen gecerli olsun; masaustu/VNC yeniden baslatilmaz.
  systemctl try-restart pios-cpu.service 2>/dev/null || systemctl start pios-cpu.service 2>/dev/null || true
fi
udevadm control --reload 2>/dev/null || true
sysctl --system >/dev/null 2>&1 || true
dconf update 2>/dev/null || true
gtk-update-icon-cache -f -q /usr/share/icons/hicolor 2>/dev/null || true

# Bu surumun gerektirdigi ama cihazda olmayan paketler (or. yeni bagimliliklar).
missing="$(dpkg-query -W -f='${Package}\n' 2>/dev/null | sort -u | comm -13 - "$DIR/packages.txt" | tr '\n' ' ')"
if [ -n "$(echo "$missing" | tr -d ' ')" ]; then
  echo "eksik paketler: $missing"
  export DEBIAN_FRONTEND=noninteractive
  apt-get -o DPkg::Lock::Timeout=600 update -qq || true
  # shellcheck disable=SC2086
  apt-get -o DPkg::Lock::Timeout=600 install -y --no-install-recommends $missing || \
    for p in $missing; do apt-get -o DPkg::Lock::Timeout=600 install -y --no-install-recommends "$p" || echo "uyari: $p kurulamadi"; done
fi
exit 0
