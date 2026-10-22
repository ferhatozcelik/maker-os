"""MediaPipe Yuz Agi (478 nokta) ve ifade olcumu: yuz agini cizer, goz
kirpmalarini sayar, gulumsemeyi ve agiz acikligini gosterir
(blendshape degerleri). Ornek fikir: goz kirpinca GPIO ile bir sey yapmak.

Calistirma:  uv run main.py
Web yayini:  http://pardus-maker.local:8000/
"""
from pios_kamera import Gosterici, Kamera, arguman_ayristirici, analiz, calisma_modu, performans_ayarla

import os

import cv2
from mediapipe.tasks.python import BaseOptions, vision
from mediapipe.tasks.python.vision import drawing_styles, drawing_utils

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "face_landmarker.task")


def main():
    args = arguman_ayristirici("MediaPipe yuz agi ve goz kirpma sayaci").parse_args()
    performans_ayarla(args)
    secenek = vision.FaceLandmarkerOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=calisma_modu(args), num_faces=1, output_face_blendshapes=True)
    kirpma, kapali = 0, False
    with vision.FaceLandmarker.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik, args.fps, args.denetim) as kam, \
            Gosterici("Yuz agi", args.port, not args.pencere_yok, kamera=kam, cikti=args.cikti, kalite=args.jpeg_kalite) as ekran:
        for kare in kam:
            if not kam.resim_modu:
                kare = cv2.flip(kare, 1)
            sonuc = analiz(model, kare, kam, "detect")
            if sonuc.face_landmarks:
                drawing_utils.draw_landmarks(
                    kare, sonuc.face_landmarks[0], vision.FaceLandmarksConnections.FACE_LANDMARKS_CONTOURS,
                    landmark_drawing_spec=None,
                    connection_drawing_spec=drawing_styles.get_default_face_mesh_contours_style())
                ifade = {b.category_name: b.score for b in sonuc.face_blendshapes[0]}
                goz = (ifade.get("eyeBlinkLeft", 0) + ifade.get("eyeBlinkRight", 0)) / 2
                if goz > 0.5 and not kapali:
                    kirpma += 1
                kapali = goz > 0.5
                gulumseme = (ifade.get("mouthSmileLeft", 0) + ifade.get("mouthSmileRight", 0)) / 2
                ekran.yaz(kare, "Goz kirpma: %d" % kirpma)
                ekran.yaz(kare, "Gulumseme: %%%d   Agiz: %%%d" % (gulumseme * 100, ifade.get("jawOpen", 0) * 100), 1)
            else:
                ekran.yaz(kare, "Yuz bulunamadi")
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
