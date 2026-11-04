# Değişiklik günlüğü

## 2.0.0

MakerOS 2'nin ilk sürümü: Buildroot ile Linux çekirdeği üzerine sıfırdan kurulan bağımsız sistem.

### Sistem

- **Taban:** Buildroot 2026.02.3, Linux 6.18 (Raspberry Pi), systemd, glibc; Raspberry Pi 4 / 400 (aarch64)
- **Salt okunur kök dosya sistemi** (EROFS) ve **A/B bölümleri**; kullanıcı verisi ayrı `DATA` bölümünde
- **İmzalı yazılım güncellemesi:** Ayarlar › Genel › Yazılım güncelleme. GitHub Releases'teki RAUC
  paketi (`makeros-<sürüm>-rpi4.raucb`) indirilir, SHA-256 ve imza doğrulanır, boştaki bölüme kurulur;
  Pi deneme açılışıyla (tryboot) yeni sürümden başlar, açılamazsa önceki sürüme döner.
  Yeni sürüm 12 saatte bir denetlenir ve bildirilir
- **Ekransız kurulum:** `wifi.txt` ya da şifresiz `MakerOS` kurulum ağı; ilk açılış sihirbazı
  (varsayılan parola yok)
- **makerd** (Go): tek HTTP portundan API, WebSocket olaylar, terminal, dosyalar, kamera, GPIO,
  Python projeleri, AI ajanları, eklentiler; kök yetkili işler dar izin listeli `makerd-priv`'de

### Arayüz

- **Ekran** (pencereli masaüstü, raf) ve **Web** (telefona uygun) görünümleri, üstte *Web | Ekran* geçişi;
  kurulabilir web uygulaması (PWA), telefonda alt sekme çubuğu
- **Chromium penceresi:** Pi'de çalışan gerçek Chromium, masaüstünde normal bir pencere olarak açılır
  (ayrı, görünmez ekran; pencereyle birlikte boyutlanır; kapanınca durur). Pi'nin kendi ekranında
  doğrudan açılır
- Dosyalar: `.py` dosyasına tıklayınca terminalde çalışır; Bildirimler: "Bildirimleri temizle"
- Temalar: Maker, Hackerspace, Donanım, Siber güvenlik, 3B baskı; Konya HackerSpace eklentisi

### Doğrulama

Arayüz, makerd ve Chromium penceresi Docker içinde derlenmiş kök dosya sistemiyle test edildi.
Gerçek donanımda ölçülmüş performans verisi henüz yok (bkz. `docs/v2/BENCHMARK.md`).
