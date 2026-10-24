"""Yerel Python uygulamaları: ZIP şablonu, içe aktarma, systemd süreçleri ve günlük."""
import base64
import binascii
import io
import json
import os
import pathlib
import re
import shutil
import stat
import tempfile
import threading
import time
import zipfile

from common import USER, run, session_env, user_info

BASE = '/var/lib/pios/plugins'
MAX_ZIP = 15 * 1024 * 1024
MAX_UNPACKED = 64 * 1024 * 1024
_lock = threading.Lock()
ID = re.compile(r'^[a-z][a-z0-9-]{0,39}$')


def _root(plugin_id):
    if not ID.fullmatch(plugin_id):
        raise ValueError('Eklenti kimliği küçük harfle başlamalı; en fazla 40 küçük harf, rakam veya tire içermelidir.')
    return os.path.join(BASE, plugin_id)


def _manifest(root):
    path = os.path.join(root, 'plugin.json')
    if os.path.islink(path) or not os.path.isfile(path):
        raise ValueError('ZIP kökünde normal bir plugin.json dosyası bulunmalıdır.')
    if os.path.getsize(path) > 16384:
        raise ValueError('plugin.json en fazla 16 KB olabilir.')
    with open(path, encoding='utf-8') as f:
        m = json.load(f)
    if not isinstance(m, dict) or not ID.fullmatch(str(m.get('id', ''))):
        raise ValueError('plugin.json içinde geçerli bir id bulunmalıdır.')
    for key in ('name', 'description', 'version'):
        if not isinstance(m.get(key), str) or not 1 <= len(m[key]) <= (500 if key == 'description' else 100):
            raise ValueError('plugin.json: %s metin alanı eksik veya çok uzun.' % key)
    entry = m.get('entrypoint', 'main.py')
    if not isinstance(entry, str) or not entry.endswith('.py'):
        raise ValueError('entrypoint bir .py dosyası olmalıdır.')
    resolved = os.path.realpath(os.path.join(root, entry))
    if not resolved.startswith(os.path.realpath(root) + os.sep) or not os.path.isfile(resolved):
        raise ValueError('Başlangıç dosyası eklenti klasörü içinde bulunmalıdır.')
    project = os.path.join(root, 'pyproject.toml')
    if not os.path.isfile(project) or os.path.islink(project):
        raise ValueError('Python projesi için pyproject.toml gereklidir.')
    port = m.get('web_port')
    if port is not None and (not isinstance(port, int) or not 1024 <= port <= 65535):
        raise ValueError('web_port 1024–65535 arasında olmalıdır veya kaldırılmalıdır.')
    args = m.get('args', [])
    if not isinstance(args, list) or len(args) > 64 or any(not isinstance(v, str) or len(v) > 1024 or '\0' in v for v in args):
        raise ValueError('args en fazla 64 kısa metin argümanı içermelidir.')
    return {k: m.get(k) for k in ('id', 'name', 'description', 'version', 'web_port')} | {'entrypoint': entry, 'args': args}


def template():
    manifest = {'id': 'benim-eklentim', 'name': 'Benim Eklentim', 'version': '1.0.0',
                'description': 'Kendi Python uygulamam', 'entrypoint': 'main.py', 'web_port': 8765, 'args': []}
    main = '''"""Kendi uygulamanızı burada geliştirin. Durdur düğmesi süreç ağacını kapatır."""
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Uygulama(BaseHTTPRequestHandler):
    def do_GET(self):
        body = "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><h1>Python eklentim çalışıyor</h1>".encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

if __name__ == "__main__":
    port = int(os.environ.get("PIOS_PLUGIN_PORT", "8765"))
    print("Eklenti başlatıldı; port:", port, flush=True)
    ThreadingHTTPServer(("0.0.0.0", port), Uygulama).serve_forever()
'''
    pyproject = '[project]\nname = "benim-eklentim"\nversion = "1.0.0"\nrequires-python = ">=3.12,<3.13"\ndependencies = []\n'
    readme = '''# Python eklentisi
main.py dosyasını geliştirin. plugin.json içindeki id, name, description ve version alanlarını düzenleyin.
entrypoint başka bir .py dosyasını gösterebilir. pyproject.toml dependencies listesine paketlerinizi ekleyin.
Web arayüzünüz varsa web_port belirtin ve PIOS_PLUGIN_PORT ortam değişkenini okuyun; yoksa web_port alanını kaldırın.
Her web uygulaması için boş ve farklı bir port seçin. Başlangıç dosyasını sürekli çalışacak şekilde yazabilirsiniz.
plugin.json, main.py, pyproject.toml ve yardımcı dosyaları ZIP'in köküne koyun; .venv/ ve __pycache__/ eklemeyin.
Merkez > Eklentiler > ZIP içe aktar ile yükleyip Çalıştır'a basın. İlk çalıştırmada uv bağımlılıkları kurar; internet gerekebilir.
Eklenti masaüstü kullanıcısı (maker) olarak çalışır; otomatik olarak root yetkisi verilmez. Kamera/GPIO aynı anda tek uygulamada kullanılabilir.
Çıktıları ev klasörünüzde (/home/maker) kendi klasörünüze yazın. Eklenti silme klasörü taşır, kişisel dosyaları silmez.
MediaPipe kullanacaksanız pios_camera.py yardımcısını ve gerekli models/ dosyalarını projeye ekleyin.
'''
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w', zipfile.ZIP_DEFLATED) as z:
        for name, body in {'plugin.json': json.dumps(manifest, ensure_ascii=False, indent=2),
                           'main.py': main, 'pyproject.toml': pyproject, 'README.md': readme}.items():
            z.writestr(name, body)
    return buf.getvalue()


