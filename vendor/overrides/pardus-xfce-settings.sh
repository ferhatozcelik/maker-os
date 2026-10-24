#!/bin/sh
# pardus-xfce-settings: zenity bagimliligi kaldirilir.
# zenity'yi yalnizca thunar-print (Thunar > "Dosya(lari) yazdir") kullanir;
# imajda yazici sistemi (CUPS) yok, eylem yalnizca "yazici bulunamadi" der.
# zenity gomulu WebKit'i ceker: zenity + libwebkit2gtk + libjavascriptcoregtk
# imajdan cikar (~140 MB). Yazdirma eylemi ve araci da kaldirilir.
# Kullanim (065-depo-harvest.sh cagirir): <orijinal.deb> <cikis-dizini>
set -eu
IN="$1"; OUT="$2"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
dpkg-deb -R "$IN" "$T/p"
sed -i -e '/^Depends:/ s/, *zenity\b//; /^Depends:/ s/\bzenity, *//' \
       -e '/^Version:/ s/$/+maker1/' "$T/p/DEBIAN/control"
grep -q '^Depends:.*zenity' "$T/p/DEBIAN/control" && { echo "zenity bagimliligi kaldirilamadi" >&2; exit 1; }
rm -f "$T/p/usr/bin/thunar-print"
sed -i '\| usr/bin/thunar-print$|d' "$T/p/DEBIAN/md5sums"
python3 - "$T/p/etc/xdg/pardus/Thunar/uca.xml" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
s2 = re.sub(r"\s*<action>(?:(?!</action>).)*thunar-print(?:(?!</action>).)*</action>", "", s, flags=re.S)
assert s2 != s, "thunar-print eylemi bulunamadi"
open(p, "w", encoding="utf-8").write(s2)
PY
# Degisen yapilandirma dosyasinin ozeti md5sums'ta yoktur (conffile); paket
# yeniden olusturulur, orijinali depodan cikar.
rm -f "$IN"
dpkg-deb -Zxz --root-owner-group -b "$T/p" "$OUT/" >/dev/null
