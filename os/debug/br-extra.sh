#!/usr/bin/env bash
# Buildroot'ta (makeros-build birimi) derlenmiş ama taban imajda olmayan dosyaları os/out/extra-rootfs.tar'a
# toplar; remaster.sh bunu kök dosya sistemine ekler (var olan dosyaların üzerine yazmaz). Buildroot'u
# baştan derlemeden yeni paket denemek için:
#   ./os/build.sh make <paket...>   →   os/debug/br-extra.sh   →   os/debug/remaster.sh
# Taban kök dosya sistemi: mk-remaster birimindeki root/ (remaster.sh bir kez çalışmış olmalı).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$ROOT/os/out"
docker run --rm -v makeros-build:/b:ro -v mk-remaster:/r:ro -v "$ROOT/os/out":/out debian:trixie-slim bash -s <<'EOS'
set -euo pipefail
T=/b/output/target; R=/r/root; S=$(ls /b/output/host/bin/*-linux-gnu-strip | head -n1)
W=$(mktemp -d)
cd $T
# Geliştirme dosyaları, belgeler ve Python standart kitaplığının .py kopyaları (imajda .pyc var) hariç;
# yalnız tkinter Python dosyaları alınır.
find . \( -type f -o -type l \) \
	! -path './usr/include/*' ! -path './usr/share/man/*' ! -path './usr/share/doc/*' ! -path './usr/share/info/*' \
	! -path './usr/lib/pkgconfig/*' ! -path './usr/share/pkgconfig/*' ! -path './usr/lib/cmake/*' ! -name '*.a' ! -name '*.la' \
	! -path './usr/lib/tk8.6/demos/*' ! -path './usr/share/aclocal/*' ! -path './usr/share/util-macros/*' \
	! -path './THIS_IS_NOT_YOUR_ROOT_FILESYSTEM' \
	| while read -r f; do
		case "$f" in ./usr/lib/python3.*/*) case "$f" in */tkinter/*|*/lib-dynload/_tkinter.so) ;; *) continue ;; esac ;; esac
		[ -e "$R/$f" ] || [ -L "$R/$f" ] && continue
		echo "$f"
	done > $W/list
tar -C $T -cf $W/x.tar -T $W/list
mkdir $W/x && tar -C $W/x -xf $W/x.tar
# ELF olmayanlarda strip hata verip dosyaya dokunmaz.
find $W/x -type f \( -perm -u+x -o -name '*.so*' \) -exec "$S" --strip-unneeded {} + 2>/dev/null || true
tar -C $W/x -cf /out/extra-rootfs.tar .
echo "eklenecek dosya: $(wc -l < $W/list)  boyut: $(du -sh $W/x | cut -f1)"
EOS
