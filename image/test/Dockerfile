# Imajin rootfs'ini Docker'da calistirarak masaustu + web paneli test eder.
# Konteynerde KMS/HDMI olmadigi icin yalnizca burada Xvfb eklenir; geri
# kalan her sey (XFCE, x11vnc, noVNC, panel) imajdakinin aynisidir.
FROM pardus-maker-rootfs
RUN printf 'deb https://ulakbimdepo2.pardus.org.tr/pardus yirmibir main contrib non-free\n' > /etc/apt/sources.list \
 && apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y xvfb \
 && rm -rf /var/lib/apt/lists/*
COPY entrypoint.sh /entrypoint.sh
EXPOSE 80 6080 5900
ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
# Docker kullanici ad alanlarina/setuid sandbox'a izin vermez; yalnizca testte.
RUN echo 'export CHROMIUM_FLAGS="$CHROMIUM_FLAGS --no-sandbox --test-type"' > /etc/chromium.d/zz-docker-test
