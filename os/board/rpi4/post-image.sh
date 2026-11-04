#!/usr/bin/env bash
# MakerOS post-image: BOOT_A/BOOT_B icerigini hazirla, genimage ile SD imaji
# olustur, xz ile sikistir.
#   $1 = BINARIES_DIR (Buildroot verir), ortamda BR2_EXTERNAL_MAKEROS_PATH vb.
set -euo pipefail

BOARD_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARIES_DIR="${1:-${BINARIES_DIR}}"
VERSION="$(cat "${BR2_EXTERNAL_MAKEROS_PATH}/VERSION")"
IMAGE_NAME="makeros-${VERSION}-rpi4.img"
DISK_SIG="4d4f5332"
GENIMAGE_CFG="${BINARIES_DIR}/genimage.cfg"
GENIMAGE_TMP="${BUILD_DIR}/genimage.tmp"

cd "${BINARIES_DIR}"

# --- Cekirdek: Pi firmware 'kernel8.img' adini bekler -------------------------
cp -f Image kernel8.img

# --- Slot basina cmdline.txt -------------------------------------------------
for slot in a b; do
	mkdir -p "slot-${slot}"
	case "${slot}" in
	a) part=05 ;;
	b) part=06 ;;
	esac
	sed -e "s/@ROOT_PARTUUID@/${DISK_SIG}-${part}/" \
	    -e "s/@SLOT@/${slot}/" \
	    "${BOARD_DIR}/cmdline.txt" | tr -d '\n' > "slot-${slot}/cmdline.txt"
	echo >> "slot-${slot}/cmdline.txt"
done

# --- MOSCFG icerigi ----------------------------------------------------------
mkdir -p moscfg
cp -f "${BOARD_DIR}/autoboot.txt" moscfg/autoboot.txt
cp -f "${BOARD_DIR}/wifi-example.txt" moscfg/wifi-example.txt

# --- Kullanici config parcasi (config.txt 'include' eder) -----------------------
cp -f "${BOARD_DIR}/makeros-user.txt" rpi-firmware/makeros-user.txt

# --- BOOT dosya listesi ------------------------------------------------------
FILES=()
for f in rpi-firmware/* bcm2711-*.dtb kernel8.img; do
	[ -e "${f}" ] || continue
	# Slot basina cmdline.txt asagida genimage ile eklenir; firmware paketininki cakisir.
	[ "${f}" = "rpi-firmware/cmdline.txt" ] && continue
	FILES+=("${f}")
done
BOOT_FILES=""
for f in "${FILES[@]}"; do
	BOOT_FILES="${BOOT_FILES}\t\t\t\"${f}\",\n"
done

sed -e "s|@BOOT_FILES@|${BOOT_FILES}|" \
    -e "s|@IMAGE_NAME@|${IMAGE_NAME}|" \
    "${BOARD_DIR}/genimage.cfg.in" > "${GENIMAGE_CFG}"

# --- genimage ----------------------------------------------------------------
trap 'rm -rf "${ROOTPATH_TMP}"' EXIT
ROOTPATH_TMP="$(mktemp -d)"
rm -rf "${GENIMAGE_TMP}"

genimage \
	--rootpath "${ROOTPATH_TMP}" \
	--tmppath "${GENIMAGE_TMP}" \
	--inputpath "${BINARIES_DIR}" \
	--outputpath "${BINARIES_DIR}" \
	--config "${GENIMAGE_CFG}"

# --- Sikistir (bos ROOT_B/DATA alanlari cok iyi sikisir) -------------------------
rm -f "${IMAGE_NAME}.xz"
xz -T0 -6 -k -f "${IMAGE_NAME}"
sha256sum "${IMAGE_NAME}.xz" > "${IMAGE_NAME}.xz.sha256"

# --- RAUC guncelleme paketi (imzali; cihazlar Ayarlar > Genel > Yazilim guncelleme ile kurar) --------
# Anahtar depoda DEGILDIR: yerelde os/keys/rauc-key.pem (gitignore), CI'da MAKEROS_RAUC_KEY sirri.
# Acik sertifika imajdaki anahtar halkasidir; RAUC kurulumda imzayi bununla dogrular.
RAUC_KEY="${MAKEROS_RAUC_KEY_FILE:-${BR2_EXTERNAL_MAKEROS_PATH}/keys/rauc-key.pem}"
RAUC_CERT="${BOARD_DIR}/rootfs-overlay/etc/rauc/keyring.pem"
BUNDLE="makeros-${VERSION}-rpi4.raucb"
rm -f "${BUNDLE}" "${BUNDLE}.sha256"
if [ -f "${RAUC_KEY}" ]; then
	BDIR="$(mktemp -d)"
	cp rootfs.erofs "${BDIR}/rootfs.erofs"
	# Acilis bolumu A slotu icin; B'ye yazilirsa kanca cmdline.txt'yi B'ye cevirir (rauc-hook.sh).
	cp boot-a.vfat "${BDIR}/boot.vfat"
	install -m 0755 "${BOARD_DIR}/rauc-hook.sh" "${BDIR}/hook.sh"
	cat > "${BDIR}/manifest.raucm" <<MANIFEST
[update]
compatible=makeros-rpi4
version=${VERSION}

[bundle]
format=verity

[hooks]
filename=hook.sh

[image.rootfs]
filename=rootfs.erofs

[image.boot]
filename=boot.vfat
hooks=post-install
MANIFEST
	"${HOST_DIR}/bin/rauc" bundle --cert="${RAUC_CERT}" --key="${RAUC_KEY}" "${BDIR}" "${BUNDLE}"
	"${HOST_DIR}/bin/rauc" info --keyring="${RAUC_CERT}" "${BUNDLE}" >/dev/null
	sha256sum "${BUNDLE}" > "${BUNDLE}.sha256"
	rm -rf "${BDIR}"
	echo "RAUC paketi: ${BINARIES_DIR}/${BUNDLE}"
else
	echo "post-image: imza anahtari yok (${RAUC_KEY}); guncelleme paketi uretilmedi" >&2
fi

echo "MakerOS imaji: ${BINARIES_DIR}/${IMAGE_NAME}.xz"
ls -la "${IMAGE_NAME}" "${IMAGE_NAME}.xz" rootfs.erofs
