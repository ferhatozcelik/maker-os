#!/bin/sh
# Gelistirme araclarini kurar:
#  - uv + en guncel Python (varsayilan python3) + hazir AI ortami
#  - Node.js LTS (Codex/Gemini gibi npm tabanli agent CLI'lari icin)
#  - VS Code (arm64)
# Agent CLI'lari (Claude Code, Codex, Gemini, Antigravity) imaja gomulmez;
# hesap gerektirdikleri icin Pardus Maker Merkezi > Agent'lar'dan kurulur.
#
# Build modunda her sey vendor deposundan gelir (uv --offline, Python
# arsivleri yerel aynadan, surumler vendor/versions.env ve
# vendor/ai-requirements.txt ile kilitli). Senkron modunda en guncel
# kararli surumler internetten cozulur ve depoya toplanir.
set -eu

. "$(dirname "$(readlink -f "$0")")/../lib/common.sh"
load_pins
require_root
require_rootfs

if is_sync; then
  # Yalnizca bu senkronda kullanilanlar depoya girsin: bos onbellek.
  UV_CACHE_HOST="$PIOS_WORK/sync/uv-cache"
  rm -rf "$UV_CACHE_HOST"
else
  UV_CACHE_HOST="$PIOS_DEPO/uv-cache"
fi
mkdir -p "$UV_CACHE_HOST" "$ROOTFS_DIR/var/cache/pios-uv"
cleanup() {
  umount "$ROOTFS_DIR/var/cache/pios-uv" 2>/dev/null || true
  chroot_umount
}
trap cleanup EXIT
chroot_mount
mount --bind "$UV_CACHE_HOST" "$ROOTFS_DIR/var/cache/pios-uv"

UVENV="UV_PYTHON_INSTALL_DIR=/opt/pios/python UV_PYTHON_BIN_DIR=/usr/local/bin UV_CACHE_DIR=/var/cache/pios-uv UV_LINK_MODE=copy UV_NO_PROGRESS=1 UV_HTTP_TIMEOUT=60 UV_HTTP_RETRIES=5"
if ! is_sync; then
  # Internet yok: paketler onbellekten, Python arsivleri depodaki aynadan.
  UVENV="$UVENV UV_OFFLINE=1 UV_PYTHON_INSTALL_MIRROR=file:///depo/files/python"
fi

echo "== uv =="
UV_VERSION="$(depo_version UV_VERSION "curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/astral-sh/uv/releases/latest | sed 's|.*/tag/||'")"
depo_file "uv-$UV_VERSION-aarch64-unknown-linux-gnu.tar.gz" \
  "https://github.com/astral-sh/uv/releases/download/$UV_VERSION/uv-aarch64-unknown-linux-gnu.tar.gz" - \
  | tar -xz -C "$ROOTFS_DIR/usr/local/bin" --strip-components=1
echo "   uv $(in_chroot uv --version | awk '{print $2}')"

py_full() {  # kurulu CPython'lardan istenen ana surumun tam surumu (3.12 -> 3.12.15)
  # shellcheck disable=SC2086
  in_chroot env $UVENV uv python list --only-installed --output-format json | python3 -c '
import json, sys
want = sys.argv[1]
v = [p["version"] for p in json.load(sys.stdin) if p["implementation"] == "cpython" and (not want or p["version"].startswith(want + "."))]
print(sorted(v, key=lambda s: [int(x) for x in s.split(".")[:3]])[-1])' "$1"
}

echo "== en guncel Python (varsayilan) ve AI ortami Python'u ($PIOS_AI_PYTHON) =="
if is_sync; then
  # shellcheck disable=SC2086
  retry_net 900 in_chroot env $UVENV uv python install
  # shellcheck disable=SC2086
  retry_net 900 in_chroot env $UVENV uv python install "$PIOS_AI_PYTHON"
  PY_LATEST="$(depo_version PY_LATEST "echo $(py_full '')")"
  PY_AI="$(depo_version PY_AI "echo $(py_full "$PIOS_AI_PYTHON")")"
  # Kurulan arsivler depoda uv'nin Python aynasi duzeninde tutulur:
  # files/python/<yayin>/<dosya> (UV_PYTHON_INSTALL_MIRROR).
  # Kurulu surumler adres icermez; bos bir kurulum dizinine bakilarak
  # indirme adresleri listelenir.
  # shellcheck disable=SC2086
  in_chroot env $UVENV UV_PYTHON_INSTALL_DIR=/tmp/pios-py-list uv python list --all-versions --output-format json | python3 -c '
