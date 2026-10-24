#!/bin/sh
# Gelistirme araclarini kurar:
#  - uv + en guncel Python (varsayilan python3) + hazir AI ortami
#  - Node.js LTS (Codex/Gemini gibi npm tabanli agent CLI'lari icin)
#  - VS Code (arm64, Microsoft deposu ile guncellenir)
# Agent CLI'lari (Claude Code, Codex, Gemini, Antigravity) imaja gomulmez;
# hesap gerektirdikleri icin Pardus Maker Merkezi > Agent'lar'dan kurulur.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root
require_rootfs

UV_CACHE_HOST="$PIOS_CACHE/uv"
mkdir -p "$UV_CACHE_HOST" "$ROOTFS_DIR/var/cache/pios-uv"
cleanup() {
  umount "$ROOTFS_DIR/var/cache/pios-uv" 2>/dev/null || true
  chroot_umount
}
trap cleanup EXIT
chroot_mount
# Indirilen wheel/Python arsivleri yeniden build'lerde tekrar inmesin.
mount --bind "$UV_CACHE_HOST" "$ROOTFS_DIR/var/cache/pios-uv"

UVENV="UV_PYTHON_INSTALL_DIR=/opt/pios/python UV_PYTHON_BIN_DIR=/usr/local/bin UV_CACHE_DIR=/var/cache/pios-uv UV_LINK_MODE=copy UV_NO_PROGRESS=1 UV_HTTP_TIMEOUT=60 UV_HTTP_RETRIES=5"

echo "== uv =="
curl -fsSL --retry 3 https://github.com/astral-sh/uv/releases/latest/download/uv-aarch64-unknown-linux-gnu.tar.gz \
  | tar -xz -C "$ROOTFS_DIR/usr/local/bin" --strip-components=1
UV_VERSION="$(in_chroot uv --version | awk '{print $2}')"
echo "   uv $UV_VERSION"

echo "== en guncel Python (varsayilan) =="
# shellcheck disable=SC2086
in_chroot env $UVENV uv python install
PY_LATEST="$(in_chroot env $UVENV uv python list --only-installed --output-format json \
  | python3 -c 'import json,sys; v=[p["version"] for p in json.load(sys.stdin) if p["implementation"]=="cpython"]; print(sorted(v,key=lambda s:[int(x) for x in s.split(".")[:3]])[-1])')"
PY_LATEST_MINOR="$(echo "$PY_LATEST" | cut -d. -f1-2)"
echo "   Python $PY_LATEST"
# Sistem araclari /usr/bin/python3 (3.9) kullanmaya devam eder; kullanici
# PATH'inde /usr/local/bin once geldigi icin "python3" en guncel surumdur.
ln -sfn "python$PY_LATEST_MINOR" "$ROOTFS_DIR/usr/local/bin/python3"
ln -sfn "python$PY_LATEST_MINOR" "$ROOTFS_DIR/usr/local/bin/python"
in_chroot /usr/local/bin/python3 --version

echo "== hazir AI ortami (Python $PIOS_AI_PYTHON, /opt/pios/envs/ai) =="
# ai-edge-litert 2.1+ libstdc++ GLIBCXX_3.4.29 (GCC 11) istiyor; Pardus 21'de
# 3.4.28 var. 2.0.3 bu tabanda calisan en yeni surum (21.10.2026 denendi).
AI_PKGS="
  numpy pillow opencv-python matplotlib scikit-learn
  onnxruntime ai-edge-litert==2.0.3 ultralytics
  jupyterlab ipykernel
  gpiozero RPi.GPIO smbus2 spidev pyserial
  requests flask
"
# shellcheck disable=SC2086
in_chroot env $UVENV uv python install "$PIOS_AI_PYTHON"
rm -rf "$ROOTFS_DIR/opt/pios/envs/ai"
# shellcheck disable=SC2086
in_chroot env $UVENV uv venv --python "$PIOS_AI_PYTHON" --prompt ai /opt/pios/envs/ai
# PyPI'deki aarch64 torch CUDA (NVIDIA, ~4 GB) ile gelir; Pi icin PyTorch'un
# CPU deposu kullanilir. Once torch kurulur, geri kalan paketler (ultralytics
# dahil) bu kurulu torch'u kullanir.
# --compile-bytecode: ortam root'a ait oldugu icin pardus kullanicisi .pyc
# yazamaz; derlenmezse torch/cv2/ultralytics her ice aktarmada yeniden derlenir.
# shellcheck disable=SC2086
in_chroot env $UVENV uv pip install --compile-bytecode --python /opt/pios/envs/ai/bin/python \
  --index-url https://download.pytorch.org/whl/cpu torch torchvision
