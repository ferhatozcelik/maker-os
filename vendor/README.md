# Paket deposu (vendor)

Pardus Maker'ın imajı ve cihazları dış depolara bağlı değildir. İmaja giren her
paket ve dosya (Pardus/Debian paketleri, Raspberry Pi çekirdeği ve kamera yığını,
VS Code, Node.js, Python sürümleri, AI ortamının Python paketleri, MediaPipe
modelleri, xterm.js) **kendi paket depomuzdan** gelir. Depoyu ne zaman
yenileyeceğinize siz karar verirsiniz.

| Kim | Nereye bağlanır |
| --- | --- |
| `vendor/sync.sh` (siz çalıştırırsınız) | Pardus, Raspberry Pi, Microsoft, Node.js, PyPI, PyTorch, GitHub, npm, Google |
| Build (GitHub Actions ve yerel) | Yalnızca bu reponun `depo-…` Release'i (`vendor/depo.lock`) |
| Cihaz | Hiçbir paket deposuna bağlanmaz; paketler Pardus Maker sürümleriyle (OTA) gelir |

## Dosyalar

| Dosya | İçerik |
| --- | --- |
| `sync.sh` | Depoyu yeniler (aşağıya bakın) |
| `depo.lock` | Kullanılan depo: Release adı, parçalar ve SHA-256 özetleri |
| `packages.lock` | İmajdaki her paket ve kesin sürümü (`paket  sürüm  mimari`) |
| `packages.base.lock` | İlk sürümün kilidi; OTA, cihazlara bundan bu yana değişen paketleri taşır |
| `versions.env` | uv, Python, Node.js ve Raspberry Pi firmware sürümleri |
| `ai-requirements.txt` | Hazır AI ortamının kesin Python paket sürümleri |
| `files.lock` | Paket olmayan dosyalar: SHA-256, depodaki ad, kaynak adres |
| `overrides/<paket>.sh` | Bizim paket değişikliklerimiz (aşağıya bakın) |

Kilitler `sync.sh` tarafından üretilir; elle düzenlemeyin. Hepsi git'te durduğu
için her yenilemenin farkı (`git diff vendor/`) incelenebilir.

## Depoyu yenileme

```bash
vendor/sync.sh            # topla + GitHub Release'e yükle
vendor/sync.sh --yerel    # yalnızca topla (deneme; yükleme yok)
```

1. İmaj, kaynaklardan sıfırdan kurulur (en güncel kararlı sürümler; Docker'da
   `image/build-docker.sh` ile aynı aşamalar, senkron modunda).
2. Kurulan her paketin `.deb`'i, Python önbellekleri ve dosyalar toplanır;
   `overrides/` uygulanır; yerel bir apt deposu oluşturulur.
3. Depo `depo-YYYYMMDD-SSDD` ön sürümü olarak bu reponun Release'lerine
   yüklenir (~2 GB parçalar). Ön sürüm olduğu için cihazlar onu yeni Pardus
   Maker sürümü sanmaz.
4. `vendor/` kilitleri güncellenir ve hangi paketlerin güncellendiği, eklendiği
   veya kaldırıldığı listelenir.

Sonra:

```bash
git diff vendor/                     # neler değişti
git add vendor && git commit -m "Paket deposu depo-…"
# Cihazlara göndermek için VERSION'ı artırın (ör. 1.0.1) ve push edin.
```

GitHub Actions yeni depoyla imajı build eder ve sürümü yayınlar. Cihazlar
yeni sürümü Merkez → Güncellemeler'de görür; OTA paketi değişen `.deb`'leri
de içerir.

## Paketleri değiştirme (overrides)

`overrides/<paket>.sh` betiği, o paketin depoya girmeden önce değiştirilmesini
sağlar: `sh <betik> <orijinal.deb> <çıkış-dizini>`. Betik orijinali silip yeni
`.deb`'i çıkış dizinine yazar. Sürümün sonuna `+makerN` eklenmelidir; böylece
kilitte bizim paketimiz olduğu görülür.

Mevcut değişiklikler (`overrides/`) ve build filtreleri:

| Paket | Değişiklik | Kazanç |
| --- | --- | --- |
| Chromium dil dosyaları | dpkg filtresiyle (`02-build-rootfs.sh`) yalnızca Türkçe ve İngilizce kurulur | ~88 MB |
| Raspberry Pi firmware | `stable` dalı; kısmi git çekimi; yalnızca Pi 4/400 ve Pi 3 açılış dosyaları | Senkronda 175 → ~35 MB indirme, açılış bölümünde ~19 MB |
| `pardus-xfce-settings` | `zenity` bağımlılığı ve kullanılmayan "Dosya(ları) yazdır" eylemi kaldırıldı (imajda yazıcı sistemi yok) | `zenity`, `libwebkit2gtk`, `libjavascriptcoregtk` imajdan çıkar (~140 MB) |

## Sınırlar

- OTA geri alma Pardus Maker dosyalarını eski haline getirir; kurulan `.deb`
  paketlerini eski sürüme indirmez.
- Cihazlar güvenlik yamalarını, siz `sync.sh` çalıştırıp yeni sürüm
  yayınladığınızda alır.
