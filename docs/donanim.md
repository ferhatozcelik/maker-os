# Donanım rehberi

## Kamera

| Kamera | Durum |
| --- | --- |
| Kamera Modülü v1 (OV5647), v2 (IMX219), 3 (IMX708), HQ (IMX477), Global Shutter (IMX296), AI Kamera (IMX500) | `camera_auto_detect=1` ile otomatik algılanır |
| Arducam ve diğer üçüncü taraf kameralar | **Merkez → Kamera → Pi kamera modeli** ile sensörü seçin, yeniden başlatın |
| USB web kameraları (UVC) | Tak-çalıştır; MJPEG desteklemiyorsa ffmpeg ile kodlanır |

**Takma:** Pi kapalıyken kabloyu **CAMERA** yazan yuvaya takın; Pi 4'te
kablonun metal temas yüzü HDMI portlarına, mavi yüzü Ethernet portuna bakar.

**Kullanım:**

- Merkez → Kamera: canlı görüntü, fotoğraf (Pi kamerada tam çözünürlük), galeri (`~/Resimler/Kamera`).
- Terminal: `libcamera-hello --list-cameras`, `libcamera-still -o foto.jpg`, `libcamera-vid -t 10000 -o video.h264`.
- Python: `picamera2` sistem Python'unda kuruludur (`/usr/bin/python3`); uv projelerinde [`pios_kamera`](mediapipe.md#pios_kamera) yardımcısını kullanın.
- `pardus` kullanıcısı `video` grubundadır; kamera için sudo gerekmez.

Kamera aynı anda yalnızca bir uygulama tarafından açılabilir. Merkez'deki
canlı görüntü izleyici kalmayınca 8 sn içinde kamerayı bırakır.

## Ekranlar

| Bağlantı | Notlar |
| --- | --- |
| HDMI 0 (USB-C güç girişinin yanındaki mikro HDMI) | Birincil ekran. Monitör yokken de etkindir (uzak masaüstü için) |
| HDMI 1 | İkinci monitör, genişletilmiş masaüstü |
| DSI (resmi Raspberry Pi dokunmatik ekranlar) | `display_auto_detect=1` ile otomatik; dokunmatik giriş libinput ile çalışır |

Pi 4 ve 400'deki "mini HDMI" girişleri aslında **mikro HDMI**'dır; mikro HDMI–HDMI kablo veya dönüştürücü gerekir.

Çözünürlük **Merkez → Ekran ve Donanım → Çözünürlük** bölümünden seçilir.
"Otomatik (monitöre göre)" monitörün EDID'deki tercih ettiği modu kullanır;
sabit bir değer seçilirse monitör yokken uzak ekran da o boyutta çalışır.
Sonradan takılan monitör algılanmazsa "Masaüstünü yeniden başlat" yeterlidir.

## USB

- **Bellekler ve diskler:** FAT32, exFAT, NTFS ve ext4 desteklenir. Pi'nin
  masaüstünde otomatik bağlanır (Thunar); ağdan **Merkez → USB Aygıtları →
  Bağla** ile `/media/pardus/<etiket>` altına bağlanır. Çıkarmadan önce
  **Güvenle çıkar** önbelleği yazar ve aygıtın gücünü keser.
- **Seri aygıtlar:** Arduino, ESP32, USB–seri dönüştürücüler `/dev/ttyACM*` /
  `/dev/ttyUSB*` olarak görünür; `pardus` kullanıcısı `dialout` grubundadır.
- **Güç:** Pi 4'ün USB portları toplam ~1,2 A verir. Harici 2,5" diskler
  için beslemeli USB hub kullanın; resmi 5V/3A güç kaynağı önerilir.
- Pi 4: mavi portlar USB 3.0, siyah portlar USB 2.0'dır.

## GPIO

40 pinli başlık **Merkez → GPIO Pinleri** bölümünde renkli şema olarak
görünür; pinler çıkış/giriş yapılabilir, I2C veri yolu taranabilir.

| Arayüz | Pinler | Varsayılan |
| --- | --- | --- |
| I2C | GPIO2 (SDA), GPIO3 (SCL) | Açık |
| SPI | GPIO7-11 | Açık |
| UART | GPIO14 (TX), GPIO15 (RX), 115200 | Açık (seri konsol) |

`pardus` kullanıcısı `gpio`, `i2c`, `spi` gruplarındadır; sudo gerekmez.
Python'da `gpiozero` + `RPi.GPIO` kullanın (Pardus 21'in glibc 2.31'i
`lgpio` tekerleklerini desteklemez). Pinler **3,3 V**'tur, 5 V uygulamayın.

## Ses

HDMI sesi monitör hoparlöründen, 3,5 mm çıkış kulaklıktan verilir; çıkış
masaüstündeki ses simgesinden seçilir. USB mikrofonlar ve ses kartları
tak-çalıştır çalışır ve **Merkez → Ekran ve Donanım → Ses** bölümünde listelenir.
Terminalden: `arecord -l`, `aplay -l`, `arecord -d 5 kayit.wav`.

## Ağ

| Bağlantı | Nasıl |
| --- | --- |
| Ethernet | Kabloyu (Cat5e/Cat6) modeme veya switch'e takın; adres DHCP ile otomatik alınır |
| Wi-Fi | Merkez → Wi-Fi ve Ağ; veya karta `wifi.txt` |
| Okul/kurum ağı (eduroam vb.) | Ağ "kurumsal (802.1X)" görünür; kullanıcı adı ve parola sorulur (PEAP/MSCHAPv2) |
| Gizli ağ | Wi-Fi ve Ağ → Gizli ağ… |
| Ağ yok | 45 sn sonra `Pardus-Maker-XXXX` erişim noktası (parola `pardus1234`, adres 10.42.0.1) |

Pi'de tek Wi-Fi radyosu vardır: bir ağa bağlanınca erişim noktası kapanır.
Wi-Fi ülke kodu `TR` olarak ayarlıdır (`cmdline.txt`).
