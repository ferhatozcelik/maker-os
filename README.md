<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os">
    <img src="shell/icons/pwa-512.png" alt="MakerOS" width="100">
  </a>
</p>

<h1 align="center">MakerOS</h1>

<p align="center">Raspberry Pi üzerinde çalışan, macOS'un kullanım deneyiminden ilham alan bağımsız ve açık kaynaklı Linux işletim sistemi.</p>

<p align="center"><strong>Make it yours. Build what matters.</strong> · <em>Kendine ait kıl. Değerli olanı üret.</em></p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os/releases/tag/v1.0.0"><img src="https://img.shields.io/badge/release-v1.0.0-f07a1c" alt="Release v1.0.0"></a>
  <img src="https://img.shields.io/badge/platform-Raspberry%20Pi%204%20%7C%20400-f07a1c" alt="Platform: Raspberry Pi 4, 400">
  <img src="https://img.shields.io/badge/taban-Buildroot%20%2B%20Linux%206.18-f07a1c" alt="Taban: Buildroot ve Linux 6.18">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-f07a1c" alt="GPL-3.0 lisansı"></a>
</p>

<p align="center">
  <a href="https://github.com/ferhatozcelik/maker-os/releases/latest">İndir</a> ·
  <a href="#philosophy">Felsefe</a> ·
  <a href="#features">Özellikler</a> ·
  <a href="#setup-guide">Kurulum rehberi</a> ·
  <a href="#documentation">Dokümantasyon</a> ·
  <a href="docs/api.md">API</a> ·
  <a href="CONTRIBUTING.md">Katkıda bulun</a>
</p>

<p align="center">
  <a href="README.md">Türkçe</a> ·
  <a href="README.en.md">English</a>
</p>

<p align="center">
  <strong>Projeyi destekleyin</strong><br>
  <a href="https://buymeacoffee.com/ferhatozcelik">Buy Me a Coffee</a> ·
  <a href="https://thanks.dev/donations">thanks.dev</a>
</p>

<br>

<table align="center">
  <tr>
    <th align="center" colspan="2">Masaüstü · Tarayıcı ve Yerel AI Asistanı</th>
  </tr>
  <tr>
    <td align="center" colspan="2"><img src="docs/screenshots/desktop.png" alt="MakerOS masaüstü görünümü" width="850"></td>
  </tr>
  <tr>
    <th align="center">Web arayüzü</th>
    <th align="center">Telefon</th>
  </tr>
  <tr>
    <td align="center" valign="top"><img src="docs/screenshots/web.png" alt="MakerOS Web arayüzü" width="560"></td>
    <td align="center" valign="top"><img src="docs/screenshots/mobile.png" alt="Telefonda MakerOS Web arayüzü" width="200"></td>
  </tr>
  <tr>
    <th align="center" colspan="2">Ayarlar · Genel ve Yazılım Güncelleme</th>
  </tr>
  <tr>
    <td align="center" colspan="2"><img src="docs/screenshots/update.png" alt="Ayarlar, Genel: yazılım güncelleme" width="640"></td>
  </tr>
</table>

## Nedir?

MakerOS, Raspberry Pi gibi erişilebilir donanımlar üzerinde çalışan, kullanıcıya kendi bilgisayarını ve dijital üretim ortamını yönetme özgürlüğü sunan bağımsız bir Linux işletim sistemidir. Hazır bir dağıtımın üzerine masaüstü giydirmek yerine; kendi önyükleme zincirini, Rust tabanlı sistem servislerini (`makerd`), web masaüstü kabuğunu, A/B güncelleme altyapısını ve yerel yapay zekâ asistanını tek bir bütün olarak sunar.

macOS'un sadeliğinden ve kullanım kolaylığından ilham alır; ancak bir kopya olmayı değil, Linux'un esnekliğiyle birleşen özgün bir üretim atölyesi olmayı hedefler.

<a id="philosophy"></a>

## Felsefe, Hikâye ve Manifesto

