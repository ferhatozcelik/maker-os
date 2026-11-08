#!/bin/sh
# MakerOS microSD yazıcı — macOS ve Linux
# MakerOS microSD flasher — macOS and Linux
#
#   sh -c "$(curl -fsSL https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.sh)"
#
# Seçenekler / options:
#   -i, --image <dosya>     yerel .img ya da .img.xz kullan (indirme yapılmaz)
#   -d, --disk <aygıt>      hedef disk (ör. /dev/disk4, /dev/sdb); verilmezse listeden seçilir
#   -v, --version <sürüm>   belirli bir sürüm (ör. v1.0.0); varsayılan en son sürüm
#       --no-wifi           Wi-Fi sorusunu atla
#
# Betik yalnız harici / çıkarılabilir diskleri listeler ve yazmadan önce disk adını
# elle yazmanızı ister. Seçilen diskteki her şey silinir.
set -eu

REPO="ferhatozcelik/maker-os"
OS="$(uname -s)"
case "${LANG:-}${LC_ALL:-}" in tr*|*tr_TR*) TR=1 ;; *) TR=0 ;; esac

say()  { if [ "$TR" = 1 ]; then printf '%s\n' "$1"; else printf '%s\n' "$2"; fi; }
step() { printf '\n\033[1;33m==>\033[0m \033[1m%s\033[0m\n' "$(say "$1" "$2")"; }
die()  { printf '\033[1;31m%s\033[0m %s\n' "$(say "Hata:" "Error:")" "$(say "$1" "$2")" >&2; exit 1; }
ask()  { printf '%s ' "$(say "$1" "$2")" >/dev/tty; IFS= read -r REPLY </dev/tty || REPLY=""; }

IMAGE="" DISK="" VERSION="latest" WIFI=1
while [ $# -gt 0 ]; do
	case "$1" in
	-i|--image) IMAGE="${2:?}"; shift 2 ;;
	-d|--disk) DISK="${2:?}"; shift 2 ;;
	-v|--version) VERSION="${2:?}"; shift 2 ;;
	--no-wifi) WIFI=0; shift ;;
	-h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
	*) die "bilinmeyen seçenek: $1" "unknown option: $1" ;;
	esac
done

case "$OS" in
Darwin|Linux) ;;
*) die "Bu betik macOS ve Linux içindir. Windows'ta flash.ps1 kullanın." "This script is for macOS and Linux. On Windows use flash.ps1." ;;
esac
[ -r /dev/tty ] || die "Etkileşimli bir terminal gerekli." "An interactive terminal is required."

printf '\n\033[1;38;5;208m  MakerOS\033[0m — %s\n' "$(say "microSD yazıcı" "microSD flasher")"

# --- 1. İmaj -------------------------------------------------------------------------
sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

if [ -z "$IMAGE" ]; then
	step "En son sürüm aranıyor" "Looking up the release"
	if [ "$VERSION" = latest ]; then api="https://api.github.com/repos/$REPO/releases/latest"; else api="https://api.github.com/repos/$REPO/releases/tags/$VERSION"; fi
	json="$(curl -fsSL "$api")" || die "GitHub'a ulaşılamadı." "Could not reach GitHub."
	url="$(printf '%s' "$json" | grep -o '"browser_download_url": *"[^"]*\.img\.xz"' | head -n1 | sed 's/.*"\(http[^"]*\)"$/\1/')"
	[ -n "$url" ] || die "Sürümde .img.xz dosyası yok." "No .img.xz asset in the release."
	name="${url##*/}"
	dir="${XDG_CACHE_HOME:-$HOME/.cache}/makeros"; [ "$OS" = Darwin ] && dir="$HOME/Library/Caches/makeros"
	mkdir -p "$dir"
	IMAGE="$dir/$name"
	say "Sürüm: $name" "Release: $name"
	expected="$(curl -fsSL "$url.sha256" | cut -d' ' -f1)"
	if [ -f "$IMAGE" ] && [ "$(sha256 "$IMAGE")" = "$expected" ]; then
		say "Önceden indirilmiş imaj kullanılıyor: $IMAGE" "Using previously downloaded image: $IMAGE"
	else
		step "İmaj indiriliyor" "Downloading image"
		curl -fL --progress-bar -C - -o "$IMAGE" "$url" || curl -fL --progress-bar -o "$IMAGE" "$url"
		step "İmaj doğrulanıyor (SHA-256)" "Verifying image (SHA-256)"
		[ "$(sha256 "$IMAGE")" = "$expected" ] || { rm -f "$IMAGE"; die "Özet uyuşmadı; dosya silindi, yeniden deneyin." "Checksum mismatch; file removed, try again."; }
		say "Doğrulandı." "Verified."
	fi
