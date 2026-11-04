#!/bin/sh
# Bir OS'un masaustu + VNC yiginini Pi 4 benzeri sinirlarla (4 CPU, 4 GB) kapta
# baslatir, measure.sh ile olcer, ardindan rfb-bench ile VNC akisini olcer.
#   run-one.sh <ad> <imaj> <oturum-komutu...>
set -eu
NAME=$1; IMG=$2; shift 2
HERE=$(cd "$(dirname "$0")/.." && pwd)
RUNS=${RUNS:-3}
docker network create mkbench >/dev/null 2>&1 || true
for r in $(seq 1 $RUNS); do
	C=mb-$NAME
	docker rm -f $C >/dev/null 2>&1 || true
	# Sayfa onbellegini esitle: her olcumden once VM'de onbellek bosaltilir.
	docker run --rm --privileged alpine:3.22 sh -c 'sync; echo 3 > /proc/sys/vm/drop_caches' >/dev/null
	docker run -d --name $C --network mkbench --cpus 4 --memory 4g --user root \
		-e HOLD=1 -e SETTLE=${SETTLE:-20} -e IDLE=${IDLE:-30} \
		-e XDG_RUNTIME_DIR=/tmp/xdg -e WLR_BACKENDS=headless -e WLR_RENDERER=pixman -e WLR_LIBINPUT_NO_DEVICES=1 \
		-e LIBGL_ALWAYS_SOFTWARE=1 -e GALLIUM_DRIVER=softpipe -e HOME=/root -e LANG=C.UTF-8 \
		-v "$HERE/tools/busybox":/bench/busybox:ro -v "$HERE/scripts":/bench/s:ro -v "$HERE/out":/out \
		--entrypoint /bench/busybox $IMG sh /bench/s/measure.sh $NAME-$r "$@" >/dev/null
	until [ -f "$HERE/out/$NAME-$r.json" ] && [ "$(docker logs $C 2>&1 | grep -c vnc_ready_ms)" = 1 ]; do sleep 2; done
	docker run --rm --network mkbench -v "$HERE/scripts":/s:ro node:24-alpine node /s/rfb-bench.mjs $C 5900 10 6 > "$HERE/out/$NAME-$r.vnc.json" || echo '{"error":"vnc"}' > "$HERE/out/$NAME-$r.vnc.json"
	docker logs $C > "$HERE/out/$NAME-$r.container.log" 2>&1
	docker rm -f $C >/dev/null
	echo "$NAME #$r: $(cat "$HERE/out/$NAME-$r.json") $(cat "$HERE/out/$NAME-$r.vnc.json")"
done
