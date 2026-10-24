"""Donanım olmadan: python3 -m unittest discover -s image/test/unit -v"""
import pathlib
import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, mock_open, patch

CENTER = pathlib.Path(__file__).resolve().parents[2] / 'overlay/usr/lib/pios/center'
sys.path.insert(0, str(CENTER))
import activity
import camera
import common
import fan
import network
import system


class NetworkTests(unittest.TestCase):
    def test_missing_hotspot_profile_created_open(self):
        with patch.object(network, 'profiles', return_value=[]), patch.object(network, 'checked') as checked:
            network.ensure_hotspot('wlan0')
        args = checked.call_args.args[0]
        self.assertIn('MakerOS', args)
        self.assertIn('10.42.0.1/24', args)
        self.assertIn(network.HOTSPOT, args)
        self.assertFalse([a for a in args if a.startswith('wifi-sec')])

    def test_old_hotspot_profile_replaced(self):
        old = [{'name': network.HOTSPOT, 'uuid': 'u', 'type': '802-11-wireless'}]
        with patch.object(network, 'profiles', return_value=old), patch.object(network, 'nm', return_value=(0, 'Pardus-Maker-1234\nwpa-psk', '')), patch.object(network, 'checked') as checked:
            network.ensure_hotspot('wlan0')
        self.assertEqual(checked.call_args_list[0].args[0], ['connection', 'delete', 'id', network.HOTSPOT])
        self.assertIn('MakerOS', checked.call_args.args[0])

    def test_current_hotspot_profile_kept(self):
        cur = [{'name': network.HOTSPOT, 'uuid': 'u', 'type': '802-11-wireless'}]
        with patch.object(network, 'profiles', return_value=cur), patch.object(network, 'nm', return_value=(0, 'MakerOS\n', '')), patch.object(network, 'checked') as checked:
            network.ensure_hotspot('wlan0')
        checked.assert_not_called()

    def test_missing_networkmanager_returns_diagnostics(self):
        with patch.object(network, 'nm', return_value=(10, '', 'NetworkManager is not running')):
            s = network.overview()
        self.assertFalse(s['nm'])
        self.assertEqual(s['saved'], [])
        self.assertIn('not running', s['warning'])

    def test_escaped_ssid(self):
        self.assertEqual(network.split(r'*:Ev\:oda\\:72:WPA2'), ['*', 'Ev:oda\\', '72', 'WPA2'])

    def test_scan_merges_and_preserves_active(self):
        rows = '*:Ev:45:WPA2\n:Ev:90:WPA2\n:Okul:55:WPA2 802.1X\n::10:--'
        with patch.object(network, 'device', return_value='wlan1'), patch.object(network, 'is_hotspot', return_value=False), patch.object(network, 'saved', return_value=['Ev']), patch.object(network, 'checked', side_effect=['', rows]) as cmd:
            nets = network.scan()
        self.assertEqual(len(nets), 2)
        self.assertEqual(nets[0]['ssid'], 'Ev')
        self.assertTrue(nets[0]['active'])
        self.assertEqual(nets[0]['signal'], 90)
        self.assertTrue(nets[1]['enterprise'])
        self.assertIn('wlan1', cmd.call_args.args[0])

    def test_ap_scan_pauses_restores_and_caches(self):
        rows = ':Ev:70:WPA2\n:MakerOS:99:--'
        with tempfile.TemporaryDirectory() as d, patch.object(network, 'SCAN_CACHE', d + '/scan.txt'), patch.object(network, 'device', return_value='wlan0'), patch.object(network, 'is_hotspot', return_value=True), patch.object(network, 'saved', return_value=[]), patch.object(network.time, 'sleep'), patch.object(network, 'checked', side_effect=['', '', rows, '']) as cmd:
            nets = network.scan()
            self.assertEqual([n['ssid'] for n in nets], ['Ev'])
            self.assertEqual(cmd.call_args_list[0].args[0], ['connection', 'down', 'id', network.HOTSPOT])
            self.assertEqual(cmd.call_args.args[0], ['connection', 'up', 'id', network.HOTSPOT])
            self.assertEqual([n['ssid'] for n in network.cached_networks()], ['Ev'])

    def test_failed_scan_restores_hotspot(self):
        def command(args, *rest, **kwargs):
            if 'list' in args:
                raise network.NetworkError('Tarama reddedildi: aygıt meşgul')
            return ''
        with patch.object(network, 'device', return_value='wlan0'), patch.object(network, 'is_hotspot', return_value=True), patch.object(network.time, 'sleep'), patch.object(network, 'checked', side_effect=command) as cmd:
            with self.assertRaisesRegex(network.NetworkError, 'meşgul'):
                network.scan()
            self.assertEqual(cmd.call_args.args[0], ['connection', 'up', 'id', network.HOTSPOT])

    def test_wrong_password_restores_previous_network_and_redacts(self):
        def command(args, *rest, **kwargs):
            if 'connect' in args:
                raise network.NetworkError('Authentication failed: super-secret')
            return ''
        with patch.object(network, 'device', return_value='wlan0'), patch.object(network, 'is_hotspot', return_value=False), patch.object(network, 'active_uuid', return_value='previous-uuid'), patch.object(network, 'profiles', return_value=[]), patch.object(network, 'checked', side_effect=command), patch.object(network, 'nm', return_value=(0, '', '')) as nm:
            with self.assertRaises(network.NetworkError) as caught:
                network.connect({'ssid': 'Ev', 'password': 'super-secret'})
            self.assertNotIn('super-secret', str(caught.exception))
            self.assertIn('geri açıldı', str(caught.exception))
            nm.assert_called_once_with(['connection', 'up', 'uuid', 'previous-uuid'], 25)

    def test_failed_recovery_opens_hotspot(self):
        with patch.object(network, 'device', return_value='wlan0'), patch.object(network, 'is_hotspot', return_value=False), patch.object(network, 'active_uuid', return_value='old'), patch.object(network, 'profiles', return_value=[]), patch.object(network, 'checked', side_effect=network.NetworkError('radio error')), patch.object(network, 'ensure_hotspot'), patch.object(network, 'nm', side_effect=[(1, '', 'old failed'), (0, '', '')]) as nm:
            with self.assertRaises(network.NetworkError):
                network.connect({'ssid': 'Ev'})
            self.assertEqual(nm.call_args.args[0], ['connection', 'up', 'id', network.HOTSPOT])

    def test_saved_password_change_preserves_original_on_failure(self):
        old = {'name': 'Ev', 'uuid': 'old', 'type': '802-11-wireless'}
        clone = {'name': 'pios-attempt-abcd', 'uuid': 'clone', 'type': '802-11-wireless'}
        def command(args, *rest, **kwargs):
            if args[:2] == ['connection', 'up']:
                raise network.NetworkError('authentication failed')
            return ''
        with patch.object(network, 'device', return_value='wlan0'), patch.object(network, 'is_hotspot', return_value=False), patch.object(network, 'active_uuid', return_value='old'), patch.object(network.secrets, 'token_hex', return_value='abcd'), patch.object(network, 'profiles', side_effect=[[old], [old, clone]]), patch.object(network, 'checked', side_effect=command) as cmd, patch.object(network, 'nm', return_value=(0, '', '')) as nm:
            with self.assertRaises(network.NetworkError):
                network.connect({'ssid': 'Ev', 'password': 'wrong'})
            self.assertFalse(any(c.args[0][:4] == ['connection', 'modify', 'uuid', 'old'] for c in cmd.call_args_list))
            self.assertEqual(nm.call_args.args[0], ['connection', 'delete', 'uuid', 'clone'])

    def test_parallel_request_rejected(self):
        network._lock.acquire()
        try:
            ok, message = network.start('connect', {'ssid': 'Ev'})
            self.assertFalse(ok)
            self.assertIn('devam', message)
        finally:
            network._lock.release()

    def test_utf8_ssid_limit(self):
        with self.assertRaises(network.NetworkError):
            network.connect({'ssid': 'ğ' * 17})


