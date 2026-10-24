#!/bin/sh
# Ekransiz ag erisimi, kurulum sihirbazi olmadan:
#  1. /boot/firmware/wifi.txt varsa (istege bagli) Wi-Fi profili olarak
#     eklenir ve parola iceren dosya silinir.
#  2. Ethernet/Wi-Fi ile varsayilan rota bekleme suresi icinde olusmazsa
#     wlan0 uzerinde "Pardus-Maker-XXXX" erisim noktasi acilir.
#  3. Bulunan adresler /boot/firmware/pardus-status.txt dosyasina yazilir
#     (SD kart bilgisayara takilarak okunabilir).
set -u

# Web paneldeki ağ değişikliğiyle ilk açılış erişim noktası yarışmasın.
if [ "${PIOS_NETWORK_LOCKED:-}" != 1 ]; then
  mkdir -p /run/pios
  exec flock -w 120 /run/pios/network.lock env PIOS_NETWORK_LOCKED=1 "$0" "$@"
fi

BOOT=/boot/firmware
WIFI_TXT="$BOOT/wifi.txt"
STATUS="$BOOT/pardus-status.txt"
HOTSPOT_CON=pardus-hotspot
HOTSPOT_PSK="${PIOS_HOTSPOT_PSK:-pardus1234}"
WAIT_SECS="${PIOS_FALLBACK_WAIT:-45}"
MODE="istemci"

log() { echo "pios-network: $*"; }

import_wifi_txt() {
  [ -f "$WIFI_TXT" ] || return 0
  ssid="$(sed -n 's/^[[:space:]]*SSID[[:space:]]*=[[:space:]]*//p' "$WIFI_TXT" | head -n1 | tr -d '\r')"
  psk="$(sed -n 's/^[[:space:]]*PAROLA[[:space:]]*=[[:space:]]*//p' "$WIFI_TXT" | head -n1 | tr -d '\r')"
  if [ -z "$ssid" ]; then
    log "wifi.txt icinde SSID yok, yok sayiliyor"
    return 0
  fi
  nmcli -t -f NAME con show | grep -Fxq "$ssid" && nmcli con delete "$ssid" >/dev/null 2>&1
  if [ -n "$psk" ]; then
    nmcli con add type wifi ifname wlan0 con-name "$ssid" ssid "$ssid" \
      wifi-sec.key-mgmt wpa-psk wifi-sec.psk "$psk" connection.autoconnect yes >/dev/null
  else
    nmcli con add type wifi ifname wlan0 con-name "$ssid" ssid "$ssid" connection.autoconnect yes >/dev/null
  fi
  log "wifi.txt'den '$ssid' profili eklendi"
  rm -f "$WIFI_TXT"
  nmcli con up "$ssid" >/dev/null 2>&1 || true
}

has_uplink() {
  ip -4 route show default 2>/dev/null | grep -q .
}

start_hotspot() {
  if ! ip link show wlan0 >/dev/null 2>&1; then
    log "wlan0 yok, erisim noktasi acilamiyor"
    return 1
  fi
  if ! nmcli -t -f NAME con show | grep -Fxq "$HOTSPOT_CON"; then
    suffix="$(tr -d ':' < /sys/class/net/wlan0/address | tail -c 5 | tr 'a-f' 'A-F')"
    nmcli con add type wifi ifname wlan0 con-name "$HOTSPOT_CON" autoconnect no \
      ssid "Pardus-Maker-$suffix" mode ap 802-11-wireless.band bg 802-11-wireless.channel 6 \
      ipv4.method shared ipv4.addresses 10.42.0.1/24 ipv6.method ignore \
      wifi-sec.key-mgmt wpa-psk wifi-sec.proto rsn wifi-sec.pairwise ccmp \
      wifi-sec.group ccmp wifi-sec.psk "$HOTSPOT_PSK" >/dev/null
  fi
  nmcli con up "$HOTSPOT_CON" >/dev/null && MODE="erisim-noktasi"
}

write_status() {
  {
    echo "Pardus Maker - ag durumu"
    echo "Zaman      : $(date '+%Y-%m-%d %H:%M:%S')"
    echo "Surum      : $(cat /etc/pios-build-version 2>/dev/null)"
    echo "Ana makine : $(hostname).local"
    echo "Mod        : $MODE"
    if [ "$MODE" = "erisim-noktasi" ]; then
      echo "Wi-Fi agi  : $(nmcli -g 802-11-wireless.ssid con show "$HOTSPOT_CON" 2>/dev/null)"
      echo "Wi-Fi sifre: $HOTSPOT_PSK"
    fi
    echo
    echo "IPv4 adresleri:"
    ip -4 -o addr show scope global | awk '{print "  " $2 ": " $4}'
    echo
    echo "Web panel (ekran + kontrol): http://$(hostname).local/  veya  http://<IP>/"
    echo "VNC istemcisi              : <IP>:5900"
    echo "SSH                : ssh $(sed -n "s/^autologin-user=//p" /etc/lightdm/lightdm.conf.d/50-pios.conf 2>/dev/null || echo maker)@$(hostname).local"
  } > "$STATUS.tmp" 2>/dev/null && mv "$STATUS.tmp" "$STATUS"
}

import_wifi_txt

i=0
while [ "$i" -lt "$WAIT_SECS" ]; do
  has_uplink && break
  sleep 1
  i=$((i + 1))
done

if has_uplink; then
  log "ag baglantisi var, erisim noktasina gerek yok"
else
  log "$WAIT_SECS sn icinde ag yok; erisim noktasi aciliyor"
  start_hotspot || log "erisim noktasi acilamadi"
fi

write_status
log "durum $STATUS dosyasina yazildi"
