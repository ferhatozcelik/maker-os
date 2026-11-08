#!/usr/bin/env bash
# MakerOS debug imajı: Buildroot'u baştan derlemeden, hazır bir imajın (os/out/makeros-*-rpi4.img.xz)
# kök dosya sistemini açıp güncel makerd + kabuk + rootfs-overlay + debug-overlay ile yeniden paketler.
# Dakikalar sürer. Her şey Docker'da (birim: mk-remaster); ana makineye araç kurulmaz.
#
#   os/debug/remaster.sh [taban.img.xz]   → os/out/makeros-<sürüm>-rpi4-debug.img.xz
#
# Debug imajında: USB-C ağ kartı (Pi 10.55.0.1), anahtarla root SSH (~/.ssh/id_ed25519_makeros.pub),
# /usr üzerinde yazılabilir katman (os/debug/push.sh). Kullanıcı imajına GİRMEZ.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/os/out"
BASE="${1:-$(ls -t "$OUT"/makeros-*-rpi4.img.xz | grep -v -- -debug | head -n1)}"
VERSION="$(cat "$ROOT/os/VERSION")"
PUBKEY="${MAKEROS_DEBUG_PUBKEY:-$HOME/.ssh/id_ed25519_makeros.pub}"
[ -r "$PUBKEY" ] || { echo "SSH açık anahtarı yok: $PUBKEY" >&2; exit 1; }
echo "taban: $BASE"

# 1) makerd (aarch64, release). colima aarch64: yerel derleme; bookworm glibc 2.36 ≤ imajınki.
"$ROOT/os/debug/build-makerd.sh"

# 2) Yeniden paketle
docker run --rm -i \
	-v "$ROOT":/src:ro -v "$OUT":/out -v mk-remaster:/r -v mk-target:/target:ro \
	-e VERSION="$VERSION" -e BASE="/out/$(basename "$BASE")" -e PUBKEY="$(cat "$PUBKEY")" \
	debian:trixie-slim bash -s <<'EOS'
set -euo pipefail
apt-get update -qq >/dev/null
apt-get install -y -qq xz-utils erofs-utils mtools fdisk gzip >/dev/null 2>&1
cd /r
if [ ! -f base.img ] || [ "$BASE" -nt base.img ]; then
	echo "taban açılıyor…"; xz -T0 -dc "$BASE" > base.img
fi
part() { sfdisk -d base.img | awk -v p="base.img$1 " 'index($0,p)==1 { gsub(/[=,]/," "); for (i=1;i<=NF;i++) { if ($i=="start") s=$(i+1); if ($i=="size") z=$(i+1) } print s, z }'; }
read -r R_START R_SIZE < <(part 5)
read -r A_START _ < <(part 2)
read -r B_START _ < <(part 3)
dd if=base.img of=root.erofs bs=512 skip="$R_START" count="$R_SIZE" status=none
rm -rf root; fsck.erofs --extract=root --preserve root.erofs >/dev/null
R=root
put() { (cd "$1" && tar cf - --exclude=.DS_Store .) | tar -C "$R" -xf - --no-same-owner --no-overwrite-dir; }

