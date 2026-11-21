# {{name}}: görüntü tanıma (Google LiteRT)

```sh
uv run main.py                     # web arayüzü: http://<pi>:8000 (fotoğraf yükle / kameradan çek)
uv run main.py --image foto.jpg    # komut satırında sınıflandır
uv run main.py --camera            # kameradan bir kare çek ve sınıflandır
uv run main.py --model fp32        # tam duyarlıklı model (~31 MB; varsayılan int8 ~9 MB)
```

İlk çalıştırmada model ve etiketler `models/` klasörüne indirilir; sonra internet gerekmez. Görüntü cihazdan çıkmaz.

| Seçenek | Varsayılan | Açıklama |
|---|---|---|
| `--model` | int8 | `int8` (ağırlıkları 8 bit, hızlı) ya da `fp32` |
| `--top` | 5 | gösterilecek sınıf sayısı |
| `--port` | 8000 | web arayüzü portu |

Model: EfficientNet-B1 (ImageNet-1k), [litert-community/efficientnet_b1](https://huggingface.co/litert-community/efficientnet_b1)
(PyTorch Vision ağırlıklarından dönüştürülmüştür; kendi lisans koşulları geçerlidir).
Çıkarım: [Google LiteRT](https://ai.google.dev/edge/litert) (`ai-edge-litert`, Apache-2.0).
Başka bir `.tflite` sınıflandırıcı denemek için `models/` içine koyup `MODELS` sözlüğüne ekleyin.