import json, sys, urllib.parse
want = set(sys.argv[1:])
for p in json.load(sys.stdin):
    url = p.get("url") or ""
    if p["implementation"] == "cpython" and p["version"] in want and "/releases/download/" in url and p.get("variant", "default") == "default":
        # uv file:// aynasindan okurken %2B -> + cozer; dosya cozulmus adla durur.
        print(urllib.parse.unquote(url.split("/releases/download/", 1)[1]), url)' "$PY_LATEST" "$PY_AI" \
    | sort -u > "$PIOS_WORK/sync/python-urls.txt"
  [ -s "$PIOS_WORK/sync/python-urls.txt" ] || { echo "HATA: Python arsiv adresleri bulunamadi" >&2; exit 1; }
  while read -r rel url; do
    depo_file "python/$rel" "$url" /dev/null
  done < "$PIOS_WORK/sync/python-urls.txt"
else
  PY_LATEST="$(depo_version PY_LATEST)"
  PY_AI="$(depo_version PY_AI)"
  # shellcheck disable=SC2086
  in_chroot env $UVENV uv python install "$PY_LATEST" "$PY_AI"
fi
PY_LATEST_MINOR="$(echo "$PY_LATEST" | cut -d. -f1-2)"
echo "   Python $PY_LATEST, AI ortami $PY_AI"
# Sistem araclari /usr/bin/python3 (3.9) kullanmaya devam eder; kullanici
# PATH'inde /usr/local/bin once geldigi icin "python3" en guncel surumdur.
ln -sfn "python$PY_LATEST_MINOR" "$ROOTFS_DIR/usr/local/bin/python3"
ln -sfn "python$PY_LATEST_MINOR" "$ROOTFS_DIR/usr/local/bin/python"
in_chroot /usr/local/bin/python3 --version

echo "== hazir AI ortami (Python $PY_AI, /opt/pios/envs/ai) =="
# ai-edge-litert 2.1+ libstdc++ GLIBCXX_3.4.29 (GCC 11) istiyor; Pardus 21'de
# 3.4.28 var. 2.0.3 bu tabanda calisan en yeni surum (21.10.2026 denendi).
AI_PKGS="
  numpy pillow opencv-python matplotlib scikit-learn
  onnxruntime ai-edge-litert==2.0.3 ultralytics
  jupyterlab ipykernel
  gpiozero RPi.GPIO smbus2 spidev pyserial
  requests flask
"
TORCH_INDEX=https://download.pytorch.org/whl/cpu
rm -rf "$ROOTFS_DIR/opt/pios/envs/ai"
# shellcheck disable=SC2086
in_chroot env $UVENV uv venv --python "$PY_AI" --prompt ai /opt/pios/envs/ai
# PyPI'deki aarch64 torch CUDA (NVIDIA, ~4 GB) ile gelir; Pi icin PyTorch'un
# CPU deposu kullanilir. Once torch kurulur, geri kalan paketler (ultralytics
# dahil) bu kurulu torch'u kullanir.
# --compile-bytecode: ortam root'a ait oldugu icin masaustu kullanicisi .pyc
# yazamaz; derlenmezse torch/cv2/ultralytics her ice aktarmada yeniden derlenir.
if is_sync; then
  # shellcheck disable=SC2086
  retry_net 1800 in_chroot env $UVENV uv pip install --compile-bytecode --python /opt/pios/envs/ai/bin/python \
    --index-url "$TORCH_INDEX" torch torchvision
  # shellcheck disable=SC2086
  retry_net 1800 in_chroot env $UVENV uv pip install --compile-bytecode --python /opt/pios/envs/ai/bin/python $AI_PKGS
  # shellcheck disable=SC2086
  in_chroot env $UVENV uv pip freeze --python /opt/pios/envs/ai/bin/python > "$PIOS_WORK/sync/ai-requirements.txt"
else
  # Ayni iki adim, vendor/ai-requirements.txt'deki kesin surumlerle.
  cp "$VENDOR_DIR/ai-requirements.txt" "$ROOTFS_DIR/tmp/ai-requirements.txt"
  TORCH_PINS="$(grep -E '^(torch|torchvision)==' "$VENDOR_DIR/ai-requirements.txt" | tr '\n' ' ')"
  # shellcheck disable=SC2086
  in_chroot env $UVENV uv pip install --compile-bytecode --python /opt/pios/envs/ai/bin/python \
    --index-url "$TORCH_INDEX" $TORCH_PINS
  # shellcheck disable=SC2086
  in_chroot env $UVENV uv pip install --compile-bytecode --python /opt/pios/envs/ai/bin/python \
    -r /tmp/ai-requirements.txt
  rm -f "$ROOTFS_DIR/tmp/ai-requirements.txt"
