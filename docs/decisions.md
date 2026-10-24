# Teknik kararlar

Bu belge, Pardus Maker'ın neden bu şekilde kurulduğunu ve Pardus 21 tabanının
getirdiği kısıtlara karşı alınan kararları kaydeder.

## Taban: Pardus 21 "Yirmibir" arm64

- Pardus'un arm64 paketi yayınlayan sürümü Pardus 21'dir
  (`depo.pardus.org.tr/pardus/dists/yirmibir/Release`:
  `Architectures: amd64 arm64 i386`; güvenlik deposu da arm64 içerir).
  Pardus 23/25 depoları arm64 içermez.
- Kök dosya sistemi `debootstrap` (minbase, Debian bullseye betiği) ile
  üretilir; 04 aşamasındaki `dist-upgrade` `base-files`'ı Pardus sürümüne geçirir
  (`/etc/os-release` Pardus olur).
- Build sırasında `ulakbimdepo2.pardus.org.tr` aynası kullanılır; imajdaki
  `sources.list` resmi adresleri içerir.

## Kurulumsuz, hazır kurulu imaj

- Kurulum sihirbazı yerine otomatik oturumlu masaüstü ve tek web uygulaması
  (Merkez) seçildi. Monitörsüz kullanım öncelikli: ilk açılışta disk genişletme
  ve SSH anahtarları kendiliğinden yapılır, ağ yoksa erişim noktası açılır.
- Masaüstü XFCE (Pi 4'te hafif), macOS benzeri düzen: üst çubuk + Plank dock.
  Uygulama menüsü kaldırıldı; uygulamalara dock'tan ve Merkez'den ulaşılır.
- Bilinçli olarak yok: ofis, e-posta, görüntüleyici gibi genel uygulamalar.

## Raspberry Pi donanım desteği

- **Çekirdek:** `raspberrypi/firmware` deposunun sabit commit'indeki önceden
  derlenmiş `kernel8.img` ve **yalnızca `-v8+`** modül dizini. Başka bir
  varyant (ör. `6.18.57+`, Pi 1/Zero) kopyalanırsa `brcmfmac` yüklenmez ve
  Wi-Fi olmaz; 03 aşaması `modules.dep`'te `brcmfmac` yoksa build'i durdurur.
- **Wi-Fi/BT donanım yazılımı:** Debian bullseye'daki 2021 tarihli
  `firmware-brcm80211` güncel çekirdekle uyumsuz; Raspberry Pi OS'in paketleri
  dpkg ile kurulur.
- **Kamera:** Pardus/Debian bullseye'da RPi kamera yığını yok. Raspberry Pi'nin
  bullseye deposundan yalnızca `libcamera*`, `libcamera-apps`,
  `python3-picamera2` ve bağımlılıkları alınır (apt tercihleriyle sınırlı).
  Çekirdek 6.18'in kamera overlay'leri `bcm2835-unicam-legacy` sürücüsünü
  kullanır; bu, 2023 tarihli libcamera ile uyumlu eski sürücüdür.
  `libcamera0`'ın `raspberrypi-kernel` bağımlılığı yer tutucu bir paketle
  karşılanır (gerçek paket `/boot`'u ezerdi).
- **Ekran:** tam KMS (`vc4-kms-v3d`), Xorg modesetting. `video=HDMI-A-1:...D`
  monitör yokken de çıkışı açar; aksi halde uzak masaüstü siyah kalır.

## Merkez

- Backend Python 3.9 standart kütüphanesiyle yazıldı: Pardus 21'in sistem
  Python'u ile ek paket olmadan çalışır ve kullanıcının Python sürümlerinden
  etkilenmez.
- Web terminali için WebSocket (RFC 6455) ve PTY stdlib ile uygulandı;
  tarayıcılar WebSocket isteğine Basic kimliğini güvenilir şekilde eklemediği
  için tek kullanımlık bilet kullanılır.
- Kamera akışı MJPEG'dir (`multipart/x-mixed-replace`): ek istemci kodu
  gerektirmez, tek kamera süreci tüm izleyicilere dağıtılır.
- Arayüz derleme adımı olmayan bir JavaScript modülüdür ve dış CDN
  kullanmaz; xterm.js ve MediaPipe modelleri sabit sürüm + sha256 ile imaja gömülür.

## Pardus 21 (glibc 2.31, GLIBCXX 3.4.28) kısıtları

| Sorun | Karar |
| --- | --- |
| `lgpio` arm64 tekerlekleri glibc 2.34 istiyor | GPIO için `gpiozero` + `RPi.GPIO` (Pi 4 için yeterli) |
| `ai-edge-litert` 2.1+ GLIBCXX 3.4.29 istiyor | 2.0.3'e sabitlendi |
| PyPI'deki aarch64 `torch` CUDA ile geliyor (~4 GB) | PyTorch CPU deposu |
| Dış depolara bağımlılık (build ve cihaz) | Tüm paketler kendi depomuzda (`vendor/`, Release); build ve cihaz dış depoya bağlanmaz; yenileme `vendor/sync.sh` ile elle (bkz. `vendor/README.md`) |
| Python 3.13+ `crypt` modülünü kaldırdı | Sistem servisleri `/usr/bin/python3` (3.9); kullanıcı `python3`'ü uv ile en güncel sürüm |
| MediaPipe 1.x yalnızca `manylinux_2_28` arm64 tekerleği sunuyor | glibc 2.31 ile uyumlu; Pi üzerinde 7 görev (el, jest, yüz, poz, nesne, segmentasyon, yüz ağı) doğrulandı |
| En güncel Python'da numpy/matplotlib arm64 tekerleği gecikebiliyor | MediaPipe projeleri Python 3.12'ye sabitlenir |
| e2fsprogs 1.47'nin `orphan_file` özelliği bullseye'da tanınmıyor | `mkfs.ext4 -O ^orphan_file,^metadata_csum_seed` |

## Build

- Build Apple Silicon Mac'te arm64 Docker (colima) ile yerel mimaride
  çalışır; qemu gerekmez. Kök dosya sistemi host bind-mount'unda değil, Linux
  dosya sistemindeki `pios-work` volume'unda üretilir (aygıt düğümleri,
  sahiplik, büyük/küçük harf duyarlılığı).
- İmaj, döngü aygıtı kullanmadan üretilir (`mkfs.ext4 -d`, mtools, sfdisk);
  Docker/colima içinde güvenilir çalışır.
- Bir aşamadan devam edilebilir: `./build-docker.sh 06`, 06–09 aşamalarını
  sırayla çalıştırır. 04 aşaması firmware paketleri zaten kuruluysa yeniden
  indirme gerektirmez; paketler eksikse 03 aşamasından devam edilmelidir.

## Lisans

Projenin özgün kaynakları **GPL-3.0-or-later** kapsamındadır. Üçüncü taraf
bileşenler kendi lisanslarını korur; [NOTICE.md](../NOTICE.md) dosyasında
kaynaklarıyla listelenir. Marka ve dağıtım koşulları [lisans notlarında](license.md)
açıklanır.
