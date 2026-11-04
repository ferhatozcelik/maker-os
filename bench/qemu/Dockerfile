FROM debian:trixie-slim
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      qemu-system-arm qemu-utils bc xz-utils fdisk jq mtools device-tree-compiler ca-certificates && rm -rf /var/lib/apt/lists/*
