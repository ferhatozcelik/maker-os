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
uv run main.py --kaynak ./resimler --cikti ./sonuclar  # klasörü sırayla analiz et
uv run main.py --profil tasarruf       # daha düşük CPU yükü ve analiz sıklığı
uv run main.py --pencere-yok           # yalnızca web yayını
uv run main.py --genislik 1280 --yukseklik 720
```

Sonuç Pi'nin ekranında bir pencerede (`q` veya `Esc` ile çıkılır) ve
`http://pardus-maker.local:8000/` adresinde canlı görünür; telefondan da
açılabilir. Terminalden `Ctrl+C` ile durdurulur.

Tek resim veya resim klasörü analizinde web yayını açılmaz: her resim
**bir kez** IMAGE API'siyle analiz edilir, işaretli PNG ve yapılandırılmış
JSON `sonuclar/` klasörüne yazılır, ardından program kapanır. Resimler
aynalama yapılmadan, en-boy oranı korunarak analiz boyutuna küçültülür;
küçük resimler büyütülmez. Segmentasyon JSON'u büyük maskelerin kendisi
yerine boyut ve değer aralığını içerir; PNG ayrılmış arka planı gösterir.

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

| Profil | En fazla analiz boyutu | En fazla analiz/sn | CPU çekirdeği sınırı | JPEG kalitesi |
| --- | --- | --- | --- | --- |
| `tasarruf` | 480×360 | 6 | 1 | 55 |
| `dengeli` (varsayılan) | 640×480 | 10 | 2 | 65 |
| `kalite` | 960×720 | 15 | 3 | 75 |

Bunlar hedef üst sınırlardır; gerçek hız model ve donanıma bağlıdır.
`--fps`, `--cekirdek`, `--genislik`, `--yukseklik` ve `--jpeg-kalite`
ile değiştirilebilir. Linux'ta CPU affinity model kurulmadan önce uygulanır;
OpenCV iş parçacıkları da sınırlandırılır. Diğer sistemlerde affinity
desteklenmiyorsa bu adım atlanır.

Canlı kameradan yalnız **son kare** saklanır: analiz yavaşken eski
kareler kuyrukta birikmez. Video dosyaları ise kareleri atlamadan, hedef
analiz hızında işlenir. Web JPEG'i yalnız izleyici varsa üretilir;
tüm izleyiciler aynı JPEG'i paylaşır, yayın en fazla yaklaşık 6,7 FPS ve
dört izleyicidir. Arka plan bulanıklığı daha küçük resimde hesaplanır.

Varsayılan sıcaklık eşiği 75 °C'dir: 68 °C'de analiz sıklığı yarıya,
75 °C'de dörtte bire iner; 80 °C'de analiz duraklar. 72 °C'ye düşünce
devam eder. 120 saniyede soğumazsa açıklamalı hatayla kapanır. Bu eşik
`--sicaklik-siniri 65` gibi 55–75 °C arasında ayarlanabilir.
Sıcaklık sensörü yoksa hız sınırlaması sürer, ısıl denetim uygulanamaz.
Canlı sayfa sıcaklığı, hedef/gerçek hızı ve soğuma durumunu gösterir.

Bu kontroller ısı yükünü azaltmayı amaçlar; fiziksel soğutmanın yerini
almaz. Gerçek Pi'de FPS ve sıcaklık ölçümü yapılmadan donmama veya belirli
bir sıcaklığı aşmama garantisi verilmez. Pi'nin [ısıl yönetim davranışı](https://github.com/raspberrypi/documentation/blob/master/documentation/asciidoc/computers/raspberry-pi/frequency-management.adoc)
ve MediaPipe'ın [IMAGE/VIDEO API ayrımı](https://developers.google.com/edge/mediapipe/solutions/vision/object_detector/python)
referans alınmıştır.

Yeni şablonlar güncellendikten sonra oluşturulan projeler bu ayarları
kullanır. Mevcut kişisel projeler otomatik olarak değiştirilmez; yeni bir
örnek proje oluşturabilir veya ortak yardımcıyı ve örnekteki
`calisma_modu`, `analiz`, `performans_ayarla` çağrılarını uyarlayabilirsin.

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
| `Kamera(kaynak="otomatik", genislik=640, yukseklik=480, fps=10, denetim=None)` | Pi/USB kamera, video, tek resim veya resim klasörü; son-kare tamponu ve ısıl denetim |
| `Gosterici(baslik, port=8000, pencere=True)` | Masaüstü penceresi (`DISPLAY` varsa) ve MJPEG web yayını; FPS'i köşeye yazar |
| `Gosterici.yaz(kare, metin, satir=0, renk=...)` | Gölgeli, okunaklı yazı |
| `mp_goruntu(kare)` | BGR → MediaPipe `Image` (SRGB) |
| `zaman_ms()` | VIDEO modu için artan zaman damgası |
| `performans_ayarla(args)` | Profil, CPU affinity, OpenCV iş parçacıkları ve sıcaklık denetimini hazırlar |
| `calisma_modu(args)` | Resim/klasör için IMAGE, kamera/video için VIDEO |
| `analiz(model, kare, kamera, tur="detect")` | IMAGE/VIDEO API'sini seçer, süreyi ve resim sonucunu kaydeder; `recognize`/`segment` de desteklenir |
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
