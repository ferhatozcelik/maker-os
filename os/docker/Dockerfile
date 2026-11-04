# MakerOS 2 Buildroot derleyici imaji (arm64 yerel; colima/Docker Desktop).
# Host'a hicbir arac kurulmaz; tum derleme bu kapta yapilir.
FROM debian:trixie-slim

ARG BUILDER_UID=1000
ARG BUILDER_GID=1000

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# Buildroot zorunlu + onerilen host bagimliliklari (manual: "System requirements")
# ve wpewebkit/mesa/podman/genimage gibi paketlerin host araclari.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential gcc g++ make binutils patch \
        bash bc bison flex gawk sed grep findutils diffutils file \
        cpio rsync unzip xz-utils bzip2 gzip zstd lz4 tar \
        perl python3 python3-setuptools python3-dev \
        libncurses-dev libssl-dev \
        git wget curl ca-certificates \
        ccache pkg-config texinfo \
        locales debianutils \
        dosfstools mtools fdisk \
        less nano procps time \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g ${BUILDER_GID} builder \
    && useradd -m -u ${BUILDER_UID} -g ${BUILDER_GID} -s /bin/bash builder \
    && mkdir -p /build /src \
    && chown builder:builder /build

# /build adlandirilmis Docker volume'u olarak baglanir (buyuk/kucuk harf
# duyarli ext4; macOS bind mount degil). Bos volume ilk baglandiginda bu
# dizinin sahipligini devralir.
VOLUME ["/build"]

USER builder
WORKDIR /build
