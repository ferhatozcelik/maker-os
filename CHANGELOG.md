# Değişiklik günlüğü

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
