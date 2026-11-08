# Ağ ve Güvenlik Mimarisi

[Türkçe](network-security.md) · [English](network-security.en.md)

MakerOS, hem yerel ekrandan hem de ağ veya USB-C üzerinden güvenli uzaktan yönetim sağlayacak şekilde tasarlanmıştır ([`makerd/src/network.rs`](../../makerd/src/network.rs) ve [`makerd/src/auth.rs`](../../makerd/src/auth.rs)).

## Ağ Bağlantısı

- **Wi-Fi, Ethernet ve Hotspot:** NetworkManager üzerinden WPA2/WPA3, kurumsal ağlar (802.1X) ve erişim noktası (varsayılan ağ geçidi `10.42.0.1` / `10.42.0.0`) desteklenir.
- **İlk Açılış Erişim Noktası:** Kayıtlı ağ bulunamazsa şifresiz `MakerOS` kurulum ağı otomatik açılır.
- **USB-C Gadget Ağı:** Raspberry Pi bilgisayara USB-C ile bağlandığında `10.55.0.1` (`maker-os.local`) adresinden doğrudan erişilebilir.

## Güvenlik Kontrolleri

- **Oturum ve Kilit:** Oturumlar `HttpOnly; SameSite=Strict` çerezleriyle korunur; ekran kilitlendiğinde sunucu oturumu da sonlandırılır ve sayfa yenilense bile parola girilmeden masaüstüne geçilemez.
- **CSRF ve Origin Denetimi:** Durum değiştiren tüm API çağrıları `X-Maker: 1` başlığı, WebSocket bağlantıları ise `Origin == Host` eşleşmesi gerektirir.
- **Vekil İzolasyonu (`browse.rs`):** Yerleşik Tarayıcı vekili yerel/özel IP aralıklarına erişimi engeller ve sayfaları `CSP: sandbox` altında çalıştırır.
