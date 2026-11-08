# Önyükleme ve Depolama Mimarisi

[Türkçe](boot-and-storage.md) · [English](boot-and-storage.en.md)

MakerOS, güç kesintilerine ve hatalı güncellemelere karşı dayanıklı olması için salt okunur EROFS kök dosya sistemi ve A/B bölümleme yapısını kullanır. Yapılandırma [`os/board/rpi4/genimage.cfg.in`](../../os/board/rpi4/genimage.cfg.in) ve [`makerd/src/ota.rs`](../../makerd/src/ota.rs) dosyalarında tanımlanır.

## Bölüm Düzeni

- **`MOSCFG` (FAT32):** `autoboot.txt` A/B önyükleme seçicisi ve isteğe bağlı `wifi.txt` ilk kurulum dosyası.
- **`BOOT_A` / `BOOT_B` (FAT32):** Çekirdek (`kernel8.img`), aygıt ağacı (DTB) ve önyükleme dosyaları.
- **`ROOT_A` / `ROOT_B` (EROFS):** Salt okunur sıkıştırılmış kök dosya sistemleri.
- **`DATA` (ext4):** İlk açılışta kart sonuna kadar büyütülen kalıcı veri bölmesi (`/home`, `/var`, `/etc` üst katmanı ve kullanıcı projeleri).

## A/B OTA Güncelleme Akışı

Yeni sürüm paketleri (`.raucb`) X.509 imzasıyla doğrulanarak boştaki bölüme (`A` veya `B`) yazılır. Yeniden başlatmada Raspberry Pi `tryboot` mekanizması yeni bölümü tek seferlik dener; sistem sağlıklı açılıp `makeros-mark-good` servisi çalıştığında yeni bölüm kalıcı hâle getirilir, aksi durumda donanım otomatik olarak önceki çalışan sürüme döner.
