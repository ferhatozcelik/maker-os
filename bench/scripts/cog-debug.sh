#!/bin/sh
URL=$1
mkdir -p -m 0700 $XDG_RUNTIME_DIR /tmp/makerd
makerd --mock --listen 0.0.0.0:80 --web /usr/share/makeros/shell --data-dir /tmp/makerd --vnc 127.0.0.1:5900 --log-level error &
sleep 1
labwc -s "cog --platform=wl --enable-write-console-messages-to-stdout=true '$URL'" > /tmp/cog.log 2>&1 &
i=0; while [ ! -S "$XDG_RUNTIME_DIR/wayland-0" ] && [ $i -lt 300 ]; do sleep 0.1; i=$((i+1)); done
WAYLAND_DISPLAY=wayland-0 wayvnc 127.0.0.1 5900 &
sleep 3600
