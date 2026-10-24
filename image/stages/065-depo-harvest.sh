#!/bin/sh
# Senkron modu (vendor/sync.sh): internetten yapilan build'de kullanilan her
# seyi kendi paket deposuna toplar. Build modunda hicbir sey yapmaz.
#   apt/            imajdaki her paketin .deb'i (tek surum) + apt dizini;
#                   vendor/overrides/ ile degistirilen paketler bizim surumumuz
#   files/          firmware, uv, Python arsivleri, Node.js, xterm.js, modeller
#   uv-cache/       AI ortaminin Python paketleri (uv onbellegi)
#   user-uv-cache.tar  MediaPipe ornekleri icin kullanici onbellegi
# Cikti: $PIOS_OUT/depo/ (Release'e yuklenecek parcalar + vendor/ kilitleri)
# ve $PIOS_DEPO (yerel build bunu dogrudan kullanir).
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root
require_rootfs

is_sync || exit 0

DEPO_ID="depo-$(date -u +%Y%m%d-%H%M)"
NEW="$PIOS_WORK/depo-yeni"
SYNC="$PIOS_WORK/sync"
rm -rf "$NEW"
mkdir -p "$NEW/apt/pool" "$NEW/files" "$SYNC/debs"

trap chroot_umount EXIT
chroot_mount

