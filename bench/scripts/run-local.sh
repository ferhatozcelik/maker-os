#!/bin/sh
# MakerOS kok dosya sistemi Docker'da: makerd (mock; NM/D-Bus sistem hizmetleri kapta yok)
# + labwc (monitorsuz) + cog web kabuk (Pi ekrani); wayvnc yalniz Ekran Modu acikken. Tarayici: http://localhost/ (baslatma: os/run-docker.sh)
# Önceki çalışmadan kalan soketler (yeniden başlatmada) temizlenir.
rm -rf "$XDG_RUNTIME_DIR"; mkdir -p -m 0700 "$XDG_RUNTIME_DIR" /tmp/makerd
# Yerel bilet: Pi ekranındaki kiosk (cog) gerçek sistemdeki gibi otomatik giriş yapar.
mkdir -p /run/makeros
makerd --mock --listen 0.0.0.0:80 --web /usr/share/makeros/shell --vnc 127.0.0.1:5900 --data-dir /tmp/makerd --token-file /run/makeros/token --log-level info &
# Ekran Modu (wayvnc) varsayilan KAPALI: Ayarlar'dan acilinca makerd kendisi baslatir (internal/screen).
exec dbus-run-session -- /usr/lib/makeros/session