**Bilgisayar yeniden kişisel olmalı.** Geleneksel sistemlerde kullanıcılar çoğu zaman kapalı ekosistemlerin ve kara kutu servislerin sınırları içinde hareket eder. MakerOS, *"Bir bilgisayarı gerçekten kendimiz için tasarlayabilir miyiz?"* sorusuyla Raspberry Pi üzerinde doğdu. Amacımız pahalı donanımlara ihtiyaç duymadan, kullanıcının sistemini anlayabildiği, değiştirebildiği ve kendi fikirlerini hayata geçirebildiği şeffaf bir ortam kurmaktır.

MakerOS beş temel ilkeye dayanır: **Özgürlük** (kontrol kullanıcıdadır), **Basitlik** (güçlü teknoloji karmaşık olmamalıdır), **Üretkenlik** (tüketim değil üretim odaklıdır), **Şeffaflık** (sistem nasıl çalıştığını açıkça gösterir) ve **Erişilebilirlik** (uygun maliyetli ARM64 donanımlarda çalışır). Yapay zekâ ise bilgisayarın kontrolünü kullanıcıdan alan bir mekanizma değil, kullanıcının yeteneklerini genişleten yerel bir yardımcıdır.

> **Manifesto:** Bilgisayarların yalnızca tüketmek için var olduğuna ve güçlü deneyimlerin kapalı ekosistemlere ait olması gerektiğine inanmıyoruz. Sadelik ile özgürlüğün, açık kaynak ile iyi tasarımın bir arada bulunabileceğini savunuyoruz. Teknoloji kullanıcıya ne yapacağını söylememeli; kullanıcının ne yapabileceğini genişletmelidir.
>
> **Make it yours. Build what matters.** — *Kendine ait kıl. Değerli olanı üret.*

## İndir

