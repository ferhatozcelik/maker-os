# MakerOS — web masaüstü kabuğu (`shell/`)

Pi 4'te WPE/cog içinde (1080p, zayıf GPU) ve uzaktan herhangi bir masaüstü/telefon tarayıcısında çalışan
masaüstü kabuğu ve sistem uygulamaları. **Derleme adımı yok**: yerel ES modülleri + Web Components (`mk-*`),
çalışma anı bağımlılığı yok (yalnız `vendor/` altındaki sabitlenmiş dosyalar). Arka uç sözleşmesi: [`docs/api.md`](../docs/api.md).

## Çalıştırma (yalnız Docker)

Ana makinede Node veya Chrome gerekmez; tüm araçlar kapsayıcıda çalışır, `shell/Makefile` yalnız `docker`/`docker-compose` çağırır.

```sh
cd shell
# Tam önizleme (gerçek makerd --mock; GPIO, Ajanlar, Kamera, Mağaza dahil): cd ../makerd && make preview → http://localhost/
make dev            # node:24-alpine içinde dev-server → http://localhost/ (maker / maker), sahte API + events WS + sahte terminal
make dev-fresh      # ilk açılış: kullanıcı yok → kurulum sihirbazı
make tokens         # tokens.json → css/tokens.css (gen-tokens.mjs kapsayıcıda)
make smoke          # dev-server + duman testi (makeros/shell-smoke: node:24-bookworm-slim + Chromium, tools/docker/Dockerfile.smoke)
make smoke-fresh    # --fresh dev-server'a karşı: sihirbaz (telefon + masaüstü) + tam test
make smoke-makerd   # gerçek makerd --mock, MAKERD_MOCK_FRESH=1 (docker-compose build makerd) → sihirbaz + tam test
make down           # mk-shell-dev, mk-makerd kapsayıcılarını ve makeros-shell ağını kaldır
make smoke OUT=$PWD/.smoke/x  # ekran görüntüleri (varsayılan shell/.smoke; Docker Desktop paylaşımlı bir yol olmalı — /tmp değil)
```

Duman testi kapsayıcısı hedefe `makeros-shell` Docker ağı üzerinden bağlanır (`http://mk-shell-dev:8090`, `http://mk-makerd:8080`).
Önizleme için tüm yığın: depo kökünde `docker-compose up -d --build` → http://localhost/.
`dev-server.mjs` uçları: `--fresh` (kurulumsuz başla), `POST /api/v1/dev/reset` (yalnız dev-server; kurulumsuz duruma dön).