def import_zip(encoded):
    if not isinstance(encoded, str) or len(encoded) > (MAX_ZIP + 2) // 3 * 4:
        return False, 'ZIP dosyası en fazla 15 MB olabilir.'
    if not _lock.acquire(blocking=False):
        return False, 'Başka bir eklenti işlemi devam ediyor.'
    stage = None
    try:
        raw = base64.b64decode(encoded, validate=True)
        if len(raw) > MAX_ZIP:
            raise ValueError('ZIP dosyası en fazla 15 MB olabilir.')
        os.makedirs(BASE, mode=0o755, exist_ok=True)
        os.makedirs(BASE + '/.imports', mode=0o700, exist_ok=True)
        stage = tempfile.mkdtemp(prefix='plugin-', dir=BASE + '/.imports')
        with zipfile.ZipFile(io.BytesIO(raw)) as z:
            entries = z.infolist()
            if len(entries) > 500 or sum(e.file_size for e in entries) > MAX_UNPACKED:
                raise ValueError('ZIP en fazla 500 dosya ve açıldığında 64 MB içerebilir.')
            for entry in entries:
                path = pathlib.PurePosixPath(entry.filename)
                if path.is_absolute() or '..' in path.parts or '\\' in entry.filename:
                    raise ValueError('ZIP içinde geçersiz dosya yolu: ' + entry.filename)
                mode = entry.external_attr >> 16
                if stat.S_ISLNK(mode):
                    raise ValueError('ZIP sembolik bağlantı içeremez.')
                if not path.parts or path.parts[0] == '__MACOSX' or path.name == '.DS_Store':
                    continue
                if any(part in ('.venv', '__pycache__', '.git') for part in path.parts):
                    raise ValueError('.venv, __pycache__ ve .git klasörlerini ZIP dosyasından çıkarın.')
                dest = os.path.join(stage, *path.parts)
                if entry.is_dir():
                    os.makedirs(dest, exist_ok=True)
                else:
                    os.makedirs(os.path.dirname(dest), exist_ok=True)
                    with z.open(entry) as src, open(dest, 'xb') as dst:
                        shutil.copyfileobj(src, dst)
        m = _manifest(stage)
        target = _root(m['id'])
        if os.path.lexists(target):
            raise ValueError('Bu kimlikle bir eklenti var. Yeni sürüm için önce çalışan eklentiyi durdurup listeden kaldırın.')
        uid, gid, _ = user_info()
        for root, dirs, files in os.walk(stage):
            for name in files:
                os.chown(os.path.join(root, name), uid, gid)
                os.chmod(os.path.join(root, name), 0o644)
            os.chown(root, uid, gid)
            os.chmod(root, 0o755)
        os.rename(stage, target)
        stage = None
        return True, '%s eklendi. Listeden çalıştırabilirsiniz.' % m['name']
    except (OSError, ValueError, KeyError, binascii.Error, zipfile.BadZipFile, RuntimeError) as e:
        return False, 'Eklenti içe aktarılamadı:\n' + str(e)
    finally:
        if stage:
            shutil.rmtree(stage)
        _lock.release()


def _unit(plugin_id):
    _root(plugin_id)
    return 'pios-plugin-' + plugin_id + '.service'


def _states(ids):
    if not ids:
        return {}, ''
    rc, out, err = run(['systemctl', 'show'] + [_unit(i) for i in ids] +
                       ['--property=Id,LoadState,ActiveState,SubState,MainPID,ExecMainStatus,MemoryCurrent,CPUUsageNSec'],
                       timeout=10, env=dict(os.environ, LC_ALL='C'))
    states = {}
    for block in out.split('\n\n'):
        props = dict(l.split('=', 1) for l in block.splitlines() if '=' in l)
        unit = props.get('Id', '')
        if unit.startswith('pios-plugin-') and unit.endswith('.service'):
            states[unit[len('pios-plugin-'):-len('.service')]] = props
    missing = 'could not be found' in err or 'not found' in err or 'LoadState=not-found' in out
    return states, (err or out) if rc and not missing else ''


