# Stage betiklerinin ortak degiskenleri ve yardimcilari. Her stage
# betigi bu dosyayi ". lib/common.sh" ile yukler.

IMAGE_DIR="${IMAGE_DIR:-$(dirname "$(dirname "$(readlink -f "$0")")")}"
PIN_FILE="$IMAGE_DIR/pins/base.env"
OVERLAY_DIR="$IMAGE_DIR/overlay"

# Rootfs host (macOS) bind-mount'u uzerinde degil, Linux dosya sisteminde
# uretilmelidir: aygit dugumleri, sahiplik ve buyuk/kucuk harf duyarliligi.
PIOS_WORK="${PIOS_WORK:-$IMAGE_DIR/out/work}"
PIOS_OUT="${PIOS_OUT:-$IMAGE_DIR/out}"
PIOS_CACHE="${PIOS_CACHE:-$PIOS_WORK/cache}"
ROOTFS_DIR="${ROOTFS_DIR:-$PIOS_WORK/rootfs}"

PIOS_HOSTNAME="${PIOS_HOSTNAME:-maker-os}"
PIOS_USER="${PIOS_USER:-maker}"
PIOS_PASSWORD="${PIOS_PASSWORD:-maker}"
# Masaustu cozunurlugu (monitor olmasa da). 1280x720: uzak ekran (VNC) Pi'de
# kararli 30 FPS verir; 1080p 2.25 kat piksel demek. Merkez > Ekran'dan degisir.
PIOS_RESOLUTION="${PIOS_RESOLUTION:-1920x1080}"

# --- Paket deposu (vendor/) ----------------------------------------------------
# Build internete cikmaz: her paket ve dosya kendi deponuzdan (vendor/depo.lock
# ile kilitli GitHub Release) gelir. Senkron modu (PIOS_SYNC=1, vendor/sync.sh)
# ayni asamalari internetten calistirir ve kullanilan her seyi depoya toplar.
REPO_DIR="$(dirname "$IMAGE_DIR")"
VENDOR_DIR="$REPO_DIR/vendor"
PIOS_DEPO="${PIOS_DEPO:-$PIOS_WORK/depo}"
PIOS_SYNC="${PIOS_SYNC:-0}"
# Senkron modunda cozulen surumler (build modunda vendor/versions.env okunur).
SYNC_VERSIONS="$PIOS_WORK/sync/versions.env"

is_sync() { [ "$PIOS_SYNC" = 1 ]; }

# Surum kilidi: depo_version KEY "internetten cozen komut"
# Senkron: komut calisir, sonuc kaydedilir. Build: vendor/versions.env'den okunur.
depo_version() {
  key="$1"
  if is_sync; then
    val="$(sh -c "$2")"
    [ -n "$val" ] || { echo "HATA: $key cozulemedi" >&2; exit 1; }
    mkdir -p "$(dirname "$SYNC_VERSIONS")"
    touch "$SYNC_VERSIONS"
    grep -v "^$key=" "$SYNC_VERSIONS" > "$SYNC_VERSIONS.tmp" || true
    echo "$key=$val" >> "$SYNC_VERSIONS.tmp"
    mv "$SYNC_VERSIONS.tmp" "$SYNC_VERSIONS"
  else
    val="$(sed -n "s/^$key=//p" "$VENDOR_DIR/versions.env" 2>/dev/null)"
    [ -n "$val" ] || { echo "HATA: vendor/versions.env icinde $key yok - vendor/sync.sh calistirin" >&2; exit 1; }
  fi
  echo "$val"
}

# Depodan dosya: depo_file <depodaki-ad> <url> <hedef>
# Senkron: url'den indirir (onbellekte tutar) ve depoya alinmak uzere kaydeder.
# Build: depo/files/<ad> kopyalanir; vendor/files.lock'taki sha256 ile dogrulanir.
depo_file() {
  name="$1"; url="$2"; dest="$3"
  if is_sync; then
    src="$PIOS_CACHE/files/$name"
    if [ ! -s "$src" ]; then
      mkdir -p "$(dirname "$src")"
      curl -fL --retry 3 --max-time 1800 "$url" -o "$src.part"
      mv "$src.part" "$src"
    fi
    mkdir -p "$PIOS_WORK/sync"
    printf '%s\t%s\n' "$name" "$url" >> "$PIOS_WORK/sync/files.txt"
  else
    src="$PIOS_DEPO/files/$name"
    [ -f "$src" ] || { echo "HATA: depoda yok: files/$name - vendor/sync.sh ile depoyu yenileyin" >&2; exit 1; }
    want="$(awk -v n="$name" '$2 == n {print $1}' "$VENDOR_DIR/files.lock")"
    [ -n "$want" ] && echo "$want  $src" | sha256sum -c - >/dev/null \
      || { echo "HATA: files/$name sha256 vendor/files.lock ile uyusmuyor" >&2; exit 1; }
  fi
  [ "$dest" = "-" ] && { cat "$src"; return; }
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
}

