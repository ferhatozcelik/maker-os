#!/bench/busybox sh
# Kap ICINDE calisir (her iki OS'un kendi kok dosya sisteminde). Ayni olcum:
#   1) oturum komutunu baslatir, VNC (127.0.0.1:5900) RFB selamina kadar gecen sure
#   2) SETTLE sn bekler; tum sureclerin PSS/RSS toplami, cgroup bellegi, surec sayisi
#   3) IDLE sn boyunca bosta CPU (cgroup cpu.stat)
# Sonuc: /out/<ad>.json ve /out/<ad>.procs.txt
set -u
BB=/bench/busybox
NAME=$1; shift
SETTLE=${SETTLE:-20}; IDLE=${IDLE:-30}
now_ms() { $BB awk '{printf "%d", $1*1000}' /proc/uptime; }
T0=$(now_ms)
$BB mkdir -p -m 0700 "$XDG_RUNTIME_DIR"
"$@" > /out/$NAME.session.log 2>&1 &
READY=-1
i=0
while [ $i -lt 600 ]; do
	B=$($BB timeout 1 $BB nc 127.0.0.1 5900 </dev/null 2>/dev/null | $BB head -c 3)
	if [ "$B" = "RFB" ]; then READY=$(( $(now_ms) - T0 )); break; fi
	$BB sleep 0.1; i=$((i+1))
done
$BB sleep $SETTLE
pss=0; rss=0; n=0
: > /out/$NAME.procs.txt
for d in /proc/[0-9]*; do
	p=${d#/proc/}
	[ "$p" = "$$" ] && continue
	c=$($BB cat $d/comm 2>/dev/null) || continue
	case "$c" in measure.sh|busybox|sleep|nc|timeout|head|cat|awk) continue;; esac
	P=$($BB awk '/^Pss:/{print $2}' $d/smaps_rollup 2>/dev/null); [ -n "$P" ] || continue
	R=$($BB awk '/^Rss:/{print $2}' $d/smaps_rollup 2>/dev/null)
	pss=$((pss+P)); rss=$((rss+R)); n=$((n+1))
	echo "$P $R $c" >> /out/$NAME.procs.txt
done
$BB sort -rn /out/$NAME.procs.txt -o /out/$NAME.procs.txt
CG=/sys/fs/cgroup
memcur=$($BB cat $CG/memory.current)
anon=$($BB awk '/^anon /{print $2}' $CG/memory.stat)
u0=$($BB awk '/^usage_usec/{print $2}' $CG/cpu.stat); t0=$(now_ms)
$BB sleep $IDLE
u1=$($BB awk '/^usage_usec/{print $2}' $CG/cpu.stat); t1=$(now_ms)
cpu=$($BB awk -v u=$((u1-u0)) -v t=$((t1-t0)) 'BEGIN{printf "%.2f", u/(t*10)}')
cat > /out/$NAME.json <<J
{"name":"$NAME","vnc_ready_ms":$READY,"procs":$n,"pss_kb":$pss,"rss_kb":$rss,"cgroup_mem_bytes":$memcur,"cgroup_anon_bytes":$anon,"idle_cpu_pct_of_one_core":$cpu}
J
$BB cat /out/$NAME.json
# Kap acik kalsin (VNC akis testi icin); disaridan durdurulur.
[ "${HOLD:-0}" = 1 ] && exec $BB sleep 3600
exit 0
