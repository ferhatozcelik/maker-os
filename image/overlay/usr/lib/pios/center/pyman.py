# Python Yoneticisi: uv ile Python surumleri (kur/kaldir/varsayilan yap),
# projeler (sanal ortamli, sablonlu) ve hazir AI ortami.
import json
import os
import re
import shutil

from common import USER, jobs, open_terminal, run, spawn_user, user_info

INSTALL_DIR = "/opt/pios/python"
BIN_DIR = "/usr/local/bin"
AI_ENV = "/opt/pios/envs/ai"
TEMPLATE_DIR = "/usr/lib/pios/templates"
MODEL_DIR = "/usr/share/pios/mediapipe"
MODEL_URL = "https://storage.googleapis.com/mediapipe-models/"
# MediaPipe modelleri (imaja gomulu; yoksa proje olusturulurken indirilir).
MODELS = {
    "hand_landmarker.task": "hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task",
    "gesture_recognizer.task": "gesture_recognizer/gesture_recognizer/float16/1/gesture_recognizer.task",
    "blaze_face_short_range.tflite": "face_detector/blaze_face_short_range/float16/1/blaze_face_short_range.tflite",
    "pose_landmarker_lite.task": "pose_landmarker/pose_landmarker_lite/float16/1/pose_landmarker_lite.task",
    "efficientdet_lite0.tflite": "object_detector/efficientdet_lite0/int8/1/efficientdet_lite0.tflite",
    "selfie_segmenter.tflite": "image_segmenter/selfie_segmenter/float16/1/selfie_segmenter.tflite",
    "face_landmarker.task": "face_landmarker/face_landmarker/float16/1/face_landmarker.task",
}
# MediaPipe'in arm64 tekerlekleri ve hazir AI ortamiyla ayni surum; en guncel
# Python'da numpy/matplotlib arm64 tekerlegi gecikebilir.
MP_PYTHON = "3.12"
MP_DEPS = ["mediapipe"]


def _mp(title, folder, model, extra=()):
    return {"title": "MediaPipe · " + title, "dir": folder, "models": [model],
            "python": MP_PYTHON, "deps": MP_DEPS + list(extra)}


UV_ENV = dict(os.environ, UV_PYTHON_INSTALL_DIR=INSTALL_DIR, UV_PYTHON_BIN_DIR=BIN_DIR,
              UV_NO_PROGRESS="1", PATH="/usr/local/bin:/usr/bin:/bin")

TEMPLATES = {
    "bos": {"title": "Boş proje", "deps": [], "main": 'print("Merhaba Pardus Maker!")\n'},
    "gpio": {
        "title": "GPIO / elektronik",
        "deps": ["gpiozero", "RPi.GPIO", "smbus2"],
        "main": (
            "# GPIO17'ye (fiziksel pin 11) bagli LED'i yakip sondurur.\n"
            "from gpiozero import LED\nfrom time import sleep\n\n"
            "led = LED(17)\nwhile True:\n    led.toggle()\n    sleep(0.5)\n"),
    },
    "ai": {
        "title": "Yapay zeka (görüntü)",
        "deps": ["numpy", "opencv-python", "onnxruntime", "pillow"],
        "main": (
            "# Kameradan/ornek goruntuden kare okuyup boyutunu yazar.\n"
            "import cv2\nimport numpy as np\n\n"
            "cap = cv2.VideoCapture(0)\nok, frame = cap.read()\n"
            "if not ok:\n    frame = np.zeros((480, 640, 3), np.uint8)\n"
            "print('Goruntu boyutu:', frame.shape)\n"),
    },
    "web": {
        "title": "Web uygulaması (Flask)",
        "deps": ["flask"],
        "main": (
            "from flask import Flask\n\napp = Flask(__name__)\n\n\n"
            "@app.route('/')\ndef index():\n    return 'Merhaba Pardus Maker!'\n\n\n"
            "if __name__ == '__main__':\n    app.run(host='0.0.0.0', port=5000)\n"),
    },
    # Kamera ile calisan MediaPipe ornekleri (/usr/lib/pios/templates).
    "mp-el": _mp("El takibi ve parmak sayma", "mp-el", "hand_landmarker.task"),
    "mp-jest": _mp("Jest ile LED kontrolü (GPIO)", "mp-jest", "gesture_recognizer.task", ["gpiozero", "RPi.GPIO"]),
    "mp-yuz": _mp("Yüz algılama ve otomatik fotoğraf", "mp-yuz", "blaze_face_short_range.tflite"),
    "mp-poz": _mp("Poz: squat ve kol kaldırma sayacı", "mp-poz", "pose_landmarker_lite.task"),
    "mp-nesne": _mp("Nesne algılama (80 sınıf, GPIO alarm)", "mp-nesne", "efficientdet_lite0.tflite",
                    ["gpiozero", "RPi.GPIO"]),
    "mp-segment": _mp("Arka plan bulanıklaştırma", "mp-segment", "selfie_segmenter.tflite"),
    "mp-yuz-agi": _mp("Yüz ağı ve göz kırpma sayacı", "mp-yuz-agi", "face_landmarker.task"),
}


