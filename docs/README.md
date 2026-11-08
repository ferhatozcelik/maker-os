# MakerOS Dokümantasyonu

[Türkçe](README.md) · [English](README.en.md)

MakerOS dokümantasyonu; işletim sistemi mimarisini, arka uç servislerini, masaüstü kabuğunu, geliştirme süreçlerini ve doğrulama yöntemlerini açık ve modüler bir yapıda sunar.

## İçindekiler

### Mimari (`architecture/`)
- [Genel Mimari](architecture/overview.md) — Katmanlar, sistem bileşenleri ve çalışma modeli
- [Önyükleme ve Depolama](architecture/boot-and-storage.md) — EROFS kök dosya sistemi, ext4 `/data` ve RAUC + `tryboot` A/B güncelleme
- [Masaüstü ve Web Kabuğu](architecture/desktop.md) — `/` masaüstü ve `/web` mobil arayüzü, pencere yönetimi ve temalar
- [Rust Arka Uç Servisleri](architecture/backend.md) — `makerd`, `makerd-priv`, `makerd-user` ve süreç koordinasyonu
- [Yerel AI Asistanı](architecture/ai-assistant.md) — `makeros-ai`, Qwen2.5-0.5B-Instruct, sohbet ve komut çalıştırma modları
- [Ağ ve Güvenlik](architecture/network-security.md) — Oturum, CSRF, WebSocket Origin denetimi, Hotspot ve USB-C ağı
- [Donanım Desteği](architecture/hardware.md) — Pi 4 / 400 uyumlu ARM64 kartlar, GPIO, I²C, SPI, UART ve Kamera altyapısı

### Geliştirme, Doğrulama ve Proje
- [Derleme ve Test](development/build-and-test.md) — Docker tabanlı imaj derleme, yerel önizleme ve duman testleri
- [Doğrulama Süreçleri](verification/validation.md) — Doğrulanmış özellikler, test ortamları ve gelecek planları
- [Marka ve Lisanslama](project/branding-and-licensing.md) — MIT lisansı ve üçüncü taraf marka kuralları
- [HTTP & WebSocket API Sözleşmesi](api.md) — `/api/v1` uç noktaları ve olay akışı
