# {{name}}: nesne tespiti

```sh
uv run main.py                # kamera + web önizleme: http://<pi>:8000
uv run main.py --source test  # kamera olmadan dene
uv run main.py --image foto.jpg
```

| Seçenek | Varsayılan | Açıklama |
|---|---|---|
| `--size` | 320 | 256 daha hızlı, 320 daha doğru |
| `--threads` | 3 | ncnn iş parçacığı (çekirdek sayısını aşmaz) |
| `--cpus` | 1,2,3 | çıkarım çekirdekleri; 0. çekirdek arayüze kalır |

Model: NanoDet-m (COCO 80 sınıf), [RangiLyu/nanodet](https://github.com/RangiLyu/nanodet) Apache-2.0;
ncnn dönüşümü [nihui/ncnn-assets](https://github.com/nihui/ncnn-assets). Çıkarım: [ncnn](https://github.com/Tencent/ncnn) (BSD-3).
