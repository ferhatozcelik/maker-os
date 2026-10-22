#!/bin/bash
# SD kartı yeniden yazmadan Merkez'i güncelle: ./update-center.sh [pardus@cihaz]
# Yalnız paket üret: ./update-center.sh --package
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
TARGET="${1:-pardus@pardus-maker.local}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/usr/lib/pios" "$ROOT/image/out"
cp -R "$ROOT/image/overlay/usr/lib/pios/center" "$STAGE/usr/lib/pios/center"
cp -R "$ROOT/image/overlay/usr/lib/pios/templates" "$STAGE/usr/lib/pios/templates"
cp "$ROOT/image/overlay/usr/lib/pios/network-fallback.sh" "$STAGE/usr/lib/pios/"
cat > "$STAGE/install-update.sh" <<'INSTALL'
#!/bin/bash
set -euo pipefail
[[ $(id -u) == 0 ]] || { echo 'sudo bash install-update.sh ile çalıştırın.'; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
/usr/bin/python3 -c 'import sys; assert sys.version_info >= (3,9), "Python 3.9 veya sonrası gerekli"'
/usr/bin/python3 -m compileall -q "$HERE/usr/lib/pios/center"
/usr/bin/python3 -m compileall -q "$HERE/usr/lib/pios/templates"
[[ -d /usr/lib/pios/center ]] || { echo 'Pardus Maker Merkezi bulunamadı.'; exit 1; }
BACKUP="$(mktemp -d /var/backups/pios-center.XXXXXXXX)"
cp -a /usr/lib/pios/center "$BACKUP/center"
if [[ -d /usr/lib/pios/templates ]]; then
  cp -a /usr/lib/pios/templates "$BACKUP/templates"
fi
if [[ -f /usr/lib/pios/network-fallback.sh ]]; then
  cp -a /usr/lib/pios/network-fallback.sh "$BACKUP/network-fallback.sh"
fi
rollback() {
  echo "Güncelleme başarısız; yedek geri yükleniyor: $BACKUP" >&2
  cp -a "$BACKUP/center/." /usr/lib/pios/center/
  if [[ -d "$BACKUP/templates" ]]; then
    cp -a "$BACKUP/templates/." /usr/lib/pios/templates/
  fi
  if [[ -f "$BACKUP/network-fallback.sh" ]]; then
    cp -a "$BACKUP/network-fallback.sh" /usr/lib/pios/network-fallback.sh
  fi
  systemctl restart pios-center.service || true
}
trap rollback ERR
systemctl stop pios-center.service
cp -R "$HERE/usr/lib/pios/center/." /usr/lib/pios/center/
mkdir -p /usr/lib/pios/templates
cp -R "$HERE/usr/lib/pios/templates/." /usr/lib/pios/templates/
chown -R root:root /usr/lib/pios/center
chown -R root:root /usr/lib/pios/templates
install -o root -g root -m 755 "$HERE/usr/lib/pios/network-fallback.sh" /usr/lib/pios/network-fallback.sh
systemctl restart pios-center.service
sleep 2
systemctl is-active --quiet pios-center.service
trap - ERR
echo "Merkez ve MediaPipe şablonları güncellendi. Önceki sürüm: $BACKUP"
echo 'Mevcut kişisel projeler değiştirilmedi. Optimize örnekler için yeni proje oluşturun.'
echo 'Tarayıcı sayfasını yenileyin. Fan ayarlarını kaydettikten sonra Pi’yi yeniden başlatın.'
INSTALL
ARCHIVE="$ROOT/image/out/pardus-maker-center-update.tar.gz"
COPYFILE_DISABLE=1 tar --exclude='__pycache__' --exclude='*.pyc' -czf "$ARCHIVE" -C "$STAGE" .
printf 'Güncelleme paketi: %s\n' "$ARCHIVE"
[[ "$TARGET" == --package ]] && exit 0
[[ "$TARGET" =~ ^[a-zA-Z0-9_.@:-]+$ && "$TARGET" != -* ]] || { echo 'Geçersiz SSH hedefi.' >&2; exit 1; }
REMOTE_DIR="$(ssh -o ConnectTimeout=8 "$TARGET" 'mktemp -d /tmp/pardus-maker-update.XXXXXXXX')"
[[ "$REMOTE_DIR" =~ ^/tmp/pardus-maker-update\.[a-zA-Z0-9]+$ ]] || { echo 'Uzak geçici klasör doğrulanamadı.' >&2; exit 1; }
scp "$ARCHIVE" "$TARGET:$REMOTE_DIR/update.tar.gz"
# sudo parolası için TTY gerekir; arşiv ayrı SCP bağlantısıyla gönderildi.
ssh -t "$TARGET" "tar -xzf '$REMOTE_DIR/update.tar.gz' -C '$REMOTE_DIR' && sudo bash '$REMOTE_DIR/install-update.sh' && rm -rf '$REMOTE_DIR'"
