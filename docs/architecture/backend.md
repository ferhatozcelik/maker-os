# Rust Arka Uç Servisleri

[Türkçe](backend.md) · [English](backend.en.md)

MakerOS'un sistem servisleri yetki ayrımı (privilege separation) ilkesiyle Rust dilinde ([`makerd/`](../../makerd/)) geliştirilmiştir. Tokio, Axum, Serde ve zbus (D-Bus) üzerine kuruludur.

## Servis Ailesi

- **`makerd` ([`makerd/src/bin/makerd.rs`](../../makerd/src/bin/makerd.rs)):** `maker` hizmet hesabıyla çalışan ana HTTP ve WebSocket sunucusu. Statik kabuk dosyalarını, `/api/v1` uçlarını, olay yayınını (`events`), sistem izlemesini ve OTA güncellemelerini koordine eder.
- **`makerd-priv` ([`makerd/src/bin/makerd-priv.rs`](../../makerd/src/bin/makerd-priv.rs)):** Yalnızca dar kapsamlı yönetici işlemleri (kullanıcı oluşturma, parola doğrulama/değiştirme) için `root` yetkisiyle çalışan Unix soket yardımcısı.
- **`makerd-user` ([`makerd/src/bin/makerd-user.rs`](../../makerd/src/bin/makerd-user.rs)):** Oturum açan kullanıcının kendi kimliğiyle (`makerd-user@<kullanıcı>.service`) çalışan servis; PTY terminal oturumlarını, dosya yönetimini, Python projelerini (`uv`), otomatik başlatma/Cron görevlerini, kamerayı, GPIO'yu ve eklentileri yönetir.
- **`makeros-ai` ([`makerd/src/bin/makeros-ai.rs`](../../makerd/src/bin/makeros-ai.rs)):** Yerel LLM çıkarımını ve sohbet/komut değerlendirmesini sağlayan bağımsız servis.
