# Değişiklik günlüğü

## 1.0.1

Uzak ekran (Merkez → Ekran) daha hızlı ve tarayıcıyı tam kaplar.

- **Tam kaplama:** ekran açılınca masaüstü tarayıcı penceresinin oranına geçer (720 piksel yükseklik); siyah boşluk kalmaz. Pi'ye monitör takılıysa monitörün çözünürlüğü korunur
- **Daha akıcı görüntü:** uzak ekran 720p'de çalışır (1080p'ye göre her karede 2,25 kat az piksel); pencere efektleri (compositing) ve dock büyütme animasyonu kapalı; fare ve klavye olayları gecikmesiz iletilir (`TCP_NODELAY`); LAN için düşük sıkıştırma
- Kurulu cihazlarda pencere efektleri OTA ile kapatılır

## 1.0.0

İlk sürümün kapsamı. Donanım doğrulaması kullanılan Pi modeliyle birlikte
raporlanmalıdır; Docker testi kamera, Wi-Fi ve GPIO desteğini doğrulamaz.

### Temel

- **MakerOS:** Pardus tabanlı maker işletim sistemi; cihaz adresi `http://maker-os.local/`, imaj `maker-os-<sürüm>.img.xz`
- **Taban:** Pardus 21 "Yirmibir" arm64, Raspberry Pi çekirdeği 6.18 (`-v8+`), XFCE + Plank, Pardus 100. Yıl teması
- **MakerOS Merkezi:** telefon uyumlu tek web uygulaması (port 80)
  - Wi-Fi ve ağ: Ethernet durumu, gizli ağ, WPA2-Enterprise, kayıtlı ağlar
  - Wi-Fi kurulumu: ağ yoksa şifresiz `MakerOS` ağı açılır; bağlanan telefonda kurulum sayfası kendiliğinden açılır (ağ seç, parola gir, bağlan). Ağ listesi kurulum ağı açılmadan önce taranır
  - Tarih ve saat: saat dilimi, NTP, tarayıcının saatini kullanma
  - Dil ve klavye: sistem dili, Türkçe Q/F ve diğer düzenler, Alt+Shift ile ikinci düzen
  - Web terminali (xterm.js, PTY, tek kullanımlık bilet, telefon tuş çubuğu)
  - Kamera: Pi kamera (libcamera, picamera2) ve USB kamera, canlı görüntü, fotoğraf, galeri
  - USB: aygıt listesi, bellek bağlama/güvenle çıkarma, seri portlar
  - Ekran ve ses: HDMI 0/1, DSI, otomatik çözünürlük, ses aygıtları
  - Uzak ekran: telefonda yatay tam ekran, ekran klavyesi
  - Python, projeler, hazır AI ortamı, agent yöneticisi, GPIO
- **MediaPipe örnekleri:** el takibi, jest ile LED, yüz, poz, nesne, arka plan ayırma, yüz ağı; modeller imajda
- **Masaüstü:** uygulama menüsü kaldırıldı, uygulamalar dock'ta; yeni MakerOS arka planı (Türkiye haritası, maker ikonları)

### Güncelleme ve dağıtım