def _vkey(v):
    return [int(x) for x in re.findall(r"\d+", v)[:3]]


def _uv_list(all_versions=False):
    cmd = ["uv", "python", "list", "--output-format", "json"]
    if all_versions:
        cmd.append("--all-versions")
    rc, out, err = run(cmd, timeout=60, env=UV_ENV)
    if rc != 0:
        return []
    try:
        return json.loads(out)
    except ValueError:
        return []


def default_version():
    try:
        target = os.readlink(os.path.join(BIN_DIR, "python3"))
    except OSError:
        return ""
    m = re.search(r"python(\d+\.\d+)$", target)
    return m.group(1) if m else ""


def versions():
    """Kurulu ve kurulabilir CPython surumleri (her ana surumun son yamasi)."""
    entries = [e for e in _uv_list() if e.get("implementation") == "cpython"
               and e.get("variant", "default") == "default"]
    installed, available = {}, {}
    for e in entries:
        v = e["version"]
        if "a" in v or "b" in v or "rc" in v:
            continue
        minor = ".".join(v.split(".")[:2])
        if e.get("path") and str(e["path"]).startswith(INSTALL_DIR):
            installed[minor] = v
        elif not e.get("path"):
            if minor not in available or _vkey(v) > _vkey(available[minor]):
                available[minor] = v
    default = default_version()
    rows = []
    for minor in sorted(set(installed) | set(available), key=_vkey, reverse=True):
        rows.append({
            "minor": minor,
            "installed": installed.get(minor),
            "latest": available.get(minor) or installed.get(minor),
            "default": minor == default,
            "update": bool(installed.get(minor) and available.get(minor)
                           and _vkey(available[minor]) > _vkey(installed[minor])),
        })
    # Cok eski/desteklenmeyen surumleri gosterme.
    return [r for r in rows if _vkey(r["minor"]) >= [3, 9]]


def install(minor):
    if not re.fullmatch(r"3\.\d+", minor):
        return None
    return jobs.start("Python %s kuruluyor" % minor,
                      ["uv", "python", "install", "--reinstall", minor] if _is_installed(minor)
                      else ["uv", "python", "install", minor], env=UV_ENV,
                      after=_fix_permissions)


def _is_installed(minor):
    return any(r["minor"] == minor and r["installed"] for r in versions())


def _fix_permissions():
    run(["chgrp", "-R", USER, INSTALL_DIR])
    run(["chmod", "-R", "g+w", INSTALL_DIR])


def uninstall(minor):
    if not re.fullmatch(r"3\.\d+", minor):
        return None, "gecersiz surum"
    if minor == default_version():
        return None, "Varsayilan surum kaldirilamaz; once baska bir surumu varsayilan yapin"
    return jobs.start("Python %s kaldiriliyor" % minor, ["uv", "python", "uninstall", minor],
                      env=UV_ENV), ""


def set_default(minor):
    exe = os.path.join(BIN_DIR, "python" + minor)
    if not re.fullmatch(r"3\.\d+", minor) or not os.path.exists(exe):
        return False, "Python %s kurulu degil" % minor
    for name in ("python3", "python"):
        link = os.path.join(BIN_DIR, name)
        tmp = link + ".tmp"
        if os.path.lexists(tmp):
            os.remove(tmp)
        os.symlink("python" + minor, tmp)
        os.replace(tmp, link)
    return True, "Varsayilan: Python %s" % minor


# --- Projeler ----------------------------------------------------------------

def projects_dir():
    return os.path.join(user_info()[2], "Projeler")


def projects():
    base = projects_dir()
    items = []
    if os.path.isdir(base):
        for name in sorted(os.listdir(base)):
            path = os.path.join(base, name)
            if not os.path.isdir(path) or name.startswith("."):
                continue
            pyver = ""
            cfg = os.path.join(path, ".venv", "pyvenv.cfg")
            for line in open(cfg).read().splitlines() if os.path.exists(cfg) else []:
                if line.startswith(("version_info", "version =")):
                    pyver = line.split("=", 1)[1].strip()
            items.append({"name": name, "path": path, "python": pyver,
                          "has_venv": os.path.exists(cfg)})
    return items


