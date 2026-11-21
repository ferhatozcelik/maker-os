# MakerOS Bench

Tek dosyalık, bağımlılıksız sistem karşılaştırma testi ve topluluk sonuç listesi. macOS, Linux, Raspberry Pi OS ve
[MakerOS](https://github.com/ferhatozcelik/maker-os)'ta çalışır (Python 3.8+, yalnız standart kitaplık).

```sh
curl -fsSLO https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/benchmark/makeros-bench.py
python3 makeros-bench.py                                   # ~1–5 dk; rapor: bench-results/*.md ve *.json
python3 makeros-bench.py --name "Pi 4 4GB · Raspberry Pi OS"  # listede görünecek ad
python3 makeros-bench.py --quick                           # kısa deneme
```

**Ölçülenler:** CPU tek çekirdek (tamsayı, kayan nokta, SHA-256, zlib, JSON, düzenli ifade), CPU çok çekirdek
(tüm çekirdeklerde aynı yük), bellek (kopyalama, nesne ayırma), disk (sıralı yazma/okuma, fsync'li rastgele 4K yazma,
rastgele 4K okuma). Raspberry Pi'de sıcaklık ve kısma (throttling) durumu da rapora girer.

**Puan:** her test sabit referansa göre ölçeklenir — Apple M1 Pro · macOS · Python 3.9 = **1000**. Kategori puanı
testlerin, genel puan kategorilerin geometrik ortalamasıdır. Python iş yükü başarımıdır; aynı betik sürümüyle alınan
sonuçlar karşılaştırılabilir. Kişisel bilgi (makine adı, kullanıcı, IP, seri no) toplanmaz.

**Liste şimdilik Raspberry Pi OS ve MakerOS içindir** (aynı Raspberry Pi'de iki işletim sistemini karşılaştırmak için); betik
macOS ve diğer Linux'larda da çalışır ama sonuçları listeye alınmaz.

**Listeye ekleme:** `bench-results/` içindeki `.json` (ve isterseniz `.md`) dosyasını bu klasördeki `results/`'a pull request ile
gönderin. GitHub Actions dosyayı doğrular; birleştirilince aşağıdaki tablo, grafikler ve
[etkileşimli sayfa](https://ferhatozcelik.github.io/maker-os/benchmark/) kendiliğinden güncellenir.

<!-- BENCH:START -->

_0 sonuç · `tools/render.py` üretir, elle düzenlemeyin._

Henüz sonuç yok. İlk sonuçlar Raspberry Pi OS ya da MakerOS üzerinde alınacak.

<!-- BENCH:END -->

Bu klasör [MakerOS](../README.md) deposunun parçasıdır (MIT).
