"""MediaPipe El Takibi: kameradaki elleri bulur, 21 eklem noktasini cizer ve
kac parmagin acik oldugunu sayar.

Calistirma:  uv run main.py            (Pi kamera veya USB kamera)
             uv run main.py --kaynak el.mp4
Web yayini:  http://pardus-maker.local:8000/
"""
import math
import os

import cv2
from mediapipe.tasks.python import BaseOptions, vision
from mediapipe.tasks.python.vision import drawing_styles, drawing_utils

from pios_kamera import Gosterici, Kamera, arguman_ayristirici, mp_goruntu, zaman_ms

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "hand_landmarker.task")
PARMAK_UCLARI = [8, 12, 16, 20]   # isaret, orta, yuzuk, serce
PARMAK_ORTA = [6, 10, 14, 18]     # her parmagin ikinci eklemi


def uzaklik(a, b):
    return math.hypot(a.x - b.x, a.y - b.y)


def acik_parmak_sayisi(el):
    """Ucu bilege (0), ikinci ekleminden belirgin sekilde uzak olan parmak
    acik sayilir; el hangi yone donuk olursa olsun calisir. Basparmak icin
    ucun isaret parmagi kokune (5) uzakligina bakilir."""
    sayi = sum(1 for u, o in zip(PARMAK_UCLARI, PARMAK_ORTA) if uzaklik(el[u], el[0]) > uzaklik(el[o], el[0]) * 1.15)
    if uzaklik(el[4], el[5]) > uzaklik(el[3], el[5]) * 1.25:
        sayi += 1
    return sayi


def main():
    args = arguman_ayristirici("MediaPipe el takibi ve parmak sayma").parse_args()
    secenek = vision.HandLandmarkerOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=vision.RunningMode.VIDEO, num_hands=2,
        min_hand_detection_confidence=0.5, min_tracking_confidence=0.5)
    with vision.HandLandmarker.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik) as kam, \
            Gosterici("El takibi", args.port, not args.pencere_yok) as ekran:
        for kare in kam:
            kare = cv2.flip(kare, 1)  # ayna goruntusu: elinizi dogal hareket ettirin
            sonuc = model.detect_for_video(mp_goruntu(kare), zaman_ms())
            toplam = 0
            for i, el in enumerate(sonuc.hand_landmarks):
                drawing_utils.draw_landmarks(
                    kare, el, vision.HandLandmarksConnections.HAND_CONNECTIONS,
                    drawing_styles.get_default_hand_landmarks_style(),
                    drawing_styles.get_default_hand_connections_style())
                sayi = acik_parmak_sayisi(el)
                toplam += sayi
                h, w = kare.shape[:2]
                cv2.putText(kare, str(sayi), (int(el[0].x * w) - 10, int(el[0].y * h) + 40),
                            cv2.FONT_HERSHEY_SIMPLEX, 1.4, (0, 220, 255), 3, cv2.LINE_AA)
            ekran.yaz(kare, "Eller: %d   Acik parmak: %d" % (len(sonuc.hand_landmarks), toplam))
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
