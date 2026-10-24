# Değişiklik günlüğü

## 1.1.0

Performans sürümü; Raspberry Pi 4 için ayarlandı. Ayarlar Docker'da
doğrulandı; hız aşırtma, frekans yöneticisi ve G/Ç ayarları gerçek Pi'de
ölçülmelidir (bkz. `docs/donanim.md` → Performans ve soğutma).

- **Pi 4 hız aşırtma:** CPU 2.0 GHz, GPU 750 MHz (`[pi4]` bölümü; soğutucu/fan gerekir)
- **Çekirdek ve sistem:** `ondemand` frekans yöneticisi hızlı tepki ayarlarıyla (`pios-cpu.service`), SD kart için BFQ zamanlayıcı, zswap kapalı (takas zram'da), zram için `vm.page-cluster=0`, açılışta `boot_delay=0` ve `initial_turbo=30`
- **Masaüstü:** pencere gölgeleri kapalı (compositing ve dock şeffaflığı korunur); Chromium GPU ile çizer
- **Merkez:** HTTP/1.1 kalıcı bağlantı, gzip ve ETag/304 (arayüz 85 KB → 26 KB, sonraki açılışta 0); agent sürümleri paralel ve önbellekli; Etkinlik örnekleyicisi izleyen yokken durur; masaüstü oturumu, GPIO çipi ve saat dilimleri önbellekte; kamera MJPEG ayrıştırması kopyasız
- **MediaPipe:** analiz geride kalınca eski kareler çözülmez; kare başına kopya kaldırıldı
- **AI ortamı:** paketler önceden derlenmiş bytecode ile gelir (torch/cv2 içe aktarma hızlanır)
- **`flash-sd.sh`:** çok çekirdekli xz açma; doğrulamada imaj ikinci kez açılmaz
- **Build:** `PIOS_BUILD_VERSION` Docker build'ine aktarılır

## 1.0.0

İlk sürümün kapsamı. Donanım doğrulaması kullanılan Pi modeliyle birlikte
raporlanmalıdır; Docker testi kamera, Wi-Fi ve GPIO desteğini doğrulamaz.

- **Taban:** Pardus 21 "Yirmibir" arm64, Raspberry Pi çekirdeği 6.18 (`-v8+`), XFCE + Plank, Pardus 100. Yıl teması
- **Pardus Maker Merkezi:** telefon uyumlu tek web uygulaması (port 80)
  - Wi-Fi ve ağ: Ethernet durumu, gizli ağ, WPA2-Enterprise, kayıtlı ağlar, yedek erişim noktası
  - Tarih ve saat: saat dilimi, NTP, tarayıcının saatini kullanma
  - Dil ve klavye: sistem dili, Türkçe Q/F ve diğer düzenler, Alt+Shift ile ikinci düzen
  - Web terminali (xterm.js, PTY, tek kullanımlık bilet, telefon tuş çubuğu)
  - Kamera: Pi kamera (libcamera, picamera2) ve USB kamera, canlı görüntü, fotoğraf, galeri
  - USB: aygıt listesi, bellek bağlama/güvenle çıkarma, seri portlar
  - Ekran ve ses: HDMI 0/1, DSI, otomatik çözünürlük, ses aygıtları
  - Uzak ekran: telefonda yatay tam ekran, ekran klavyesi
  - Python, projeler, hazır AI ortamı, agent yöneticisi, GPIO
- **MediaPipe örnekleri:** el takibi, jest ile LED, yüz, poz, nesne, arka plan ayırma, yüz ağı; modeller imajda
- Masaüstünde uygulama menüsü kaldırıldı; uygulamalar dock'ta
