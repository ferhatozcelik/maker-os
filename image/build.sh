#!/bin/sh
# stages/ icindeki asamalari sirayla calistirir (Linux, root). macOS'ta
# dogrudan degil, build-docker.sh uzerinden calistirilir.
# Tek bir asamadan devam etmek icin: ./build.sh 05
set -eu

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
STAGES_DIR="$SCRIPT_DIR/stages"
FROM="${1:-00}"

for stage in "$STAGES_DIR"/*.sh; do
  name="$(basename "$stage")"
  [ "${name%%-*}" \< "$FROM" ] && continue
  echo
  echo "================ $name ================"
  "$stage"
done
