#!/bin/sh
# Chromium paketi (yalnız Docker'da, debian:trixie arm64): Debian'ın chromium paketini ve glibc dışındaki
# tüm kütüphanelerini /opt/chromium ağacına toplar. Buildroot Chromium paketlemiyor; MakerOS bu ağacı
# os/package/chromium-bin ile imaja koyar. glibc imajınki kullanılır (Debian 2.41 < MakerOS glibc 2.42: uyumlu).
#   docker run --rm -v "$PWD/os/chromium":/s:ro -v "$PWD/os/out":/out debian:trixie sh /s/build-bundle.sh
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
# GPU: Mesa (v3d/vc4) + GBM/EGL; dosya seçici: GTK3; yazı tipi: sistemdeki fontconfig ile.
apt-get install -y -qq --no-install-recommends chromium chromium-sandbox libgl1-mesa-dri libegl-mesa0 libgbm1 \
  libgtk-3-0 libglib2.0-bin librsvg2-common shared-mime-info ca-certificates p11-kit-modules file xz-utils >/dev/null
VER=$(dpkg-query -W -f='${Version}' chromium)
O=/tmp/chromium; rm -rf $O; mkdir -p $O/lib $O/share
cp -a /usr/lib/chromium/. $O/
# Arayüz dilleri: yalnız Türkçe ve İngilizce (diğerleri ~40 MB).
find $O/locales -name "*.pak" ! -name "tr.pak" ! -name "en-US.pak" -delete 2>/dev/null || true
# glibc çekirdeği imajdan gelir; aynı süreçte iki glibc olamaz.
SKIP='^(ld-linux-aarch64\.so\.1|libc\.so\.6|libm\.so\.6|libpthread\.so\.0|libdl\.so\.2|librt\.so\.1|libresolv\.so\.2|libutil\.so\.1|libanl\.so\.1|libnss_.*|libmvec\.so\.1)$'
LIBDIR=/usr/lib/aarch64-linux-gnu
# Çalışma anında dlopen edilenler (ldd göstermez): GTK3, Mesa, GBM, EGL, VA, pixbuf yükleyicileri.
EXTRA="$LIBDIR/libgtk-3.so.0 $LIBDIR/libgdk-3.so.0 $LIBDIR/libEGL_mesa.so.0 $LIBDIR/libgbm.so.1 $LIBDIR/libGLESv2.so.2 $LIBDIR/libEGL.so.1 $LIBDIR/libGLX.so.0 $LIBDIR/libGL.so.1"
EXTRA="$EXTRA $(ls $LIBDIR/dri/*.so $LIBDIR/libgallium-*.so 2>/dev/null) $(ls $LIBDIR/gdk-pixbuf-2.0/2.10.0/loaders/*.so 2>/dev/null)"
# NSS modülleri (dlopen; ldd göstermez): yazılım belirteci, freebl, sertifika güveni (p11-kit).
EXTRA="$EXTRA $(ls $LIBDIR/libsoftokn3.so $LIBDIR/libfreebl3.so $LIBDIR/libfreeblpriv3.so $LIBDIR/libnssckbi.so $LIBDIR/libnssdbm3.so 2>/dev/null)"
todo=$(find $O -maxdepth 1 -type f \( -name chromium -o -name '*.so' -o -name chrome_crashpad_handler \); echo $EXTRA)
seen=" "
while [ -n "$todo" ]; do
  next=""
  for f in $todo; do
    for dep in $(ldd "$f" 2>/dev/null | awk '/=> \//{print $3}'); do
      b=$(basename "$dep")
      echo "$b" | grep -Eq "$SKIP" && continue
      case "$seen" in *" $b "*) continue;; esac
      seen="$seen$b "
      cp -L "$dep" $O/lib/"$b"
      next="$next $dep"
    done
  done
  todo=$next
