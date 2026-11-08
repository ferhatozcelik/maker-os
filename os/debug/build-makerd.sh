#!/usr/bin/env bash
# makerd + makerd-priv + makerd-user, aarch64 release (Docker; önbellek: mk-cargo, mk-target).
# Çıktı mk-target biriminde: /target/release/{makerd,makerd-priv,makerd-user}
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
docker run --rm -v "$ROOT/makerd":/src -v mk-cargo:/usr/local/cargo/registry -v mk-target:/target \
	-e CARGO_TARGET_DIR=/target -e MAKEROS_VERSION="$(cat "$ROOT/os/VERSION")" -w /src \
	"${RUST_IMG:-rust:1.88-slim-bookworm}" cargo build --release --locked --bins -j "${JOBS:-4}"