[Sürüm 1.0.0](https://github.com/ferhatozcelik/maker-os/releases/latest): `makeros-1.0.0-rpi4.img.xz` (Raspberry Pi 4 / 400, 64 bit). İndirdikten sonra `sha256sum -c makeros-*.img.xz.sha256` ile doğrulayın.

<a id="features"></a>

## Özellikler

| Alan | İçerik |
| --- | --- |
| **Taban** | Buildroot + Linux 6.18 (ARM64) üzerine kurulu bağımsız sistem; salt okunur EROFS kök dosya sistemi, ext4 `/data` bölmesi, RAUC + `tryboot` A/B güncelleme, systemd |
| **Masaüstü ve Web** | Yerel ekranda Cog/WPE WebKit ve ağ üzerinden tarayıcıda aynı deneyim: pencereli **Ekran** (`/`) ve mobil uyumlu **Web** (`/web`) görünümü |
| **Rust Arka Uç** | `makerd` (HTTP/WS API), `makerd-priv` (ayrıcalıklı işlemler), `makerd-user` (kullanıcı süreçleri ve PTY terminal), `makeros-ai` (yerel AI servisi) |
| **Yerel AI Asistanı** | Qwen2.5-0.5B-Instruct GGUF + `llama.cpp` tabanlı Türkçe asistan; sohbet modu, MakerOS bilgi bankası, bağlam temizleme, risk sınıflandırması ve Full Bypass / İzinli komut çalıştırma |
| **Python ve Otomasyon** | `uv` tabanlı sanal ortam ve proje yöneticisi, hazır GPIO/Kamera/Web/AI şablonları; **Otomatik Başlatma** ve **Cron** zamanlayıcı ile açılışta veya zamanlanmış görevlerle çalıştırma |
| **Donanım ve Kamera** | 40-pin GPIO canlı kontrolü, I²C, SPI, UART; CSI ve USB kameralar için canlı MJPEG akışı ve fotoğraf çekimi |
| **Eklentiler ve Temalar** | Python eklenti mimarisi, Eklenti Mağazası (`makeros-plugins`), `.zip` içe/dışa aktarma; Maker (varsayılan), Donanım, Siber, 3B Baskı ve Konya HackerSpace temaları, düz (flat) düşük GPU modu ve Sistem UI gizleme |

<a id="documentation"></a>

## Dokümantasyon Ağacı

Teknik ayrıntılar ve mimari belgeler Türkçe ve İngilizce olarak [`docs/`](docs/README.md) dizininde yer alır:

```text
maker-os/
├── README.md · README.en.md
├── docs/
│   ├── README.md · README.en.md
│   ├── architecture/
│   │   ├── overview.md · overview.en.md
│   │   ├── boot-and-storage.md · boot-and-storage.en.md
│   │   ├── desktop.md · desktop.en.md
│   │   ├── backend.md · backend.en.md
│   │   ├── ai-assistant.md · ai-assistant.en.md
│   │   ├── network-security.md · network-security.en.md
│   │   └── hardware.md · hardware.en.md
│   ├── development/
│   │   └── build-and-test.md · build-and-test.en.md
│   ├── verification/
│   │   └── validation.md · validation.en.md
│   ├── project/
│   │   └── branding-and-licensing.md · branding-and-licensing.en.md
│   └── api.md · api.en.md
├── os/         # Buildroot dış ağacı, çekirdek, paketler ve imaj üretimi
├── makerd/     # Rust sistem servisleri ve yerel AI asistanı
└── shell/      # Web masaüstü kabuğu ve yerleşik uygulamalar
```

<a id="setup-guide"></a>

## Kurulum Rehberi

1. **SD Karta Yazın:** [Sürüm sayfasından](https://github.com/ferhatozcelik/maker-os/releases/latest) `makeros-1.0.0-rpi4.img.xz` dosyasını indirin ve Raspberry Pi Imager (*Use custom*) ile SD karta yazın. İsteğe bağlı olarak `MOSCFG` bölümündeki `wifi-example.txt` dosyasını `wifi.txt` adıyla kopyalayıp Wi-Fi bilgilerinizi girebilirsiniz.
2. **Başlatın ve Bağlanın:** SD kartı Raspberry Pi 4 / 400'e takıp gücü verin. Aynı ağdaysanız `http://maker-os.local/`, USB-C ile bağlıysanız `http://10.55.0.1/`, ağ tanımlı değilse `MakerOS` kurulum Wi-Fi ağına bağlanarak `http://10.42.0.1/kurulum` adresini açın.
3. **Oturum Açın:** İlk kurulumdan sonra varsayılan hesap **`makeros` / `makeros`** olarak oluşturulur; parolanızı **Ayarlar › Kullanıcı** bölümünden değiştirebilirsiniz.
4. **OTA Güncelleme:** **Ayarlar › Genel › Yazılım güncelleme** üzerinden yeni sürümler denetlenir, imzalı `.raucb` paketi boştaki bölüme (`ROOT_A` / `ROOT_B`) kurulur ve `tryboot` ile güvenle devreye alınır.

<a id="build-from-source"></a>

## Kaynaktan Derleme ve Hızlı Komutlar

```bash
make -C makerd preview     # yerel önizleme: makerd (mock) + kabuk → http://localhost:8080/
make -C shell smoke        # kabuk duman testi (Docker içinde başsız Chromium)
./os/build.sh build        # Raspberry Pi imajı → os/out/makeros-1.0.0-rpi4.img.xz
```

Ayrıntılı derleme ve geliştirme adımları için bkz. [`docs/development/build-and-test.md`](docs/development/build-and-test.md).

## Doğrulama Durumu

Masaüstü kabuğu (`shell/`), `makerd` servis ailesi, yerel AI asistanı (`makeros-ai`), terminal oturumları, Python Yöneticisi, Otomatik Başlatma / Cron zamanlayıcı ve OTA güncelleme akışı hem Docker duman testleriyle hem de gerçek Raspberry Pi 4 donanımı üzerinde doğrulanmıştır. Nelerin gerçek donanımda doğrulandığı ve gelecek planlarının ayrımı için bkz. [`docs/verification/validation.md`](docs/verification/validation.md).

## Lisans ve Marka Açıklaması

[GPL-3.0-or-later](LICENSE) — Üçüncü taraf bileşenlerin lisans bildirimleri için [`NOTICE.md`](NOTICE.md) ve [`docs/project/branding-and-licensing.md`](docs/project/branding-and-licensing.md) belgelerine bakın.

**MakerOS bağımsız bir açık kaynak projesidir.** Raspberry Pi Ltd adına geliştirilmez ve resmî ortaklık iddia edilmez. **Raspberry Pi is a trademark of Raspberry Pi Ltd.**
