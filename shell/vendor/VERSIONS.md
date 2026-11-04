# Üçüncü taraf dosyalar (vendor)

Kabuk çalışma anında yalnızca bu dizindeki yerel kopyaları kullanır (CDN yok, çevrimdışı çalışır).
Paket tarball'ları npm kayıt defterinden indirildi; sha256 değerleri aşağıdadır.

| Bileşen | Sürüm | Lisans | Kaynak tarball (sha256) |
|---|---|---|---|
| @xterm/xterm | 5.5.0 | MIT (`xterm/LICENSE`) | `https://registry.npmjs.org/@xterm/xterm/-/xterm-5.5.0.tgz` (bd954fa721872170188cc5d7e83e88db3c83c9a18a4e8d24c2783d26491f59d2) |
| @xterm/addon-fit | 0.10.0 | MIT (`xterm/LICENSE.addon-fit`) | `https://registry.npmjs.org/@xterm/addon-fit/-/addon-fit-0.10.0.tgz` (917ac44972453d5eed52edc1e50260c76398ce48cf2290c2e60671102bba0b33) |
| Inter (değişken, wght) | @fontsource-variable/inter 5.3.0 | SIL OFL 1.1 (`fonts/OFL-Inter.txt`) | `https://registry.npmjs.org/@fontsource-variable/inter/-/inter-5.3.0.tgz` (d26710dd38e7217484a1d47ee76c024977ec550548876f26d06ed1844af09a14) |
| JetBrains Mono (değişken, wght) | @fontsource-variable/jetbrains-mono 5.3.0 | SIL OFL 1.1 (`fonts/OFL-JetBrainsMono.txt`) | `https://registry.npmjs.org/@fontsource-variable/jetbrains-mono/-/jetbrains-mono-5.3.0.tgz` (996fe6368a480c9ce15d4de22a2682b7c40b403718fee2b15e7272f244fd993f) |
| Lucide simgeleri (alt küme, `vendor/lucide/svg/`) | lucide-static 1.66.0 | ISC (`lucide/LICENSE`) | `https://registry.npmjs.org/lucide-static/-/lucide-static-1.66.0.tgz` (6a1debc19076123ece73f6729a9643ac90aab9c9db15f7c16debf72a9a02a82a) |

## Dosya sha256 değerleri

```
1f991ac3b4b283ebf96e60ae23a00a52765dd3a2e46fa6fdda9f1aab032f7495  xterm/xterm.js        (package/lib/xterm.js)
ba8e6985669488981ccf40c0cefe3aba80722cb6c92de7ad628b0bd717faf2b6  xterm/xterm.css       (package/css/xterm.css)
bdaefa370b1bfc42ee88d46fe6072400902a4d4b2d45cd93438dda9b23c97089  xterm/addon-fit.js    (package/lib/addon-fit.js)
3100e775e8616cd2611beecfa23a4263d7037586789b43f035236a2e6fbd4c62  fonts/inter-latin-wght-normal.woff2
34b9c504cab7a73e37b746343a449132e56cf7b5481af2cb81dc74dcff25c956  fonts/inter-latin-ext-wght-normal.woff2
18be452724bfdc236c074ca94a249a7f41a86752c7d04ab258ce9ed5651f6a7e  fonts/jetbrains-mono-latin-wght-normal.woff2
79bfdab9ba467e26eea4122e6f2567e188dd8a09a8c730d501fc487c4ab99c6e  fonts/jetbrains-mono-latin-ext-wght-normal.woff2
```

Yazı tiplerinin `latin` + `latin-ext` alt kümeleri Türkçe karakterleri (ğ ş ı İ ç ö ü) kapsar.

## Güncelleme

- Lucide: yeni simgenin svg'sini `lucide/svg/` altına kopyalayın, `node shell/tools/gen-icons.mjs` çalıştırın (→ `shell/js/icon-set.js`).
- xterm: tarball'ı indirip sha256'yı doğrulayın, `lib/xterm.js`, `css/xterm.css`, `addon-fit/lib/addon-fit.js` dosyalarını kopyalayın ve bu tabloyu güncelleyin.

Apple varlığı, SF yazı tipi veya SF Symbols kullanılmaz. Uygulama simgeleri ve duvar kâğıdı MakerOS'a özgüdür
(`tools/gen-app-icons.mjs`, `tools/gen-wallpaper.mjs`).

## noVNC (Ekran Modu ve Chromium penceresi)

| Bileşen | Sürüm | Lisans | Kaynak tarball (sha256) |
|---|---|---|---|
| @novnc/novnc (`core/`, `vendor/pako`) | 1.7.0 | MPL-2.0 (`novnc/LICENSE.txt`; pako MIT) | `https://registry.npmjs.org/@novnc/novnc/-/novnc-1.7.0.tgz` (32689f18d6abe96bc6530828a6bd0b9ae33bda07c083a6575ed255b5a8f2e903) |

Değiştirilmeden kopyalanmıştır; `apps/screen/screen.js` doğrudan `core/rfb.js` ES modülünü içe aktarır.
