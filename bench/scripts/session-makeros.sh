#!/bin/sh
# MakerOS'un kendi oturumu: makerd (sistem hizmetleri kapta olmadigi icin --mock)
# + labwc (/usr/share/makeros/labwc: autostart -> kiosk-browser -> cog web kabuk)
# + wayvnc (Pi'de makeros-vnc kullanici hizmeti; kapta systemd --user yok).
mkdir -p /tmp/makerd
makerd --mock --listen 127.0.0.1:80 --web /usr/share/makeros/shell --vnc 127.0.0.1:5900 --data-dir /tmp/makerd --log-level warn &
INNER='labwc -C /usr/share/makeros/labwc &
i=0; while [ ! -S "$XDG_RUNTIME_DIR/wayland-0" ] && [ $i -lt 300 ]; do sleep 0.1; i=$((i+1)); done
export WAYLAND_DISPLAY=wayland-0
exec wayvnc --gpu --render-cursor --max-fps=30 0.0.0.0 5900'
if command -v dbus-run-session >/dev/null 2>&1; then exec dbus-run-session -- sh -c "$INNER"; fi
exec sh -c "$INNER"
