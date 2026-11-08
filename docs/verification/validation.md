# Doğrulama Süreçleri

[Türkçe](validation.md) · [English](validation.en.md)

Bu doküman, MakerOS bileşenlerinin hangi koşullarda ve hangi yöntemlerle doğrulandığını açıklar. Doğrulama dokümanı performans yarışı veya yapay kıyaslama içermez; yalnızca gerçek test süreçlerini ve henüz tamamlanmamış gelecek planlarını şeffaf biçimde ayırır.

## Doğrulanmış Bileşenler

1. **Birim ve Entegrasyon Testleri (`makerd/`):** Kimlik doğrulama, CSRF/Origin korumaları, PTY terminal biletleri, dosya işlemleri, `uv` Python proje yaşam döngüsü, Otomatik Başlatma / Cron kuralları, RAUC sürüm karşılaştırması, AI komut risk sınıflandırması ve tarayıcı vekili (`browse.rs`) otomatik testlerle doğrulanmıştır.
2. **Kabuk Duman Testleri (`shell/tools/smoke.mjs`):** İlk açılış kurulum sihirbazı, oturum açma/kilitleme, masaüstü pencere yönetimi, `/web` mobil görünümü ve tüm yerleşik uygulamalar Docker içinde başsız Chromium ile uçtan uca test edilmiştir.
3. **Raspberry Pi 4 Donanım Doğrulaması:** Buildroot imajı, EROFS + `/data` overlay yapısı, USB-C (`10.55.0.1`) ve Wi-Fi bağlantısı, `makerd` servis ailesi, `makeros-ai` (Qwen2.5-0.5B + `llama.cpp`), GPIO ve Kamera akışı gerçek Raspberry Pi 4 Model B üzerinde doğrulanmıştır.

## Gelecek Planları (Henüz Doğrulanmamış / Yol Haritası)

- Farklı ARM64 tek kartlı bilgisayarlara (Raspberry Pi 5 ve diğer SBC platformları) taşınabilir görüntü profilleri.
- Ayarlar arayüzünden SSH anahtar yönetimi ve açma/kapatma düğmesi.
- Yerel Wayland uygulama pencerelerinin web masaüstü pencere yöneticisiyle doğrudan birleştirilmesi.
