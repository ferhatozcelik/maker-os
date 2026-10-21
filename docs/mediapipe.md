# MediaPipe örnekleri

[MediaPipe](https://github.com/google-ai-edge/mediapipe), Google'ın cihaz
üzerinde çalışan görüntü işleme ve yapay zeka kütüphanesidir. Pardus Maker,
Raspberry Pi kamerasıyla doğrudan çalışan 7 hazır proje şablonu ve bunların
modellerini imajla birlikte getirir; internet olmadan da kurulup çalışır.

## Hızlı başlangıç

1. **Merkez → Projeler → Yeni proje**: bir ad yazın, şablon olarak
   **MediaPipe** grubundan bir örnek seçin, **Oluştur**'a basın.
2. Proje satırındaki **Terminal** (veya **VS Code**) ile açın.
3. Çalıştırın:

```bash
uv run main.py                         # kamera otomatik: önce Pi kamera, yoksa USB
uv run main.py --kaynak usb:1          # ikinci USB kamera
uv run main.py --kaynak video.mp4      # video dosyası
uv run main.py --kaynak resim.jpg      # tek resim
uv run main.py --pencere-yok           # yalnızca web yayını
uv run main.py --genislik 1280 --yukseklik 720
```

Sonuç Pi'nin ekranında bir pencerede (`q` veya `Esc` ile çıkılır) ve
`http://pardus-maker.local:8000/` adresinde canlı görünür; telefondan da
açılabilir. Terminalden `Ctrl+C` ile durdurulur.

## Örnekler

| Şablon | Model | Ne yapar | Ek seçenekler |
| --- | --- | --- | --- |
| El takibi ve parmak sayma | `hand_landmarker.task` | İki ele kadar 21 eklem noktası, her eldeki açık parmak sayısı (el yönünden bağımsız) | — |
| Jest ile LED kontrolü | `gesture_recognizer.task` | 👍 LED yakar, 👎 söndürür, ✋ buzzer öttürür | GPIO17 LED, GPIO27 buzzer |
| Yüz algılama ve otomatik fotoğraf | `blaze_face_short_range.tflite` | Yüzleri çerçeveler, yeni bir yüz görününce `fotograflar/` klasörüne kaydeder | `--fotograf-yok` |
| Poz: squat ve kol kaldırma | `pose_landmarker_lite.task` | 33 noktalı iskelet, diz açısıyla squat ve iki kolu kaldırma sayacı | — |
| Nesne algılama | `efficientdet_lite0.tflite` | 80 COCO sınıfı (Türkçe adlarla), kişi görünce alarm | `--alarm`, `--esik 0.45` |
| Arka plan bulanıklaştırma | `selfie_segmenter.tflite` | Kişiyi ayırır; bulanık, tek renk veya resimli arka plan | `--mod bulanik\|renk\|resim`, `--resim arka.jpg` |
| Yüz ağı ve göz kırpma | `face_landmarker.task` | 478 noktalı yüz ağı, göz kırpma sayacı, gülümseme ve ağız açıklığı | — |

### Jest ile LED devresi

```
GPIO17 (pin 11) ──[330 Ω]──►|── GND (pin 9)        LED (uzun bacak dirence)
GPIO27 (pin 13) ──────────── buzzer (+) ; buzzer (−) ── GND (pin 14)
```

GPIO kullanılamazsa (pin meşgul, Pi dışı bilgisayar) örnek yalnızca ekranda gösterir.

### Performans

Anlık hız görüntünün sol alt köşesinde FPS olarak yazar. Raspberry Pi 4'te
daha akıcı sonuç için çözünürlüğü düşürün (`--genislik 480 --yukseklik 360`);
nesne algılama ve yüz ağı diğerlerinden ağırdır. Pi 3B+ belirgin şekilde yavaştır.

## Proje yapısı

```
~/Projeler/el-takibi/
├── main.py            # örnek kod (Türkçe açıklamalı)
├── pios_kamera.py     # kamera + gösterim yardımcısı
├── models/            # MediaPipe modeli (imajdan kopyalanır)
├── pyproject.toml     # uv projesi (Python 3.12, mediapipe)
├── uv.lock
└── .venv/
```

MediaPipe projeleri Python 3.12 ile kurulur: MediaPipe'ın arm64 tekerleği ve
hazır AI ortamı bu sürümdedir. Paketler imajdaki uv önbelleğinden gelir; her
proje diskte neredeyse yer kaplamaz (sert bağlantı).

## `pios_kamera`

Örneklerin ortak yardımcısı; kendi projelerinizde de kullanabilirsiniz.

```python
from pios_kamera import Gosterici, Kamera, mp_goruntu, zaman_ms
from mediapipe.tasks.python import BaseOptions, vision

secenek = vision.HandLandmarkerOptions(
    base_options=BaseOptions(model_asset_path="models/hand_landmarker.task"),
    running_mode=vision.RunningMode.VIDEO)

with vision.HandLandmarker.create_from_options(secenek) as model, \
        Kamera() as kam, Gosterici("Benim projem") as ekran:
    for kare in kam:                                  # OpenCV BGR karesi
        sonuc = model.detect_for_video(mp_goruntu(kare), zaman_ms())
        ekran.yaz(kare, "El sayisi: %d" % len(sonuc.hand_landmarks))
        if not ekran.goster(kare):                    # pencere + web yayını
            break
```

| Öğe | Açıklama |
| --- | --- |
| `Kamera(kaynak="otomatik", genislik=640, yukseklik=480, fps=30)` | `"pi"`: `libcamera-vid` MJPEG akışı; `"usb"`/`"usb:N"`: OpenCV V4L2; dosya yolu: video veya resim |
| `Gosterici(baslik, port=8000, pencere=True)` | Masaüstü penceresi (`DISPLAY` varsa) ve MJPEG web yayını; FPS'i köşeye yazar |
| `Gosterici.yaz(kare, metin, satir=0, renk=...)` | Gölgeli, okunaklı yazı |
| `mp_goruntu(kare)` | BGR → MediaPipe `Image` (SRGB) |
| `zaman_ms()` | VIDEO modu için artan zaman damgası |
| `arguman_ayristirici(aciklama)` | Ortak `--kaynak`, `--genislik`, `--yukseklik`, `--port`, `--pencere-yok` seçenekleri |

## Başka modeller

Modeller [MediaPipe Solutions](https://ai.google.dev/edge/mediapipe/solutions/guide)
sayfasında listelenir. Örneğin tam yüz algılama modeli:

```bash
curl -L -o models/blaze_face_full_range.tflite \
  https://storage.googleapis.com/mediapipe-models/face_detector/blaze_face_full_range/float16/1/blaze_face_full_range.tflite
```

İmajdaki modeller `/usr/share/pios/mediapipe/` altındadır. MediaPipe ve
modelleri Apache-2.0 lisanslıdır; bkz. [NOTICE.md](../NOTICE.md).
