#!/usr/bin/env bash
# MakerOS post-build: hedef kok dizinine (TARGET_DIR) son dokunuslar.
#   $1 = TARGET_DIR
set -euo pipefail

TARGET_DIR="${1:-${TARGET_DIR}}"
BOARD_DIR="$(cd "$(dirname "$0")" && pwd)"
VERSION="$(cat "${BR2_EXTERNAL_MAKEROS_PATH}/VERSION")"
VERSION_ID="${VERSION%%-*}"
BUILD_ID="$(date -u +%Y%m%d%H%M)"
DISK_SIG="4d4f5332"

# --- os-release ---------------------------------------------------------------
rm -f "${TARGET_DIR}/etc/os-release"
cat > "${TARGET_DIR}/usr/lib/os-release" <<EOF
NAME="MakerOS"
PRETTY_NAME="MakerOS ${VERSION}"
ID=makeros
VERSION="${VERSION}"
VERSION_ID=${VERSION_ID}
BUILD_ID=${BUILD_ID}
IMAGE_ID=makeros-rpi4
IMAGE_VERSION=${VERSION}
HOME_URL="https://github.com/ferhatozcelik/maker-os"
SUPPORT_URL="https://github.com/ferhatozcelik/maker-os/issues"
ANSI_COLOR="0;36"
EOF
ln -sf ../usr/lib/os-release "${TARGET_DIR}/etc/os-release"
echo "${VERSION}" > "${TARGET_DIR}/etc/makeros-version"

# --- Bagalama noktalari ---------------------------------------------------------
# /data (DATA bolumu) preinit tarafindan baglanir; digerleri fstab ile.
mkdir -p "${TARGET_DIR}/data" \
         "${TARGET_DIR}/opt/data" \
         "${TARGET_DIR}/boot/moscfg" \
         "${TARGET_DIR}/boot/a" \
         "${TARGET_DIR}/boot/b" \
         "${TARGET_DIR}/usr/local"

# --- fstab ----------------------------------------------------------------------
# Kok (EROFS) salt-okunur ve cekirdek tarafindan baglanir; '/' satiri yok ki
# systemd-remount-fs onu rw yapmaya calismasin.
cat > "${TARGET_DIR}/etc/fstab" <<EOF
# MakerOS fstab - kok EROFS (ro) cekirdek tarafindan baglanir.
# <aygit>                <nokta>       <tur>    <secenekler>                                                          <dump> <fsck>
PARTUUID=${DISK_SIG}-07   /data         ext4     noatime,lazytime,commit=60,errors=remount-ro                         0 0
overlay                  /var          overlay  lowerdir=/var,upperdir=/data/overlay/var/upper,workdir=/data/overlay/var/work,x-systemd.requires-mounts-for=/data  0 0
/data/home               /home         none     bind,x-systemd.requires-mounts-for=/data                              0 0
/data/opt                /opt/data     none     bind,x-systemd.requires-mounts-for=/data                              0 0
/data/usr-local          /usr/local    none     bind,x-systemd.requires-mounts-for=/data                              0 0
tmpfs                    /tmp          tmpfs    mode=1777,nosuid,nodev,noatime,size=25%                               0 0
PARTUUID=${DISK_SIG}-01   /boot/moscfg  vfat     noauto,x-systemd.automount,x-systemd.idle-timeout=30,noatime,fmask=0133,dmask=0022  0 0
PARTUUID=${DISK_SIG}-02   /boot/a       vfat     noauto,x-systemd.automount,x-systemd.idle-timeout=30,noatime,fmask=0133,dmask=0022  0 0
PARTUUID=${DISK_SIG}-03   /boot/b       vfat     noauto,x-systemd.automount,x-systemd.idle-timeout=30,noatime,fmask=0133,dmask=0022  0 0
EOF

# --- machine-id: ilk acilista (yazilabilir /etc ust katmaninda) uretilir ----------
: > "${TARGET_DIR}/etc/machine-id"

# --- Varsayilan hedef: grafik kiosk -------------------------------------------------
ln -sf ../../../../usr/lib/systemd/system/graphical.target \
	"${TARGET_DIR}/etc/systemd/system/default.target"

# --- PEP 668: sistem python'una pip ile kurulumu engelle (uv/venv kullanin) ----------
PYLIB="$(find "${TARGET_DIR}/usr/lib" -maxdepth 1 -type d -name 'python3.*' | head -n1)"
if [ -n "${PYLIB}" ]; then
	cat > "${PYLIB}/EXTERNALLY-MANAGED" <<'EOF'
[externally-managed]
Error=MakerOS sistem Python'u salt-okunurdur. Proje icin: uv venv && uv pip install <paket>
 (veya: python3 -m venv --system-site-packages ~/.venvs/proje)
EOF
fi

# --- Parola ozeti: SHA-512 (makerd-priv yescrypt dogrulayamaz) ------------------------
# Bkz. makerd/dist/makeros.login.defs.snippet
LOGIN_DEFS="${TARGET_DIR}/etc/login.defs"
touch "${LOGIN_DEFS}"
sed -i -e '/^[[:space:]]*ENCRYPT_METHOD[[:space:]]/d' \
       -e '/^[[:space:]]*SHA_CRYPT_M[IA][NX]_ROUNDS[[:space:]]/d' \
       -e '/^[[:space:]]*YESCRYPT_COST_FACTOR[[:space:]]/d' "${LOGIN_DEFS}"
cat >> "${LOGIN_DEFS}" <<'EOF'
# MakerOS: makerd-priv yescrypt dogrulayamaz; parolalar SHA-512 ile ozetlenir.
ENCRYPT_METHOD SHA512
SHA_CRYPT_MIN_ROUNDS 65536
EOF
# pam_unix parola modulu de sha512 kullansin (passwd/chpasswd ile degisim)
for f in "${TARGET_DIR}"/etc/pam.d/*; do
	[ -f "${f}" ] || continue
	sed -i -E '/^password[[:space:]].*pam_unix\.so/{/sha512/!s/pam_unix\.so/pam_unix.so sha512/; s/[[:space:]]yescrypt//}' "${f}"
done

# --- Gereksiz parcalar ---------------------------------------------------------------
rm -rf "${TARGET_DIR}/usr/share/man" "${TARGET_DIR}/usr/share/info" \
       "${TARGET_DIR}/usr/share/doc" "${TARGET_DIR}/usr/share/gtk-doc"

# --- SSH: ilk acilis sihirbazi tamamlanana kadar kapali (makerd-priv acar) ------------
rm -f "${TARGET_DIR}/etc/systemd/system/multi-user.target.wants/sshd.service"

# --- Calistirilabilir izinleri (overlay git/zip'ten bozulmus olabilir) ----------------
for f in "${TARGET_DIR}"/usr/lib/makeros/* "${TARGET_DIR}"/usr/bin/mos \
         "${TARGET_DIR}"/usr/bin/docker "${TARGET_DIR}"/usr/lib/rauc/rpi-tryboot-backend; do
	[ -f "${f}" ] && chmod 0755 "${f}"
done

echo "post-build: MakerOS ${VERSION} (${BUILD_ID})"
