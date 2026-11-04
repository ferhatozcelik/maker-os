# MakerOS

[English](README.en.md)

**Raspberry Pi 4 için maker işletim sistemi.** Pi'yi bir monitöre bağlamadan, aynı ağdaki herhangi
bir tarayıcıdan (bilgisayar, tablet, telefon) kullanırsınız: dosyalar, terminal, Python projeleri,
GPIO, kamera, yapay zekâ ajanları ve Chromium tek bir web masaüstünde.

MakerOS 2, Buildroot ile Linux çekirdeği üzerine sıfırdan kurulan bağımsız bir sistemdir; salt okunur
kök dosya sistemi, A/B bölümleri ve imzalı yazılım güncellemesi vardır. (MakerOS 1.x Pardus
tabanlıydı; bu depo artık yalnız MakerOS 2'yi içerir. 1.x sürümleri
[Releases](https://github.com/ferhatozcelik/maker-os/releases) sayfasında durur.)

![MakerOS masaüstü: Chromium penceresi](docs/v2/screenshots/desktop.png)

| Web arayüzü | Telefonda | Yazılım güncelleme |
|---|---|---|
| ![Web arayüzü](docs/v2/screenshots/web.png) | ![Telefon](docs/v2/screenshots/mobile.png) | ![Güncelleme](docs/v2/screenshots/update.png) |

> Ekran görüntüleri Docker önizlemesinden alınmıştır; sıcaklık, bellek, ağ gibi cihaz bilgileri
> sahte (mock) veridir.

## Özellikler

- **İki görünüm, tek adres** (`http://makeros.local/`): pencereli **Ekran** (masaüstü, raf, pencereler)
  ve telefona uygun **Web** arayüzü. Üstteki *Web | Ekran* düğmesiyle geçilir. Kurulabilir web
  uygulaması (PWA) olarak ana ekrana eklenebilir.
- **Chromium**, masaüstünde normal bir pencere olarak açılır. Pi'de çalışan gerçek tarayıcıdır; ayrı,
  görünmez bir ekranda çizilir ve yalnız o pencere size aktarılır. Pencere kapanınca durur.
- **Uygulamalar:** Dosyalar (`.py` dosyasına tıklayınca çalışır), Terminal (web ile Pi ekranı aynı
  oturumu paylaşır), Python (uv ile sanal ortamlı projeler ve şablonlar; LED, buton, I2C, kamera,
  yapay zekâ nesne tespiti), AI Ajanları (Claude Code, Codex CLI, Gemini CLI, OpenCode, Ollama),
  Kamera, GPIO (40 pin), Eklentiler (zip içe/dışa aktarma), Etkinlik Monitörü, Ayarlar.
- **Ekran Modu** (varsayılan kapalı): Pi'nin kendi ekranını tarayıcıda gösterir. Ayarlar › Ekran
  Modu'ndan yönetici açar; ayrı port açılmaz, kapalıyken arka planda hiçbir şey çalışmaz. Monitör
  bağlı değilse Pi'nin ekranı tarayıcı pencerenizin boyutuna uyar; telefonda yatay döner.
- **Yazılım güncelleme:** Ayarlar › Genel › Yazılım güncelleme, GitHub'daki yeni sürümü bulur;
  imzalı paketi indirip boştaki bölüme kurar ve yeniden başlatır. Yeni sürüm açılamazsa Pi önceki
  sürüme kendiliğinden döner. Dosyalarınız ve ayarlarınız korunur.
- **Ekransız kurulum:** Ağ yoksa Pi şifresiz `MakerOS` Wi-Fi ağını açar; telefonunuzu bağlayınca
  kurulum sayfası kendiliğinden gelir.
- **Temalar:** Maker, Hackerspace, Donanım, Siber güvenlik, 3B baskı; eklentiyle yeni tema
  (örnek: Konya HackerSpace).

## Kurulum

1. [Releases](https://github.com/ferhatozcelik/maker-os/releases) sayfasından en yeni
   `makeros-<sürüm>-rpi4.img.xz` dosyasını indirin (`.sha256` ile doğrulayabilirsiniz).
2. Raspberry Pi Imager ile SD karta yazın (*Use custom* / *Özel imaj kullan*). Pi Imager'ın
   "işletim sistemi özelleştirme" ayarları MakerOS'ta kullanılmaz.
3. *(İsteğe bağlı)* Kartın `MOSCFG` bölümündeki `wifi-example.txt` dosyasını `wifi.txt` adıyla
   kopyalayıp ağ adını ve parolasını yazın. İlk açılışta okunur ve güvenlik için silinir.
4. Pi'yi açın ve bir tarayıcıdan **http://makeros.local/** adresine gidin. Kablo ya da `wifi.txt`
   yoksa telefonunuzu `MakerOS` Wi-Fi ağına bağlayın; kurulum sayfası açılır (açılmazsa
   `http://10.42.0.1/kurulum`).
5. İlk açılış sihirbazında bölge ve klavyeyi, gerekirse Wi-Fi ağını seçin; hesabınızı ve cihaz
   adını belirleyin. Varsayılan parola yoktur; ilk hesap yöneticidir.

**Donanım:** Raspberry Pi 4 Model B ve Raspberry Pi 400 (aarch64). A/B güncellemesi için Pi
önyükleyici (EEPROM) sürümü 2025-03 veya yenisi gerekir. MakerOS 1.x kurulu kartlar uzaktan
MakerOS 2'ye geçirilemez; kart yeniden yazılmalıdır.

## Doğrulama durumu

Açık olmak adına: MakerOS 2'nin arayüzü, makerd ve Chromium penceresi **Docker içinde**,
derlenmiş kök dosya sistemiyle uçtan uca test edilmiştir. Gerçek Raspberry Pi donanımında
ölçülmüş performans verisi henüz yoktur; Docker'daki karşılaştırma için bkz.
[docs/v2/BENCHMARK.md](docs/v2/BENCHMARK.md). İmzalı güncellemenin donanımdaki
güç kesintisi ve bozuk paket testleri ilk sürümlerde sürmektedir. Sorunları
[Issues](https://github.com/ferhatozcelik/maker-os/issues) sayfasına bildirin.

## Kaynaktan derleme

Tüm derleme ve testler Docker içinde çalışır; ana makineye araç kurulmaz.

```bash
./os/run-docker.sh                    # derlenmiş kök dosya sistemini Docker'da çalıştır → http://localhost/
docker-compose up -d --build          # yalın önizleme: makerd (sahte veri) + kabuk → http://localhost/
make -C makerd test                   # makerd testleri
make -C shell smoke                   # kabuk duman testi (başsız Chromium)
./os/build.sh build                   # Raspberry Pi imajı → os/out/makeros-<sürüm>-rpi4.img.xz
```

Chromium paketi derlemeden önce bir kez hazırlanır:
`docker run --rm -v "$PWD/os/chromium":/s:ro -v "$PWD/os/out":/out debian:trixie sh /s/build-bundle.sh`

İmzalı güncelleme paketi (`.raucb`) yalnız imza anahtarı varsa üretilir
(`os/keys/rauc-key.pem`, depoya girmez). Sürümler GitHub Actions ile yayınlanır:
`os/VERSION` artırılıp `main`'e gönderildiğinde imaj ve paket derlenir, Release oluşturulur
(bkz. [.github/workflows/release.yml](.github/workflows/release.yml)).

| Klasör | İçerik |
|---|---|
| `os/` | Buildroot dış ağacı: defconfig, kart dosyaları, paketler, imaj betikleri |
| `makerd/` | Arka uç hizmeti (Go): API, terminal, dosyalar, güncelleme, Ekran Modu |
| `shell/` | Web kabuğu: masaüstü, Web arayüzü, uygulamalar (bağımlılıksız JS) |
| `docs/v2/` | Plan, API, yol haritası, karşılaştırma |
| `bench/` | Karşılaştırma betikleri ve ham veriler |
| `makerd-rs/`, `wm/` | Uzun vadeli Rust çalışmaları (henüz imajda kullanılmıyor) |

## Lisans ve marka açıklaması

MakerOS için bu depoda geliştirilen özgün kaynak kodu, derleme betikleri, yapılandırmalar ve
örnekler **GNU Genel Kamu Lisansı sürüm 3 veya sonraki herhangi bir sürüm**
(`GPL-3.0-or-later`) kapsamında sunulur; bkz. [`LICENSE`](LICENSE).

Bu lisans, imajdaki bütün yazılımların aynı lisansa sahip olduğu anlamına gelmez. Linux çekirdeği,
Raspberry Pi açılış yazılımı, Chromium, WPE WebKit, Mesa ve diğer üçüncü taraf bileşenler kendi hak
sahiplerine ve lisanslarına tabidir. Bileşen listesi ve kaynak bağlantıları
[`NOTICE.md`](NOTICE.md) dosyasındadır. Yeniden dağıtımda ilgili lisans ve telif bildirimleri
korunmalı; kaynak kodu sağlama yükümlülükleri bileşen ve dağıtım biçimine göre yerine
getirilmelidir.

**MakerOS bağımsız bir topluluk projesidir.** Raspberry Pi Ltd adına geliştirilmez; bir ortaklık,
sponsorluk, sertifikasyon, onay veya resmî destek ilişkisi iddia edilmez.
**Raspberry Pi is a trademark of Raspberry Pi Ltd.** Bu belgelerdeki marka atıfları yalnız hedef
donanımı tanımlamak içindir; yazılım lisansı marka veya logo kullanım izni vermez. Bkz.
[Raspberry Pi marka kuralları](https://www.raspberrypi.com/trademark-rules/). Varsayılan eklentideki
Konya HackerSpace adı ve işaretine ilişkin haklar Konya HackerSpace'e aittir.

Yazılım, yürürlükteki hukukun izin verdiği ölçüde, lisanslarda belirtilen şekilde **olduğu gibi ve
garanti verilmeksizin** sunulur. Bu açıklama hukuki uygunluk belgesi değildir.
