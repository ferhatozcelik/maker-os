"""MediaPipe Jest Tanima + GPIO: el jestleriyle LED/role kontrolu.

  Basparmak yukari  -> LED yanar      (GPIO17, fiziksel pin 11)
  Basparmak asagi   -> LED soner
  Acik avuc         -> buzzer otar    (GPIO27, fiziksel pin 13; istege bagli)

Devre: GPIO17 -> 330 ohm direnc -> LED (+) ; LED (-) -> GND (pin 9).
GPIO yoksa (bilgisayarda calistirma) yalnizca ekranda gosterir.

Calistirma:  uv run main.py
Web yayini:  http://pardus-maker.local:8000/
"""
from pios_kamera import Gosterici, Kamera, arguman_ayristirici, analiz, calisma_modu, performans_ayarla, resim_kaynagi

import os

import cv2
from mediapipe.tasks.python import BaseOptions, vision
from mediapipe.tasks.python.vision import drawing_styles, drawing_utils

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "gesture_recognizer.task")
JESTLER = {
    "Thumb_Up": "Basparmak yukari", "Thumb_Down": "Basparmak asagi", "Open_Palm": "Acik avuc",
    "Closed_Fist": "Yumruk", "Pointing_Up": "Isaret parmagi", "Victory": "Zafer (V)",
    "ILoveYou": "Seni seviyorum", "None": "-",
}



def main():
    args = arguman_ayristirici("MediaPipe jest tanima ile LED kontrolu").parse_args()
    performans_ayarla(args)
    led = buzzer = None
    if not resim_kaynagi(args.kaynak):
        try:
            from gpiozero import LED, Buzzer
            led, buzzer = LED(17), Buzzer(27)
        except Exception as e:
            print("GPIO kullanılamıyor, yalnızca ekranda gösterilecek:", e)
    secenek = vision.GestureRecognizerOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=calisma_modu(args), num_hands=1)
    try:
        with vision.GestureRecognizer.create_from_options(secenek) as model, \
                Kamera(args.kaynak, args.genislik, args.yukseklik, args.fps, args.denetim) as kam, \
                Gosterici("Jest ile LED", args.port, not args.pencere_yok, kamera=kam, cikti=args.cikti, kalite=args.jpeg_kalite) as ekran:
            for kare in kam:
                if not kam.resim_modu:
                    kare = cv2.flip(kare, 1)
                sonuc = analiz(model, kare, kam, "recognize")
                jest = "None"
                if sonuc.gestures:
                    en_iyi = sonuc.gestures[0][0]
                    if en_iyi.score > 0.6:
                        jest = en_iyi.category_name
                for el in sonuc.hand_landmarks:
                    drawing_utils.draw_landmarks(
                        kare, el, vision.HandLandmarksConnections.HAND_CONNECTIONS,
                        drawing_styles.get_default_hand_landmarks_style(),
                        drawing_styles.get_default_hand_connections_style())
                if led:
                    if jest == "Thumb_Up":
                        led.on()
                    elif jest == "Thumb_Down":
                        led.off()
                    buzzer.value = jest == "Open_Palm"
                ekran.yaz(kare, "Jest: " + JESTLER.get(jest, jest))
                ekran.yaz(kare, "LED: " + ("YANIK" if led and led.is_lit else "sonuk" if led else "GPIO yok"), 1,
                          (0, 255, 255) if led and led.is_lit else (200, 200, 200))
                if not ekran.goster(kare):
                    break
    finally:
        for aygit in (led, buzzer):
            if aygit:
                aygit.off()
                aygit.close()



if __name__ == "__main__":
    main()