echo "== .deb dosyalari toplaniyor =="
# Elde olan her .deb: (paket surum mimari) -> dosya
INDEX="$SYNC/deb-index.txt"
: > "$INDEX"
for f in "$ROOTFS_DIR"/var/cache/apt/archives/*.deb "$SYNC"/debs/*.deb; do
  [ -f "$f" ] || continue
  printf '%s %s %s\t%s\n' "$(dpkg-deb -f "$f" Package)" "$(dpkg-deb -f "$f" Version)" "$(dpkg-deb -f "$f" Architecture)" "$f" >> "$INDEX"
done
in_chroot dpkg-query -W -f='${Package} ${Version} ${Architecture}\n' | sort -u > "$SYNC/installed.txt"
missing=0
while read -r pkg ver arch; do
  # Yer tutucu: build sirasinda yeniden uretilir (04).
  [ "$pkg" = raspberrypi-kernel ] && continue
  f="$(awk -F'\t' -v k="$pkg $ver $arch" '$1 == k {print $2; exit}' "$INDEX")"
  if [ -z "$f" ]; then
    rm -rf "$ROOTFS_DIR/tmp/dl" && mkdir -p "$ROOTFS_DIR/tmp/dl"
    if in_chroot sh -c "cd /tmp/dl && apt-get download -q '$pkg=$ver'" >/dev/null 2>&1; then
      f="$(ls "$ROOTFS_DIR"/tmp/dl/*.deb)"
      mv "$f" "$SYNC/debs/"
      f="$SYNC/debs/$(basename "$f")"
    else
      echo "   HATA: $pkg $ver bulunamadi" >&2
      missing=$((missing + 1))
      continue
    fi
  fi
  cp "$f" "$NEW/apt/pool/${pkg}_$(echo "$ver" | sed 's/^[0-9]*://')_$arch.deb"
done < "$SYNC/installed.txt"
rm -rf "$ROOTFS_DIR/tmp/dl"
[ "$missing" -eq 0 ] || { echo "HATA: $missing paketin .deb'i bulunamadi" >&2; exit 1; }
echo "   $(ls "$NEW/apt/pool" | wc -l) paket"

echo "== bizim paket degisikliklerimiz (vendor/overrides) =="
for o in "$VENDOR_DIR"/overrides/*.sh; do
  [ -f "$o" ] || continue
  pkg="$(basename "$o" .sh)"
  orig="$(ls "$NEW/apt/pool/${pkg}_"*.deb 2>/dev/null | head -n1)"
  [ -n "$orig" ] || { echo "   uyari: $pkg depoda yok, $o atlandi"; continue; }
  sh "$o" "$orig" "$NEW/apt/pool"
  echo "   $pkg: $(basename "$orig") -> $(ls "$NEW/apt/pool/${pkg}_"*.deb | xargs -n1 basename | tr '\n' ' ')"
done

echo "== apt dizini =="
( cd "$NEW/apt"
  mkdir -p "dists/$PARDUS_SUITE/main/binary-arm64"
  apt-ftparchive packages pool > "dists/$PARDUS_SUITE/main/binary-arm64/Packages" 2>/dev/null
  gzip -9kn "dists/$PARDUS_SUITE/main/binary-arm64/Packages"
  apt-ftparchive -o APT::FTPArchive::Release::Origin="MakerOS" \
    -o APT::FTPArchive::Release::Label="MakerOS" \
    -o APT::FTPArchive::Release::Suite="$PARDUS_SUITE" \
    -o APT::FTPArchive::Release::Codename="$PARDUS_SUITE" \
    -o APT::FTPArchive::Release::Architectures="arm64 all" \
    -o APT::FTPArchive::Release::Components=main \
    release "dists/$PARDUS_SUITE" > "$NEW/Release.tmp"
  mv "$NEW/Release.tmp" "dists/$PARDUS_SUITE/Release" )

echo "== dosyalar =="
sort -u "$SYNC/files.txt" > "$SYNC/files.uniq"
: > "$SYNC/files.lock"
while IFS="$(printf '\t')" read -r name url; do
  mkdir -p "$(dirname "$NEW/files/$name")"
  cp "$PIOS_CACHE/files/$name" "$NEW/files/$name"
  printf '%s  %s  %s\n' "$(sha256sum "$NEW/files/$name" | cut -d' ' -f1)" "$name" "$url" >> "$SYNC/files.lock"
done < "$SYNC/files.uniq"
echo "   $(wc -l < "$SYNC/files.lock") dosya"

echo "== Python onbellekleri =="
mv "$SYNC/uv-cache" "$NEW/uv-cache"
tar -C "$ROOTFS_DIR/home/$PIOS_USER/.cache" -cf "$NEW/user-uv-cache.tar" uv

echo "== kilitler (vendor/) =="
LOCKS="$PIOS_OUT/depo/vendor"
rm -rf "$PIOS_OUT/depo"
mkdir -p "$LOCKS"
for f in "$NEW"/apt/pool/*.deb; do
  printf '%s\t%s\t%s\n' "$(dpkg-deb -f "$f" Package)" "$(dpkg-deb -f "$f" Version)" "$(dpkg-deb -f "$f" Architecture)"
done | LC_ALL=C sort > "$LOCKS/packages.lock"
{ echo "# vendor/sync.sh tarafindan uretilir; elle duzenlemeyin."
  LC_ALL=C sort "$SYNC_VERSIONS"; } > "$LOCKS/versions.env"
cp "$SYNC/ai-requirements.txt" "$LOCKS/ai-requirements.txt"
{ echo "# sha256  depodaki-ad  kaynak (vendor/sync.sh uretir)"; cat "$SYNC/files.lock"; } > "$LOCKS/files.lock"
mkdir -p "$NEW/vendor" && cp "$LOCKS"/* "$NEW/vendor/"

echo "== paketleniyor ($DEPO_ID) =="
tar -C "$NEW" -cf - . | xz -T0 -6 | split -b 1900M -d -a 2 - "$PIOS_OUT/depo/$DEPO_ID.tar.xz.part"
{ echo "# vendor/sync.sh tarafindan uretilir; build bu parcalari indirip dogrular."
  echo "id=$DEPO_ID"
  echo "url=https://github.com/${PIOS_GITHUB_REPO:-ferhatozcelik/maker-os}/releases/download/$DEPO_ID"
  for p in "$PIOS_OUT/depo/$DEPO_ID".tar.xz.part*; do
    echo "part=$(sha256sum "$p" | cut -d' ' -f1) $(basename "$p")"
  done; } > "$LOCKS/depo.lock"

# Yerel build ayni depoyu indirmeden kullanir.
echo "$DEPO_ID" > "$NEW/.id"
rm -rf "$PIOS_DEPO"
mv "$NEW" "$PIOS_DEPO"
ls -lh "$PIOS_OUT/depo/"
echo "Depo toplandi: $DEPO_ID ($(du -sh "$PIOS_DEPO" | cut -f1)); kilitler: $LOCKS"
