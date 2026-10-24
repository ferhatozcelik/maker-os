"""MediaPipe Yuz Algilama: yuzleri cerceveler, yeni bir yuz gorununce
otomatik fotograf ceker (photos/ klasorune).

Calistirma:  uv run main.py
             uv run main.py --fotograf-yok
Web yayini:  http://maker-os.local:8000/
"""
from pios_camera import Gosterici, Kamera, arguman_ayristirici, analiz, calisma_modu, performans_ayarla

import os
import time

import cv2
from mediapipe.tasks.python import BaseOptions, vision

KLASOR = os.path.dirname(os.path.abspath(__file__))
MODEL = os.path.join(KLASOR, "models", "blaze_face_short_range.tflite")
FOTO_KLASORU = os.path.join(KLASOR, "photos")
BEKLEME_SN = 5   # iki otomatik fotograf arasi en az sure


def main():
    p = arguman_ayristirici("MediaPipe yuz algilama ve otomatik fotograf")
    p.add_argument("--fotograf-yok", action="store_true", help="otomatik fotograf cekme")
    args = p.parse_args()
    performans_ayarla(args)
    os.makedirs(FOTO_KLASORU, exist_ok=True)
    secenek = vision.FaceDetectorOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=calisma_modu(args), min_detection_confidence=0.5)
    onceki, son_foto = 0, 0.0
    with vision.FaceDetector.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik, args.fps, args.denetim) as kam, \
            Gosterici("Yuz algilama", args.port, not args.pencere_yok, kamera=kam, cikti=args.cikti, kalite=args.jpeg_kalite) as ekran:
        for kare in kam:
            sonuc = analiz(model, kare, kam, "detect")
            temiz = kare.copy()
            for yuz in sonuc.detections:
                k = yuz.bounding_box
                cv2.rectangle(kare, (k.origin_x, k.origin_y), (k.origin_x + k.width, k.origin_y + k.height),
                              (0, 200, 255), 2)
                cv2.putText(kare, "%%%d" % (yuz.categories[0].score * 100), (k.origin_x, k.origin_y - 8),
                            cv2.FONT_HERSHEY_SIMPLEX, 0.6, (0, 200, 255), 2)
            sayi = len(sonuc.detections)
            if not args.fotograf_yok and sayi > onceki and time.time() - son_foto > BEKLEME_SN:
                ad = os.path.join(FOTO_KLASORU, time.strftime("yuz-%Y%m%d-%H%M%S.jpg"))
                cv2.imwrite(ad, temiz)
                son_foto = time.time()
                print("Fotograf kaydedildi:", ad)
            onceki = sayi
            ekran.yaz(kare, "Yuz sayisi: %d" % sayi)
            if time.time() - son_foto < 1.5:
                ekran.yaz(kare, "Fotograf cekildi!", 1, (0, 255, 0))
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
