# Mimari

Pardus Maker, Raspberry Pi için **hazır kurulu** bir Pardus imajıdır: kurulum
sihirbazı yoktur, SD karta yazılıp açıldığında otomatik oturumlu masaüstü ve
web yönetimi çalışır durumdadır.

## Katmanlar

```
┌──────────────────────────────────────────────────────────────────────┐
│ Kullanıcı: tarayıcı (telefon/bilgisayar) · Pi'nin monitörü · SSH     │
├──────────────────────────────────────────────────────────────────────┤
│ Pardus Maker Merkezi (port 80: panel + ekran)     x11vnc (5900)        │
│  ayarlar · terminal · kamera · USB · GPIO · Python · agent'lar        │
├──────────────────────────────────────────────────────────────────────┤
│ XFCE + Plank (LightDM otomatik oturum, kullanıcı: pardus)            │
│ Chromium · VS Code · Thunar · xfce4-terminal                         │
├──────────────────────────────────────────────────────────────────────┤
│ Geliştirme: uv + CPython sürümleri · ai-env (Python 3.12) · Node.js  │
│ Kamera: libcamera, libcamera-apps, picamera2 · ffmpeg · MediaPipe    │
├──────────────────────────────────────────────────────────────────────┤
│ Pardus 21 "Yirmibir" arm64 (Debian 11), paketler vendor deposundan   │
│ systemd · NetworkManager · avahi (mDNS) · zram · udisks2             │
├──────────────────────────────────────────────────────────────────────┤
│ Raspberry Pi çekirdeği 6.18 (-v8+), DTB/overlay, açılış yazılımı,    │
│ Wi-Fi/BT donanım yazılımı (raspberrypi/firmware, RPi deposu)         │
└──────────────────────────────────────────────────────────────────────┘
```

Pardus'un arm64 deposunda Raspberry Pi'ye özel çekirdek ve kamera yığını
bulunmadığı için bunlar Raspberry Pi'nin kaynaklarından alınır:

- Çekirdek, modüller ve açılış dosyaları: `raspberrypi/firmware` deposunun
  sabitlenmiş commit'i (`image/pins/base.env.example`).
- Wi-Fi/Bluetooth donanım yazılımı: Raspberry Pi OS paketleri (dpkg ile kurulu, izlenebilir).
- Kamera yığını: Raspberry Pi'nin **bullseye** deposu yalnızca `libcamera*`,
  `python3-picamera2` ve bağımlılıkları için açılır; apt tercihleri
  (`/etc/apt/preferences.d/50-pios-raspberrypi`) bu depodaki diğer her
  paketi `-1` önceliğiyle engeller. `libcamera0`'ın istediği
  `raspberrypi-kernel` için yer tutucu bir paket kurulur ve tutulur.

## Disk düzeni

| Bölüm | Dosya sistemi | Boyut | İçerik |
| --- | --- | --- | --- |
| 1 | FAT32 (`PARDUSBOOT`) | 512 MiB | `config.txt`, `cmdline.txt`, çekirdek, DTB, overlay'ler, `wifi-example.txt`, `pardus-status.txt` |
| 2 | ext4 (`pardusroot`) | ilk açılışta kartın tamamı | Kök dosya sistemi |

`/tmp` tmpfs'tir, takas zram (zstd) ile bellekte tutulur; SD kart yazımlarını
azaltmak için `noatime`, journald sınırı ve kapatılmış zamanlayıcılar kullanılır.

## Açılış akışı

1. **Açılış yazılımı** `config.txt`'yi okur: `vc4-kms-v3d` (tam KMS),
   `camera_auto_detect=1`, `display_auto_detect=1`, I2C/SPI/UART açık.
   `cmdline.txt` içindeki `video=HDMI-A-1:...D` monitör takılı olmasa da HDMI
   çıkışını etkinleştirir; böylece masaüstü uzak ekrandan kullanılabilir.
2. **`pios-firstboot.service`** (yalnızca ilk açılış): kök bölümü kartın
   tamamına genişletir (`growpart` + `resize2fs`) ve cihaza özel SSH
   anahtarlarını üretir.
3. **`pios-network-fallback.service`**: `bootfs/wifi.txt` varsa Wi-Fi
   profili olarak ekler ve dosyayı siler; 45 sn içinde varsayılan rota
   oluşmazsa `Pardus-Maker-XXXX` erişim noktasını (10.42.0.1) açar; adresleri
   `bootfs/pardus-status.txt` dosyasına yazar.