# rootfs-overlay (depodaki güncel hâl) + depodan silinmiş eski dosyalar
put /src/os/board/rpi4/rootfs-overlay
rm -rf $R/usr/lib/makeros/browser-session $R/usr/lib/systemd/user/makeros-browser.service $R/usr/share/makeros/labwc-browser
rm -rf $R/usr/bin/wayvnc $R/usr/bin/wayvncctl $R/usr/lib/libneatvnc.so* $R/usr/lib/libaml.so* $R/usr/lib/systemd/user/makeros-vnc.service $R/usr/share/makeros/shell
# debug katmanı
put /src/os/board/rpi4/debug-overlay
for f in $R/usr/lib/makeros/* $R/usr/bin/mos $R/usr/bin/docker $R/usr/lib/rauc/rpi-tryboot-backend; do
	[ -f "$f" ] && [ "$(basename "$f")" != debug-image ] && chmod 0755 "$f"
done

# makerd (os/package/makerd ile aynı yerleşim)
for b in makerd makerd-priv makerd-user; do install -m 0755 /target/release/$b $R/usr/bin/$b; done
for u in makerd.service makerd-priv.socket makerd-priv.service makerd-user@.service; do
	install -m 0644 /src/makerd/dist/$u $R/usr/lib/systemd/system/$u
done
install -m 0644 /src/makerd/dist/makeros.tmpfiles.conf $R/usr/lib/tmpfiles.d/makerd.conf
install -m 0644 /src/os/package/makerd/50-makerd.rules $R/usr/share/polkit-1/rules.d/50-makerd.rules

# Kamera: rpicam-apps 1.7 libcamera 0.7 ile çöküyordu (os/patches/libcamera-apps). Taban imaj o yama olmadan
# derlendiyse, os/out/rpa ile yamalı derlenmiş kitaplık konur (jpeg8 ABI; Buildroot derlemesi gelince gereksiz).
if [ -f /out/rpa/out/librpicam_app.so.1.7.0 ]; then install -m 0755 /out/rpa/out/librpicam_app.so.1.7.0 $R/usr/lib/librpicam_app.so.1.7.0; fi

# Web kabuk (os/package/makeros-shell ile aynı)
S=$R/usr/share/makeros/shell
rm -rf $S; mkdir -p $S
(cd /src/shell && tar cf - --exclude=tools --exclude=node_modules --exclude=tests --exclude=test --exclude='*.md' --exclude='*/.*' .) | tar -C $S -xf - --no-same-owner
find $S -type f \( -name '*.js' -o -name '*.mjs' -o -name '*.css' -o -name '*.html' -o -name '*.svg' -o -name '*.json' \) -size +1k -exec gzip -9 -n -k -f {} \;
find $S -type d -exec chmod 0755 {} +; find $S -type f -exec chmod 0644 {} +

# SSH: debug'da hep açık, yalnız anahtarla root
grep -q '^Include /etc/ssh/sshd_config.d' $R/etc/ssh/sshd_config || sed -i '1i Include /etc/ssh/sshd_config.d/*.conf' $R/etc/ssh/sshd_config
install -d -m 0755 $R/etc/ssh/authorized_keys
printf '%s\n' "$PUBKEY" > $R/etc/ssh/authorized_keys/root; chmod 0644 $R/etc/ssh/authorized_keys/root
mkdir -p $R/etc/systemd/system/multi-user.target.wants $R/etc/systemd/system/sysinit.target.wants
ln -sf /usr/lib/systemd/system/sshd.service $R/etc/systemd/system/multi-user.target.wants/sshd.service
ln -sf /usr/lib/systemd/system/makeros-usb-gadget.service $R/etc/systemd/system/sysinit.target.wants/makeros-usb-gadget.service
ln -sf /usr/lib/systemd/system/makeros-usb-dhcp.service $R/etc/systemd/system/multi-user.target.wants/makeros-usb-dhcp.service
mkdir -p $R/etc/systemd/system/timers.target.wants
ln -sf /usr/lib/systemd/system/makeros-debug-log.timer $R/etc/systemd/system/timers.target.wants/makeros-debug-log.timer
echo "MakerOS $VERSION debug $(date -u +%Y%m%d%H%M)" > $R/usr/lib/makeros/debug-image

# EROFS (özgün imajla aynı biçim)
rm -f new.erofs
mkfs.erofs -zlz4hc,12 -Eztailpacking,fragments,dedupe -T"$(date +%s)" --quiet new.erofs $R
SZ=$(stat -c %s new.erofs); MAX=$((R_SIZE * 512))
[ "$SZ" -le "$MAX" ] || { echo "rootfs çok büyük: $SZ > $MAX" >&2; exit 1; }
echo "rootfs: $((SZ / 1048576)) MiB / $((MAX / 1048576)) MiB"

IMG=makeros-$VERSION-rpi4-debug.img
cp --sparse=always base.img $IMG
dd if=new.erofs of=$IMG bs=512 seek="$R_START" conv=notrunc,sparse status=none

# BOOT_A/B config.txt: USB-C cihaz modu (dwc2 peripheral)
for off in "$A_START" "$B_START"; do
	I="$IMG@@$((off * 512))"
	mtype -i "$I" ::config.txt > cfg.txt
	if ! grep -q 'dr_mode=peripheral' cfg.txt; then
		printf '\n[all]\n# MakerOS debug: USB-C -> USB ag karti (os/debug)\ndtoverlay=dwc2,dr_mode=peripheral\n' >> cfg.txt
		mcopy -o -i "$I" cfg.txt ::config.txt
	fi
done

echo "sıkıştırılıyor…"
xz -T0 -1 -f -c $IMG > /out/$IMG.xz
rm -f $IMG
(cd /out && sha256sum $IMG.xz > $IMG.xz.sha256)
echo "hazır: os/out/$IMG.xz"
EOS