# Senkron modunda ag islemi: takilan indirme (yanitsiz baglanti) sonsuza dek
# beklemesin. retry_net <saniye> <komut...>: 3 deneme, her biri sure sinirli.
retry_net() {
  limit="$1"; shift
  if [ "$1" = in_chroot ]; then  # timeout kabuk fonksiyonu calistiramaz
    shift
    set -- chroot "$ROOTFS_DIR" /usr/bin/env -i \
      PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin HOME=/root LANG=C.UTF-8 \
      DEBIAN_FRONTEND=noninteractive "$@"
  fi
  for attempt in 1 2 3; do
    timeout --kill-after=30 "$limit" "$@" && return 0
    echo "   uyari: deneme $attempt basarisiz ($*)" >&2
    sleep 10
  done
  return 1
}

# chroot icindeki apt kaynaklari: build -> yalnizca depo; senkron -> Pardus aynalari.
write_build_sources() {
  if is_sync; then
    cat > "$ROOTFS_DIR/etc/apt/sources.list" <<CFG
deb $PARDUS_BUILD_MIRROR $PARDUS_SUITE $PARDUS_COMPONENTS
deb $PARDUS_BUILD_SECURITY_MIRROR $PARDUS_SECURITY_SUITE $PARDUS_COMPONENTS
CFG
  else
    echo "deb [trusted=yes] file:/depo/apt $PARDUS_SUITE main" > "$ROOTFS_DIR/etc/apt/sources.list"
  fi
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "HATA: bu adim root yetkisi gerektirir" >&2
    exit 1
  fi
}

require_rootfs() {
  if [ ! -f "$ROOTFS_DIR/etc/debian_version" ]; then
    echo "HATA: $ROOTFS_DIR bir rootfs gibi gorunmuyor - once 02-build-rootfs.sh calistirilmali" >&2
    exit 1
  fi
}

load_pins() {
  if [ ! -f "$PIN_FILE" ]; then
    echo "HATA: $PIN_FILE yok - once 01-pin-base.sh calistirilmali" >&2
    exit 1
  fi
  . "$PIN_FILE"
}

# chroot icin sanal dosya sistemlerini baglar; cikista trap ile cozulur.
chroot_mount() {
  mount -t proc proc "$ROOTFS_DIR/proc"
  mount -t sysfs sysfs "$ROOTFS_DIR/sys"
  mount -o bind /dev "$ROOTFS_DIR/dev"
  mount -t devpts devpts "$ROOTFS_DIR/dev/pts" 2>/dev/null || true
  rm -f "$ROOTFS_DIR/etc/resolv.conf"
  cp /etc/resolv.conf "$ROOTFS_DIR/etc/resolv.conf"
  # Paket kurulumu sirasinda servisler chroot icinde baslatilmasin.
  printf '#!/bin/sh\nexit 101\n' > "$ROOTFS_DIR/usr/sbin/policy-rc.d"
  chmod +x "$ROOTFS_DIR/usr/sbin/policy-rc.d"
  # Build modunda paket deposu chroot icinde /depo olarak gorunur.
  if ! is_sync && [ -d "$PIOS_DEPO/apt" ]; then
    mkdir -p "$ROOTFS_DIR/depo"
    mount --bind "$PIOS_DEPO" "$ROOTFS_DIR/depo"
  fi
}

chroot_umount() {
  umount "$ROOTFS_DIR/depo" 2>/dev/null && rmdir "$ROOTFS_DIR/depo" 2>/dev/null || true
  rm -f "$ROOTFS_DIR/usr/sbin/policy-rc.d"
  umount "$ROOTFS_DIR/dev/pts" 2>/dev/null || true
  umount "$ROOTFS_DIR/dev" 2>/dev/null || true
  umount "$ROOTFS_DIR/sys" 2>/dev/null || true
  umount "$ROOTFS_DIR/proc" 2>/dev/null || true
}

in_chroot() {
  chroot "$ROOTFS_DIR" /usr/bin/env -i \
    PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin HOME=/root LANG=C.UTF-8 \
    DEBIAN_FRONTEND=noninteractive "$@"
}
