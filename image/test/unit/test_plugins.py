import base64
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'overlay/usr/lib/pios/center'))
import plugins


class PluginTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name) / 'plugins'
        self.base_patch = patch.object(plugins, 'BASE', str(self.base))
        self.base_patch.start()
        self.user_patch = patch.object(plugins, 'user_info', return_value=(os.getuid(), os.getgid(), self.tmp.name))
        self.user_patch.start()

    def tearDown(self):
        self.base_patch.stop()
        self.user_patch.stop()
        self.tmp.cleanup()

    def import_template(self):
        return plugins.import_zip(base64.b64encode(plugins.template()).decode())

    def archive(self, entries):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, 'w') as z:
            for name, body in entries.items():
                z.writestr(name, body)
        return base64.b64encode(buf.getvalue()).decode()

    def test_template_imports_and_lists(self):
        ok, message = self.import_template()
        self.assertTrue(ok, message)
        self.assertTrue((self.base / 'benim-eklentim/main.py').is_file())
        with patch.object(plugins, '_states', return_value=({}, '')):
            listed = plugins.list_plugins()['plugins']
        self.assertEqual(listed[0]['name'], 'Benim Eklentim')
        self.assertEqual(listed[0]['state'], 'inactive')

    def test_duplicate_import_preserves_existing_code(self):
        self.import_template()
        script = self.base / 'benim-eklentim/main.py'
        script.write_text('print("my changes")')
        self.assertFalse(self.import_template()[0])
        self.assertIn('my changes', script.read_text())

    def test_path_traversal_rejected(self):
        ok, _ = plugins.import_zip(self.archive({'../../escaped.py': 'bad'}))
        self.assertFalse(ok)
        self.assertFalse((Path(self.tmp.name) / 'escaped.py').exists())
        self.assertEqual(list((self.base / '.imports').iterdir()), [])

    def test_symlink_rejected(self):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, 'w') as z:
            entry = zipfile.ZipInfo('main.py')
            entry.create_system = 3
            entry.external_attr = (stat.S_IFLNK | 0o777) << 16
            z.writestr(entry, '/etc/passwd')
        self.assertFalse(plugins.import_zip(base64.b64encode(buf.getvalue()).decode())[0])

    def test_file_limit(self):
        archive = self.archive({'%s.py' % i: '' for i in range(501)})
        ok, message = plugins.import_zip(archive)
        self.assertFalse(ok)
        self.assertIn('500', message)

    def test_missing_manifest(self):
        ok, message = plugins.import_zip(self.archive({'main.py': 'print(1)'}))
        self.assertFalse(ok)
        self.assertIn('plugin.json', message)

    def test_invalid_base64(self):
        self.assertFalse(plugins.import_zip('not base64!')[0])

    def test_start_runs_as_user_and_controls_process_tree(self):
        self.import_template()
        with patch.object(plugins, '_states', return_value=({}, '')), patch.object(plugins.shutil, 'which', return_value='/usr/local/bin/uv'), patch.object(plugins, 'session_env', return_value={'HOME': '/home/maker', 'PATH': '/usr/local/bin:/usr/bin:/bin'}), patch.object(plugins, 'run', return_value=(0, '', '')) as run:
            ok, _ = plugins.action('benim-eklentim', 'start')
        self.assertTrue(ok)
        cmd = run.call_args.args[0]
        self.assertIn('--property=User=' + plugins.USER, cmd)
        self.assertIn('--property=KillMode=control-group', cmd)
        self.assertEqual(cmd[-1], 'main.py')
        self.assertIn('--setenv=PIOS_PLUGIN_PORT=8765', cmd)

    def test_stop_and_remove(self):
        self.import_template()
        with patch.object(plugins, 'run', return_value=(0, '', '')) as run:
            self.assertTrue(plugins.action('benim-eklentim', 'stop')[0])
        run.assert_called_once_with(['systemctl', 'stop', 'pios-plugin-benim-eklentim.service'], timeout=20)
        with patch.object(plugins, '_states', return_value=({'benim-eklentim': {'ActiveState': 'active'}}, '')):
            self.assertFalse(plugins.action('benim-eklentim', 'remove')[0])
        with patch.object(plugins, '_states', return_value=({}, '')):
            self.assertTrue(plugins.action('benim-eklentim', 'remove')[0])
        self.assertFalse((self.base / 'benim-eklentim').exists())
        self.assertEqual(len(list((self.base / '.silinenler').iterdir())), 1)

    def test_missing_service_is_not_fatal_for_new_plugin(self):
        with patch.object(plugins, 'run', return_value=(1, '', 'Unit pios-plugin-test.service could not be found.')):
            self.assertEqual(plugins._states(['test']), ({}, ''))

    def test_invalid_plugin_id_does_not_invoke_systemctl(self):
        with patch.object(plugins, 'run') as run:
            self.assertFalse(plugins.action('../../ssh', 'stop')[0])
        run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
