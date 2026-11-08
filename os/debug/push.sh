#!/usr/bin/env bash
# Debug imajlı Pi'ye USB-C (ya da ağ) üzerinden anında güncelleme. Yeniden açılış gerekmez.
#
#   os/debug/push.sh            makerd + kabuk + OS betikleri
#   os/debug/push.sh shell      yalnız web kabuk (saniyeler)
#   os/debug/push.sh makerd     yalnız makerd
#   os/debug/push.sh os         yalnız rootfs-overlay (betikler, birimler, ayarlar)
#   os/debug/push.sh llama      llama.cpp (llama-server/cli/bench; önce os/debug/build-llama.sh)
#   os/debug/push.sh ssh [komut]  Pi'de kabuk/komut
#   os/debug/push.sh reset      tüm push'ları geri al (/data/dev silinir, yeniden açılır)
#
# Hedef: MAKEROS_HOST (varsayılan 10.55.0.1 = USB). Dosyalar debug imajının /usr katmanına yazılır.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HOST="${MAKEROS_HOST:-10.55.0.1}"
KEY="${MAKEROS_DEBUG_KEY:-$HOME/.ssh/id_ed25519_makeros}"
SSH=(ssh -i "$KEY" -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5 "root@$HOST")
what="${1:-all}"

case "$what" in
ssh) shift; exec "${SSH[@]}" "$@" ;;
reset) "${SSH[@]}" 'rm -rf /data/dev && systemctl reboot'; echo "sıfırlandı, Pi yeniden açılıyor"; exit 0 ;;
all | shell | makerd | os | llama) ;;
*) echo "kullanım: $0 [all|shell|makerd|os|llama|ssh|reset]" >&2; exit 2 ;;
esac

# Pi'de RTC yok; USB'de internet de yoksa saati 1970/eski kalır. Mac'in saatini ver (tar zaman damgaları, TLS).
"${SSH[@]}" "test -e /usr/lib/makeros/debug-image && date -s @$(date +%s) >/dev/null" || { echo "$HOST debug imajı değil ya da erişilemiyor" >&2; exit 1; }
[ "$what" = all ] || [ "$what" = makerd ] && "$ROOT/os/debug/build-makerd.sh"

t0=$(date +%s)
# Paket Docker'da hazırlanır (gzip'li kabuk dosyaları imajdakiyle aynı), Pi'de / altına açılır.
docker run --rm -v "$ROOT":/src:ro -v mk-target:/target:ro -v mk-llama:/llama:ro -e WHAT="$what" debian:trixie-slim bash -c '
set -e; R=/tmp/p; mkdir -p $R
if [ "$WHAT" = all ] || [ "$WHAT" = os ]; then
	(cd /src/os/board/rpi4/rootfs-overlay && tar cf - --exclude=.DS_Store usr etc) | tar -C $R -xf - --no-same-owner
	for f in $R/usr/lib/makeros/* $R/usr/bin/mos $R/usr/bin/docker; do [ -f "$f" ] && chmod 0755 "$f"; done
fi
if [ "$WHAT" = all ] || [ "$WHAT" = makerd ]; then
	mkdir -p $R/usr/bin; for b in makerd makerd-priv makerd-user makeros-ai; do [ -f /target/release/$b ] && install -m 0755 /target/release/$b $R/usr/bin/$b; done
	mkdir -p $R/usr/lib/systemd/system; for u in makerd.service makerd-priv.socket makerd-priv.service makerd-user@.service; do [ -f /src/makerd/dist/$u ] && install -m 0644 /src/makerd/dist/$u $R/usr/lib/systemd/system/$u; done
fi
if [ "$WHAT" = all ] || [ "$WHAT" = llama ]; then
	if [ -d /llama/bin ]; then
		mkdir -p $R/usr/bin $R/usr/share/licenses/llama.cpp; install -m 0755 /llama/bin/* $R/usr/bin/
		install -m 0644 /llama/LICENSE /llama/VERSION $R/usr/share/licenses/llama.cpp/
	fi
fi
if [ "$WHAT" = all ] || [ "$WHAT" = shell ]; then
	S=$R/usr/share/makeros/shell; mkdir -p $S
	(cd /src/shell && tar cf - --exclude=tools --exclude=node_modules --exclude=tests --exclude=test --exclude="*.md" --exclude="*/.*" .) | tar -C $S -xf - --no-same-owner
	find $S -type f \( -name "*.js" -o -name "*.mjs" -o -name "*.css" -o -name "*.html" -o -name "*.svg" -o -name "*.json" \) -size +1k -exec gzip -9 -n -k -f {} \;
	find $S -type d -exec chmod 0755 {} +; find $S -type f -exec chmod 0644 {} +
fi
cd $R && tar -czf - $(ls)' | "${SSH[@]}" "
set -e
[ '$what' = all ] || [ '$what' = shell ] && rm -rf /usr/share/makeros/shell
tar -C / -xzmf - --no-same-owner --no-overwrite-dir
systemctl daemon-reload
case '$what' in all|makerd) systemctl restart makerd-priv.socket makerd.service; systemctl try-restart makeros-ai.service ;; esac
"
echo "gönderildi ($what) → $HOST, $(( $(date +%s) - t0 )) sn. Pi ekranını yenilemek için: $0 ssh systemctl restart makeros-kiosk"
