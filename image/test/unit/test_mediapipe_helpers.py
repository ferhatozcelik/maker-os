import dataclasses
import importlib.util
import json
import pathlib
import sys
import tempfile
import types
import threading
import unittest
from unittest.mock import Mock, patch

TEMPLATES = pathlib.Path(__file__).resolve().parents[2] / 'overlay/usr/lib/pios/templates'
sys.path.insert(0, str(TEMPLATES / '_ortak'))
try:
    import cv2
    import numpy as np
    import pios_kamera as kamera
    HAVE_CV = True
except ImportError:
    HAVE_CV = False


@unittest.skipUnless(HAVE_CV, 'OpenCV ve NumPy gerekli')
class MediaPipeHelperTests(unittest.TestCase):
    def test_live_camera_keeps_latest_frame(self):
        ready, release = threading.Event(), threading.Event()
        class Capture:
            def __init__(self):
                self.count = 0
            def set(self, *args):
                pass
            def isOpened(self):
                return True
            def read(self):
                if self.count >= 20:
                    ready.set()
                    release.wait(2)
                    return False, None
                self.count += 1
                return True, np.full((16, 16, 3), self.count, dtype=np.uint8)
            def release(self):
                pass
        with patch.object(cv2, 'VideoCapture', return_value=Capture()):
            source = kamera.Kamera('usb')
            try:
                self.assertTrue(ready.wait(2))
                frame = next(iter(source))
                self.assertEqual(int(frame[0, 0, 0]), 20)
            finally:
                release.set()
                source.kapat()

    def test_profiles_and_limits(self):
        p = kamera.arguman_ayristirici('test')
        a = p.parse_args(['--profil', 'tasarruf'])
        self.assertEqual((a.genislik, a.yukseklik, a.fps, a.cekirdek), (480, 360, 6, 1))
        with patch('sys.stderr'), self.assertRaises(SystemExit):
            p.parse_args(['--fps', '100'])

    def test_thermal_rate_and_bounded_cooling_wait(self):
        governor = kamera.IsiDenetimi(10, 75)
        self.assertEqual(governor.hedef(60)[0], 10)
        self.assertEqual(governor.hedef(70)[0], 5)
        self.assertEqual(governor.hedef(75)[0], 2.5)
        self.assertEqual(governor.hedef(80)[0], 0)
        with patch.object(governor, 'oku', return_value=80), patch.object(kamera.time, 'monotonic', side_effect=[0, 121]):
            with self.assertRaisesRegex(RuntimeError, '120 saniyede'):
                governor.bekle()

    def test_static_image_processed_once_and_aspect_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            file = pathlib.Path(tmp) / 'resim.png'
            cv2.imwrite(str(file), np.zeros((600, 1200, 3), dtype=np.uint8))
            with kamera.Kamera(str(file), 640, 480) as source:
                frames = list(source)
            self.assertEqual(len(frames), 1)
            self.assertEqual(frames[0].shape, (320, 640, 3))

    def test_image_folder_visits_once_each(self):
        with tempfile.TemporaryDirectory() as tmp:
            for n in ['b.png', 'a.jpg']:
                cv2.imwrite(str(pathlib.Path(tmp) / n), np.zeros((20, 20, 3), dtype=np.uint8))
            with kamera.Kamera(tmp) as source:
                paths = []
                for frame in source:
                    paths.append(pathlib.Path(source.son_kaynak).name)
            self.assertEqual(paths, ['a.jpg', 'b.png'])

    def test_video_timestamps_strictly_increasing(self):
        with patch.object(kamera.time, 'monotonic', return_value=100):
            times = [kamera.zaman_ms() for _ in range(10)]
        self.assertEqual(times, sorted(set(times)))

    def test_no_jpeg_encoding_without_viewer(self):
        with kamera.Gosterici('test', port=0, pencere=False) as screen:
            screen.sunucu = Mock()
            with patch.object(cv2, 'imencode') as encode:
                screen.goster(np.zeros((100, 100, 3), dtype=np.uint8))
                encode.assert_not_called()
            screen.sunucu = None

    def test_export_image_and_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = types.SimpleNamespace(resim_modu=True, son_kaynak='/a/resim.jpg', analiz_ms=42,
                                           sonuc={'detections': []}, denetim=kamera.IsiDenetimi())
            with kamera.Gosterici('test', 0, False, kamera=source, cikti=tmp) as screen:
                screen.goster(np.zeros((100, 100, 3), dtype=np.uint8))
            self.assertEqual(len(list(pathlib.Path(tmp).glob('*.png'))), 1)
            data = json.loads(next(pathlib.Path(tmp).glob('*.json')).read_text())
            self.assertEqual(data['analiz_ms'], 42)
            self.assertEqual(data['sonuc'], {'detections': []})

    def test_all_seven_examples_use_image_api_and_export(self):
        @dataclasses.dataclass
        class Result:
            detections: list = dataclasses.field(default_factory=list)
            hand_landmarks: list = dataclasses.field(default_factory=list)
            gestures: list = dataclasses.field(default_factory=list)
            face_landmarks: list = dataclasses.field(default_factory=list)
            pose_landmarks: list = dataclasses.field(default_factory=list)
            confidence_masks: list = dataclasses.field(default_factory=lambda: [types.SimpleNamespace(numpy_view=lambda: np.ones((32, 32), dtype=np.float32))])
        vision = types.ModuleType('mediapipe.tasks.python.vision')
        vision.RunningMode = types.SimpleNamespace(IMAGE='IMAGE', VIDEO='VIDEO')
        models = []
        for name in ['HandLandmarker', 'GestureRecognizer', 'FaceDetector', 'PoseLandmarker', 'ObjectDetector', 'ImageSegmenter', 'FaceLandmarker']:
            setattr(vision, name + 'Options', lambda **kwargs: types.SimpleNamespace(**kwargs))
            model = Mock()
            model.__enter__ = Mock(return_value=model)
            model.__exit__ = Mock(return_value=False)
            for method in ['detect', 'recognize', 'segment']:
                getattr(model, method).return_value = Result()
            factory = Mock()
            factory.create_from_options.return_value = model
            setattr(vision, name, factory)
            models.append((name, factory, model))
        vision.drawing_styles = Mock()
        vision.drawing_utils = Mock()
        python = types.ModuleType('mediapipe.tasks.python')
        python.BaseOptions = lambda **kwargs: kwargs
        python.vision = vision
        modules = {'mediapipe': types.ModuleType('mediapipe'), 'mediapipe.tasks': types.ModuleType('mediapipe.tasks'),
                   'mediapipe.tasks.python': python, 'mediapipe.tasks.python.vision': vision,
                   'gpiozero': None}
        with tempfile.TemporaryDirectory() as tmp, patch.dict(sys.modules, modules), patch.object(kamera, 'mp_goruntu', side_effect=lambda x: x):
            image = pathlib.Path(tmp) / 'input.png'
            cv2.imwrite(str(image), np.zeros((120, 160, 3), dtype=np.uint8))
            for file in sorted(TEMPLATES.glob('mp-*/main.py')):
                out = pathlib.Path(tmp) / file.parent.name
                spec = importlib.util.spec_from_file_location('example_' + file.parent.name, file)
                mod = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(mod)
                def configure(args):
                    args.denetim = kamera.IsiDenetimi(args.fps)
                with patch.object(mod, 'performans_ayarla', side_effect=configure), patch.object(mod, 'FOTO_KLASORU', str(pathlib.Path(tmp) / 'photos'), create=True), patch.object(sys, 'argv', [str(file), '--kaynak', str(image), '--port', '0', '--pencere-yok', '--cikti', str(out)]):
                    mod.main()
                self.assertEqual(len(list(out.glob('*.png'))), 1, file.parent.name)
                self.assertEqual(len(list(out.glob('*.json'))), 1, file.parent.name)
            for name, factory, model in models:
                self.assertEqual(factory.create_from_options.call_args.args[0].running_mode, 'IMAGE', name)
                calls = sum(getattr(model, kind).call_count for kind in ['detect', 'recognize', 'segment'])
                self.assertEqual(calls, 1, name)
                model.detect_for_video.assert_not_called()


if __name__ == '__main__':
    unittest.main()
