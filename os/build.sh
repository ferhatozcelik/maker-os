#!/usr/bin/env bash
# MakerOS 2 imaj derleyicisi: Buildroot'u Docker icinde calistirir.
#
#   ./build.sh defconfig        makeros_rpi4_defconfig'i uygula
#   ./build.sh build            tam derleme -> os/out/makeros-<ver>-rpi4.img.xz
#   ./build.sh menuconfig       Buildroot menuconfig (interaktif)
#   ./build.sh linux-menuconfig cekirdek menuconfig (interaktif)
#   ./build.sh savedefconfig    .config -> configs/makeros_rpi4_defconfig
#   ./build.sh make <hedef...>  serbest make hedefi (orn. makerd-rebuild)
#   ./build.sh shell            derleyici kapta bash
#   ./build.sh clean            output/ dizinini sil (dl/ ve ccache kalir)
#   ./build.sh clean --all      Docker volume'unu tamamen sil
#
# Ortam degiskenleri:
#   MAKEROS_VOLUME   Docker volume adi (varsayilan: makeros-build) ya da mutlak klasor yolu (CI)
#   MAKEROS_JOBS     derleme paralelligi (varsayilan: 0 = cpu+1)
#   MAKEROS_HEAVY_JOBS agir C++ paketleri icin paralellik (varsayilan: 4;
#                    OOM olursa 2 ve 1 ile otomatik yeniden denenir)
#   MAKEROS_HEAVY_PKGS once dusuk paralellikle derlenen paketler
#                    (varsayilan: "wpewebkit"; 10 GB VM'de -j7 cc1plus'i OOM ile oldurur)
#   MAKEROS_DEFCONFIG defconfig adi (varsayilan: makeros_rpi4_defconfig)
#
# Not: macOS'taki /bin/bash 3.2 ile uyumlu tutulmustur.
set -euo pipefail

# --- Sabitler -----------------------------------------------------------------
BR_VERSION="2026.02.3"
BR_SHA256="5a59e7501b0b4ec52c41f4bfa79412320e0b37eae5f719605a258e8d0c6fc7fb"
BR_URL="https://buildroot.org/downloads/buildroot-${BR_VERSION}.tar.xz"

OS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${OS_DIR}/.." && pwd)"
VOLUME="${MAKEROS_VOLUME:-makeros-build}"
DEFCONFIG="${MAKEROS_DEFCONFIG:-makeros_rpi4_defconfig}"
JOBS="${MAKEROS_JOBS:-0}"
HEAVY_JOBS="${MAKEROS_HEAVY_JOBS:-4}"
HEAVY_PKGS="${MAKEROS_HEAVY_PKGS:-wpewebkit}"
IMAGE_TAG="makeros-builder:br${BR_VERSION}"

# Kap icindeki yollar
C_SRC=/src
C_EXT=/src/os
C_BUILD=/build
C_BR="${C_BUILD}/buildroot-${BR_VERSION}"
C_OUT="${C_BUILD}/output"
C_DL="${C_BUILD}/dl"
C_CCACHE="${C_BUILD}/ccache"

die() { echo "build.sh: $*" >&2; exit 1; }

# =============================================================================
# Kap ICI: bu betik /src/os/build.sh olarak --inside ile cagrilir.
# =============================================================================
inside_prepare() {
	mkdir -p "${C_DL}" "${C_CCACHE}" "${C_OUT}"
	local tarball="${C_DL}/buildroot-${BR_VERSION}.tar.xz"
	if [ ! -d "${C_BR}" ]; then
		if [ ! -f "${tarball}" ]; then
			echo ">> Buildroot ${BR_VERSION} indiriliyor"
			curl -fL --retry 3 -o "${tarball}.part" "${BR_URL}"
			mv "${tarball}.part" "${tarball}"
		fi
		echo ">> sha256 dogrulaniyor"
		echo "${BR_SHA256}  ${tarball}" | sha256sum -c - \
			|| { rm -f "${tarball}"; die "Buildroot tarball sha256 uyusmuyor"; }
		tar -C "${C_BUILD}" -xf "${tarball}"
	fi
}

br_make() {
	BR2_DL_DIR="${C_DL}" BR2_CCACHE_DIR="${C_CCACHE}" \
		make -C "${C_BR}" O="${C_OUT}" BR2_EXTERNAL="${C_EXT}" \
		BR2_JLEVEL="${BR_JOBS:-${JOBS}}" "$@"
}

# Bellek yiyen paketleri (wpewebkit ~1,5 GB/cc1plus) once sinirli paralellikle
# derle; zaten derlendiyse make aninda doner.
build_heavy() {
	local p
	for p in ${HEAVY_PKGS}; do
		grep -q "^BR2_PACKAGE_$(echo "${p}" | tr 'a-z-' 'A-Z_')=y" "${C_OUT}/.config" || continue
		local j ok=0
		# Bagimliliklar (toolchain dahil) tam paralellikle; yalnizca paketin
		# kendisi sinirli -j ile derlenir.
		br_make "${p}-depends"
		for j in ${HEAVY_JOBS} 2 1; do
			echo ">> ${p}: BR2_JLEVEL=${j} ile derleniyor"
			if BR_JOBS="${j}" br_make "${p}"; then ok=1; break; fi
			echo ">> ${p} -j${j} ile basarisiz (OOM?); daha dusuk paralellikle yeniden"
		done
		[ "${ok}" = 1 ] || die "${p} derlenemedi"
	done
}