- **Kablosuz güncelleme (OTA):** Merkez → Güncellemeler yeni MakerOS sürümünü GitHub'dan denetler (6 saatte bir ve elle) ve haber verir; kurulumu kullanıcı başlatır. Önce üzerine yazılacak dosyaların yedeği alınır; Merkez yeni sürümle açılmazsa kendiliğinden geri dönülür. Yeni çekirdek Pi'nin `tryboot` özelliğiyle bir kez denenir. Kurulumdan sonra açılan pencerede onaylayınca yedek ve geçici dosyalar silinir; "Geri al" önceki sürüme döner. Ayrıntılar: `docs/ota.md`
- **Kullanıcı ve parola:** `maker` / `maker` (ilk iş değiştirin). Merkez kullanıcıyı cihazdan okur
- **Uzak ekran:** varsayılan masaüstü 1280×720; x11vnc kararlı 30 FPS (33 ms yoklama, bekleme modu kapalı); tam ekran ve yön değişiminde görüntü hemen yeniden sığdırılır
- **Sistem güncellemesi:** Pardus deposu ve VS Code; süren apt işine bağlanır, apt kilidi için bekler
- **Kendi paket depomuz:** build ve cihazlar Pardus veya başka bir dış depoya bağlanmaz; tüm paketler ve dosyalar `vendor/` kilitleriyle kendi Release'imizdeki depodan gelir. Depo `vendor/sync.sh` ile istenen zaman yenilenir; paket değişiklikleri `vendor/overrides/` ile yapılır (ilk örnek: `pardus-xfce-settings` zenity'siz, ~140 MB daha küçük imaj)
- **GitHub Actions:** `main`'e push'ta arm64 makinede imaj build edilir; `VERSION` yeni bir sürümse imaj ve OTA paketi doğrudan GitHub Release'e eklenir
- Birim testleri (Merkez, OTA, eklentiler, MediaPipe yardımcıları)
- **Tek port:** uzak ekran da Merkez portu (80) üzerinden çalışır (`/api/vnc/ws`); websockify yalnızca içeriden erişilir, 6080 dışarı kapalı
- **Pencereler ortada açılır:** Merkez pencereleri ekranın ~%70'i boyutunda ve ortada; tüm pencereler xfwm4 ile ortalanır
- **Varsayılan çözünürlük 1920x1080:** 1080p monitörlerde büyütülmüş (zoom) görüntü yok; Merkez ara genişlik ve kısa ekranlarda daha sıkı menü
- **Uluslararası dosya adları:** tüm doküman, betik, şablon ve cihazda oluşan dosya/klasör adları İngilizce (`~/Projects`, `pardus-status.txt`, `mp-hand`…); OTA eski adları taşır

### Performans (Raspberry Pi 4)

Ayarlar Docker'da doğrulandı; hız aşırtma, frekans yöneticisi ve G/Ç ayarları gerçek Pi'de
ölçülmelidir (bkz. `docs/hardware.md` → Performans ve soğutma).

- **Pi 4 hız aşırtma:** CPU 2.0 GHz, GPU 750 MHz (`[pi4]` bölümü; soğutucu/fan gerekir)
- **Çekirdek ve sistem:** `ondemand` frekans yöneticisi hızlı tepki ayarlarıyla (`pios-cpu.service`), SD kart için BFQ zamanlayıcı, zswap kapalı (takas zram'da), zram için `vm.page-cluster=0`, açılışta `boot_delay=0` ve `initial_turbo=30`
- **Masaüstü:** pencere gölgeleri kapalı (compositing ve dock şeffaflığı korunur); Chromium GPU ile çizer
- **Merkez:** HTTP/1.1 kalıcı bağlantı, gzip ve ETag/304 (arayüz 85 KB → 26 KB, sonraki açılışta 0); agent sürümleri paralel ve önbellekli; Etkinlik örnekleyicisi izleyen yokken durur; masaüstü oturumu, GPIO çipi ve saat dilimleri önbellekte; kamera MJPEG ayrıştırması kopyasız
- **MediaPipe:** analiz geride kalınca eski kareler çözülmez; kare başına kopya kaldırıldı
- **AI ortamı:** paketler önceden derlenmiş bytecode ile gelir (torch/cv2 içe aktarma hızlanır)
- **Çekirdek ve açılış:** Raspberry Pi'nin `stable` dalı (Raspberry Pi OS'in test edilmiş çekirdeği); açılış bölümünde yalnızca Pi 4/400 ve Pi 3 firmware'i (hata ayıklama, eski kamera ve küçültülmüş varyantlar çıkarıldı)
- **Daha küçük imaj:** Chromium'da yalnızca Türkçe/İngilizce dil dosyaları (~88 MB), `zenity`/WebKit yok (~140 MB), yardım belgeleri yok
- **Paket deposu senkronu:** firmware deposunun tamamı (175 MB) yerine kısmi git çekimi (~35 MB); takılan indirmeler süre sınırı ve yeniden denemeyle sürer
- **`flash-sd.sh`:** çok çekirdekli xz açma; doğrulamada imaj ikinci kez açılmaz
