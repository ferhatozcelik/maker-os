# MakerOS gelistirme ortami.
export UV_PYTHON_INSTALL_DIR=/opt/pios/python
case ":$PATH:" in *":/opt/pios/node/bin:"*) ;; *) PATH="/opt/pios/node/bin:$PATH" ;; esac
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) PATH="$HOME/.local/bin:$PATH" ;; esac
export PATH
# Hazir AI ortamini (torch, opencv, onnxruntime, litert, gpiozero...) etkinlestirir.
alias ai-env='. /opt/pios/envs/ai/bin/activate'