4. **`pios-center.service`**: Merkez'i port 80'de başlatır ve
   `/run/pios/token` dosyasına yerel giriş jetonunu yazar.
5. **LightDM** `maker` kullanıcısıyla otomatik oturum açar;
   **`pios-vnc`** (x11vnc) ve **`pios-novnc`** (websockify) uzak ekranı sunar.

## Servisler

| Birim | İş | Port |
| --- | --- | --- |
| `pios-center.service` | Pardus Maker Merkezi (root, Python 3.9 stdlib) | 80 |
| `pios-vnc.service` | x11vnc, ekran `:0` | 5900 |
| `pios-novnc.service` | websockify (Merkez > Ekran bunu port 80 üzerinden `/api/vnc/ws` ile aktarır) | 127.0.0.1:6080 (dışarı kapalı) |
| `pios-network-fallback.service` | Ağ yedeği ve durum dosyası | — |
| `pios-firstboot.service` | İlk açılış işleri | — |
| `ssh`, `avahi-daemon`, `NetworkManager`, `systemd-timesyncd`, `fake-hwclock`, `zramswap`, `bluetooth` | Sistem | 22 |

Servis ve yol adlarındaki `pios` öneki teknik kimliktir; kurulu sistemlerde
betikler ve servisler arasında ortak kullanılır.

## Dizinler

| Yol | İçerik |
| --- | --- |
| `/usr/lib/pios/center/` | Merkez backend'i (`server.py`, `system.py`, `camera.py`, `devices.py`, `terminal.py`, `pyman.py`, `agents.py`, `gpio.py`) ve `web/` arayüzü |
| `/usr/lib/pios/templates/` | MediaPipe örnek projeleri ve ortak `pios_camera.py` |
| `/usr/share/pios/xterm/` | xterm.js (sabit sürüm, sha256 doğrulamalı) |
| `/usr/share/pios/mediapipe/` | MediaPipe modelleri (sabit sürüm, sha256 doğrulamalı) |
| `/opt/pios/python/` | uv ile kurulan CPython sürümleri (`maker` grubu yazabilir) |
| `/opt/pios/envs/ai/` | Hazır yapay zeka ortamı (`ai-env` komutu) |
| `/opt/pios/node/` | Node.js LTS |
| `/etc/pios/` | VNC parolası, kamera ayarları |
| `~/Projects/` | Kullanıcı projeleri (her biri kendi `.venv`'i ile) |
| `~/Resimler/Camera/` | Merkez'den çekilen fotoğraflar |

## Güvenlik modeli

- **Kimlik doğrulama:** ağdan gelen istekler Merkez giriş sayfası
  (`/api/login`) ile sistem kullanıcısının parolasıyla (`/etc/shadow`)
  doğrulanır; tarayıcıya `WWW-Authenticate` gönderilmez. Oturum
  `pios_t` çerezidir. İstemci araçları isteğe bağlı Basic kimlik
  kullanabilir. Pi'nin kendi masaüstünden açılan pencereler
  `/run/pios/token` jetonunu kullanır (HttpOnly, SameSite=Strict); parola
  sorulmaz.
- **CSRF:** durum değiştiren tüm istekler `POST` ve `X-Pios: 1` başlığı ister.
- **Web terminali:** WebSocket, kimliği doğrulanmış bir istekle alınan tek
  kullanımlık ve 30 sn geçerli bir biletle açılır; `Origin` başlığı `Host`
  ile aynı olmalıdır. Kabuk root değil `maker` kullanıcısıdır. En fazla 4
  oturum, 2 saat boşta kalınca kapanır; açılış/kapanış günlüğe yazılır.
- **Dosya yolları:** statik dosya sunumu `realpath` ile kök dizine
  hapsedilir; fotoğraf ve proje adları düzenli ifadeyle doğrulanır; USB
  bağlama yalnızca `lsblk`'te USB olarak görünen bölümler için yapılır ve
  sistem diski çıkarılamaz.
- **Sınırlar:** Varsayılan parola (`maker`) tüm imajlarda aynıdır ve ilk iş
  değiştirilmelidir. Web ve VNC trafiği şifrelenmez; yalnızca güvenilir yerel
  ağda kullanılmalıdır. Erişim noktası parolası (`pardus1234`) de ortaktır.
