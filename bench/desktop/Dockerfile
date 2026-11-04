# Sanal masaüstü: MakerOS'un labwc + wayvnc yığını, monitörsüz (headless) ve
# GPU'suz (pixman). Docker'da uçtan uca VNC testi ve kıyaslama içindir.
FROM debian:trixie-slim
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      labwc wayvnc wlr-randr foot fonts-dejavu-core xkb-data procps ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 maker
COPY os/board/rpi4/rootfs-overlay/usr/share/makeros/labwc/rc.xml /etc/makeros-labwc/rc.xml
COPY os/board/rpi4/rootfs-overlay/usr/share/makeros/labwc/environment /etc/makeros-labwc/environment
COPY bench/desktop/autostart /etc/makeros-labwc/autostart
USER maker
ENV XDG_RUNTIME_DIR=/tmp/xdg WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
CMD ["sh", "-c", "mkdir -p -m 0700 $XDG_RUNTIME_DIR && exec labwc -C /etc/makeros-labwc"]