inside_copy_image() {
	local ver img
	ver="$(cat "${C_EXT}/VERSION")"
	img="${C_OUT}/images/makeros-${ver}-rpi4.img.xz"
	[ -f "${img}" ] || die "imaj bulunamadi: ${img}"
	mkdir -p "${C_EXT}/out"
	cp -f "${img}" "${C_EXT}/out/"
	[ -f "${img}.sha256" ] && cp -f "${img}.sha256" "${C_EXT}/out/"
	echo ">> imaj: os/out/$(basename "${img}")"
	# Imzali guncelleme paketi (anahtar varsa post-image.sh uretir)
	local bundle="${C_OUT}/images/makeros-${ver}-rpi4.raucb"
	if [ -f "${bundle}" ]; then
		cp -f "${bundle}" "${bundle}.sha256" "${C_EXT}/out/"
		echo ">> guncelleme paketi: os/out/$(basename "${bundle}")"
	fi
	ls -la "${C_OUT}/images/"
}

inside_main() {
	local cmd="${1:-build}"; shift || true
	inside_prepare
	case "${cmd}" in
	defconfig)
		br_make "${DEFCONFIG}"
		;;
	olddefconfig|menuconfig|linux-menuconfig|busybox-menuconfig|show-info|legal-info|source)
		[ -f "${C_OUT}/.config" ] || br_make "${DEFCONFIG}"
		br_make "${cmd}"
		;;
	savedefconfig)
		# Kaynak agac salt-okunur baglanir; sonucu out/ altina yazariz.
		br_make savedefconfig BR2_DEFCONFIG="${C_EXT}/out/${DEFCONFIG}"
		echo ">> os/out/${DEFCONFIG} yazildi (configs/ ile karsilastirin)"
		;;
	build)
		# defconfig her derlemede yeniden uygulanir: eski .config'te kalan secenekler
		# (or. yeni eklenen paketler) sessizce eksik kalmasin. Derlenmis paketler korunur.
		br_make "${DEFCONFIG}"
		build_heavy
		br_make
		inside_copy_image
		;;
	make)
		[ -f "${C_OUT}/.config" ] || br_make "${DEFCONFIG}"
		br_make "$@"
		;;
	clean)
		echo ">> ${C_OUT} siliniyor"
		rm -rf "${C_OUT}"
		;;
	shell)
		cd "${C_OUT}"
		exec bash
		;;
	*)
		die "bilinmeyen komut: ${cmd}"
		;;
	esac
}

if [ "${1:-}" = "--inside" ]; then
	shift
	inside_main "$@"
	exit $?
fi

# =============================================================================
# Host (macOS/Linux): Docker imajini hazirla ve komutu kapta calistir.
# =============================================================================
command -v docker >/dev/null 2>&1 || die "docker bulunamadi (colima start?)"
docker info >/dev/null 2>&1 || die "Docker daemon calismiyor (colima start?)"

cmd="${1:-build}"

if [ "${cmd}" = "clean" ] && [ "${2:-}" = "--all" ]; then
	echo ">> Docker volume '${VOLUME}' siliniyor (dl/ ve ccache dahil)"
	docker volume rm -f "${VOLUME}"
	exit 0
fi

echo ">> derleyici imaji: ${IMAGE_TAG}"
docker build -q -t "${IMAGE_TAG}" "${OS_DIR}/docker" >/dev/null
# MAKEROS_VOLUME mutlak bir yolsa (CI: onbellege alinan dizin) klasor olarak baglanir.
case "${VOLUME}" in
/*) mkdir -p "${VOLUME}" ;;
*) docker volume inspect "${VOLUME}" >/dev/null 2>&1 || docker volume create "${VOLUME}" >/dev/null ;;
esac

mkdir -p "${OS_DIR}/out" && chmod 777 "${OS_DIR}/out" 2>/dev/null || true

TTY_ARGS=""
if [ -t 0 ] && [ -t 1 ]; then
	TTY_ARGS="-it"
fi
case "${cmd}" in
menuconfig|linux-menuconfig|busybox-menuconfig|shell)
	[ -n "${TTY_ARGS}" ] || die "'${cmd}' interaktif terminal ister"
	;;
esac

# Depo salt-okunur baglanir (makerd/ ve shell/ 'local' kaynak olarak okunur);
# yalnizca os/out yazilabilir. Buyuk derleme dizinleri volume'dadir.
# shellcheck disable=SC2086
exec docker run --rm ${TTY_ARGS} \
	--name "makeros-build-$$" \
	-e MAKEROS_JOBS="${JOBS}" \
	-e MAKEROS_HEAVY_JOBS="${HEAVY_JOBS}" \
	-e MAKEROS_HEAVY_PKGS="${HEAVY_PKGS}" \
	-e MAKEROS_DEFCONFIG="${DEFCONFIG}" \
	-v "${VOLUME}:${C_BUILD}" \
	-v "${REPO_DIR}:${C_SRC}:ro" \
	-v "${OS_DIR}/out:${C_EXT}/out" \
	"${IMAGE_TAG}" \
	bash "${C_EXT}/build.sh" --inside "$@"
