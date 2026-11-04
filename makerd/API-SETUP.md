# M2 ekleri: makerd uygulamasından (master `docs/v2/api.md`'ye birleştirecek)

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

- `hostname` önerisi MAC adresinin son 4 hanesinden üretilir (`makeros-xxxx`).
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