def create_project(name, minor, template):
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,40}", name):
        return None, "Proje adi: harf, rakam, '-' ve '_' (en fazla 40)"
    tpl = TEMPLATES.get(template)
    if not tpl:
        return None, "gecersiz sablon"
    path = os.path.join(projects_dir(), name)
    if os.path.exists(path):
        return None, "Bu adla bir proje zaten var"
    py = tpl.get("python") or (minor if re.fullmatch(r"3\.\d+", minor or "") else default_version())
    if tpl.get("dir"):
        # Ornek proje: ortak kamera yardimcisi + ornek dosyalari + modeller.
        files = "cp -r '{t}/_ortak/.' '{t}/{d}/.' .; mkdir -p models; ".format(t=TEMPLATE_DIR, d=tpl["dir"])
        for m in tpl.get("models", []):
            files += ("if [ -f '{src}' ]; then cp '{src}' models/; else echo 'Model indiriliyor: {m}'; "
                      "curl -fL --retry 3 -o 'models/{m}' '{url}'; fi; ").format(
                src=os.path.join(MODEL_DIR, m), m=m, url=MODEL_URL + MODELS[m])
        files += "printf '.venv/\\n__pycache__/\\nfotograflar/\\n' > .gitignore; "
    else:
        files = "printf '%s' '{}' > main.py; ".format(tpl["main"].replace("'", "'\\''"))
    run_hint = "echo 'Calistirmak icin: cd {path} && uv run main.py'; ".format(path=path) if tpl.get("dir") else ""
    script = (
        "set -e; mkdir -p '{base}'; cd '{base}'; "
        "uv init --app --no-workspace --vcs git --python {py} --name '{name}' '{name}'; cd '{name}'; "
        "{files}{add} uv sync; echo; echo 'Proje hazir: {path}'; {hint}"
    ).format(base=projects_dir(), py=py, name=name, path=path, files=files, hint=run_hint,
             add=("uv add %s; " % " ".join(tpl["deps"])) if tpl["deps"] else "")
    env = {"UV_PYTHON_INSTALL_DIR": INSTALL_DIR, "UV_NO_PROGRESS": "1",
           "UV_PYTHON_DOWNLOADS": "automatic"}
    return jobs.start("Proje olusturuluyor: %s" % name, ["bash", "-c", script],
                      env=env, user=True), ""


def add_package(name, package):
    path = os.path.join(projects_dir(), name)
    if not os.path.isdir(path) or not re.fullmatch(r"[A-Za-z0-9_.\-\[\]=<>~!, ]{1,200}", package):
        return None, "gecersiz istek"
    return jobs.start("%s: paket ekleniyor" % name,
                      ["bash", "-c", "cd '%s' && uv add %s" % (path, package)],
                      env={"UV_PYTHON_INSTALL_DIR": INSTALL_DIR, "UV_NO_PROGRESS": "1"},
                      user=True), ""


def _project_path(name):
    path = os.path.join(projects_dir(), name)
    return path if re.fullmatch(r"[A-Za-z0-9_-]{1,40}", name) and os.path.isdir(path) else None


def open_project(name, how):
    path = _project_path(name)
    if not path:
        return False
    if how == "code":
        spawn_user(["code", path])
    elif how == "files":
        spawn_user(["thunar", path])
    else:
        open_terminal("source .venv/bin/activate 2>/dev/null; uv run python --version",
                      title=name, cwd=path)
    return True


def delete_project(name):
    path = _project_path(name)
    if not path:
        return False
    trash = os.path.join(projects_dir(), ".silinenler")
    os.makedirs(trash, exist_ok=True)
    shutil.move(path, os.path.join(trash, name))
    uid, gid, _ = user_info()
    os.chown(trash, uid, gid)
    return True


# --- Hazir AI ortami -----------------------------------------------------------

def ai_env():
    py = os.path.join(AI_ENV, "bin", "python")
    if not os.path.exists(py):
        return {"present": False}
    rc, out, _ = run(["uv", "pip", "list", "--python", py, "--format", "json"], timeout=30, env=UV_ENV)
    highlight = {"torch", "torchvision", "onnxruntime", "ai-edge-litert", "opencv-python",
                 "ultralytics", "numpy", "scikit-learn", "jupyterlab", "gpiozero", "rpi-gpio"}
    pkgs = []
    try:
        pkgs = [p for p in json.loads(out) if p["name"].lower() in highlight]
    except ValueError:
        pass
    _, ver, _ = run([py, "--version"])
    return {"present": True, "path": AI_ENV, "python": ver.replace("Python ", ""),
            "packages": sorted(pkgs, key=lambda p: p["name"].lower())}


def ai_open(how):
    if how == "jupyter":
        open_terminal("source %s/bin/activate && cd ~ && jupyter lab" % AI_ENV,
                      title="Jupyter Lab")
    else:
        open_terminal("source %s/bin/activate && python --version" % AI_ENV,
                      title="AI ortami")
    return True
