#!/bin/sh
# Host (macOS/Linux) tarafindan cagrilir: builder imajini hazirlar ve
# build.sh'i ayricalikli konteyner icinde calistirir. Rootfs ve onbellek
# "pios-work" Docker volume'unda (Linux dosya sistemi) tutulur; host'a
# yalnizca image/out/ altindaki .img/.img.xz dosyalari yazilir.
#
# Apple Silicon: colima start --arch aarch64 --cpu 6 --memory 8 --disk 60
# Kullanim: ./build-docker.sh [baslangic-asamasi]
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

docker build -t pardus-maker-builder -f "$SCRIPT_DIR/Dockerfile.builder" "$SCRIPT_DIR"
docker volume create pios-work >/dev/null

docker run --rm --privileged \
  -v "$REPO_DIR":/workspace \
  -v pios-work:/work \
  -e PIOS_WORK=/work \
  -e PIOS_OUT=/workspace/image/out \
  ${PIOS_RESOLUTION:+-e PIOS_RESOLUTION="$PIOS_RESOLUTION"} \
  ${PIOS_PASSWORD:+-e PIOS_PASSWORD="$PIOS_PASSWORD"} \
  ${PIOS_COMPRESS:+-e PIOS_COMPRESS="$PIOS_COMPRESS"} \
  ${PIOS_BUILD_VERSION:+-e PIOS_BUILD_VERSION="$PIOS_BUILD_VERSION"} \
  pardus-maker-builder /workspace/image/build.sh "$@"
