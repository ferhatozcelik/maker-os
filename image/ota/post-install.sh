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

# Dosya ve klasor adlari Ingilizce oldu: eski adlar tasinir (veri korunur;
# hedef zaten varsa dokunulmaz).
HOME_DIR="$(getent passwd "$DEVICE_USER" | cut -d: -f6)"
move_old() { [ -e "$1" ] && [ ! -e "$2" ] && mv "$1" "$2" && echo "tasindi: $1 -> $2"; return 0; }
if [ -n "$HOME_DIR" ]; then
  move_old "$HOME_DIR/Projeler" "$HOME_DIR/Projects"
  move_old "$HOME_DIR/Projects/.silinenler" "$HOME_DIR/Projects/.trash"
  move_old "$HOME_DIR/.pios-sessiz" "$HOME_DIR/.pios-quiet"
  PICS="$(sed -n 's/^XDG_PICTURES_DIR="\(.*\)"/\1/p' "$HOME_DIR/.config/user-dirs.dirs" 2>/dev/null | sed "s|\$HOME|$HOME_DIR|")"
  [ -n "$PICS" ] && move_old "$PICS/Kamera" "$PICS/Camera"
fi
move_old /var/lib/pios/plugins/.silinenler /var/lib/pios/plugins/.trash
move_old /boot/firmware/wifi-ornek.txt /boot/firmware/wifi-example.txt
rm -f /boot/firmware/pardus-durum.txt

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

# Paketler: cihazin dis deposu yok; bu surumle degisen .deb'ler paketin
# icinde gelir ve bagimliliklari birbirinden/kurulu sistemden karsilanir.
if ls "$DIR"/debs/*.deb >/dev/null 2>&1; then
  echo "paketler: $(ls "$DIR"/debs | wc -l) .deb kuruluyor"
  export DEBIAN_FRONTEND=noninteractive
  apt-get -o DPkg::Lock::Timeout=600 -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold \
    install -y --no-install-recommends --allow-downgrades "$DIR"/debs/*.deb || exit 1
fi
exit 0
