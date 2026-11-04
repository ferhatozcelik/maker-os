#!/bin/sh
# MakerOS kök dosya sistemini Docker'da çalıştırır → http://localhost/ (giriş: maker / maker).
# İmaj (makeros:2.0.0-dev) derlenmiş rootfs'ten üretilir:
#   docker run --rm -v makeros-build:/build:ro -v "$PWD/bench/out":/out debian:trixie sh /s/extract-makeros.sh | docker import - makeros:2.0.0-dev
# İçeride: makerd (mock; NM/D-Bus sistem hizmetleri kapta yok) + labwc (monitörsüz) + cog (Pi ekranı) + wayvnc.
# PORT=8080 ./os/run-docker.sh ile başka port seçilebilir.
set -eu
cd "$(dirname "$0")/.."
PORT=${PORT:-80}
IMG=${IMG:-makeros:2.0.0-dev}
docker rm -f makeros >/dev/null 2>&1 || true
docker run -d --name makeros -p "$PORT:80" --shm-size=256m \
  -e XDG_RUNTIME_DIR=/tmp/xdg -e HOME=/root -e LANG=C.UTF-8 -e WLR_LIBINPUT_NO_DEVICES=1 \
  -v "$PWD/bench/scripts":/bench/s:ro --entrypoint /bin/sh "$IMG" /bench/s/run-local.sh >/dev/null
echo "→ http://localhost$( [ "$PORT" = 80 ] || echo ":$PORT" )/  (maker / maker)"