class RemoteFitTests(unittest.TestCase):
    def test_monitor_attached_keeps_resolution(self):
        with patch.object(system, 'monitor_attached', return_value=True), patch.object(system, 'run_user') as cmd:
            ok, _ = system.fit_remote(2000, 1037)
        self.assertFalse(ok)
        cmd.assert_not_called()

    def test_browser_ratio_at_720p(self):
        with patch.object(system, 'monitor_attached', return_value=False), patch.object(system, 'run_user', return_value=(0, 'same\n', '')) as cmd:
            self.assertEqual(system.fit_remote(2000, 1037), (True, '1392x720'))
            self.assertEqual(cmd.call_args.args[0][-2:], ['1392', '720'])
            # Cok genis/dar pencere 21:9 ve 4:3 ile sinirlanir.
            self.assertEqual(system.fit_remote(5000, 500)[1], '1680x720')
            self.assertEqual(system.fit_remote(500, 900)[1], '960x720')

    def test_changed_mode_restarts_vnc(self):
        with patch.object(system, 'monitor_attached', return_value=False), patch.object(system, 'run_user', return_value=(0, '', '')), patch.object(system.os.path, 'isdir', return_value=True), patch.object(system, 'run', return_value=(0, '', '')) as root:
            self.assertTrue(system.fit_remote(2000, 1037)[0])
        root.assert_called_once_with(['systemctl', 'restart', 'pios-vnc.service'], timeout=20)

    def test_same_mode_keeps_vnc(self):
        with patch.object(system, 'monitor_attached', return_value=False), patch.object(system, 'run_user', return_value=(0, 'same\n', '')), patch.object(system, 'run') as root:
            self.assertTrue(system.fit_remote(2000, 1037)[0])
        root.assert_not_called()

    def test_invalid_size_rejected(self):
        with patch.object(system, 'run_user') as cmd:
            self.assertFalse(system.fit_remote('x', 10)[0])
            self.assertFalse(system.fit_remote(100, 50)[0])
        cmd.assert_not_called()


class FanTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.config = pathlib.Path(self.tmp.name) / 'config.txt'
        self.config.write_text('[pi4]\ndtoverlay=vc4-kms-v3d\n[all]\n')
        self.patch = patch.object(fan.system, 'CONFIG_TXT', str(self.config))
        self.patch.start()
        self.hardware = patch.object(fan.system, 'hardware', return_value={})
        self.hardware.start()

    def tearDown(self):
        self.hardware.stop()
        self.patch.stop()
        self.tmp.cleanup()

    def test_thresholds_roundtrip_and_no_duplicate(self):
        for _ in range(2):
            ok, _ = fan.save({'mode': 'auto', 'pin': 14, 'on_c': 65, 'off_c': 50})
            self.assertTrue(ok)
        self.assertEqual(self.config.read_text().count('dtoverlay=gpio-fan'), 1)
        s = fan.status()
        self.assertEqual((s['on_c'], s['off_c'], s['pin']), (65, 50, 14))
        self.assertIn('dtoverlay=vc4-kms-v3d', self.config.read_text())

    def test_invalid_threshold_does_not_write(self):
        before = self.config.read_text()
        self.assertFalse(fan.save({'mode': 'auto', 'on_c': 50, 'off_c': 60})[0])
        self.assertEqual(before, self.config.read_text())

    def test_uart_pin_conflict(self):
        with patch.object(fan.system, 'hardware', return_value={'serial': True}):
            self.assertFalse(fan.save({'mode': 'auto', 'pin': 14})[0])

    def test_always_and_disable(self):
        self.assertTrue(fan.save({'mode': 'always', 'pin': 18})[0])
        self.assertEqual(fan.status()['mode'], 'always')
        self.assertTrue(fan.save({'mode': 'unmanaged'})[0])
        self.assertNotIn('gpio-fan', self.config.read_text())


