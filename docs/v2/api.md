# makerd API v1 — sözleşme (M1)

Bu belge `makerd` (Go) ile web kabuğu (`shell/`) arasındaki tek kaynaktır.
Değişiklikleri yalnızca master yapar.

## Genel

- Taban: `/api/v1`. JSON (UTF-8). Zaman: RFC 3339. Boyutlar: bayt (int).
- Kimlik: `POST /api/v1/auth/login` başarılıysa `mk_session` çerezi
  (`HttpOnly; SameSite=Strict; Path=/`, HTTPS'de `Secure`).
  Yerel kabuk: `GET /api/v1/auth/local?token=<run/makeros/token>` → çerez + `302 /`.
- CSRF: durum değiştiren tüm istekler (`POST/PUT/DELETE`) `X-Maker: 1` başlığı ister; yoksa `403`.
- Hata gövdesi: `{"error": {"code": "string_snake", "message": "insan okunur (TR)"}}`.
  Kodlar: `unauthorized` (401), `forbidden` (403), `not_found` (404),
  `invalid` (400), `conflict` (409), `internal` (500), `unavailable` (503).
- Uzun işler: `202 {"job": "<id>"}`; ilerleme `jobs` konusu ile events WS'den.
- Mock modu: `makerd --mock` gerçek sistem yerine sahte veri döner (önizleme/test).

## Uç noktalar

### Auth
| Yöntem | Yol | Gövde → Yanıt |
|---|---|---|
| POST | `/auth/login` | `{"user","password"}` → `{"user","admin":bool}` |
| POST | `/auth/logout` | → `204` |
| GET | `/session` | → `{"user","admin","setup_done":bool}` veya 401 |

### Sistem
| GET | `/system` | `{"hostname","os":{"name":"MakerOS","version"},"kernel","model","uptime_s","cpu":{"cores","freq_mhz","temp_c","load":[1,5,15]},"mem":{"total","used","available","zram_used"},"disk":{"total","used"},"throttled":bool}` |
|---|---|---|
| PUT | `/system/hostname` | `{"hostname"}` → `204` |
| GET/PUT | `/system/time` | `{"now","timezone","ntp":bool}` |
| GET | `/system/timezones` | `["Europe/Istanbul", ...]` |
| GET/PUT | `/system/locale` | `{"lang":"tr_TR.UTF-8","keymap":"tr"}` |
| PUT | `/system/password` | `{"old","new"}` → `204` |
| POST | `/system/power` | `{"action":"reboot"|"poweroff"}` → `202` |

### Ağ
| GET | `/net` | `{"online":bool,"ifaces":[{"name","type":"ethernet"|"wifi","state","ipv4":[],"mac","speed_mbps"?,"ssid"?}],"hotspot":{"active":bool,"ssid"}}` |
|---|---|---|
| POST | `/net/wifi/scan` | → `[{"ssid","signal":0-100,"security":"open"|"wpa2"|"wpa3"|"enterprise","known":bool,"active":bool}]` |
| POST | `/net/wifi/connect` | `{"ssid","password"?,"hidden"?:bool,"eap"?:{"identity","password","method"}}` → `202 {"job"}` |
| POST | `/net/wifi/forget` | `{"ssid"}` → `204` |

### Uygulama kaydı (kabuk için)
| GET | `/apps` | `[{"id":"makeros.settings","name":"Ayarlar","icon":"/shell/icons/settings.svg","kind":"web"|"native","url"?:"/shell/apps/settings/","exec"?:"foot","categories":[]}]` |
|---|---|---|
| POST | `/apps/{id}/launch` | → `202` (native) ; web uygulamalarını kabuk kendisi açar |

### Etkinlik
| GET | `/processes` | `[{"pid","name","user","cpu_pct","rss","slice"}]` (cpu'ya göre sıralı, ilk 100) |
|---|---|---|
| POST | `/processes/{pid}/signal` | `{"signal":"TERM"|"KILL"}` → `204` (yalnız kendi süreçleri) |

### Bildirimler
| GET | `/notifications` | `[{"id","app","title","body","time","read":bool}]` |
|---|---|---|
| POST | `/notifications/{id}/read` | → `204` |
| DELETE | `/notifications` | Tümünü temizler → `204`; `notifications` konusuna `{"cleared":true}` yayımlanır |

### Terminal
| POST | `/term` | `{"cols","rows"}` → `{"id","ticket"}` (bilet 30 sn, tek kullanım) |
|---|---|---|
| WS | `/term/{id}?ticket=` | İkili çerçeve = PTY verisi (iki yön). Metin çerçeve = kontrol JSON: `{"type":"resize","cols","rows"}`. `Origin` == `Host` zorunlu. Oturum WS kopunca 5 dk yaşar (256 KB ring buffer, yeniden bağlanınca tekrar gönderilir) |

### Olaylar
`WS /events` — metin çerçeve JSON.
- İstemci → `{"op":"sub","topics":["system","net","jobs","notifications","processes"]}` / `{"op":"unsub",...}`
- Sunucu → `{"topic","data"}`:
  - `system`: `/system` gövdesi, 2 sn'de bir (yalnız aboneyken)
  - `processes`: `/processes` gövdesi, 2 sn'de bir
  - `net`: `/net` gövdesi, değişince
  - `jobs`: `{"id","state":"running"|"done"|"failed","progress":0-100,"log"?:"satır"}`
  - `notifications`: tek bildirim nesnesi

## Statik sunum
- `/` → `shell/index.html` (kabuk). `/shell/*` → `shell/` dizini.
- Kabuk kök dizini: cihazda `/usr/share/makeros/shell`, geliştirmede `--web ./shell`.
- Önbellek: `/shell/vendor/*` ve `*.woff2` için `Cache-Control: max-age=31536000, immutable`; diğerleri `no-cache` + ETag. gzip/br ön-sıkıştırılmış dosya varsa onu sunar.

## M1 revizyonu (makerd uygulamasından kabul edilen ekler)

- Hata kodu eklendi: `rate_limited` (429, `Retry-After` başlığı) — giriş denemesi sınırı 5/dk/IP.
- `POST /term/{id}/ticket` → `{"id","ticket"}`: kopan terminal oturumuna yeniden bağlanma bileti.
- `GET /jobs/{id}` → jobs olay gövdesiyle aynı (WS koptuysa durum sorgusu).
- `PUT /system/time` ve `PUT /system/locale` → `200` + güncel durum (204 değil). `PUT /system/time` kısmi: `{timezone?, ntp?, now?}`.
- `POST /apps/{id}/launch` web uygulaması için `400 invalid`; terminal oturum sınırı (6) aşılınca `POST /term` → `409 conflict`.
- WS yolları tam: `/api/v1/events`, `/api/v1/term/{id}`; ikisinde de `Origin` başlığı `Host` ile eşleşmeli.
- Yönetici (`admin: true`) gerektirenler: hostname, saat, dil/klavye, güç, Wi-Fi tara/bağlan/unut.
- `setup_done`: gerçek modda `<data-dir>/setup-done` dosyası var mı. İlk açılış sihirbazı tamamlanınca oluşturur.
- Hotspot NM profil adı sabit: `makeros-hotspot` (SSID `MakerOS`, 10.42.0.1).

## M2 için kabul edilen ekler (kabuktan)

- `/system` → `cpu.usage_pct` (anlık toplam CPU %, örnekleme farkıyla).
- Native uygulama simgesi yoksa `icon` = `/shell/icons/app.svg`.
- `/apps[].url` hash içerebilir (ör. `/shell/apps/settings/#wifi`).
- `PUT /net/wifi {"enabled": bool}` — Wi-Fi radyosu aç/kapat (admin).
- `GET/PUT /prefs` — kullanıcı başına kabuk tercihleri (tema, efekt, vurgu, dock) `{...}` serbest JSON ≤ 16 KB; localStorage yalnız önbellek.

## İlk açılış kurulum API (M2)


## İlk açılış kurulumu (`setup_done=false`)

Kurulum bitmemişken hiçbir hesapla giriş yapılamaz. Kimlik doğrulama gerektirmeyen
uçlar yalnızca şunlardır. Kurulum bittikten sonra hepsi `409 conflict` döner.
İstemci adresi loopback, özel (RFC 1918, ULA `fc00::/7`) veya bağlantı-yerel
(169.254/16, fe80::/10) değilse `403 forbidden` döner. CSRF kuralı (`X-Maker: 1`)
POST için geçerlidir.

| Yöntem | Yol | Gövde → Yanıt |
|---|---|---|
| GET | `/setup` | → `{"setup_done":false,"hostname":"makeros-abcd","defaults":{"lang","keymap","timezone"},"languages":[{"id":"tr_TR.UTF-8","name":"Türkçe"}],"keymaps":[{"id":"trq","name":"Türkçe Q"}],"timezones":["Europe/Istanbul"],"online":bool,"wifi_needed":bool,"hotspot":bool,"min_password":6}` |
| POST | `/setup/wifi/scan` | → `/net/wifi/scan` ile aynı (kurulum ekranı için kimliksiz) |
| POST | `/setup/complete` | `{"user","password","fullname"?,"hostname","timezone","lang","keymap","wifi"?:{"ssid","password"?,"hidden"?,"eap"?}}` → `200 {"user","admin":true,"job"?,"warnings"?:["..."]}` + `mk_session` çerezi |

- `hostname` önerisi her zaman `maker-os` (web arayüzü `http://maker-os.local/`).
  `wifi_needed` alanı, çevrimdışıyken veya kurulum erişim noktası açıkken `true` olur.
- Doğrulama kuralları şöyledir. `user`: `^[a-z_][a-z0-9_-]{0,31}$` olmalı ve ayrılmış
  adlardan (root, daemon, nobody, systemd-* …) biri olmamalıdır. `password`: en az
  6 karakter olmalıdır. `fullname`: en fazla 64 karakter olabilir; `: , =` ve satır
  sonu içeremez. `hostname`, `timezone`, `lang` ve `keymap` için `/system/*` uçlarıyla
  aynı kurallar geçerlidir. Kural dışı değerler `400 invalid` döner.
- Uç tek seferliktir ve yarışa karşı güvenlidir. `<data-dir>/setup.lock` üzerinde
  `flock` ve süreç içi kilit tutulur. Aynı anda gelen ikinci istek `409` alır.
- İşlem sırası:
  1. makerd-priv `user.create` ile hesap oluşturulur. Hesap şu gruplara eklenir:
     wheel, video, gpio, i2c, spi, dialout, input, render, audio, plugdev (sistemde
     olmayan gruplar atlanır). Parola SHA-512 ile özetlenir. Ardından
     `makerd-user@<user>` etkinleştirilir.
  2. hostname, saat dilimi, dil ve klavye ayarlanır. Bunlardan biri başarısız olursa
     kurulum yine tamamlanır ve hata `warnings` içinde döner.
  3. `<data-dir>/setup-done` dosyası yazılır (`{"user","time"}`).
  4. Oturum açılır.
  5. `wifi` verildiyse bağlantı **yanıttan sonra** iş (`job`) olarak başlar.
     Bağlanma, kurulum erişim noktasını kapatabilir. İstemci bağlantının kopmasına
     hazır olmalı ve ardından cihaza yeni ağdan (`<hostname>.local`) ulaşmalıdır.
- Hesap oluşturma başarısız olursa `setup-done` yazılmaz ve istek yeniden denenebilir.
  Aynı ada sahip ve UID'si ≥ 1000 olan var olan hesap devralınır; parolası ve
  grupları güncellenir.
- Kurulumdan önce `/auth/local` oturum açmaz, yalnızca `302 /` döner. Kurulumdan
  sonra yerel kabuk, kurulumda oluşturulan kullanıcıyla açılır.
- Mock: `MAKERD_MOCK_FRESH=1 makerd --mock` ile kurulum yapılmamış durumda başlar
  (hiç kullanıcı yoktur). Bu değişken yoksa durum `maker`/`maker` ile kurulmuş sayılır.

## M2 için kabul edilen eklerin uygulanan biçimi

- `/system` → `cpu.usage_pct` (float, bir ondalık). Değer iki `/proc/stat` örneği
  arasındaki farktan hesaplanır. İlk çağrıda iki örnek arasında 100 ms beklenir.
- `/net` yanıtına `"wifi_enabled": bool` alanı eklendi (NM `WirelessEnabled`).
- `PUT /net/wifi {"enabled": bool}` (admin) → `200` + güncel `/net` gövdesi. Aynı
  gövde `net` konusunda da yayımlanır. `enabled` yoksa `400` döner. Radyo kapalıyken
  Wi-Fi arayüzünün `state` alanı `unavailable` olur. `connect` isteği radyoyu yeniden
  açar.
- `GET /prefs` → kayıtlı JSON nesnesi döner; kayıt yoksa `{}` döner.
  `PUT /prefs {…}` → `200` + sıkıştırılmış hâliyle kayıtlı nesne döner. Gövde JSON
  nesnesi değilse `400`, 16 KB'tan büyükse `413` döner. Kayıt
  `<data-dir>/prefs/<user>.json` dosyasına atomik olarak yazılır.
- Yerel uygulamanın simgesi yoksa `"/shell/icons/app.svg"` kullanılır. `url` alanı
  olduğu gibi döner (`#hash` korunur).

## Terminal (iç yapı, API değişmedi)

`POST /term` ve `WS /term/{id}` aynı kaldı. Gerçek modda PTY, kullanıcının
`makerd-user@<user>.service` hizmetinde açılır (`/run/makeros/user/<uid>.sock`).
Bu sayede terminalde `sudo` çalışır. Hizmet çalışmıyorsa `POST /term` →
`503 unavailable` döner. Tek istisna makerd'nin kendi kullanıcısıdır: onun için
süreç içinde bir PTY açılır.

## M3 için bekleyen öneriler

- Kurulum bekliyorken `/session` → 401 `error.code: "setup_required"` (kabuk `/setup` yoklamasın).
- `GET /setup` önbellekteki `wifi_networks` listesini de dönsün.
- Uygulama↔kabuk köprüsü mesajı `{mk:"visibility",visible}` belgelensin (shell/README.md).
- SSH aç/kapat: makerd-priv `ssh.enable` komutu + `PUT /system/ssh {enabled}` (admin).

## Uzak tarayıcı (2026-11-08)

Uzak masaüstü (VNC: wayvnc, `/vnc`, `/screen`, Ekran Modu) kaldırıldı. Web arayüzü bağlanan bilgisayarın
tarayıcısında çizilir; Pi'nin ekranı aktarılmaz. Uzaktan "Chromium" tıklanınca kabuk `/shell/apps/browser/`
(Tarayıcı, iframe) penceresini açar; Pi'de bir şey başlamaz. Pi'nin kendi ekranında Chromium yerel uygulamadır.

## Eklenti mağazası (2026-11-05)

Kaynak: [`ferhatozcelik/makeros-plugins`](https://github.com/ferhatozcelik/makeros-plugins) deposunun `plugins/<id>/`
klasörleri; derleme/yayın adımı yoktur. makerd `--plugin-catalog sahip/depo[@dal]` (varsayılan `ferhatozcelik/makeros-plugins@main`;
boş: kapalı → 503). Liste: dalın commit'i (`/repos/…/branches/<dal>`) + o commit'in ağacı (`git/trees/<sha>?recursive=1`),
her eklentinin `plugin.json`'u ve simgesi raw.githubusercontent.com'dan **commit adresiyle** okunur (dal önbelleği sorun olmaz).
Her dosya ağaçtaki git blob özetiyle (SHA-1) denetlenir. 10 dk önbellek.

- `GET /plugins/catalog[?refresh=1]` → `{source, repo_url, plugins:[{id,name,version,description,author?,icon,tags[],size,url,installed,update}]}`.
  `icon` data: URI (svg/png/webp, ≤ 32 KB) ya da boş; `url` GitHub'daki klasör; `installed` kurulu sürüm ya da `null`;
  `update` depodaki sürüm daha yeniyse `true`. Geçersiz klasörler (id ≠ klasör adı, eksik `pyproject.toml`/başlangıç dosyası) listelenmez.
  Ulaşılamazsa veya GitHub istek sınırı dolarsa `503 unavailable`.
- `POST /plugins/catalog/{id}/install` → kurulan `plugin.json`. Ağaç her seferinde yenilenir; klasördeki dosyalar (gizli, `.venv`,
  `__pycache__` hariç) indirilir, boyut ve git özeti denetlenir, `/plugins/import` kurallarıyla kurulur. Çalışan eklenti yeniden başlatılır.
  Hatalar: `404` katalogda yok, `400` dosya uyuşmazlığı/geçersiz eklenti/boyut, `503` indirme hatası veya zaman aşımı (180 sn).

## Dosyalar ve Python yöneticisi (2026-11-08)

`POST /files/op {op, path, to}` işlemleri: `mkdir`, `rename`, `remove`, `stat`, `copy` (hedef klasör; çakışmada
"ad (2)"), `move`, `trash` / `trash-list` / `restore` (path = çöp kimliği) / `trash-delete` (path boş = çöpü boşalt;
freedesktop `~/.local/share/Trash`), `search` (path = klasör, to = sorgu; ≤300 sonuç, 8 düzey), `places`
(standart klasörler + `/media`, `/run/media/<kullanıcı>`, `/mnt` altındaki sürücüler).

| Yöntem | Yol | Açıklama |
|---|---|---|
| GET | `/projects/{ad}/status` | `{running, autostart, python, template}` |
| POST | `/projects/{ad}/start` · `/stop` | Arka planda (setsid) çalıştır / süreç grubunu durdur; çıktı `.makeros-run.log` |
| GET | `/projects/{ad}/log?from=N` | Günlüğün N. bayttan sonrası (≤64 KB): `{size, from, text, reset}` |
| GET | `/projects/{ad}/packages` | `{deps: [pyproject], installed: [uv pip list]}` |
| POST | `/projects/{ad}/packages` | `{op: add\|remove\|upgrade\|sync, pkg}` → terminal oturumu (yalnız uv) |
| PUT | `/projects/{ad}/python` | `{python: "system"\|"3.12"}` → ortam yeniden kurulur (terminal) |
| PUT | `/projects/{ad}/autostart` | `{enabled}`; işaret `~/.config/makeros/autostart/<ad>`, makerd açılışta birincil kullanıcının işaretli projelerini başlatır |
| GET/PUT/DELETE | `/wallpaper/{desktop\|desktop-blur\|lock}` | Kullanıcı duvar kâğıdı (GET herkese açık: kilit ekranı girişten önce görünür) |
