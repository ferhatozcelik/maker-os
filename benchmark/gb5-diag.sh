#!/bin/sh
# MakerOS · Geekbench 5 farkını teşhis eder: aynı kartta hangi etken kaç puan götürüyor?
#
#   sh gb5-diag.sh            4 senaryo × 3 tur (~60–70 dk)
#   RUNS=1 sh gb5-diag.sh     4 senaryo × 1 tur (hızlı bakış)
#
# Senaryolar:
#   a  olduğu gibi (masaüstü açık, ondemand)
#   b  ekran kabuğu durduruldu (makeros-kiosk)      → arka plan yükünün payı
#   c  performance yöneticisi (masaüstü açık)       → frekans yöneticisinin payı
#   d  b + c
# Her turdan önce boşta yük, sıcaklık ve kısma (throttle) durumu kaydedilir. Sonuçlar
# browser.geekbench.com'a yüklenir (Linux Preview puanı ekrana yazmaz); bağlantılar
# ~/gb5-diag-<tarih>.txt dosyasına yazılır. Sonunda masaüstü ve yönetici eski haline döner.
set -u
RUNS="${RUNS:-3}"
CACHE="${HOME}/.cache/makeros-bench"
TGZ="${CACHE}/Geekbench-5.4.0-LinuxARMPreview.tar.gz"
OUT="${HOME}/gb5-diag-$(date +%Y%m%d-%H%M).txt"
GOV0="$(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_governor)"

mkdir -p "${CACHE}"
[ -f "${TGZ}" ] || curl -fL -o "${TGZ}" https://cdn.geekbench.com/Geekbench-5.4.0-LinuxARMPreview.tar.gz || exit 1
EXE="$(find "${CACHE}" -name geekbench5 -type f 2>/dev/null | head -1)"
if [ -z "${EXE}" ]; then
	mkdir -p "${CACHE}/gb5" && tar -xzf "${TGZ}" -C "${CACHE}/gb5"
	EXE="$(find "${CACHE}/gb5" -name geekbench5 -type f | head -1)"
fi
chmod +x "${EXE}"

echo ">> sudo parolası bir kez sorulur (kabuğu durdurmak ve frekans yöneticisini değiştirmek için)"
sudo -v || exit 1

gov() { for p in /sys/devices/system/cpu/cpufreq/policy*; do echo "$1" | sudo tee "$p/scaling_governor" >/dev/null; done; }
restore() {
	gov "${GOV0}"; sudo /usr/lib/makeros/cpu-tune >/dev/null 2>&1
	sudo systemctl start makeros-kiosk 2>/dev/null
}
trap 'restore; exit 130' INT TERM

state() {
	echo "  sıcaklık: $(vcgencmd measure_temp 2>/dev/null | cut -d= -f2) · kısma: $(vcgencmd get_throttled 2>/dev/null | cut -d= -f2)" \
	     "· yönetici: $(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_governor)"
	echo "  boşta yük (5 sn):"
	top -b -n2 -d5 | awk '/^top/{n++} n==2' | sed -n '1,4p;7,14p' | sed 's/^/    /'
}

run() {  # $1 senaryo adı
	i=1
	while [ "${i}" -le "${RUNS}" ]; do
		sleep 20   # bir önceki turun ısısı ve önbelleği yatışsın
		echo "== $1 · tur ${i}/${RUNS} · $(date +%H:%M:%S)"
		state
		url="$(cd "$(dirname "${EXE}")" && "${EXE}" 2>&1 | grep -o 'https://browser.geekbench.com/v5/cpu/[0-9]*' | head -1)"
		echo "  sonuç: ${url:-YÜKLENEMEDİ}"
		echo "  bitiş: $(vcgencmd measure_temp 2>/dev/null | cut -d= -f2) · kısma: $(vcgencmd get_throttled 2>/dev/null | cut -d= -f2)"
		i=$((i + 1))
	done
}

{
	echo "# MakerOS GB5 teşhis · $(date) · $(uname -r) · $(tr -d '\0' < /proc/device-tree/model)"
	echo "# en yüksek frekans: $(($(cat /sys/devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq) / 1000)) MHz · tur: ${RUNS}"
	sudo -v
	run "a · olduğu gibi"
	sudo -v; sudo systemctl stop makeros-kiosk; sleep 10
	run "b · kabuk kapalı"
	sudo -v; sudo systemctl start makeros-kiosk; sleep 30; gov performance
	run "c · performance"
	sudo -v; sudo systemctl stop makeros-kiosk; sleep 10
	run "d · kabuk kapalı + performance"
} 2>&1 | tee "${OUT}"
restore
echo ">> bitti: ${OUT}"
