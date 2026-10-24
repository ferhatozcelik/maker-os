"""OTA: python3 -m unittest discover -s image/test/unit -v (root gerekmez)."""
import hashlib
import importlib
import io
import json
import os
import pathlib
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

CENTER = pathlib.Path(__file__).resolve().parents[2] / 'overlay/usr/lib/pios/center'
sys.path.insert(0, str(CENTER))


def build_bundle(path, version, files, hook='exit 0\n'):
    with tarfile.open(path, 'w:xz') as t:
        def add(name, data, mode=0o644):
            info = tarfile.TarInfo(name)
            info.size, info.mode = len(data), mode
            t.addfile(info, io.BytesIO(data))
        add('ota.json', json.dumps({'format': 1, 'version': version, 'kernel': ''}).encode())
        for name, data in files.items():
            add('root/' + name, data)
        add('hooks/post-install.sh', hook.encode(), 0o755)
        add('enable.txt', b'')
        add('packages.txt', b'')


class OtaTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name)
        env = {'PIOS_OTA_ROOT': str(self.root), 'PIOS_OTA_SKIP_HEALTH': '1'}
        self.env = patch.dict(os.environ, env)
        self.env.start()
        import ota
        self.ota = importlib.reload(ota)
        (self.root / 'etc').mkdir()
        (self.root / 'etc/pios-build-version').write_text('1.1.0\n')
        (self.root / 'usr/lib/pios/center').mkdir(parents=True)
        (self.root / 'usr/lib/pios/center/server.py').write_text('eski')
        self.notify = patch.object(self.ota, '_notify')
        self.notify.start()

    def tearDown(self):
        self.notify.stop()
        self.env.stop()
        self.tmp.cleanup()

    def serve(self, version='1.2.0', files=None, hook='exit 0\n', sha=None):
        """Sahte GitHub: son yayin + paket + sha256."""
        files = files if files is not None else {
            'usr/lib/pios/center/server.py': b'yeni', 'usr/lib/pios/center/ota_yeni.py': b'x',
            'etc/pios-build-version': version.encode() + b'\n'}
        bundle = self.root.parent / ('b-%s.tar.xz' % os.getpid())
        build_bundle(bundle, version, files, hook)
        data = bundle.read_bytes()
        bundle.unlink()
        name = 'pardus-maker-%s-ota.tar.xz' % version
        release = {'tag_name': 'v' + version, 'body': 'notlar', 'html_url': 'https://example/rel',
                   'assets': [{'name': name, 'size': len(data), 'browser_download_url': 'u/bundle'},
                              {'name': name + '.sha256', 'size': 90, 'browser_download_url': 'u/sha'}]}
        digest = sha or hashlib.sha256(data).hexdigest()
        urls = {self.ota.API: json.dumps(release).encode(), 'u/bundle': data,
                'u/sha': ('%s  %s\n' % (digest, name)).encode()}
        return patch.object(self.ota, '_get', side_effect=lambda url, timeout=20: io.BytesIO(urls[url]))

    def read(self, rel):
        return (self.root / rel).read_text()

    def test_version_compare(self):
        self.assertTrue(self.ota.is_newer('1.2.0', '1.1.0'))
        self.assertFalse(self.ota.is_newer('1.1.0', '1.1.0'))
        self.assertFalse(self.ota.is_newer('1.10.0', '1.10.1'))
        self.assertTrue(self.ota.is_newer('1.10.0', '1.9.9'))
        self.assertTrue(self.ota.is_newer('1.0.0', '20261024T111910Z-8b2928e2'))   # gelistirme build'i
        self.assertFalse(self.ota.is_newer('bozuk', '1.0.0'))

    def test_check_only_notifies(self):
        with self.serve():
            s = self.ota.check()
        self.assertEqual(s['status'], 'available')
        self.assertEqual(s['latest'], '1.2.0')
        self.assertEqual(self.read('usr/lib/pios/center/server.py'), 'eski')   # kurulum yok
        self.ota._notify.assert_called_once()

    def test_apply_backup_then_confirm_cleans_up(self):
        with self.serve():
            self.ota.check()
            self.ota.apply()
        s = self.ota.status()
        self.assertEqual(s['status'], 'confirm')
        self.assertEqual(s['current'], '1.2.0')
        self.assertEqual(self.read('usr/lib/pios/center/server.py'), 'yeni')
        self.assertTrue(os.path.isdir(self.ota.BACKUP))
        self.ota.confirm()
        self.assertFalse(os.path.exists(self.ota.BACKUP))
        self.assertFalse(os.path.exists(self.ota.TMP))
        self.assertEqual(self.ota.status()['status'], 'idle')

    def test_rollback_restores_and_removes_new_files(self):
        with self.serve():
            self.ota.check()
            self.ota.apply()
        self.ota.rollback()
        self.assertEqual(self.read('usr/lib/pios/center/server.py'), 'eski')
        self.assertFalse((self.root / 'usr/lib/pios/center/ota_yeni.py').exists())
        self.assertEqual(self.ota.current_version(), '1.1.0')
        self.assertFalse(os.path.exists(self.ota.BACKUP))
        self.assertIn('geri alındı', self.ota.status()['message'])

    def test_bad_checksum_changes_nothing(self):
        with self.serve(sha='0' * 64):
            self.ota.check()
            with self.assertRaises(self.ota.OtaError):
                self.ota.apply()
        self.assertEqual(self.read('usr/lib/pios/center/server.py'), 'eski')
        self.assertEqual(self.ota.status()['status'], 'error')
        self.assertFalse(os.path.exists(self.ota.BACKUP))

    def test_failed_hook_rolls_back_automatically(self):
        with self.serve(hook='exit 3\n'):
            self.ota.check()
            with self.assertRaises(self.ota.OtaError):
                self.ota.apply()
        self.assertEqual(self.read('usr/lib/pios/center/server.py'), 'eski')
        self.assertFalse((self.root / 'usr/lib/pios/center/ota_yeni.py').exists())
        s = self.ota.status()
        self.assertEqual(s['status'], 'error')
        self.assertIn('geri yüklendi', s['message'])

    def test_interrupted_install_restored_on_boot(self):
        with self.serve():
            self.ota.check()
            self.ota.apply()
        self.ota.save_state(status='running')   # kurulum sirasinda elektrik kesildi
        self.ota.boot_check()
        self.assertEqual(self.read('usr/lib/pios/center/server.py'), 'eski')
        self.assertEqual(self.ota.status()['status'], 'error')

    def test_unsafe_bundle_rejected(self):
        evil = self.root / 'evil.tar'
        with tarfile.open(evil, 'w') as t:
            link = tarfile.TarInfo('root/etc')
            link.type, link.linkname = tarfile.SYMTYPE, '/tmp'
            t.addfile(link)
            f = tarfile.TarInfo('root/etc/passwd')
            f.size = 1
            t.addfile(f, io.BytesIO(b'x'))
        with self.assertRaises(self.ota.OtaError):
            self.ota.safe_extract(str(evil), str(self.root / 'x'))
        with tarfile.open(evil, 'w') as t:
            f = tarfile.TarInfo('../kacis')
            f.size = 1
            t.addfile(f, io.BytesIO(b'x'))
        with self.assertRaises(self.ota.OtaError):
            self.ota.safe_extract(str(evil), str(self.root / 'x'))

    def test_no_update_when_current(self):
        with self.serve(version='1.1.0'):
            s = self.ota.check()
            self.assertEqual(s['status'], 'idle')
            with self.assertRaises(self.ota.OtaError):
                self.ota.apply()


if __name__ == '__main__':
    unittest.main()
