# makerd HTTP ve WebSocket API Sözleşmesi (`/api/v1`)

[Türkçe](api.md) · [English](api.en.md)

Bu belge [`makerd/`](../makerd/) arka ucu ile [`shell/`](../shell/) masaüstü/web kabuğu arasındaki `/api/v1` sözleşmesini tanımlar.

## Genel Kurallar

- **Taban Yolu:** `/api/v1` (JSON, UTF-8).
- **Kimlik Doğrulama:** `POST /api/v1/auth/login` çağrısı `mk_session` çerezi (`HttpOnly; SameSite=Strict; Path=/`) oluşturur.
- **CSRF Koruması:** Durum değiştiren tüm `POST`, `PUT`, `DELETE` istekleri (`/api/v1/browse` hariç) `X-Maker: 1` başlığı gerektirir.
- **Hata Biçimi:** `{"error": {"code": "kod", "message": "açıklama"}}` (`unauthorized`, `forbidden`, `not_found`, `invalid`, `conflict`, `rate_limited`, `internal`, `unavailable`).

## Temel Uç Noktalar

| Grup | Uç Noktalar | Açıklama |
| --- | --- | --- |
| **Oturum ve Kurulum** | `POST /auth/login`, `POST /auth/logout`, `GET /session`, `GET /setup`, `POST /setup/complete` | Oturum yönetimi ve ilk açılış kurulum sihirbazı |
| **Sistem ve Güncelleme** | `GET /system`, `PUT /system/hostname`, `GET\|PUT /system/time`, `GET\|PUT /system/locale`, `PUT /system/password`, `POST /system/power`, `GET /system/update`, `POST /system/update/check`, `POST /system/update/apply` | Sistem durumu, ayarlar, güç ve RAUC OTA güncellemesi |
| **Ağ** | `GET /net`, `PUT /net/wifi`, `POST /net/wifi/scan`, `POST /net/wifi/connect`, `POST /net/wifi/forget` | Ethernet, Wi-Fi ve Hotspot durumu/yönetimi |
| **Kabuk ve Tercihler** | `GET\|PUT /prefs`, `GET\|PUT\|DELETE /wallpaper/{name}`, `GET /apps`, `POST /apps/{id}/launch`, `GET /processes`, `POST /processes/{pid}/signal`, `GET\|DELETE /notifications` | Kullanıcı tercihleri, duvar kâğıdı, uygulamalar, süreçler ve bildirimler |
| **Terminal ve Olaylar** | `GET\|POST /term`, `WS /term/{id}?ticket=`, `POST /term/{id}/ticket`, `DELETE /term/{id}`, `WS /events` | PTY kabuk oturumları ve canlı sistem/ağ/iş olay akışı |
| **Dosyalar ve Python** | `GET /files`, `GET\|PUT /files/raw`, `POST /files/op`, `POST /files/run`, `GET /python`, `POST /python/{op}`, `GET\|POST /projects`, `GET /projects/templates`, `POST /projects/{name}/{run\|start\|stop\|packages}` | Dosya gezgini ve `uv` tabanlı Python proje yöneticisi |
| **Otomatik Başlatma** | `GET\|POST /autostart`, `POST /autostart/{id}/{toggle\|run}`, `DELETE /autostart/{id}` | Açılışta (`boot`) veya Cron zamanlamasıyla uygulama/Python/betik çalıştırma |
| **Donanım ve Eklentiler** | `GET /gpio`, `POST /gpio/{n}`, `GET /camera`, `GET /camera/stream`, `POST /camera/photo`, `GET /plugins`, `GET /plugins/catalog`, `POST /plugins/catalog/{id}/install`, `POST /plugins/import` | 40-pin GPIO, Kamera MJPEG/fotoğraf ve Eklenti Mağazası |
| **AI ve Tarayıcı** | `POST /asistan/calistir`, `ANY /ai/{*rest}` (`POST /v1/chat/completions`, `GET /v1/makeros/durum`, `POST /v1/makeros/temizle`), `POST /browse/ticket`, `GET\|POST /browse` | Yerel AI asistanı, komut çalıştırma ve Tarayıcı vekili |
