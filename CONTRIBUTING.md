# Katkı rehberi

Katkılarınız memnuniyetle karşılanır: hata bildirimi, çeviri, yeni Python şablonu, eklenti veya kod.

## Hata bildirimi

[Issues](https://github.com/ferhatozcelik/maker-os/issues) sayfasında yeni bir kayıt açın ve şunları ekleyin:

- Pi modeli ve bellek miktarı, MakerOS sürümü (**Ayarlar › Genel**)
- Takılı donanım (kamera modeli, ekran, USB aygıtları)
- Ne beklediğiniz, ne olduğu; mümkünse ekran görüntüsü
- İlgili günlük: `journalctl -u makerd -b` (Terminal'de) ya da SD karttaki `MOSCFG/makeros-status.txt`

## Geliştirme

Tüm derleme ve testler **Docker içinde** çalışır; ana makineye Go, Node veya Buildroot kurmayın.

1. Depoyu çatallayın (fork) ve bir dal açın: `git checkout -b ozellik/kisa-ad`
2. Değişikliği deneyin:
   ```bash
   make -C makerd test        # makerd birim testleri
   make -C shell smoke        # kabuk duman testi (başsız Chromium)
   ./os/run-docker.sh         # derlenmiş kök dosya sistemiyle uçtan uca (http://localhost/)
   ```
3. Kod ve arayüz metinleri Türkçe ve İngilizcedir (`shell/i18n/tr.js`, `en.js`); yeni metni ikisine de ekleyin.
4. Anlamlı bir commit iletisiyle pull request açın.

## Sürüm yayınlama (bakımcılar)

1. `os/VERSION`'ı artırın ve `CHANGELOG.md`'ye aynı sürümün bölümünü ekleyin (`## X.Y.Z`).
2. `main`'e gönderin. GitHub Actions imajı ve imzalı güncelleme paketini derler, `vX.Y.Z` Release'ini
   oluşturur. İmza anahtarı depoda değildir; `MAKEROS_RAUC_KEY` sırrı olarak tanımlı olmalıdır.
3. Cihazlar yeni sürümü Ayarlar › Genel › Yazılım güncelleme'de görür.

## Lisans

Katkılarınız [GPL-3.0-or-later](LICENSE) kapsamında yayımlanır.
