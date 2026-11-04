#!/bin/sh
# Tek bir URL'yi cog ile acar (labwc headless), 15 sn bekler, 15 sn CPU ve PSS olcer.
URL=$1
mkdir -p -m 0700 $XDG_RUNTIME_DIR /tmp/makerd
makerd --mock --listen 127.0.0.1:80 --web /usr/share/makeros/shell --data-dir /tmp/makerd --log-level error &
sleep 1
labwc -s "cog --platform=wl '$URL'" >/dev/null 2>&1 &
sleep 15
CG=/sys/fs/cgroup
u0=$(awk '/^usage_usec/{print $2}' $CG/cpu.stat); sleep 15; u1=$(awk '/^usage_usec/{print $2}' $CG/cpu.stat)
pss=0; for d in /proc/[0-9]*; do c=$(cat $d/comm 2>/dev/null); case "$c" in WPE*|cog) P=$(awk '/^Pss:/{print $2}' $d/smaps_rollup 2>/dev/null); pss=$((pss+${P:-0}));; esac; done
echo "$URL cpu=$(( (u1-u0)/150000 ))% wpe_pss_mb=$((pss/1024))"