fi
if ls "$ROOTFS_DIR"/opt/pios/envs/ai/lib/python*/site-packages | grep -qE '^(nvidia|triton)$|^nvidia_(cu|nccl|cudnn)'; then
  echo "HATA: AI ortamina CUDA paketleri girdi (nvidia/triton)" >&2
  exit 1
fi
# shellcheck disable=SC2086
in_chroot env $UVENV uv pip list --python /opt/pios/envs/ai/bin/python --format freeze > "$PIOS_WORK/ai-packages.txt"
in_chroot /opt/pios/envs/ai/bin/python -c 'import numpy, cv2, torch, onnxruntime, ai_edge_litert, gpiozero; print("   AI ortami ice aktarma testi tamam: torch", torch.__version__, "opencv", cv2.__version__)'
in_chroot /opt/pios/envs/ai/bin/python -m ipykernel install --name ai --display-name "Python (AI ortami)" --prefix /usr/local >/dev/null

echo "== Node.js LTS =="
NODE_VERSION="$(depo_version NODE_VERSION "curl -fsSL https://nodejs.org/dist/index.json | python3 -c 'import json,sys; print(next(r[\"version\"] for r in json.load(sys.stdin) if r[\"lts\"]))'")"
rm -rf "$ROOTFS_DIR/opt/pios/node"
mkdir -p "$ROOTFS_DIR/opt/pios/node"
depo_file "node-$NODE_VERSION-linux-arm64.tar.xz" \
  "https://nodejs.org/dist/$NODE_VERSION/node-$NODE_VERSION-linux-arm64.tar.xz" - \
  | tar -xJ -C "$ROOTFS_DIR/opt/pios/node" --strip-components=1
for b in node npm npx corepack; do
  ln -sfn "/opt/pios/node/bin/$b" "$ROOTFS_DIR/usr/local/bin/$b"
done
in_chroot node --version

echo "== VS Code (arm64) =="
# Yarim kalmis bir onceki build'den kalan dpkg islemlerini tamamla.
in_chroot dpkg --configure -a
# Cihaz paketleri yalnizca Pardus Maker surumleriyle (OTA) gunceller;
# Microsoft deposu eklenmez.
echo "code code/add-microsoft-repo boolean false" | in_chroot debconf-set-selections
if is_sync; then
  CODE_URL="$(curl -fsSIL -o /dev/null -w '%{url_effective}' 'https://code.visualstudio.com/sha/download?build=stable&os=linux-deb-arm64')"
  CODE_DEB="$PIOS_CACHE/$(basename "$CODE_URL")"
  [ -s "$CODE_DEB" ] || curl -fL --retry 3 "$CODE_URL" -o "$CODE_DEB"
  cp "$CODE_DEB" "$ROOTFS_DIR/tmp/code.deb"
  cp "$CODE_DEB" "$PIOS_WORK/sync/debs/"
  in_chroot apt-get install -y /tmp/code.deb
  rm -f "$ROOTFS_DIR/tmp/code.deb"
else
  in_chroot apt-get install -y code
fi
rm -f "$ROOTFS_DIR/etc/apt/sources.list.d/vscode.list" "$ROOTFS_DIR/etc/apt/sources.list.d/vscode.sources"
CODE_VERSION="$(in_chroot dpkg-query -W -f='${Version}' code)"
echo "   VS Code $CODE_VERSION"

echo "== paylasilan dizin izinleri =="
# Masaustu kullanicisi da "uv python install" ile surum ekleyebilsin.
in_chroot chgrp -R "$PIOS_USER" /opt/pios/python
chmod -R g+w "$ROOTFS_DIR/opt/pios/python"
find "$ROOTFS_DIR/opt/pios/python" -type d -exec chmod g+s {} +

cat > "$PIOS_WORK/dev-tools.txt" <<TXT
uv         : $UV_VERSION
python     : $PY_LATEST (varsayilan), AI ortami $PY_AI
node       : $NODE_VERSION
vscode     : $CODE_VERSION
TXT
cat "$PIOS_WORK/dev-tools.txt"
