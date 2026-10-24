"""MediaPipe Arka Plan Ayirma (selfie segmentasyon): kisiyi arka plandan
ayirir; arka plani bulaniklastirir, tek renk yapar veya bir resimle
degistirir (goruntulu gorusme efekti).

Calistirma:  uv run main.py                       (bulanik)
             uv run main.py --mod renk
             uv run main.py --mod resim --resim arka.jpg
Web yayini:  http://pardus-maker.local:8000/
"""
from pios_camera import Gosterici, Kamera, arguman_ayristirici, analiz, calisma_modu, performans_ayarla

import os

import cv2
import numpy as np
from mediapipe.tasks.python import BaseOptions, vision

MODEL = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models", "selfie_segmenter.tflite")


def main():
    p = arguman_ayristirici("MediaPipe arka plan ayirma")
    p.add_argument("--mod", choices=["bulanik", "renk", "resim"], default="bulanik")
    p.add_argument("--resim", help="--mod resim icin arka plan resmi")
    args = p.parse_args()
    performans_ayarla(args)
    arka_resim = cv2.imread(args.resim) if args.resim else None
    if args.mod == "resim" and arka_resim is None:
        raise RuntimeError("Arka plan resmi okunamadı; --resim ile geçerli bir dosya verin.")
    arka_cache, onceki_boyut = None, None
    secenek = vision.ImageSegmenterOptions(
        base_options=BaseOptions(model_asset_path=MODEL),
        running_mode=calisma_modu(args), output_confidence_masks=True)
    with vision.ImageSegmenter.create_from_options(secenek) as model, \
            Kamera(args.kaynak, args.genislik, args.yukseklik, args.fps, args.denetim) as kam, \
            Gosterici("Arka plan ayirma", args.port, not args.pencere_yok, kamera=kam, cikti=args.cikti, kalite=args.jpeg_kalite) as ekran:
        for kare in kam:
            if not kam.resim_modu:
                kare = cv2.flip(kare, 1)
            sonuc = analiz(model, kare, kam, "segment")
            maske = np.squeeze(sonuc.confidence_masks[0].numpy_view())
            if maske.shape != kare.shape[:2]:
                maske = cv2.resize(maske, (kare.shape[1], kare.shape[0]))
            maske = cv2.GaussianBlur(maske, (7, 7), 0)[..., None]  # kenarlari yumusat
            if args.mod == "renk":
                arka = np.zeros_like(kare)
                arka[:] = (90, 170, 40)
            elif args.mod == "resim" and arka_resim is not None:
                if onceki_boyut != kare.shape[:2]:
                    arka_cache = cv2.resize(arka_resim, (kare.shape[1], kare.shape[0]))
                    onceki_boyut = kare.shape[:2]
                arka = arka_cache
            else:
                kucuk = cv2.resize(kare, (max(1, kare.shape[1] // 4), max(1, kare.shape[0] // 4)))
                arka = cv2.resize(cv2.GaussianBlur(kucuk, (15, 15), 0), (kare.shape[1], kare.shape[0]))
            kare = (kare * maske + arka * (1 - maske)).astype(np.uint8)
            ekran.yaz(kare, "Mod: " + args.mod)
            if not ekran.goster(kare):
                break


if __name__ == "__main__":
    main()