def list_plugins():
    items = []
    try:
        ids = sorted(n for n in os.listdir(BASE) if ID.fullmatch(n) and os.path.isdir(_root(n)))
    except FileNotFoundError:
        ids = []
    for plugin_id in ids:
        try:
            m = _manifest(_root(plugin_id))
            if m['id'] != plugin_id:
                raise ValueError('Eklenti klasörü ile plugin.json kimliği uyuşmuyor.')
            items.append(dict(m, error='', path=_root(plugin_id)))
        except (ValueError, OSError) as e:
            items.append({'id': plugin_id, 'name': plugin_id, 'description': '', 'version': '', 'error': str(e)})
    states, warning = _states(ids)
    for m in items:
        props = states.get(m['id'], {})
        m.update(state=props.get('ActiveState', 'inactive'), substate=props.get('SubState', ''),
                 pid=props.get('MainPID', '0'), exit_code=props.get('ExecMainStatus', '0'),
                 memory=props.get('MemoryCurrent', ''), cpu_ns=props.get('CPUUsageNSec', ''))
    return {'plugins': items, 'warning': warning, 'python_runner': shutil.which('uv') is not None}


def action(plugin_id, operation):
    if not _lock.acquire(blocking=False):
        return False, 'Başka bir eklenti işlemi sürüyor.'
    try:
        root, unit = _root(plugin_id), _unit(plugin_id)
        if not os.path.isdir(root):
            return False, 'Eklenti bulunamadı.'
        if operation == 'stop':
            rc, out, err = run(['systemctl', 'stop', unit], timeout=20)
            return rc == 0, err or out or 'Eklenti ve alt süreçleri durduruldu.'
        states, warning = _states([plugin_id])
        if warning:
            return False, 'Eklenti hizmeti sorgulanamadı:\n' + warning
        active = states.get(plugin_id, {}).get('ActiveState') in ('active', 'activating', 'deactivating')
        if operation == 'remove':
            if active:
                return False, 'Kaldırmadan önce eklentiyi durdurun.'
            os.makedirs(BASE + '/.trash', exist_ok=True)
            shutil.move(root, BASE + '/.trash/%s-%s' % (plugin_id, time.time_ns()))
            return True, 'Eklenti listeden kaldırılıp .trash klasörüne taşındı.'
        if operation != 'start':
            return False, 'Geçersiz eklenti işlemi.'
        if active:
            return False, 'Eklenti zaten çalışıyor.'
        m = _manifest(root)
        if m['id'] != plugin_id:
            return False, 'Eklenti kimliği klasörle uyuşmuyor.'
        uv = shutil.which('uv')
        if not uv:
            return False, 'uv bulunamadı. Pardus Maker geliştirme araçlarını kurun.'
        env = session_env()
        cmd = ['systemd-run', '--unit=' + unit, '--service-type=exec',
               '--property=User=' + USER, '--property=WorkingDirectory=' + root,
               '--property=KillMode=control-group', '--property=CPUAccounting=yes', '--property=MemoryAccounting=yes']
        for key in ('HOME', 'USER', 'LOGNAME', 'PATH', 'DISPLAY', 'XAUTHORITY', 'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'LANG'):
            if key in env:
                cmd.append('--setenv=%s=%s' % (key, env[key]))
        cmd += ['--setenv=PYTHONUNBUFFERED=1', '--setenv=PIOS_PLUGIN_ID=' + plugin_id,
                '--setenv=UV_PYTHON_INSTALL_DIR=/opt/pios/python', '--setenv=UV_NO_PROGRESS=1']
        if m['web_port']:
            cmd += ['--setenv=PIOS_PLUGIN_PORT=' + str(m['web_port'])]
        cmd += ['--', uv, 'run', '--project', root, 'python', m['entrypoint']] + m['args']
        run(['systemctl', 'reset-failed', unit], timeout=5)
        rc, out, err = run(cmd, timeout=20)
        return rc == 0, ((err or out) if rc else 'Eklenti başlatıldı; ilk açılışta bağımlılıklar kurulabilir. Günlükten izleyebilirsiniz.')
    except (ValueError, OSError) as e:
        return False, 'Eklenti işlemi başarısız:\n' + str(e)
    finally:
        _lock.release()


def logs(plugin_id):
    unit = _unit(plugin_id)
    rc, out, err = run(['journalctl', '-u', unit, '-n', '200', '--no-pager', '-o', 'short-iso'], timeout=10)
    return {'id': plugin_id, 'text': out[-100000:] if rc == 0 else 'Günlük okunamadı:\n' + (err or out)}
