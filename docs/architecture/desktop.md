# Masaüstü ve Web Kabuğu

[Türkçe](desktop.md) · [English](desktop.en.md)

MakerOS kabuğu ([`shell/`](../../shell/)), harici JavaScript çerçevesi veya derleme adımı gerektirmeyen yalın ES modülleriyle geliştirilmiştir. Raspberry Pi 4 üzerinde düşük GPU/CPU tüketimi için düz (`flat`) yüzey katmanını kullanır.

## İki Görünüm Modu

- **Ekran Görünümü (`/`, [`shell/index.html`](../../shell/index.html)):** Üst menü çubuğu, uygulama rafı (Dock), çoklu pencere yöneticisi ([`shell/js/wm/`](../../shell/js/wm/)), masaüstü AI Asistan widget'ı, Bul (`⌘K`), Kontrol Merkezi, Bildirim Merkezi ve Kilit ekranı sunar.
- **Web Görünümü (`/web`, [`shell/web/index.html`](../../shell/web/index.html)):** Telefon, tablet ve uzak tarayıcılar için pencere katmanı olmadan hızlı sistem kartları ve tam ekran uygulama erişimi sağlar.

## Yerleşik Uygulamalar ve Kişiselleştirme

Kabuk içinde Dosyalar, Terminal, Python Yöneticisi, AI Ajanları, Kamera, GPIO, Eklentiler, Etkinlik Monitörü, Tarayıcı (Google araması ve `/api/v1/browse` vekili destekli) ve Ayarlar yer alır. **Ayarlar › Görünüm** altından tema, duvar kâğıdı ve sistem arayüz öğelerinin (Üst Menü Çubuğu, Raf, AI Asistan, Sıcaklık, Bildirimler) görünürlüğü ayrı ayrı yönetilebilir.
