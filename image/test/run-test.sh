#!/bin/sh
# build-docker.sh sonrasi "pios-work" volume'undaki rootfs'ten test
# konteyneri olusturur ve calistirir. Panel: http://localhost:8080/
set -eu
cd "$(dirname "$0")"
# Eski test konteyneri/imajlari once silinir: her biri ~7 GB, colima diski
# dolarsa "docker import" yarida kesilir (broken pipe).
docker rm -f pardus-maker-test >/dev/null 2>&1 || true
docker rmi pardus-maker-test pardus-maker-rootfs >/dev/null 2>&1 || true
docker image prune -f >/dev/null
docker run --rm -v pios-work:/work debian:bookworm-slim tar -C /work/rootfs -c . \
  | docker import - pardus-maker-rootfs
docker build -t pardus-maker-test .
docker run -d --restart unless-stopped --name pardus-maker-test --hostname pardus-maker \
  -p 8080:80 pardus-maker-test
echo "Panel: http://localhost:8080/  (maker / maker) - durdurmak: docker rm -f pardus-maker-test"
