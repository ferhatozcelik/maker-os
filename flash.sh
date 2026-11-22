#!/bin/sh
# MakerOS yazıcı — macOS ve Linux
# MakerOS flasher — macOS and Linux
#
#   sh -c "$(curl -fsSL https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.sh)"
#
# İki bağlantı türü (mod) vardır; başta sorulur ya da -m ile verilir:
#   sd   microSD kart bilgisayarda (kart okuyucu): kart baştan yazılır (yeni kurulum).
#   usb  Kart Pi'de, MakerOS çalışıyor; Pi bilgisayara USB-C ile bağlı: yeni sürüm
#        A/B olarak kurulur, veriler korunur (imzalı RAUC paketi, makerd USB-C ağı 10.42.0.1).
#
# Seçenekler / options:
#   -m, --mode <sd|usb>     bağlantı türü; verilmezse sorulur
#   -i, --image <dosya>     yerel .img / .img.xz (sd) ya da .raucb (usb) kullan (indirme yapılmaz)
#   -d, --disk <aygıt>      sd: hedef disk (ör. /dev/disk4, /dev/sdb); verilmezse listeden seçilir
#   -v, --version <sürüm>   belirli bir sürüm (ör. v1.0.0); varsayılan en son sürüm
#       --host <adres>      usb: Pi'nin adresi (varsayılan 10.42.0.1)
#   -u, --user <ad>         usb: MakerOS yönetici hesabı (verilmezse sorulur)
#       --no-wifi           sd: Wi-Fi sorusunu atla
#
# sd modunda betik yalnız harici / çıkarılabilir diskleri listeler ve yazmadan önce disk
# adını elle yazmanızı ister. Seçilen diskteki her şey silinir.
set -eu

REPO="ferhatozcelik/maker-os"
OS="$(uname -s)"
case "${LANG:-}${LC_ALL:-}" in tr*|*tr_TR*) TR=1 ;; *) TR=0 ;; esac

say()  { if [ "$TR" = 1 ]; then printf '%s\n' "$1"; else printf '%s\n' "$2"; fi; }
step() { printf '\n\033[1;33m==>\033[0m \033[1m%s\033[0m\n' "$(say "$1" "$2")"; }
die()  { printf '\033[1;31m%s\033[0m %s\n' "$(say "Hata:" "Error:")" "$(say "$1" "$2")" >&2; exit 1; }
ask()  { printf '%s ' "$(say "$1" "$2")" >/dev/tty; IFS= read -r REPLY </dev/tty || REPLY=""; }

IMAGE="" DISK="" VERSION="latest" WIFI=1 MODE="" HOST="10.42.0.1" MUSER=""
while [ $# -gt 0 ]; do
	case "$1" in
	-m|--mode) MODE="${2:?}"; shift 2 ;;
	--host) HOST="${2:?}"; shift 2 ;;
	-u|--user) MUSER="${2:?}"; shift 2 ;;
	-i|--image) IMAGE="${2:?}"; shift 2 ;;
	-d|--disk) DISK="${2:?}"; shift 2 ;;
	-v|--version) VERSION="${2:?}"; shift 2 ;;
	--no-wifi) WIFI=0; shift ;;
	-h|--help)
		cat <<-'USAGE'
		MakerOS yazıcı / flasher (macOS, Linux)
		  -m, --mode <sd|usb>         sd: kart okuyucu · card reader (fresh install)
		                              usb: Pi USB-C ile bağlı · Pi over USB-C (update, keeps data)
		  -i, --image <dosya|file>    yerel .img / .img.xz / .raucb · use a local image
		  -d, --disk <aygıt|device>   sd: hedef disk · target disk (/dev/disk4, /dev/sdb)
		  -v, --version <vX.Y.Z>      belirli sürüm · specific release (default: latest)
		      --host <adres|address>  usb: Pi adresi · Pi address (default 10.42.0.1)
		  -u, --user <ad|name>        usb: MakerOS yönetici · admin account
		      --no-wifi               sd: Wi-Fi sorusunu atla · skip the Wi-Fi question
		USAGE
		exit 0 ;;
	*) die "bilinmeyen seçenek: $1" "unknown option: $1" ;;
	esac
