# Değişiklik Günlüğü

## 1.0.0

MakerOS'un ilk kararlı sürümü: Buildroot ile Linux çekirdeği üzerine sıfırdan kurulan bağımsız işletim sistemi.

### Sistem

- **Taban:** Buildroot 2026.02.3, Linux 6.18 (Raspberry Pi), systemd, glibc; Raspberry Pi 4 / 400 (aarch64)
- **Salt okunur kök dosya sistemi** (EROFS) ve **A/B bölümleri**; kullanıcı verisi ayrı `DATA` bölümünde
- **İmzalı yazılım güncellemesi:** Ayarlar › Genel › Yazılım güncelleme. GitHub Releases'teki RAUC
  paketi (`makeros-<sürüm>-rpi4.raucb`) indirilir, SHA-256 ve imza doğrulanır, boştaki bölüme kurulur;
  Pi deneme açılışıyla (tryboot) yeni sürümden başlar, açılamazsa önceki sürüme döner.
  Yeni sürüm 12 saatte bir denetlenir ve bildirilir
- **Ekransız kurulum:** `wifi.txt` ya da şifresiz `MakerOS` kurulum ağı; tek adımlı ilk açılış sihirbazı
  (varsayılan `makeros` / `makeros` hesabı)
- **makerd** (Rust): tek HTTP portundan API, WebSocket olaylar, terminal, dosyalar, kamera, GPIO,
  Python projeleri, Otomatik Başlatma ve Cron zamanlayıcı, AI ajanları, eklenti mağazası (`makeros-plugins` deposundan doğrudan kurulum ve güncelleme);
  kök yetkili işler dar izin listeli `makerd-priv`'de
- **makeros-ai (İnternetsiz Türkçe Linux Asistanı):** Qwen2.5-0.5B Q4_K_M ve `llama.cpp` tabanlı yerel asistan;
  OpenAI uyumlu API, kurallı komut risk sınıflandırması (`SAFE` / `WARNING` / `DANGEROUS`), iki çalıştırma izin modu
  (*izin iste* ve *otomatik*), masaüstü **AI Asistan** widget'ı, `makeros-ai` terminal istemcisi, masaüstü eylem yönlendiricisi (`[[ACTION:...]]`) ve
  bildirim olay günlüğü (`/run/makeros/olaylar.log`) entegrasyonu

### Arayüz

- **Ekran** (pencereli masaüstü, raf, masaüstü widget kartları) ve **Web** (telefona uygun) görünümleri,
  üstte *Web | Ekran* geçişi; kurulabilir web uygulaması (PWA), telefonda alt sekme çubuğu
- **Yerleşik Tarayıcı ve Ters Vekil (`/api/v1/browse`):** Google Arama entegrasyonu, yer imleri ve sekme yönetimi,
  Raspberry Pi üzerinden vekil tarama ve bot korumalı siteler için yeni sekme yönlendirmesi
- **Dosyalar ve Otomatik Başlatma:** `.py` dosyasına tıklayınca terminalde çalışır; açılışta uygulama, Python projesi veya
  betik başlatma ve Cron zamanlayıcı; Bildirimler: "Bildirimleri temizle"
- **Eklenti Mağazası ve Temalar:** `ferhatozcelik/makeros-plugins` deposundaki eklentileri doğrudan listeleme,
  SHA-1 doğrulamasıyla kurma ve güncelleme; Maker (varsayılan), Konya HackerSpace (canlı kapı durumu göstergesi ve
  masaüstü/web widget kartı), Donanım, Siber güvenlik, 3B baskı temaları

### Doğrulama

Arayüz, `makerd` servisleri, yerleşik tarayıcı ve yerel AI asistanı hem otomatik test paketleriyle hem de Raspberry Pi 4 donanımı üzerinde doğrulandı (bkz. `docs/verification/validation.md`).
