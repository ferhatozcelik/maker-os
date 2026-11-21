# MakerOS eklenti şablonu

1. `plugin.json` içinde `id` (küçük harf, rakam, tire), `name`, `version`, `description` alanlarını düzenleyin.
2. Bağımlılıkları `pyproject.toml` dosyasına ekleyin (`uv add paket` de olur).
3. Klasörü zip'leyin (zip kökünde `plugin.json` olmalı) ve MakerOS → Eklentiler → İçe aktar ile yükleyin.

Kurallar: zip ≤ 15 MB, açılmış ≤ 64 MB, ≤ 2000 dosya, sembolik bağ yok. `web_port` varsa sunucu yalnız
127.0.0.1'i dinlemelidir; MakerOS oturum doğrulamasından sonra arayüzü `/plugin/<id>/` adresinden açar.
Komut satırı: `mos plugin new`, `mos plugin import x.zip`, `mos plugin export <id>`, `mos plugin start|stop <id>`.

## Tema ve dil (eklenti kiti)

Arayüz MakerOS ile aynı görünsün ve eşitlensin diye sayfaya ekleyin:

```html
<link rel=stylesheet href="/shell/css/tokens.css">   <!-- --accent, --text, --bg-window, --separator … -->
<script src="/shell/js/plugin-kit.js"></script>      <!-- window.MK -->
```

`MK.lang` (tr|en), `MK.dark`, `MK.accent`; `MK.i18n({tr:{…}, en:{…}})` + `data-t="anahtar"` / `MK.t("anahtar")`;
`MK.on((mk, changed) => …)` kabukta dil veya tema değişince çağrılır (sayfa yenilenmez). Sunucu mesajları için
isteklere `X-MakerOS-Lang: MK.lang` başlığını ekleyin. `MK.theme()` gerçek paleti verir (gömülü üçüncü taraf arayüzlere aktarmak için).
