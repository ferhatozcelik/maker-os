# MakerOS imaj build ortami. privileged calistirilmali (chroot icin
# proc/sys/dev baglama); loop aygiti gerekmez. Bkz. build-docker.sh.
# Bookworm: mkfs.ext4 -d icin e2fsprogs >= 1.43, bullseye debootstrap scripti.
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    debootstrap \
    apt-utils \
    qemu-user-static \
    curl \
    ca-certificates \
    git \
    gnupg \
    kmod \
    xz-utils \
    dosfstools \
    mtools \
    e2fsprogs \
    fdisk \
    util-linux \
    python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
