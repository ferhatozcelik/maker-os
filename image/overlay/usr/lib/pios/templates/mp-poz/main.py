"""MediaPipe Poz Algilama: vucut iskeletini (33 nokta) cizer, squat ve
kol kaldirma tekrarlarini sayar. Kameraya tum vucudunuz gorunecek kadar
uzakta durun (yaklasik 2-3 m).

Calistirma:  uv run main.py
Web yayini:  http://pardus-maker.local:8000/
"""
import math
import os

from mediapipe.tasks.python import BaseOptions, vision
from mediapipe.tasks.python.vision import drawing_styles, drawing_utils

from pios_kamera import Gosterici, Kamera, arguman_ayristirici, mp_goruntu, zaman_ms

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "pose_landmarker_lite.task")
# Nokta numaralari: 11/12 omuz, 15/16 bilek, 23/24 kalca, 25/26 diz, 27/28 ayak bilegi


def aci(a, b, c):
    """b noktasindaki aci (derece)."""
    ab = (a.x - b.x, a.y - b.y)
    cb = (c.x - b.x, c.y - b.y)
    cos = (ab[0] * cb[0] + ab[1] * cb[1]) / (math.hypot(*ab) * math.hypot(*cb) + 1e-6)
    return math.degrees(math.acos(max(-1.0, min(1.0, cos))))


def main():
    args = arguman_ayristirici("MediaPipe poz algilama ve egzersiz sayaci").parse_args()
    secenek = vision.PoseLandmarkerOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=vision.RunningMode.VIDEO, num_poses=1)
    squat, kol, asagida, yukarida = 0, 0, False, False
    with vision.PoseLandmarker.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik) as kam, \
            Gosterici("Poz ve egzersiz", args.port, not args.pencere_yok) as ekran:
        for kare in kam:
            sonuc = model.detect_for_video(mp_goruntu(kare), zaman_ms())
            if sonuc.pose_landmarks:
                p = sonuc.pose_landmarks[0]
                drawing_utils.draw_landmarks(kare, p, vision.PoseLandmarksConnections.POSE_LANDMARKS,
                                             drawing_styles.get_default_pose_landmarks_style())
                diz = (aci(p[23], p[25], p[27]) + aci(p[24], p[26], p[28])) / 2
                if diz < 100:
                    asagida = True
                elif diz > 160 and asagida:
                    asagida = False
                    squat += 1
                kollar = p[15].y < p[11].y and p[16].y < p[12].y
                if kollar and not yukarida:
                    kol += 1
                yukarida = kollar
                ekran.yaz(kare, "Diz acisi: %d" % diz, 2, (200, 200, 200))
            ekran.yaz(kare, "Squat: %d" % squat)
            ekran.yaz(kare, "Kol kaldirma: %d" % kol, 1)
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
