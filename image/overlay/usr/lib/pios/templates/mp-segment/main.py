"""MediaPipe Arka Plan Ayirma (selfie segmentasyon): kisiyi arka plandan
ayirir; arka plani bulaniklastirir, tek renk yapar veya bir resimle
degistirir (goruntulu gorusme efekti).

Calistirma:  uv run main.py                       (bulanik)
             uv run main.py --mod renk
             uv run main.py --mod resim --resim arka.jpg
Web yayini:  http://pardus-maker.local:8000/
"""
import os

import cv2
import numpy as np
from mediapipe.tasks.python import BaseOptions, vision

from pios_kamera import Gosterici, Kamera, arguman_ayristirici, mp_goruntu, zaman_ms

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "selfie_segmenter.tflite")


def main():
    p = arguman_ayristirici("MediaPipe arka plan ayirma")
    p.add_argument("--mod", choices=["bulanik", "renk", "resim"], default="bulanik")
    p.add_argument("--resim", help="--mod resim icin arka plan resmi")
    args = p.parse_args()
    arka_resim = cv2.imread(args.resim) if args.resim else None
    secenek = vision.ImageSegmenterOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=vision.RunningMode.VIDEO, output_confidence_masks=True)
    with vision.ImageSegmenter.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik) as kam, \
            Gosterici("Arka plan ayirma", args.port, not args.pencere_yok) as ekran:
        for kare in kam:
            kare = cv2.flip(kare, 1)
            sonuc = model.segment_for_video(mp_goruntu(kare), zaman_ms())
            maske = np.squeeze(sonuc.confidence_masks[0].numpy_view())
            maske = cv2.GaussianBlur(maske, (7, 7), 0)[..., None]  # kenarlari yumusat
            if args.mod == "renk":
                arka = np.zeros_like(kare)
                arka[:] = (90, 170, 40)
            elif args.mod == "resim" and arka_resim is not None:
                arka = cv2.resize(arka_resim, (kare.shape[1], kare.shape[0]))
            else:
                arka = cv2.GaussianBlur(kare, (55, 55), 0)
            kare = (kare * maske + arka * (1 - maske)).astype(np.uint8)
            ekran.yaz(kare, "Mod: " + args.mod)
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
