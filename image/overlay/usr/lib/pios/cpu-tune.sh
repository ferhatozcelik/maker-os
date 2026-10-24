#!/bin/sh
# CPU frekans yoneticisi: ondemand, Raspberry Pi OS'un ayarlariyla.
# Cekirdek varsayilani (esik %80, hemen dusus) yuk gelince frekansi gec
# yukseltip hemen dusurur; masaustu ve derleme agir hissedilir. Burada:
#  - up_threshold=50        : yuk %50'yi gecince en yuksek frekansa cik
#  - sampling_rate=100000   : 100 ms'de bir olc
#  - sampling_down_factor=50: yuksek frekansta 5 sn kal (etkilesim kesintisiz)
set -u

for policy in /sys/devices/system/cpu/cpufreq/policy*; do
  [ -w "$policy/scaling_governor" ] && echo ondemand > "$policy/scaling_governor"
done

OD=/sys/devices/system/cpu/cpufreq/ondemand
if [ -d "$OD" ]; then
  echo 50 > "$OD/up_threshold"
  echo 100000 > "$OD/sampling_rate"
  echo 50 > "$OD/sampling_down_factor"
fi
echo "pios-cpu: $(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_governor 2>/dev/null)," \
  "en fazla $(($(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq 2>/dev/null || echo 0) / 1000)) MHz"
