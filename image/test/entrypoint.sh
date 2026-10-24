#!/bin/sh
# Pi'deki systemd servislerinin (lightdm, pios-vnc, pios-novnc,
# pios-center) konteyner karsiligi. GPIO/Wi-Fi donanimi konteynerde yoktur.
# Docker testinde 1600x900 varsayilan: 1080p'den daha dusuk VNC gecikmesi,
# 720p gibi buyutulmus (zoom) gorunmez.
RES="${PIOS_RESOLUTION:-1600x900}"
mkdir -p /run/dbus /run/user/1000 && chown maker:maker /run/user/1000 && chmod 700 /run/user/1000
dbus-daemon --system --fork
NetworkManager --no-daemon >/tmp/nm.log 2>&1 &
Xvfb :0 -screen 0 "${RES}x24" -nolisten tcp &
sleep 1
# Merkez once baslasin ki /run/pios/token masaustu acilmadan hazir olsun.
# systemd yok: otomatik saati ag uzerinden bir kez esitle (basarisizlik zararsiz).
/usr/bin/python3 -I -c 'import sys; sys.path.insert(0,"/usr/lib/pios/center"); import system; system.set_ntp(True)' >/tmp/pios-ntp.log 2>&1 || true
/usr/bin/python3 -I /usr/lib/pios/center/server.py &
sleep 1
su - maker -c "NO_AT_BRIDGE=1 DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/1000 LANG=tr_TR.UTF-8 dbus-launch --exit-with-session startxfce4" >/tmp/xfce.log 2>&1 &
x11vnc -display :0 -localhost -forever -shared -rfbauth /etc/pios/vnc.pass -rfbport 5900 \
  -repeat -threads -nonap -pointer_mode 4 -defer 10 -wait 33 -speeds lan -cursor arrow -xrandr resize \
  -quiet -bg -o /tmp/x11vnc.log
websockify 127.0.0.1:6080 localhost:5900 >/tmp/novnc.log 2>&1 &
echo "MakerOS test: http://localhost:8080/  (maker / maker)"
wait
