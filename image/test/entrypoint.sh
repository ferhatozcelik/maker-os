#!/bin/sh
# Pi'deki systemd servislerinin (lightdm, pios-vnc, pios-novnc,
# pios-center) konteyner karsiligi. GPIO/Wi-Fi donanimi konteynerde yoktur.
RES="${PIOS_RESOLUTION:-1920x1080}"
mkdir -p /run/dbus /run/user/1000 && chown pardus:pardus /run/user/1000 && chmod 700 /run/user/1000
dbus-daemon --system --fork
Xvfb :0 -screen 0 "${RES}x24" -nolisten tcp &
sleep 1
# Merkez once baslasin ki /run/pios/token masaustu acilmadan hazir olsun.
/usr/bin/python3 -I /usr/lib/pios/center/server.py &
sleep 1
su - pardus -c "NO_AT_BRIDGE=1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/1000 LANG=tr_TR.UTF-8 dbus-launch --exit-with-session startxfce4" >/tmp/xfce.log 2>&1 &
x11vnc -display :0 -forever -shared -rfbauth /etc/pios/vnc.pass -rfbport 5900 -noxdamage -quiet -bg -o /tmp/x11vnc.log
websockify 6080 localhost:5900 >/tmp/novnc.log 2>&1 &
echo "Pardus Maker test: http://localhost:8080/  (pardus / pardus)"
wait
