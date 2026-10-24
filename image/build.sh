#!/bin/sh
# stages/ icindeki asamalari sirayla calistirir (Linux, root). macOS'ta
# dogrudan degil, build-docker.sh uzerinden calistirilir.
# Tek bir asamadan devam etmek icin: ./build.sh 05
set -eu

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
STAGES_DIR="$SCRIPT_DIR/stages"
FROM="${1:-00}"
# PIOS_STOP_AFTER=065: bu asamadan sonra dur (vendor/sync.sh depoyu toplayinca biter).
STOP_AFTER="${PIOS_STOP_AFTER:-}"

for stage in "$STAGES_DIR"/*.sh; do
  name="$(basename "$stage")"
  [ "${name%%-*}" \< "$FROM" ] && continue
  echo
  echo "================ $name ================"
  "$stage"
  if [ -n "$STOP_AFTER" ] && [ "${name%%-*}" = "$STOP_AFTER" ]; then break; fi
done
