# makeros-wm geliştirme/test imajı (arm64, GPU yok). Yalnızca Docker'da derlenir.
#   DOCKER_BUILDKIT=0 docker build -t makeros/wm-dev:1 wm/
FROM rust:1.99.1-trixie

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libwayland-dev libxkbcommon-dev libudev-dev libinput-dev libseat-dev \
        libgbm-dev libdrm-dev libegl-dev libgles-dev libpixman-1-dev libxcb1-dev \
        mesa-libgallium libegl-mesa0 \
        foot weston grim wayland-utils socat jq xwayland x11-apps woff2 \
        fonts-dejavu-core procps file \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_BUILD_JOBS=2 \
    CARGO_TARGET_DIR=/target \
    XDG_RUNTIME_DIR=/run/user/0
RUN mkdir -p /run/user/0 && chmod 700 /run/user/0 && rustup component add clippy rustfmt
WORKDIR /src
