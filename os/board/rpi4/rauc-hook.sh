#!/bin/sh
# MakerOS RAUC paket kancasi (paketin icinde gelir; manifest [hooks] + image.boot hooks=post-install).
# Paketteki acilis bolumu imaji A slotu icin hazirlanmistir (cmdline.txt: root=PARTUUID 4d4f5332-05,
# makeros.slot=a). B slotuna yazildiysa (boot.1) cmdline.txt B'ye cevrilir; yoksa yeni slot eski
# kok FS'i acar. Kanca yalnizca bu satiri duzeltir; imzali paketin geri kalanina dokunmaz.
set -eu
case "${1:-}" in
slot-post-install)
	[ "${RAUC_SLOT_CLASS:-}" = boot ] || exit 0
	case "${RAUC_SLOT_NAME:-}" in
	boot.0) part=05 slot=a ;;
	boot.1) part=06 slot=b ;;
	*) echo "rauc-hook: bilinmeyen acilis slotu: ${RAUC_SLOT_NAME:-}" >&2; exit 1 ;;
	esac
	f="${RAUC_SLOT_MOUNT_POINT:?}/cmdline.txt"
	[ -f "$f" ] || { echo "rauc-hook: $f yok" >&2; exit 1; }
	sed -i -e "s/4d4f5332-0[56]/4d4f5332-${part}/" -e "s/makeros\.slot=[ab]/makeros.slot=${slot}/" "$f"
	grep -q "makeros.slot=${slot}" "$f" || { echo "rauc-hook: cmdline.txt guncellenemedi" >&2; exit 1; }
	sync
	;;
esac
exit 0
