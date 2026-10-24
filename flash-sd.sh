#!/bin/bash
# macOS: ./flash-sd.sh [imaj.img veya imaj.img.xz]
set -euo pipefail

die() { printf 'Hata: %s\n' "$*" >&2; exit 1; }
[[ "$(uname -s)" == Darwin ]] || die 'Bu betik macOS içindir.'
[[ -t 0 ]] || die 'Disk seçimi için betiği Terminal içinde çalıştırın.'
ROOT="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

IMAGE="${1:-}"
if [[ -z "$IMAGE" ]]; then
    images=()
    for f in "$ROOT"/image/out/*.img "$ROOT"/image/out/*.img.xz; do
        [[ -f "$f" ]] && images+=("$f")
    done
    [[ ${#images[@]} -gt 0 ]] || die 'İmaj bulunamadı. Kullanım: ./flash-sd.sh /yol/imaj.img.xz'
    # Aynı imajın sıkıştırılmamış kopyası varsa onu tercih et.
    IMAGE="${images[0]}"
    if [[ ${#images[@]} -gt 1 ]]; then
        printf 'Kullanılacak imaj: %s\n' "$IMAGE"
        printf 'Başka imaj için dosya yolunu betiğe argüman olarak verin.\n'
    fi
fi
[[ -f "$IMAGE" ]] || die "İmaj bulunamadı: $IMAGE"
IMAGE="$(cd "$(dirname "$IMAGE")" && pwd)/$(basename "$IMAGE")"
case "$IMAGE" in
    *.img) ;;
    *.img.xz) command -v xz >/dev/null || die 'xz gerekli. Kurulum: brew install xz' ;;
    *) die 'Yalnızca .img ve .img.xz dosyaları desteklenir.' ;;
esac

expected_raw=""
if [[ -f "$IMAGE.sha256" ]]; then
    printf 'İmaj SHA-256 doğrulanıyor…\n'
    expected="$(awk 'NR == 1 {print $1}' "$IMAGE.sha256")"
    actual="$(shasum -a 256 "$IMAGE" | awk '{print $1}')"
    [[ "$expected" == "$actual" ]] || die 'SHA-256 uyuşmuyor; yazma iptal edildi.'
    # Ham imajın özeti artık biliniyor; kart doğrulamasında yeniden hesaplanmaz.
    [[ "$IMAGE" == *.img ]] && expected_raw="$actual"
else
    printf 'SHA-256 dosyası bulunamadı: %s.sha256\n' "$IMAGE"
fi
# .img.xz için build'in ürettiği ham imaj özeti (.img.sha256) varsa kullanılır;
# yoksa doğrulamada imaj bir kez daha açılır.
if [[ "$IMAGE" == *.xz && -f "${IMAGE%.xz}.sha256" ]]; then
    expected_raw="$(awk 'NR == 1 {print $1}' "${IMAGE%.xz}.sha256")"
fi
case "$IMAGE" in
    *.xz) image_size="$(xz --robot --list "$IMAGE" | awk '$1 == "totals" {print $5}')" ;;
    *) image_size="$(stat -f %z "$IMAGE")" ;;
esac
[[ "$image_size" =~ ^[0-9]+$ ]] || die 'İmaj boyutu okunamadı.'

field() { /usr/bin/plutil -extract "$1" raw -o - "$WORK/disk.plist" 2>/dev/null; }
inspect_disk() {
    diskutil info -plist "$1" > "$WORK/disk.plist" || return 1
    [[ "$(field WholeDisk)" == true ]] || return 1
    # Adaylar zaten `diskutil list physical` ile fiziksel disklerden
    # geliyor. Bazı macOS sürümlerinde VirtualOrPhysical alanı yoktur.
    [[ "$(field VirtualOrPhysical || true)" != Virtual ]] || return 1
    # Dahili SD okuyucular Internal=true bildirebilir. Sabit dahili
    # diskleri hariç tutarken çıkarılabilir medyaya izin ver.
    [[ "$(field Internal)" == false ||
       "$(field RemovableMedia || true)" == true ||
       "$(field Ejectable || true)" == true ]] || return 1
}

printf '\nHarici veya çıkarılabilir fiziksel diskler (SD / USB):\n'
diskutil list physical > "$WORK/list.txt" || die 'Disk listesi alınamadı.'
disks=()
names=()
sizes=()
while IFS= read -r disk; do
    if inspect_disk "$disk"; then
        disks+=("$disk")
        names+=("$(field MediaName)")
        sizes+=("$(field TotalSize)")
        printf '  %s) %s — %s — %s bayt\n' "${#disks[@]}" "$disk" "$(field MediaName)" "$(field TotalSize)"
    fi
done < <(awk '/^\/dev\/disk[0-9]+ / {print $1}' "$WORK/list.txt")
[[ ${#disks[@]} -gt 0 ]] || die 'Uygun disk bulunamadı. SD kartı takıp tekrar çalıştırın. Tanılama: diskutil list'
read -r -p 'Disk numarasını seçin (iptal: q): ' choice
[[ "$choice" == q ]] && exit 0
[[ "$choice" =~ ^[0-9]+$ && ${#choice} -lt 5 ]] || die 'Geçersiz seçim.'
index=$((10#$choice - 1))
[[ $index -ge 0 && $index -lt ${#disks[@]} ]] || die 'Geçersiz seçim.'
target="${disks[$index]}"
[[ ${sizes[$index]} -ge $image_size ]] || die 'Seçilen disk imaj için çok küçük.'
diskutil info "$target"
printf '\nİmaj: %s\nDİKKAT: %s üzerindeki TÜM VERİLER SİLİNECEK!\n' "$IMAGE" "$target"
read -r -p "Onaylamak için 'SIL $target' yazın: " confirmation
[[ "$confirmation" == "SIL $target" ]] || die 'Onay verilmedi; hiçbir şey yazılmadı.'

sudo -v
inspect_disk "$target" || die 'Disk artık harici veya çıkarılabilir fiziksel disk olarak doğrulanamıyor.'
[[ "$(field MediaName)" == "${names[$index]}" && "$(field TotalSize)" == "${sizes[$index]}" ]] || die 'Disk bilgileri değişti; tekrar çalıştırın.'
diskutil unmountDisk "$target" || die 'Disk ayrılamadı.'
raw="/dev/r${target#/dev/}"
printf '\nYazılıyor… İlerlemeyi görmek için Ctrl+T kullanabilirsiniz.\n'
# -T0: build imajı çok iş parçacıklı (bloklu) sıkıştırır; açma da tüm
# çekirdekleri kullanır.
case "$IMAGE" in
    *.xz) xz -T0 -dc "$IMAGE" | sudo dd of="$raw" bs=4m ;;
    *) sudo dd if="$IMAGE" of="$raw" bs=4m ;;
esac
sync
printf 'Yazılan veri SHA-256 ile doğrulanıyor…\n'
if [[ -z "$expected_raw" ]]; then
    if [[ "$IMAGE" == *.xz ]]; then
        expected_raw="$(xz -T0 -dc "$IMAGE" | shasum -a 256 | awk '{print $1}')"
    else
        expected_raw="$(shasum -a 256 "$IMAGE" | awk '{print $1}')"
    fi
fi
# head yalnızca imaj uzunluğu kadar okur; kartın kalan alanı karşılaştırılmaz.
actual_raw="$(sudo head -c "$image_size" "$raw" | shasum -a 256 | awk '{print $1}')"
[[ "$expected_raw" == "$actual_raw" ]] || die 'Kart doğrulaması başarısız! Kartı kullanmadan yeniden yazın.'
diskutil eject "$target"
printf '\nYazma ve doğrulama tamamlandı. Kartı Raspberry Pi’ye takabilirsiniz.\n'
