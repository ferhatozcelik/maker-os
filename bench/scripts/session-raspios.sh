#!/bin/sh
# Raspberry Pi OS Desktop'un kendi oturumu: labwc-pi (panel, masaustu, kanshi,
# lxsession autostart) + wayvnc (Pi OS'ta wayvnc.service ile ayni). D-Bus oturumu ile.
exec dbus-run-session -- sh -c '
labwc-pi &
i=0; while [ ! -S "$XDG_RUNTIME_DIR/wayland-0" ] && [ $i -lt 300 ]; do sleep 0.1; i=$((i+1)); done
export WAYLAND_DISPLAY=wayland-0
exec wayvnc --render-cursor --max-fps=30 0.0.0.0 5900'