done
for f in $EXTRA; do case "$f" in */dri/*|*/gdk-pixbuf-2.0/*) ;; *) cp -nL "$f" $O/lib/ 2>/dev/null || true;; esac; done
mkdir -p $O/lib/dri $O/lib/gdk-pixbuf/loaders
cp -L $LIBDIR/dri/*.so $O/lib/dri/ 2>/dev/null || true
cp -L $LIBDIR/libgallium-*.so $O/lib/ 2>/dev/null || true
cp -L $LIBDIR/gdk-pixbuf-2.0/2.10.0/loaders/*.so $O/lib/gdk-pixbuf/loaders/ 2>/dev/null || true
GDK_PIXBUF_MODULEDIR=$O/lib/gdk-pixbuf/loaders $LIBDIR/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders > $O/lib/gdk-pixbuf/loaders.cache
sed -i "s|$O|/opt/chromium|g" $O/lib/gdk-pixbuf/loaders.cache
cp -a /usr/share/glib-2.0 $O/share/ && glib-compile-schemas $O/share/glib-2.0/schemas
cp -a /usr/share/mime $O/share/ 2>/dev/null || true
# EGL/GLVND satıcı dosyası (Mesa)
mkdir -p $O/share/glvnd/egl_vendor.d && sed 's|libEGL_mesa.so.0|/opt/chromium/lib/libEGL_mesa.so.0|' /usr/share/glvnd/egl_vendor.d/50_mesa.json > $O/share/glvnd/egl_vendor.d/50_mesa.json
cp /usr/lib/chromium/chrome-sandbox $O/ 2>/dev/null || true
# Lisans/telif bildirimleri: Chromium ve pakete giren her kutuphanenin Debian copyright dosyasi
# (/opt/chromium/share/doc/<paket>/copyright). Yeniden dagitimda bildirimler korunur (NOTICE.md).
mkdir -p $O/share/doc
for f in /usr/lib/chromium/chromium $O/lib/*.so* $O/lib/dri/*.so; do
	b=$(basename "$f")
	pkg=$(dpkg -S "*/$b" 2>/dev/null | head -1 | cut -d: -f1)
	[ -n "$pkg" ] || continue
	[ -f "/usr/share/doc/$pkg/copyright" ] && mkdir -p "$O/share/doc/$pkg" && cp -n "/usr/share/doc/$pkg/copyright" "$O/share/doc/$pkg/" || true
done
cp -n /usr/share/doc/chromium/copyright $O/share/doc/chromium-copyright 2>/dev/null || true
echo "lisans dosyalari: $(ls $O/share/doc | wc -l) paket"
cat > $O/chromium-launcher <<'L'
#!/bin/sh
# MakerOS Chromium başlatıcı: Wayland (labwc), donanım hızlandırma (V3D), kullanıcı profili ~/.config/chromium.
D=/opt/chromium
export LD_LIBRARY_PATH="$D/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LIBGL_DRIVERS_PATH="$D/lib/dri" GBM_BACKENDS_PATH="$D/lib/dri" __EGL_VENDOR_LIBRARY_DIRS="$D/share/glvnd/egl_vendor.d"
export GDK_PIXBUF_MODULE_FILE="$D/lib/gdk-pixbuf/loaders.cache" GSETTINGS_SCHEMA_DIR="$D/share/glib-2.0/schemas" XDG_DATA_DIRS="$D/share:${XDG_DATA_DIRS:-/usr/share}"
# Pi 4 (4 GB): az süreç, düşük bellek; Wayland yerel.
exec "$D/chromium" --ozone-platform-hint=auto --enable-features=UseOzonePlatform \
  --renderer-process-limit=4 --disk-cache-size=104857600 --no-default-browser-check --no-first-run "$@"
L
chmod +x $O/chromium-launcher
du -sh $O; ls $O/lib | wc -l
T=/out/chromium-bundle-${VER}-arm64.tar.xz
tar -C /tmp -cf - chromium | xz -T0 -6 > $T
sha256sum $T | tee $T.sha256
echo "sürüm: $VER"