class ActivityTests(unittest.TestCase):
    def test_proc_parsing_process_name_and_memory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / '42').mkdir()
            fields = ['S'] + ['0'] * 21
            fields[11], fields[12], fields[19], fields[21] = '20', '10', '100', '25'
            (root / '42/stat').write_text('42 (worker (camera)) ' + ' '.join(fields))
            (root / 'stat').write_text('cpu  10 0 20 100 5 0 0 0 0 0\n')
            (root / 'meminfo').write_text('MemTotal: 1000 kB\nMemAvailable: 400 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB')
            with patch.object(activity, 'PROC', tmp), patch.object(activity.os, 'sysconf', return_value=4096):
                p = activity._processes()[42]
                self.assertEqual(p['name'], 'worker (camera)')
                self.assertEqual(p['ticks'], 30)
                self.assertEqual(p['memory'], 25 * 4096)
                self.assertEqual(activity._cpu(), (135, 105))
                self.assertEqual(activity._memory()['percent'], 60)

    def test_sampler_delta_and_counter_reset(self):
        from copy import deepcopy
        def process(ticks):
            return {1: {'pid': 1, 'ticks': ticks, 'start': '100', 'memory': 100, 'name': 'worker'}}
        def net(rx):
            return {'wlan0': {'rx': rx, 'tx': rx, 'name': 'wlan0'}}
        with patch.object(activity, '_cpu', side_effect=[(100, 50), (300, 100)]), patch.object(activity, '_memory', return_value={'total': 1000}), patch.object(activity, '_processes', side_effect=[process(10), process(110)]), patch.object(activity, '_interfaces', side_effect=[net(1000), net(100)]), patch.object(activity.os, 'sysconf', return_value=100), patch.object(activity.time, 'monotonic', side_effect=[10, 10, 12, 12]), patch.object(activity.time, 'sleep', side_effect=[None, StopIteration]):
            with self.assertRaises(StopIteration):
                activity._loop()
        s = deepcopy(activity._snapshot)
        self.assertTrue(s['ready'])
        self.assertEqual(s['cpu_percent'], 75)
        self.assertEqual(s['processes'][0]['cpu'], 50)
        self.assertEqual(s['interfaces'][0]['rx_bps'], 0)


class StabilityTests(unittest.TestCase):
    def test_timeout_does_not_expose_password(self):
        cmd = ['nmcli', 'password', 'secret-password']
        with patch.object(common.subprocess, 'run', side_effect=subprocess.TimeoutExpired(cmd, 10)):
            rc, out, err = common.run(cmd, timeout=10)
        self.assertEqual(rc, 124)
        self.assertNotIn('secret-password', err)

    def test_job_cursor_survives_log_rotation(self):
        jobs = common.Jobs()
        jobs._jobs['1'] = {'id': '1', 'title': 'test', 'status': 'running', 'offset': 500, 'lines': ['line'] * 1501}
        self.assertEqual(jobs.get('1', 2000)['lines'], ['line'])
        self.assertEqual(jobs.get('1', 2000)['next'], 2001)
        self.assertEqual(jobs.get('1', 0)['next'], 2001)

    def test_photo_name_rejects_path(self):
        ok, message, name = camera.take_photo('uvc:video0', '../escape')
        self.assertFalse(ok)
        self.assertIsNone(name)

    def test_photo_name_accepts_turkish(self):
        with patch.object(camera, '_take_photo', return_value=(True, 'ok', 'test.jpg')) as capture:
            self.assertTrue(camera.take_photo('cam', 'Öğrenci deney-1.jpg')[0])
            capture.assert_called_once_with('cam', 'Öğrenci deney-1')


class ApiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Token oluşturmayı ve Linux crypt bağımlılığını testte yalıt.
        spec = importlib.util.spec_from_file_location('pios_server_test', CENTER / 'server.py')
        cls.server = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'crypt': Mock()}), patch('builtins.open', mock_open()), patch('os.makedirs'), patch('os.replace'), patch('os.chmod'), patch('os.chown'):
            spec.loader.exec_module(cls.server)

    def handler(self, path, data=None):
        handler = object.__new__(self.server.Handler)
        handler.path = path
        handler.client_address = ('127.0.0.1', 1234)
        handler.try_auth = Mock(return_value=True)
        handler.send_json = Mock()
        handler.connection = Mock()
        handler.headers = {'X-Pios': '1'}
        if data is not None:
            raw = json.dumps(data).encode()
            handler.rfile = io.BytesIO(raw)
            handler.headers['Content-Length'] = str(len(raw))
        return handler

    def test_setup_requires_hotspot_client(self):
        h = self.handler('/api/setup/connect', {'ssid': 'Ev', 'password': 'parola123'})
        with patch.object(network, 'hotspot_on', return_value=True), patch.object(network, 'start') as start:
            h.do_POST()
            start.assert_not_called()
        self.assertEqual(h.send_json.call_args.args[1], 403)
        h = self.handler('/api/setup/connect', {'ssid': 'Ev', 'password': 'parola123'})
        h.client_address = ('10.42.0.23', 5555)
        with patch.object(network, 'hotspot_on', return_value=True), patch.object(network, 'start', return_value=(True, 'ok')) as start:
            h.do_POST()
            start.assert_called_once_with('connect', {'ssid': 'Ev', 'password': 'parola123'})

    def test_captive_redirects_foreign_host_only_in_hotspot(self):
        h = self.handler('/generate_204')
        h.headers['Host'] = 'connectivitycheck.gstatic.com'
        h.redirect = Mock()
        with patch.object(network, 'hotspot_on', return_value=True):
            h.do_GET()
        h.redirect.assert_called_once_with('http://10.42.0.1/kurulum')
        h = self.handler('/api/wifi')
        h.headers['Host'] = 'router.lan'
        h.redirect = Mock()
        with patch.object(network, 'hotspot_on', return_value=False), patch.object(network, 'operation', return_value={}):
            h.do_GET()
        h.redirect.assert_not_called()

    def test_scan_is_async_and_get_is_readonly(self):
        h = self.handler('/api/wifi/scan', {'interrupt': True})
        with patch.object(network, 'start', return_value=(True, 'Başlatıldı')) as start:
            h.do_POST()
            start.assert_called_once_with('scan', {'interrupt': True})
        self.assertTrue(h.send_json.call_args.args[0]['ok'])
        h = self.handler('/api/wifi')
        with patch.object(network, 'operation', return_value={'status': 'idle'}), patch.object(network, 'start') as start:
            h.do_GET()
            start.assert_not_called()
        self.assertEqual(h.send_json.call_args.args[0]['status'], 'idle')

    def test_fan_post_and_photo_name_dispatch(self):
        h = self.handler('/api/fan', {'mode': 'auto', 'pin': 14, 'on_c': 60, 'off_c': 50})
        with patch.object(fan, 'save', return_value=(True, 'Kaydedildi')) as save:
            h.do_POST()
            self.assertEqual(save.call_args.args[0]['on_c'], 60)
        h = self.handler('/api/camera/photo', {'cam': 'csi:0', 'name': 'Deney'})
        with patch.object(camera, 'take_photo', return_value=(True, 'Kaydedildi', 'deney.jpg')) as photo:
            h.do_POST()
            photo.assert_called_once_with('csi:0', 'Deney')

    def test_unexpected_error_has_details_and_reference(self):
        h = self.handler('/api/activity')
        with patch.object(activity, 'snapshot', side_effect=OSError('proc okunamadı')), patch.object(self.server.logging, 'exception'):
            h.do_GET()
        payload, code = h.send_json.call_args.args
        self.assertEqual(code, 500)
        self.assertIn('proc okunamadı', payload['details'])
        self.assertTrue(payload['reference'])

    def test_invalid_json_shape_rejected(self):
        h = self.handler('/api/fan', ['unexpected'])
        h.do_POST()
        self.assertEqual(h.send_json.call_args.args[1], 400)


if __name__ == '__main__':
    unittest.main()
