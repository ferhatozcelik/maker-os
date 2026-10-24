#!/bin/bash
# Paket deposunu yeniler. Istediginiz zaman calistirin; build ve cihazlar
# Pardus'a veya baska bir dis kaynaga hic baglanmaz, yalnizca bu betik baglanir.
#
#  1. Pardus, Raspberry Pi, VS Code, Node.js, PyPI... kaynaklarindan imaj
#     sifirdan kurulur; kullanilan her paket ve dosya toplanir.
#  2. vendor/overrides/ ile bizim paket degisikliklerimiz uygulanir.
#  3. Depo GitHub Release'ine (on surum, "depo-YYYYMMDD-SSDD") yuklenir.
#  4. vendor/ altindaki kilitler guncellenir; neyin degistigi ozetlenir.
#
# Sonra: git diff vendor/  ->  inceleyin, commit edin; cihazlara gitmesi
# icin VERSION'i artirip push edin (GitHub Actions build + yayin).
#
# Kullanim: vendor/sync.sh            (topla + yukle)
#           vendor/sync.sh --yerel    (yalnizca topla; yukleme yok, deneme)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UPLOAD=1
[[ "${1:-}" == --yerel ]] && UPLOAD=0
REPO="${PIOS_GITHUB_REPO:-$(cd "$ROOT" && gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null || echo ferhatozcelik/maker-os)}"
OUT="$ROOT/image/out/depo"

if (( UPLOAD )); then
  gh auth status >/dev/null 2>&1 || { echo "GitHub'a giris yapin: gh auth login" >&2; exit 1; }
fi

echo "== 1/3 internetten kurulum ve toplama (01 -> 065) =="
PIOS_SYNC=1 PIOS_STOP_AFTER=065 PIOS_GITHUB_REPO="$REPO" "$ROOT/image/build-docker.sh" 01

ID="$(sed -n 's/^id=//p' "$OUT/vendor/depo.lock")"
[[ -n "$ID" ]] || { echo "HATA: depo kilidi uretilmedi" >&2; exit 1; }

echo "== 2/3 degisiklikler =="
if [[ -f "$ROOT/vendor/packages.lock" ]]; then
  diff <(cut -f1,2 "$ROOT/vendor/packages.lock") <(cut -f1,2 "$OUT/vendor/packages.lock") \
    | awk '/^</ {old[$2]=$3} /^>/ {new[$2]=$3}
           END {for (p in new) print (p in old ? "  guncellendi " p ": " old[p] " -> " new[p] : "  yeni        " p " " new[p]);
                for (p in old) if (!(p in new)) print "  kaldirildi  " p " " old[p]}' | sort || true
fi
cp "$OUT/vendor/"{packages.lock,versions.env,ai-requirements.txt,files.lock,depo.lock} "$ROOT/vendor/"
# OTA, ilk kilitten bu yana degisen paketleri cihazlara tasir (10-ota-bundle).
[[ -f "$ROOT/vendor/packages.base.lock" ]] || cp "$ROOT/vendor/packages.lock" "$ROOT/vendor/packages.base.lock"

if (( UPLOAD )); then
  echo "== 3/3 GitHub Release: $ID =="
  # On surum: OTA'nin "son yayin" denetimi bunu imaj yayini sanmaz.
  gh release create "$ID" --repo "$REPO" --prerelease --title "Paket deposu $ID" \
    --notes "MakerOS paket deposu (vendor/sync.sh). Build bu dosyalari vendor/depo.lock ile dogrulayarak kullanir; elle indirmeniz gerekmez." \
    "$OUT/$ID".tar.xz.part*
  echo
  echo "Depo yuklendi. Sonraki adim:"
  echo "  git add vendor && git commit -m 'Paket deposu $ID'"
  echo "  Cihazlara gondermek icin VERSION'i artirin ve push edin."
else
  echo "== 3/3 yerel deneme: yukleme yapilmadi =="
  echo "Yerel build bu depoyu kullanir: image/build-docker.sh 00"
fi