# shellcheck disable=SC2086
in_chroot env $UVENV uv pip install --compile-bytecode --python /opt/pios/envs/ai/bin/python $AI_PKGS
if ls "$ROOTFS_DIR"/opt/pios/envs/ai/lib/python*/site-packages | grep -qE '^(nvidia|triton)$|^nvidia_(cu|nccl|cudnn)'; then
  echo "HATA: AI ortamina CUDA paketleri girdi (nvidia/triton)" >&2
  exit 1
fi
# shellcheck disable=SC2086
in_chroot env $UVENV uv pip list --python /opt/pios/envs/ai/bin/python --format freeze > "$PIOS_WORK/ai-packages.txt"
in_chroot /opt/pios/envs/ai/bin/python -c 'import numpy, cv2, torch, onnxruntime, ai_edge_litert, gpiozero; print("   AI ortami ice aktarma testi tamam: torch", torch.__version__, "opencv", cv2.__version__)'
in_chroot /opt/pios/envs/ai/bin/python -m ipykernel install --name ai --display-name "Python (AI ortami)" --prefix /usr/local >/dev/null

echo "== Node.js LTS =="
NODE_VERSION="$(curl -fsSL https://nodejs.org/dist/index.json | python3 -c 'import json,sys; print(next(r["version"] for r in json.load(sys.stdin) if r["lts"]))')"
NODE_TAR="$PIOS_CACHE/node-$NODE_VERSION-linux-arm64.tar.xz"
[ -s "$NODE_TAR" ] || curl -fL --retry 3 "https://nodejs.org/dist/$NODE_VERSION/node-$NODE_VERSION-linux-arm64.tar.xz" -o "$NODE_TAR"
rm -rf "$ROOTFS_DIR/opt/pios/node"
mkdir -p "$ROOTFS_DIR/opt/pios/node"
tar -xJf "$NODE_TAR" -C "$ROOTFS_DIR/opt/pios/node" --strip-components=1
for b in node npm npx corepack; do
  ln -sfn "/opt/pios/node/bin/$b" "$ROOTFS_DIR/usr/local/bin/$b"
done
in_chroot node --version

echo "== VS Code (arm64) =="
CODE_URL="$(curl -fsSIL -o /dev/null -w '%{url_effective}' 'https://code.visualstudio.com/sha/download?build=stable&os=linux-deb-arm64')"
CODE_DEB="$PIOS_CACHE/$(basename "$CODE_URL")"
[ -s "$CODE_DEB" ] || curl -fL --retry 3 "$CODE_URL" -o "$CODE_DEB"
cp "$CODE_DEB" "$ROOTFS_DIR/tmp/code.deb"
# Yarim kalmis bir onceki build'den kalan dpkg islemlerini tamamla.
in_chroot dpkg --configure -a
echo "code code/add-microsoft-repo boolean true" | in_chroot debconf-set-selections
in_chroot apt-get install -y /tmp/code.deb
rm -f "$ROOTFS_DIR/tmp/code.deb"
# code'un postinst'i Microsoft anahtarini "gpg --dearmor" ile yazar; minbase'de
# gpg olmadigi icin dosya bos kalir ve "apt update" imza hatasi verir. Anahtar
# builder'daki gpg ile yazilir ve depo dogrulanir.
curl -fsSL https://packages.microsoft.com/keys/microsoft.asc \
  | gpg --dearmor > "$ROOTFS_DIR/usr/share/keyrings/microsoft.gpg"
[ -s "$ROOTFS_DIR/usr/share/keyrings/microsoft.gpg" ] || { echo "HATA: Microsoft anahtari yazilamadi" >&2; exit 1; }
in_chroot apt-get update --error-on=any
CODE_VERSION="$(in_chroot dpkg-query -W -f='${Version}' code)"
echo "   VS Code $CODE_VERSION"

echo "== paylasilan dizin izinleri =="
# pardus kullanicisi da "uv python install" ile surum ekleyebilsin.
in_chroot chgrp -R "$PIOS_USER" /opt/pios/python
chmod -R g+w "$ROOTFS_DIR/opt/pios/python"
find "$ROOTFS_DIR/opt/pios/python" -type d -exec chmod g+s {} +

cat > "$PIOS_WORK/dev-tools.txt" <<TXT
uv         : $UV_VERSION
python     : $PY_LATEST (varsayilan), AI ortami $PIOS_AI_PYTHON
node       : $NODE_VERSION
vscode     : $CODE_VERSION
TXT
cat "$PIOS_WORK/dev-tools.txt"
