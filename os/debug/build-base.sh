#!/usr/bin/env bash
# MakerOS taban imajı yerelde (bu bilgisayarda) derlenir ve GitHub'daki "taban-rpi4" ön sürümüne yüklenir.
# Buildroot GitHub Actions'ta derlenmez: Actions yalnız bu tabanı yeniden paketler (makerd + kabuk + overlay)
# ve MAKEROS_RAUC_KEY sırrıyla imzalayıp v<sürüm> olarak yayınlar (release.yml "remaster").
#
#   ./os/debug/build-base.sh            Chromium paketi (yoksa) + Buildroot + taban-rpi4'e yükle
#   ./os/debug/build-base.sh --no-upload yalnız derle
#
# Gerekenler: Docker (Linux/WSL2/colima; disk ≥ 100 GB), gh (giriş yapılmış). arm64 ya da x86_64 olabilir:
# Buildroot kendi çapraz derleyicisini kurar. Windows'ta depoyu WSL2'nin kendi diskine (ör. ~/maker-os)
# klonlayın, /mnt/c altına değil (CRLF satır sonları ve yavaş dosya sistemi derlemeyi bozar).
# Çekirdek, defconfig ya da os/package değişince çalıştırın; ardından os/VERSION'ı artırıp main'e push edin.
set -euo pipefail
OS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${OS_DIR}/.."
UPLOAD=1; [ "${1:-}" = "--no-upload" ] && UPLOAD=0
V="$(tr -d ' \r\n' < os/VERSION)"

mkdir -p os/out && chmod 777 os/out
if ! ls os/out/chromium-bundle-*-arm64.tar.xz >/dev/null 2>&1; then
	echo ">> Chromium paketi (Debian trixie arm64)"
	# Debian'ın arm64 Chromium'u gerekir; x86_64 makinede qemu ile çalışır (Docker Desktop'ta hazır).
	docker run --rm --platform linux/arm64 -v "$PWD/os/chromium":/s:ro -v "$PWD/os/out":/out debian:trixie sh /s/build-bundle.sh
fi

echo ">> Buildroot (taban imaj, imzasız)"
# Derleme klasörü (Docker volume makeros-build) silinmez: yalnız değişen paketler yeniden derlenir.
# Paralellik Docker'ın gördüğü çekirdek ve belleğe göre: wpewebkit derleyici süreci başına ~3 GB ister
# (12 GB'ta -j4 bellek yetmedi). Güçlü makinede (ör. 32 çekirdek / 64 GB) tam derleme ~1 saat.
CPUS="$(docker info --format '{{.NCPU}}')"; MEM_GB=$(( $(docker info --format '{{.MemTotal}}') / 1073741824 ))
HJ=$(( MEM_GB / 3 )); [ "${HJ}" -gt "${CPUS}" ] && HJ="${CPUS}"; [ "${HJ}" -lt 1 ] && HJ=1
echo ">> Docker: ${CPUS} çekirdek, ${MEM_GB} GB → wpewebkit -j${MAKEROS_HEAVY_JOBS:-${HJ}}"
MAKEROS_HEAVY_JOBS="${MAKEROS_HEAVY_JOBS:-${HJ}}" ./os/build.sh build </dev/null
IMG="os/out/makeros-${V}-rpi4.img.xz"
test -f "${IMG}"
(cd os/out && { command -v sha256sum >/dev/null && sha256sum "$(basename "${IMG}")" || shasum -a 256 "$(basename "${IMG}")"; } > "$(basename "${IMG}").sha256")

[ "${UPLOAD}" = 1 ] || { echo ">> hazır: ${IMG}"; exit 0; }
echo ">> taban-rpi4 ön sürümüne yükleniyor"
gh release view taban-rpi4 >/dev/null 2>&1 || gh release create taban-rpi4 --prerelease --title "MakerOS taban imajı (Buildroot)" \
	--notes "Yeniden paketleme (remaster) için yerelde Buildroot ile derlenen temiz imaj. Kurulum için kullanmayın; sürümler: Releases."
for a in $(gh release view taban-rpi4 --json assets --jq '.assets[].name'); do gh release delete-asset taban-rpi4 "$a" -y; done
gh release upload taban-rpi4 "${IMG}" "${IMG}.sha256"
echo ">> tamam: taban-rpi4 ← $(basename "${IMG}")"