done

case "$OS" in
Darwin|Linux) ;;
*) die "Bu betik macOS ve Linux içindir. Windows'ta flash.ps1 kullanın." "This script is for macOS and Linux. On Windows use flash.ps1." ;;
esac
[ -r /dev/tty ] || die "Etkileşimli bir terminal gerekli." "An interactive terminal is required."

printf '\n\033[1;38;5;208m  MakerOS\033[0m — %s\n' "$(say "yazıcı" "flasher")"

sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

cache_dir() {
	dir="${XDG_CACHE_HOME:-$HOME/.cache}/makeros"; [ "$OS" = Darwin ] && dir="$HOME/Library/Caches/makeros"
	mkdir -p "$dir"; printf '%s' "$dir"
}

# GitHub sürümünden $1 uzantılı dosyayı (ör. .img.xz, .raucb) önbelleğe indirir, SHA-256 doğrular; yolu IMAGE'a yazar.
fetch_release() {
	step "En son sürüm aranıyor" "Looking up the release"
	if [ "$VERSION" = latest ]; then api="https://api.github.com/repos/$REPO/releases/latest"; else api="https://api.github.com/repos/$REPO/releases/tags/$VERSION"; fi
	json="$(curl -fsSL "$api")" || die "GitHub'a ulaşılamadı." "Could not reach GitHub."
	ext="$(printf '%s' "$1" | sed 's/\./\\./g')"
	url="$(printf '%s' "$json" | grep -o "\"browser_download_url\": *\"[^\"]*$ext\"" | head -n1 | sed 's/.*"\(http[^"]*\)"$/\1/')"
	[ -n "$url" ] || die "Sürümde $1 dosyası yok." "No $1 asset in the release."
	name="${url##*/}"
	IMAGE="$(cache_dir)/$name"
	say "Sürüm: $name" "Release: $name"
	expected="$(curl -fsSL "$url.sha256" | cut -d' ' -f1)"
	[ -n "$expected" ] || die "Özet dosyası alınamadı." "Could not fetch the checksum."
	if [ -f "$IMAGE" ] && [ "$(sha256 "$IMAGE")" = "$expected" ]; then
		say "Önceden indirilmiş dosya kullanılıyor: $IMAGE" "Using previously downloaded file: $IMAGE"
	else
		step "İndiriliyor" "Downloading"
		curl -fL --progress-bar -C - -o "$IMAGE" "$url" || curl -fL --progress-bar -o "$IMAGE" "$url"
		step "Doğrulanıyor (SHA-256)" "Verifying (SHA-256)"
		[ "$(sha256 "$IMAGE")" = "$expected" ] || { rm -f "$IMAGE"; die "Özet uyuşmadı; dosya silindi, yeniden deneyin." "Checksum mismatch; file removed, try again."; }
		say "Doğrulandı." "Verified."
	fi
}

# --- 0. Bağlantı türü ----------------------------------------------------------------
if [ -z "$MODE" ]; then
	case "$IMAGE" in *.raucb) MODE=usb ;; ?*) MODE=sd ;; esac
fi
if [ -z "$MODE" ] && [ -n "$DISK" ]; then MODE=sd; fi
if [ -z "$MODE" ]; then
	step "Bağlantı türünü seçin" "Choose how the card is connected"
	say "  1) microSD kart bilgisayarda (kart okuyucu) — kart baştan yazılır, yeni kurulum" \
		"  1) microSD card in this computer (card reader) — card is erased, fresh install"
	say "  2) Kart Pi'de, Pi USB-C ile bağlı — MakerOS güncellenir, veriler korunur" \
		"  2) Card in the Pi, Pi connected over USB-C — MakerOS is updated, data is kept"
	ask "Seçim [1-2]:" "Choice [1-2]:"
	case "$REPLY" in 1|sd) MODE=sd ;; 2|usb) MODE=usb ;; *) die "Geçersiz seçim." "Invalid choice." ;; esac
