#!/usr/bin/env bash
# llama.cpp (llama-server, llama-cli) — Pi 4 (Cortex-A72, ARMv8-A + NEON) için aarch64 release (Docker).
# Çıktı mk-llama biriminde: /out/bin/{llama-server,llama-cli}, /out/LICENSE, /out/VERSION
#
# - OpenMP kapalı: MakerOS'ta libgomp yok; ggml'in kendi iş parçacığı havuzu kullanılır.
# - libstdc++/libgcc gömülü, yalnız glibc'ye bağlı (bookworm glibc 2.36 ≤ MakerOS glibc 2.42).
# - curl yok: çalışma zamanında internet gerekmez (model yerel dosyadan).
set -euo pipefail
TAG="${LLAMA_TAG:-v0.9.0}"
docker run --rm --platform linux/arm64 -v mk-llama:/out -v mk-llama-src:/src -e TAG="$TAG" -e JOBS="${JOBS:-6}" debian:bookworm-slim bash -euc '
apt-get update -qq && apt-get install -y -qq --no-install-recommends build-essential cmake git ca-certificates >/dev/null
if [ "$(cat /src/.tag 2>/dev/null)" != "$TAG" ]; then
	rm -rf /src/llama.cpp && git clone -q --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp /src/llama.cpp && echo "$TAG" > /src/.tag
fi
cd /src/llama.cpp
F="-mcpu=cortex-a72 -O3"
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
	-DGGML_NATIVE=OFF -DGGML_CPU_ARM_ARCH=armv8-a+crc -DGGML_OPENMP=OFF -DLLAMA_CURL=OFF \
	-DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TOOLS=ON -DLLAMA_BUILD_SERVER=ON \
	-DCMAKE_C_FLAGS="$F" -DCMAKE_CXX_FLAGS="$F" -DCMAKE_EXE_LINKER_FLAGS="-static-libstdc++ -static-libgcc" >/dev/null
cmake --build build -j "$JOBS" --target llama-server llama-cli 2>&1 | grep -E "error|warning: unused|Built target llama-(server|cli)" || true
mkdir -p /out/bin
install -m 0755 -s build/bin/llama-server build/bin/llama-cli /out/bin/
cp LICENSE /out/LICENSE; echo "$TAG $(git rev-parse --short HEAD)" > /out/VERSION
ls -la /out/bin; cat /out/VERSION
'