fi
[ -f "$IMAGE" ] || die "İmaj bulunamadı: $IMAGE" "Image not found: $IMAGE"

# xz açıcı: xz > Apple gzcat (macOS'ta yerleşik, xz okur) > python3
decompress() {
	case "$IMAGE" in
	*.xz)
		if command -v xz >/dev/null 2>&1; then xz -dc "$IMAGE"
		elif [ "$OS" = Darwin ] && [ -x /usr/bin/gzcat ]; then /usr/bin/gzcat "$IMAGE"
		elif command -v python3 >/dev/null 2>&1; then python3 -c 'import lzma,shutil,sys; shutil.copyfileobj(lzma.open(sys.argv[1]), sys.stdout.buffer, 4<<20)' "$IMAGE"
		else die "xz bulunamadı (Linux: sudo apt install xz-utils)." "xz not found (Linux: sudo apt install xz-utils)."; fi ;;
	*) cat "$IMAGE" ;;
	esac
}

# --- 2. Disk seçimi ------------------------------------------------------------------
human() { awk -v b="$1" 'BEGIN { split("B KB MB GB TB", u); i = 1; while (b >= 1000 && i < 5) { b /= 1000; i++ } printf "%.1f %s", b, u[i] }'; }

list_disks() { # satır: aygıt|boyut(bayt)|ad
	if [ "$OS" = Darwin ]; then
		boot="$(diskutil info / 2>/dev/null | sed -n 's/^ *Part of Whole: *//p')"
		for d in $(diskutil list physical 2>/dev/null | sed -n 's#^\(/dev/disk[0-9]*\).*#\1#p'); do
			case "${d##*/}" in disk0|"$boot") continue ;; esac
			info="$(diskutil info "$d")"
			# Harici diskler ve dahili SD kart yuvası (internal görünür ama çıkarılabilir / Secure Digital).
			printf '%s\n' "$info" | grep -Eq 'Device Location: *External|Removable Media: *(Removable|Yes)|Protocol: *(USB|Secure Digital)' || continue
			size="$(printf '%s\n' "$info" | sed -n 's/.*Disk Size:.*(\([0-9]*\) Bytes).*/\1/p' | head -n1)"
			model="$(printf '%s\n' "$info" | sed -n 's/^ *Device \/ Media Name: *//p' | head -n1)"
			printf '%s|%s|%s\n' "$d" "${size:-0}" "${model:-?}"
		done
	else
		root="$(lsblk -no PKNAME "$(findmnt -no SOURCE /)" 2>/dev/null || true)"
		lsblk -dbpno NAME,SIZE,RM,TRAN,MODEL 2>/dev/null | while read -r name size rm tran model; do
			[ "${name##*/}" = "$root" ] && continue
			case "$name" in /dev/loop*|/dev/zram*|/dev/sr*) continue ;; esac
			if [ "$rm" = 1 ] || [ "$tran" = usb ] || [ "$tran" = mmc ] || case "$name" in /dev/mmcblk*) true ;; *) false ;; esac; then
				[ "$size" -gt 0 ] && printf '%s|%s|%s\n' "$name" "$size" "${model:-?}"
			fi
		done
	fi
}

if [ -z "$DISK" ]; then
	step "Harici diskler aranıyor" "Looking for external disks"
	disks="$(list_disks)"
	while [ -z "$disks" ]; do
		ask "microSD kartı takın ve Enter'a basın (çıkmak için q)." "Insert the microSD card and press Enter (q to quit)."
		[ "$REPLY" = q ] && exit 1
		disks="$(list_disks)"
	done
	i=0
	printf '%s\n' "$disks" | while IFS='|' read -r d s m; do i=$((i + 1)); printf '  %d) %-14s %10s  %s\n' "$i" "$d" "$(human "$s")" "$m"; done
	count="$(printf '%s\n' "$disks" | wc -l | tr -d ' ')"
	if [ "$count" = 1 ]; then n=1; else ask "Numara seçin [1-$count]:" "Choose a number [1-$count]:"; n="$REPLY"; fi
	case "$n" in ''|*[!0-9]*) die "Geçersiz seçim." "Invalid choice." ;; esac
	DISK="$(printf '%s\n' "$disks" | sed -n "${n}p" | cut -d'|' -f1)"
	[ -n "$DISK" ] || die "Geçersiz seçim." "Invalid choice."