fi
case "$MODE" in sd|usb) ;; *) die "bilinmeyen mod: $MODE (sd ya da usb)" "unknown mode: $MODE (sd or usb)" ;; esac

# --- USB-C modu: çalışan MakerOS'a imzalı paketi gönder -------------------------------
json_str() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }
json_get() { sed -n "s/.*\"$1\": *\"\\([^\"]*\\)\".*/\\1/p" | head -n1; }
json_num() { sed -n "s/.*\"$1\": *\\([0-9-]*\\).*/\\1/p" | head -n1; }

usb_flash() {
	base="http://$HOST"
	alive() { c="$(curl -s -o /dev/null -w '%{http_code}' -m 3 "$base/api/v1/session" 2>/dev/null || true)"; [ "$c" = 200 ] || [ "$c" = 401 ]; }

	step "Pi aranıyor ($HOST)" "Looking for the Pi ($HOST)"
	until alive; do
		ask "Kartı Pi'ye takın, Pi'yi USB-C kablosuyla bu bilgisayara bağlayın, açılmasını bekleyip Enter'a basın (çıkmak için q)." \
			"Insert the card into the Pi, connect the Pi to this computer with a USB-C cable, wait for it to boot and press Enter (q to quit)."
		[ "$REPLY" = q ] && exit 1
	done
	say "MakerOS bulundu." "MakerOS found."

	case "$IMAGE" in
	"") fetch_release .raucb ;;
	*.raucb) [ -f "$IMAGE" ] || die "Paket bulunamadı: $IMAGE" "Bundle not found: $IMAGE" ;;
	*) die "USB-C modu .raucb paketi ister (imaj değil): $IMAGE" "USB-C mode needs a .raucb bundle, not an image: $IMAGE" ;;
	esac
	sum="$(sha256 "$IMAGE")"

	jar="$(mktemp)"; out="$(mktemp)"
	trap 'rm -f "$jar" "$out"' EXIT INT TERM
	step "MakerOS'a giriş" "Signing in to MakerOS"
	[ -n "$MUSER" ] || { ask "Yönetici kullanıcı adı:" "Admin user name:"; MUSER="$REPLY"; }
	printf '%s ' "$(say "Parola:" "Password:")" >/dev/tty
	stty -echo </dev/tty 2>/dev/null || true
	IFS= read -r PW </dev/tty || PW=""
	stty echo </dev/tty 2>/dev/null || true
	printf '\n' >/dev/tty
	code="$(printf '{"user":"%s","password":"%s"}' "$(json_str "$MUSER")" "$(json_str "$PW")" |
		curl -s -o "$out" -w '%{http_code}' -m 20 -c "$jar" -H 'X-Maker: 1' -H 'Content-Type: application/json' --data-binary @- "$base/api/v1/auth/login")"
	PW=""
	[ "$code" = 200 ] || die "Giriş başarısız: $(json_get message <"$out")" "Sign-in failed: $(json_get message <"$out")"
	grep -q '"admin": *true' "$out" || die "Bu hesap yönetici değil; güncelleme için yönetici hesabı gerekir." "This account is not an admin; an admin account is required."

	cur="$(curl -s -m 10 -b "$jar" "$base/api/v1/system/update" | json_get current)"
	new="$(basename "$IMAGE" | sed -n 's/^makeros-\(.*\)-rpi4\.raucb$/\1/p')"
	say "Pi'deki sürüm: ${cur:-?}  →  kurulacak: ${new:-$(basename "$IMAGE")}" "On the Pi: ${cur:-?}  →  installing: ${new:-$(basename "$IMAGE")}"
	if [ -n "$cur" ] && [ "$cur" = "$new" ]; then
		ask "Bu sürüm zaten kurulu. Yine de yeniden kurulsun mu? [e/H]" "This version is already installed. Reinstall anyway? [y/N]"
		case "$REPLY" in e|E|y|Y) ;; *) say "Hiçbir şey yapılmadı." "Nothing was changed."; exit 0 ;; esac
	fi

	step "Paket USB-C üzerinden gönderiliyor" "Sending the bundle over USB-C"
	code="$(curl -o "$out" -w '%{http_code}' --progress-bar -b "$jar" -H 'X-Maker: 1' -H "X-Sha256: $sum" \
		-H 'Content-Type: application/octet-stream' -T "$IMAGE" "$base/api/v1/system/update/upload" || true)"
	if [ "$code" != 202 ]; then
		die "Paket gönderilemedi (HTTP ${code:-?}). $(json_get message <"$out")" "Could not send the bundle (HTTP ${code:-?}). $(json_get message <"$out")"
	fi
	job="$(json_get job <"$out")"

	step "Kuruluyor (Pi kapanmasın, kabloyu çıkarmayın)" "Installing (keep the Pi powered, do not unplug)"
	last="" fails=0 phase=""
	while :; do
		st="$(curl -s -m 5 -b "$jar" "$base/api/v1/jobs/$job" 2>/dev/null || true)"
		state="$(printf '%s' "$st" | json_get state)"
		if [ -z "$state" ]; then
			# Kurulum bitince Pi kendiliğinden yeniden başlar; bağlantının kopması beklenir.
			[ "$phase" = rebooting ] && break
			fails=$((fails + 1))
			[ "$fails" -ge 30 ] && die "Pi'ye ulaşılamıyor. Durumu Ayarlar › Genel › Yazılım güncelleme'den izleyin." "Lost contact with the Pi. Check Settings › General › Software update."
			sleep 2; continue
		fi
		fails=0
		up="$(curl -s -m 5 -b "$jar" "$base/api/v1/system/update" 2>/dev/null || true)"
		phase="$(printf '%s' "$up" | json_get phase)"
		pct="$(printf '%s' "$st" | json_num progress)"
		line="  $(say "%${pct:-0}" "${pct:-0}%")  $(printf '%s' "$up" | json_get message)"
		[ "$line" != "$last" ] && { printf '\r\033[K%s' "$line"; last="$line"; }
		case "$state" in
		done) break ;;
		failed) printf '\n'; die "Kurulum başarısız: $(printf '%s' "$st" | json_get error)" "Install failed: $(printf '%s' "$st" | json_get error)" ;;
		esac
		[ "$phase" = rebooting ] && break
		sleep 2
	done
	printf '\n'

	step "Pi yeni sürümle yeniden başlatılıyor" "The Pi is restarting with the new version"
	i=0; while alive && [ $i -lt 30 ]; do sleep 2; i=$((i + 1)); done
	i=0; until alive; do
		i=$((i + 1)); [ $i -gt 90 ] && { say "Pi henüz geri dönmedi; birkaç dakika bekleyip http://$HOST/ adresini açın." "The Pi is not back yet; wait a few minutes and open http://$HOST/"; exit 0; }
		sleep 2
	done
	printf '\n\033[1;32m✓ %s\033[0m\n' "$(say "Güncellendi! MakerOS yeni sürümle açıldı: http://$HOST/" "Updated! MakerOS is running the new version: http://$HOST/")"
	say "Yeni sürüm açılmazsa Pi bir sonraki açılışta eski sürüme kendiliğinden döner." "If the new version fails to boot, the Pi falls back to the previous one automatically."
}

if [ "$MODE" = usb ]; then usb_flash; exit 0; fi

# --- 1. İmaj -------------------------------------------------------------------------
[ -n "$IMAGE" ] || fetch_release .img.xz
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
