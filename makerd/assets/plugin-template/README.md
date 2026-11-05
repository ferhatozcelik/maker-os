# MakerOS eklenti şablonu

1. `plugin.json` içinde `id` (küçük harf, rakam, tire), `name`, `version`, `description` alanlarını düzenleyin.
2. Bağımlılıkları `pyproject.toml` dosyasına ekleyin (`uv add paket` de olur).
3. Klasörü zip'leyin (zip kökünde `plugin.json` olmalı) ve MakerOS → Eklentiler → İçe aktar ile yükleyin.

Kurallar: zip ≤ 15 MB, açılmış ≤ 64 MB, ≤ 2000 dosya, sembolik bağ yok. `web_port` varsa sunucu yalnız
127.0.0.1'i dinlemelidir; MakerOS oturum doğrulamasından sonra arayüzü `/plugin/<id>/` adresinden açar.
Komut satırı: `mos plugin new`, `mos plugin import x.zip`, `mos plugin export <id>`, `mos plugin start|stop <id>`.