fi
[ -e "$DISK" ] || die "Disk yok: $DISK" "No such disk: $DISK"

# --- 3. Onay ------------------------------------------------------------------------
short="${DISK##*/}"
printf '\n\033[1;31m%s\033[0m\n' "$(say "UYARI: $DISK üzerindeki HER ŞEY silinecek." "WARNING: EVERYTHING on $DISK will be erased.")"
ask "Onaylamak için disk adını yazın ($short):" "Type the disk name to confirm ($short):"
[ "$REPLY" = "$short" ] || die "Onaylanmadı; hiçbir şey yazılmadı." "Not confirmed; nothing was written."

# --- 4. Wi-Fi (isteğe bağlı) --------------------------------------------------------
SSID="" PSK=""
if [ "$WIFI" = 1 ]; then
	ask "Kartın Wi-Fi'a kendiliğinden bağlanması için ağ adı (boş bırakılabilir):" "Wi-Fi network name for the board (optional, leave empty to skip):"
	SSID="$REPLY"
	if [ -n "$SSID" ]; then
		printf '%s ' "$(say "Wi-Fi parolası:" "Wi-Fi password:")" >/dev/tty
		stty -echo </dev/tty 2>/dev/null || true
		IFS= read -r PSK </dev/tty || PSK=""
		stty echo </dev/tty 2>/dev/null || true
		printf '\n' >/dev/tty
	fi
fi

# --- 5. Yazma -----------------------------------------------------------------------
step "Yazılıyor: $DISK (yönetici parolası istenebilir)" "Writing to $DISK (you may be asked for your password)"
sudo -v
if [ "$OS" = Darwin ]; then
	diskutil unmountDisk force "$DISK" >/dev/null
	raw="/dev/r${short}"
	decompress | sudo dd of="$raw" bs=4m status=progress 2>&1 || die "Yazma başarısız." "Write failed."
else
	for p in $(lsblk -lnpo NAME "$DISK" | tail -n +2); do sudo umount "$p" 2>/dev/null || true; done
	decompress | sudo dd of="$DISK" bs=4M conv=fsync status=progress || die "Yazma başarısız." "Write failed."
fi
sync

# --- 6. Wi-Fi dosyası ---------------------------------------------------------------
write_wifi() { printf 'SSID=%s\nPAROLA=%s\n' "$SSID" "$PSK" > "$1/wifi.txt"; }
if [ -n "$SSID" ]; then
	step "Wi-Fi ayarı yazılıyor" "Writing Wi-Fi settings"
	if [ "$OS" = Darwin ]; then
		diskutil mountDisk "$DISK" >/dev/null 2>&1 || true
		mnt=""; for _ in 1 2 3 4 5 6 7 8 9 10; do [ -d /Volumes/MOSCFG ] && { mnt=/Volumes/MOSCFG; break; }; sleep 1; done
		if [ -n "$mnt" ]; then write_wifi "$mnt"; sync; else say "MOSCFG bölümü açılamadı; wifi.txt'yi elle ekleyin." "Could not mount MOSCFG; add wifi.txt manually."; fi
	else
		sudo partprobe "$DISK" 2>/dev/null || true; sleep 2
		part="$(lsblk -lnpo NAME,LABEL "$DISK" | awk '$2 == "MOSCFG" { print $1 }')"
		if [ -n "$part" ]; then
			mnt="$(mktemp -d)"
			sudo mount "$part" "$mnt" && printf 'SSID=%s\nPAROLA=%s\n' "$SSID" "$PSK" | sudo tee "$mnt/wifi.txt" >/dev/null && sudo umount "$mnt"
			rmdir "$mnt"
		else say "MOSCFG bölümü bulunamadı; wifi.txt'yi elle ekleyin." "MOSCFG partition not found; add wifi.txt manually."; fi
	fi
fi

# --- 7. Çıkar ----------------------------------------------------------------------
if [ "$OS" = Darwin ]; then diskutil eject "$DISK" >/dev/null 2>&1 || true; else sudo eject "$DISK" 2>/dev/null || true; fi

printf '\n\033[1;32m✓ %s\033[0m\n' "$(say "Hazır! Kartı çıkarıp cihaza takın ve gücü verin." "Done! Remove the card, insert it into the board and power it on.")"
say "Ardından: http://maker-os.local/  (ağ yoksa \"MakerOS\" Wi-Fi'ına bağlanın)" "Then open: http://maker-os.local/  (no network? join the \"MakerOS\" Wi-Fi)"
