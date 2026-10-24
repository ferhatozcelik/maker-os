# Katkı rehberi

Katkılarınız memnuniyetle karşılanır: hata bildirimi, çeviri, yeni örnek proje
veya kod.

## Hata bildirimi

[Issues](https://github.com/ferhatozcelik/pardus-maker/issues) sayfasında yeni
bir kayıt açın ve şunları ekleyin:

- Pi modeli ve bellek miktarı, kullanılan imaj sürümü (**Merkez → Genel → Sürüm**)
- Takılı donanım (kamera modeli, ekran, USB aygıtları)
- Ne beklediğiniz, ne olduğu; mümkünse ekran görüntüsü
- İlgili günlük: `journalctl -u pios-center -b` veya `bootfs/pardus-status.txt`

## Geliştirme

1. Depoyu çatallayın (fork) ve bir dal açın: `git checkout -b ozellik/kisa-ad`
2. Merkez'i değiştirdiyseniz Docker testinde deneyin:
   ```bash
   cd image && ./build-docker.sh 06 && ./test/run-test.sh   # http://localhost:8080/
   ```
3. Donanıma dokunan değişiklikleri (kamera, Wi-Fi, GPIO, ekran) gerçek bir Pi'de deneyin ve PR açıklamasına hangi modelde denediğinizi yazın.
4. Pull request açın.

## Kod kuralları

- Merkez backend'i yalnızca Python 3.9 standart kütüphanesini kullanır (Pardus 21 sistem Python'u); yeni bağımlılık eklemeyin.
- Web arayüzü derleme adımı olmayan düz JavaScript modülüdür; dış CDN kullanılmaz (Pi ağsız da çalışmalı).
- Betikler POSIX `sh` ile yazılır, `set -eu` kullanılır.
- Kod içi yorumlar ASCII Türkçedir; arayüz metinleri tam Türkçe karakterlerle yazılır.
- İmaja indirilen her dış dosya sabit sürüm ve sha256 ile doğrulanır.

## Lisans

Gönderdiğiniz katkılar projenin lisansı olan **GNU GPL v3 veya sonrası**
kapsamında yayınlanır.
