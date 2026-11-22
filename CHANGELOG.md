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
- **Tek komutla kurulum ve güncelleme (`flash.sh`):** başta bağlantı türü sorulur. *SD*: kart okuyucudaki
  kart baştan yazılır. *USB*: kart Pi'de kalır, Pi USB-C ile bağlanır; imzalı paket USB-C ağı (`10.42.0.1`)
  üzerinden gönderilip A/B olarak kurulur, veriler korunur
- **USB-C ağ kartı:** Pi bilgisayara USB-C ile takılınca `10.42.0.1` adresinden doğrudan erişilir
- **Ekransız kurulum:** `wifi.txt` ya da şifresiz `MakerOS` kurulum ağı; tek adımlı ilk açılış sihirbazı
  (varsayılan `maker` / `maker` hesabı)
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

### Bu sürümle gelenler

- **Varsayılan hesap `maker` / `maker`** (ilk açılış sihirbazı). makerd hizmet hesabının adı `makerd` oldu (UID 999 aynı);
  eski cihazlarda açılışta bir kez çevrilir (`makeros-account-migrate`)
- **Yerel AI asistanı kendiliğinden kurulur:** model (Qwen2.5-0.5B, ~470 MB) ilk açılışta internet gelince indirilir ve doğrulanır
  (`makeros-ai-model.service`); arayüz indirme ilerlemesini gösterir
- **Eklentiler cihazın içinde:** Pi-hole ve Home Assistant rootless Podman kapsayıcısında bu cihazda çalışır; arayüzleri kendi
  panelleri, MakerOS temasıyla ve otomatik girişle. Pi-hole için 53. port kullanıcı süreçlerine açıldı
- **Chromecast ekran paylaşımı:** MakerOS masaüstü televizyonda (tek kullanımlık, 60 sn'lik bilet; yönetici yetkisi yok), *Bağlantıyı kes*
- Konya HackerSpace eklentisi artık kendiliğinden kurulmaz (Eklentiler › Mağaza)
- Resmî eklentiler (Pi-hole, Home Assistant, Chromecast, Konya HackerSpace) *Resmî* rozetiyle; tema simge paketi
  eklenti uygulamalarına da uygulanır (Konya HackerSpace teması)
- Bluetooth listesi MAC adresi yerine cihaz adlarını gösterir; adı henüz gelmemiş eşleşmemiş cihazlar listelenmez
- Web arayüzünde kenar çubuğu gizlenebilir (menü düğmesi)
- AI asistanı modeli 20 dk boşta kalınca bellekten atılır (Pi 4'te sık soğuk başlangıç olmasın)
- Yazılım güncelleme: RAUC önyükleyici betiği ilk kablosuz güncellemede kurulumu durduruyordu; düzeltildi

### Eklentiler

- **Canlı eşitleme:** eklenti başlatılınca, durdurulunca, kurulunca ya da kaldırılınca makerd `plugins` olayı yayınlar;
  raf, uygulamalar, menüler, tema, açık pencereler ve Web arayüzü kendiliğinden yenilenir (durdurulan eklentinin penceresi kapanır)
- **Masaüstü widget katmanı:** `plugin.json`'da `widget` tanımlayan her eklenti temadan bağımsız olarak masaüstüne ve Web ana sayfasına
  eklenebilir (Eklentiler › *Masaüstüne ekle*, widget'taki ×)
- **Eklenti kiti** (`/shell/js/plugin-kit.js` + `tokens.css`): eklenti arayüzleri MakerOS'un dilini (tr/en) ve temasını
  (vurgu, koyu/açık, yazı tipi) kullanır ve değişince yenilenmeden güncellenir; sunucu mesajları `X-MakerOS-Lang` ile aynı dilde.
  Eklenti şablonu kiti kullanır
- Mağazada yeni eklentiler: **Home Assistant** (kendi arayüzü MakerOS temasıyla, MakerOS kullanıcısıyla otomatik giriş, ilk kurulumu
  MakerOS yapabilir, varlık widget'ı), **Pi-hole**, **Chromecast**

### Python

- Yeni proje şablonu **AI · Görüntü tanıma (LiteRT)**: Google LiteRT ile cihaz üzerinde EfficientNet-B1 sınıflandırma
  (web arayüzü, `--image`, `--camera`)

### Sistem ve arayüz

- **Bluetooth:** radyo, arama, eşleştirme, bağlanma. Kulaklık/hoparlöre ses yönlendirmesi bluez-alsa ister; bu imaj
  bluez-alsa içermez, yönlendirme bir sonraki tam derlemeyle gelir (makerd bu arada sesi bozmaz)
- **GPIO:** pin bağlantı taraması (iç pull-up/down ile boşta / dışarıdan sürülüyor tahmini)
- **AI Asistan:** kurulu CLI ajanı (Claude Code, Codex, Gemini CLI, OpenCode, Ollama) ve model seçimi; tam sayfa görünüm