URL parametreleri: `?fx=full|lite|flat` (efekt katmanını zorla), `?open=makeros.terminal` (açılışta uygulama aç),
`?perf=1` (sol altta kare hızı, en kötü kare, uzun görevler, pencere sayısı, JS yığını; WPE'de `longtask` yoksa rAF aralığından tahmin, `*` ile işaretli).

## Dizin yapısı

```
index.html            kabuk sayfası (katmanlar: #desktop #windows #menubar #dock #overlays #popups #lock)
tokens.json           tasarım belirteçleri (tek kaynak)  →  css/tokens.css (üretilir)
css/kit.css           ortak denetimler (düğme, alan, anahtar, gruplu liste) + yazı tipleri
css/shell.css         masaüstü çekirdeği: malzemeler, menü çubuğu, raf, pencereler, Kontrol/Bildirim Merkezi
css/overlays.css, lock.css, mobile.css   tembel stiller (ilgili modülle birlikte yüklenir)
css/app.css           uygulama yerleşimleri (kenar çubuğu, sayfa, bağımsız başlık çubuğu)
js/main.js            açılış, oturum/kurulum kararı, kısayol eylemleri, S bağlamı, tembel modül tablosu (M)
js/shell.js           S: bileşenler arası paylaşılan bağlam (wm, events, launch, openSettings …)
js/api.js             fetch sarmalayıcı (X-Maker, hata gövdesi, 401 → kilit) + Events (WS, yeniden bağlanma, konu aboneliği)
js/keys.js            kısayol tablosu, eşleştirici, ⌘/Ctrl yedeği
js/prefs.js           tema / fx / vurgu / raf / dil — kaynak GET/PUT /prefs, localStorage önbellek (tüm pencerelerde canlı)
js/perf.js            ?perf=1 performans göstergesi (tembel)
js/apps.js            uygulama kaydı (GET /apps + yerleşik meta), Ayarlar sayfa listesi
js/wm/index.js        WindowManager — arka uçtan bağımsız durum, odak, z-sırası, yerleşim, genel bakış, kalıcılık
js/wm/web.js          <mk-window> + WebBackend (iframe), uygulama↔kabuk postMessage köprüsü
js/wm/native.js       M3 native pencere arka ucu taslağı (makerd WS köprüsü)
js/ui/*.js            çekirdek: menubar, dock, control (Bildirim Merkezi, afiş, toast), layer (katman yöneticisi), popup
                      tembel: cc (Kontrol Merkezi), find (Bul + hesap makinesi), launchpad, switcher, overview,
                      lock (giriş/kilit/iletişim), mobile
setup/                ilk açılış sihirbazı (setup.js + setup.css, yalnız kurulum bekliyorsa yüklenir)
js/app-kit.js         uygulama çerçevesi (iframe veya bağımsız sayfa)
apps/<ad>/            settings, terminal, activity + "Yakında": camera, gpio, python, files, store (apps/soon)
i18n/tr.js, en.js     arayüz metinleri (tr varsayılan; seçili olmayan dil yüklenmez)
icons/                özgün uygulama simgeleri + logo (tools/gen-app-icons.mjs)
wallpapers/           özgün "Akış" duvar kâğıdı + önceden bulanık sürümler (tools/gen-wallpaper.mjs)
vendor/               xterm.js 5.5.0, Inter, JetBrains Mono, Lucide alt kümesi — bkz. vendor/VERSIONS.md
tools/                gen-tokens, gen-icons, gen-app-icons, gen-wallpaper, dev-server, smoke
```

## Bileşenler

| Bileşen | Açıklama |
|---|---|
| Menü çubuğu | Logo menüsü (Bu MakerOS Hakkında, Sistem Ayarları, Uygulamalar, Kilitle, Oturumu Kapat, Yeniden Başlat, Kapat), etkin uygulama adı + Dosya/Düzen/Görünüm/Pencere/Yardım; sağda sıcaklık, Wi-Fi, Bul, Kontrol Merkezi, saat (→ Bildirim Merkezi) |
| Raf (dock) | Sabitlenmiş + çalışan uygulamalar, çalışma noktası, sağ tık/uzun basış menüsü (Rafta Tut, Yeni Pencere, Gizle, Çık), açılış zıplaması, isteğe bağlı büyütme (yalnız `fx=full`) |
| Pencere yöneticisi | Trafik ışıkları (kapat/küçült/yakınlaştır, Alt = tam ekran), sürükleme, 8 yönden boyutlandırma, kenara yaslama (üst = büyüt, sol/sağ = yarım), çift tık = yakınlaştır, z-sırası, küçültme (rafa ölçek/opaklık), tam ekran, oturum/boyut geri yükleme |
| Bul | Bulanık arama (Türkçe katlama: "monitoru" → Etkinlik Monitörü), uygulamalar, Ayarlar sayfaları, güvenli hesap makinesi (`eval` yok; `3,5*2`, `2^10`, `%`) |
| Değiştirici / Genel Bakış | ⌘Tab (MRU uygulamalar, değiştirici bırakılınca seçer), F3/Ctrl+↑ pencereleri transform ölçeğiyle döşer |
| Kilit / giriş | `GET /session`, `POST /auth/login`; 401 gelirse oturum yeniden açılır, pencereler korunur |
| Kurulum sihirbazı | `/session` 401 + `GET /setup` 200 ya da `setup_done:false` → sihirbaz (aşağıda); `/setup` 409/403 → giriş ekranı |
| Kontrol Merkezi | Wi-Fi döşemesi: daire = radyo aç/kapat (`PUT /net/wifi {enabled}`, yönetici; değilse Ayarlar → Wi-Fi), metin = Ayarlar; koyu görünüm, efekt katmanı |
| Menü çubuğu CPU | Küçük dikey gösterge `cpu.usage_pct` (yoksa 1 dk yük/çekirdek tahmini) + sıcaklık; tıklayınca Etkinlik Monitörü |
| Mobil (< 700 px) | Başlatıcı ızgarası + tam ekran uygulamalar + alt ana çubuk; pencere yok |

### Uygulama ↔ kabuk köprüsü (`postMessage`, aynı köken)

Kabuk → uygulama ayrıca: `{mk:"visibility",visible}`.
Uygulama → kabuk: `{mk:"title",title}`, `{mk:"chrome",sidebar,tone:"dark",tbRight,title:false}`, `{mk:"path",path}`,
`{mk:"open",app,path}`, `{mk:"notify",title,body}`, `{mk:"unauthorized"}`, `{mk:"close"}`, `{mk:"shortcut",id}`, `{mk:"power",action}`.
Kabuk → uygulama: `{mk:"command",cmd:"settings"|"new"}`, `{mk:"navigate",path}`.
Kabuk, aynı kökenli iframe'lerin `keydown`'ını yakalama aşamasında dinler; kısayollar uygulama odaktayken de çalışır.
Uygulamalar kabuğun tek events WS bağlantısını paylaşır (`app.events`).

### Pencere yöneticisi arayüzü

`WindowManager` (`js/wm/index.js`) arka uca yalnız şu arayüzle konuşur: `backend.create(win) → view`,
`view.render(win)`, `view.play(kind, from)`, `view.send(msg)`, `view.focusContent()`, `view.destroy()`.
M3'te `NativeBackend` aynı arayüzü compositor IPC'siyle (window.move/resize/focus, genie dikdörtgeni) uygular;
compositor'dan gelen pencereler `wm.adopt(app, view)` ile yönetime alınır.

## İlk açılış sihirbazı (`setup/`)

Tek adım: **Wi-Fi** (her zaman gösterilir; ağ seç + parola, gizli ağ, yeniden tara; varsayılan seçim "Şimdilik atla",
düğme "Atla" / "Bağlan ve devam et") → `POST /setup/complete` → **Bitti** → masaüstü. Hesap, dil, klavye, saat dilimi ve cihaz adı
sorulmaz: varsayılan yönetici `makeros` / `makeros` (`DEFAULT_USER`; `maker` makerd hizmet hesabıdır), dil/klavye `defaults`,
saat dilimi tarayıcınınki (listede varsa) ya da `defaults`, cihaz adı `GET /setup` önerisi (`makeros-xxxx`). Hepsi sonradan
Ayarlar'dan değiştirilir; Bitti ekranı ağ adresini, kullanıcı adını ve parolayı gösterip parolanın değiştirilmesini önerir.
Pi'nin kendi ekranı `/auth/local` ile otomatik oturum açar. Telefonda (MakerOS erişim noktası → captive portal `/`) kart tam
ekran olur; Wi-Fi seçildiyse cihazın ağ değiştireceği ve `http://<ad>.local` adresi bildirilir. Metinler `i18n/*.js` içinde
`su.*`. `POST /setup/complete` oturum çerezi verir; kabuk `location.replace("/")` ile doğrudan masaüstüne geçer. `409` →
"zaten kurulu" + giriş ekranı.

API (docs/api.md → "İlk açılış kurulumu"; sahte uygulama: `dev-server.mjs --fresh`, gerçek: `MAKERD_MOCK_FRESH=1 makerd --mock`):
`GET /setup` → `{hostname, defaults:{lang,keymap,timezone}, languages:[{id,name}], keymaps:[{id,name}], timezones[], online, wifi_needed, hotspot, min_password}`;
Wi-Fi adımına girilince `POST /setup/wifi/scan` (kimliksiz); `POST /setup/complete {user,password,fullname?,hostname,timezone,lang,keymap,wifi?:{ssid,password?,hidden?}}`
→ `{user,admin,job?,warnings?}` + çerez (`warnings` Bitti ekranında gösterilir).

## Tercihler (`GET/PUT /prefs`)

Doğruluk kaynağı sunucudur: oturum açılınca `syncPrefs()` sunucu değerlerini önbelleğe (localStorage, `mk.*`) yazar; sunucu boşsa
yerel değerler yüklenir. `setPref()` 700 ms debounce ile tüm gövdeyi `PUT` eder (sayfa kapanırken `keepalive`). Eşitlenen anahtarlar:
`theme, fx, accent, magnify, ctrlCmd, dockApps, lang` — sunucudaki dil farklıysa kabuk bir kez yeniden yüklenir. Pencere
konumları (`wm.*`) cihaza özeldir, yalnız yerel kalır. Arka uç `404/405` dönerse eşitleme sessizce kapanır.
Telefonda doğrudan açılan uygulamalar (`app-kit`, bağımsız mod) da açılışta `syncPrefs()` çağırır.

## Performans (Pi 4 / WPE)

- **Tembel yükleme**: masaüstü ilk karesi için yalnız çekirdek modüller içe aktarılır; Bul/değiştirici boşta önceden yüklenir,
  Uygulamalar ızgarası, Genel Bakış, Kontrol Merkezi, kilit/giriş, mobil düzen, sihirbaz, perf HUD ve native arka uç ilk kullanımda
  `import()` ile gelir (`main.js → M`, `lib.js → lazy()/loadCSS()`).
- **Efektler**: `lite`/`flat` katmanında hiçbir yüzeyde `backdrop-filter` yoktur (önceden bulanık duvar kâğıdı); `full`'da yalnız
  menü çubuğu, raf, menü/popover, Bul ve Bildirim Merkezi. Sihirbaz kartı her katmanda opaktır.
- **Sürükleme**: `pointermove` yalnız son konumu saklar, rAF başına bir kez `transform` yazılır; WM durumu `pointerup`'ta işlenir.
  **Boyutlandırma**: `full`'da canlı (rAF), `lite/flat`'te yalnız çerçeve (`.rz-ghost`) izlenir, iframe bir kez yeniden yerleşir.
- `render()` yerleşim okumaz (raf yüksekliği `ResizeObserver` ile önbellekte). Pencerelerde `contain: layout size style`,
  gövdede `contain: strict`, küçültülmüş pencerede `content-visibility: hidden`.
- **Görünürlük**: kabuk her pencereye `{mk:"visibility",visible}` gönderir (küçültme, gizleme, masaüstünü göster, mobilde arka plan);
  `app.visible` / `app.onVisibility(fn)`. Etkinlik Monitörü gizliyken `processes` aboneliğini bırakır ve grafik çizmez.

## Yeni uygulama ekleme

1. `apps/<ad>/index.html` oluşturun (tokens.css, kit.css, app.css bağlayın) ve bir modül yükleyin:
   ```js
   import { app, t } from "/shell/js/app-kit.js";
   app.init({ title: "Örnek" });                 // bağımsız açılışta başlık çubuğu ekler
   app.events.subscribe("system", s => …);      // paylaşılan olay akışı
   app.on("settings", () => …);                 // ⌘, ile gelen komut
   app.onVisibility(v => …);                    // gizliyken yoklama/çizimi durdurun
   ```
2. Simge: `tools/gen-app-icons.mjs` içindeki listeye ekleyip çalıştırın (→ `icons/<ad>.svg`).
3. Kayıt: makerd `/apps` listesine `{"id":"makeros.<ad>","kind":"web","url":"/shell/apps/<ad>/"}` ekleyin;
   pencere boyutu ve arama anahtar kelimeleri için `js/apps.js` → `BUILTIN`.
4. Metinler `i18n/tr.js` ve `en.js` içine. Uygulama telefonda doğrudan `/shell/apps/<ad>/` adresiyle de açılabilmeli (duyarlı yerleşim).

## Tasarım belirteçleri

`tokens.json` → `make tokens` (gen-tokens.mjs kapsayıcıda) → `css/tokens.css` (üretilen dosya depoya eklenir). Katmanlar:
temel (`--fs-*`, `--radius-*`, `--space-*`, `--dur-*`, `--ease-*`), bileşen (`--mk-menubar-height`, `--mk-traffic-*`, `--mk-window-radius`…),
tema (`--bg*`, `--text*`, `--material-*`; açık `:root`, koyu `[data-theme=dark]` veya sistem tercihi), vurgu (`[data-accent=blue|purple|…]`).
`<html>` öznitelikleri: `data-theme` (yoksa otomatik), `data-fx`, `data-accent`, `data-layout=desktop|mobile`.

**Efekt katmanları (`data-fx`)** — yalnız `transform`/`opacity` canlandırılır:
- `full`: küçük, sabit yüzeylerde canlı `backdrop-filter` (menü çubuğu, raf, popover, Bul).
- `lite` (Pi'de varsayılan; `hardwareConcurrency ≤ 4`, `deviceMemory ≤ 4` veya WPE): önceden bulanık duvar kâğıdı `background-attachment: fixed` ile, canlı bulanıklık yok.
- `flat`: düz yüzeyler (`prefers-reduced-transparency` ile otomatik).

Yazı tipleri Inter + JetBrains Mono (OFL), simgeler Lucide (ISC). SF yazı tipleri/simgeleri, Apple görselleri ve marka adları kullanılmaz
("Bul", "Genel Bakış", "Uygulamalar", "Raf").

## Klavye kısayolları

"Mod" = Super/⌘. Uzak tarayıcıda Ctrl de ⌘ sayılır (Ayarlar → Klavye → "Ctrl'yi ⌘ gibi kullan": Otomatik/Açık/Kapalı;
Otomatik = WPE dışında açık). Terminal odaktayken Ctrl her zaman terminale gider.

| Eylem | ⌘ | PC (alternatifler) |
|---|---|---|
| Bul | ⌘Space | Super+Space, Ctrl+Space, Alt+Space |
| Uygulama değiştirici (ileri/geri) | ⌘⇥ / ⇧⌘⇥ | Super+Tab, Alt+Tab / +Shift |
| Uygulamanın sonraki penceresi | ⌘` | Super+` |
| Çık / Pencereyi kapat / Küçült / Gizle | ⌘Q / ⌘W / ⌘M / ⌘H | Super+Q / W / M / H |
| Uygulama ayarları / Yeni pencere | ⌘, / ⌘N | Super+, / Super+N |
| Genel bakış | ⌃↑ | Ctrl+↑, Super+↑, F3 |
| Masaüstünü göster | ⌘D | Super+D, Super+F3 |
| Uygulamalar ızgarası | F4 | F4, Super+Shift+A |
| Tam ekran | ⌃⌘F | Ctrl+Super+F, F11 |
| Ekranı kilitle | ⌃⌘Q | Ctrl+Super+Q |
| Ekran görüntüsü (yer tutucu) | ⇧⌘3 / ⇧⌘4 | Super+Shift+3 / 4 |
| Bildirim merkezi | ⇧⌘N | Super+Shift+N |
| Sola / sağa yasla | ⌥⌘← / ⌥⌘→ | Super+Alt+← / → |
| Terminal | ⌘↩ | Ctrl+Alt+T, Super+Enter |
| Etkinlik Monitörü | ⌥⌘esc | Super+Alt+Esc |
| Kısayol listesi | ⌘/ | Super+/ |

Tarayıcıların ayırdığı birleşimler (Ctrl+W/N/T/Q, Ctrl+Tab) uzak oturumda yakalanamaz; Super kullanın.
Terminal içinde: ⌘C/⌘V kopyala/yapıştır, ⌘T yeni sekme, ⌘K temizle; dokunmatik cihazlarda ek tuş çubuğu (Esc, Tab, Ctrl, oklar, |, /, ~, ^C, ^D).

## Boyut bütçesi

Hedef: masaüstü açılışında yüklenen JS+CSS (vendor ve i18n hariç, sıkıştırmasız) < 150 KB. Duman testi her çalışmada
gerçek açılış boyutunu yazdırır (`ℹ açılış …`). Elle ölçüm:

```sh
cd shell && cat css/{tokens,kit,shell}.css js/{main,lib,i18n,prefs,api,apps,keys,shell,icons}.js js/wm/{index,web}.js \
  js/ui/{menubar,dock,control,layer,popup}.js | wc -c          # açılış (M2: ~97 KB, gzip ~30 KB)
cat $(find js css -name '*.js' -o -name '*.css') | wc -c        # kabuğun tamamı (uygulamalar ve sihirbaz hariç)
```

| | M1 | M2 |
|---|---|---|
| Açılış (render-blocking + statik modüller) | 117 KB | 97 KB |
| Boşta ön yükleme (Bul, değiştirici) | — | 11 KB |
| Kabuk tamamı (`js/` + `css/`) | 123 KB | 136 KB |
| Sihirbaz (`setup/`, yalnız ilk açılış) | — | 30 KB |
| Uygulamalar (`apps/`, kendi iframe'lerinde) | 31 KB | 31 KB |
