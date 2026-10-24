"""MediaPipe Nesne Algilama (EfficientDet-Lite0, 80 COCO sinifi): kisi,
araba, kedi, sise, telefon... Kisi gorulunce GPIO17'deki LED/buzzer'i
calistirabilir (--alarm).

Calistirma:  uv run main.py
             uv run main.py --alarm          (kisi gorulunce GPIO17)
Web yayini:  http://pardus-maker.local:8000/
"""
from pios_camera import Gosterici, Kamera, arguman_ayristirici, analiz, calisma_modu, performans_ayarla

import os

import cv2
from mediapipe.tasks.python import BaseOptions, vision

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "efficientdet_lite0.tflite")
TURKCE = {
    "person": "kisi", "bicycle": "bisiklet", "car": "araba", "motorcycle": "motosiklet", "bus": "otobus",
    "truck": "kamyon", "cat": "kedi", "dog": "kopek", "bird": "kus", "bottle": "sise", "cup": "fincan",
    "chair": "sandalye", "couch": "kanepe", "tv": "televizyon", "laptop": "dizustu", "mouse": "fare",
    "keyboard": "klavye", "cell phone": "telefon", "book": "kitap", "clock": "saat", "scissors": "makas",
    "backpack": "sirt cantasi", "umbrella": "semsiye", "banana": "muz", "apple": "elma", "orange": "portakal",
    "potted plant": "saksi bitkisi", "dining table": "masa", "remote": "kumanda", "teddy bear": "oyuncak ayi",
}


def main():
    p = arguman_ayristirici("MediaPipe nesne algilama")
    p.add_argument("--alarm", action="store_true", help="kisi gorulunce GPIO17'yi etkinlestir")
    p.add_argument("--esik", type=float, default=0.45, help="en dusuk guven (0-1)")
    args = p.parse_args()
    performans_ayarla(args)
    alarm = None
    if args.alarm:
        try:
            from gpiozero import LED
            alarm = LED(17)
        except Exception as e:
            print("GPIO kullanilamiyor:", e)
    secenek = vision.ObjectDetectorOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=calisma_modu(args), max_results=8, score_threshold=args.esik)
    with vision.ObjectDetector.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik, args.fps, args.denetim) as kam, \
            Gosterici("Nesne algilama", args.port, not args.pencere_yok, kamera=kam, cikti=args.cikti, kalite=args.jpeg_kalite) as ekran:
        for kare in kam:
            sonuc = analiz(model, kare, kam, "detect")
            kisi = 0
            for d in sonuc.detections:
                k, c = d.bounding_box, d.categories[0]
                kisi += c.category_name == "person"
                ad = TURKCE.get(c.category_name, c.category_name)
                cv2.rectangle(kare, (k.origin_x, k.origin_y), (k.origin_x + k.width, k.origin_y + k.height),
                              (255, 160, 0), 2)
                cv2.putText(kare, "%s %%%d" % (ad, c.score * 100), (k.origin_x + 4, k.origin_y + 22),
                            cv2.FONT_HERSHEY_SIMPLEX, 0.65, (255, 160, 0), 2, cv2.LINE_AA)
            if alarm:
                alarm.value = kisi > 0
            ekran.yaz(kare, "Nesne: %d   Kisi: %d" % (len(sonuc.detections), kisi))
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
