# MakerOS kabuk ortami
export PATH="$HOME/.local/bin:/opt/node/bin:/opt/data/bin:$PATH"
# uv: Python kurulumlari ve onbellek kullanicinin evinde (/data)
export UV_PYTHON_INSTALL_DIR="$HOME/.local/share/uv/python"
export UV_LINK_MODE=copy
# npm global paketleri kullanici dizinine
export NPM_CONFIG_PREFIX="$HOME/.local"
[ -n "$BASH_VERSION" ] && [ -t 0 ] && PS1='\[\e[36m\]\u@\h\[\e[0m\]:\w\$ '
# npm: bildirimsiz (sürüm uyarısı, fund, audit yok); hata ayrıntısı kalır.
export NPM_CONFIG_UPDATE_NOTIFIER=false NPM_CONFIG_FUND=false NPM_CONFIG_AUDIT=false
