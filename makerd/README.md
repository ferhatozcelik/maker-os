# makerd — MakerOS 2 arka ucu

`makerd`, MakerOS 2'nin Go ile yazılmış arka uç hizmetidir. REST `/api/v1`,
olay WebSocket'i, web terminali ve web kabuğunun statik sunumu bu hizmettedir.
Sözleşme: [`docs/v2/api.md`](../docs/v2/api.md).

- `cmd/makerd`: yetkisiz çalışır (`User=maker`).
- `cmd/makerd-priv`: kök yetkili, küçük bir yardımcıdır. Soketle etkinleşir,
  sabit bir komut listesini kabul eder ve boşta kalınca kapanır.
- `cmd/makerd-user`: kullanıcı başına çalışan terminal hizmetidir
  (`makerd-user@<kullanıcı>.service`, `User=%i`). Web terminalinin PTY'leri
  burada açılır. makerd yalnızca vekillik eder.

M2 sözleşme ekleri ve ilk açılış kurulumu API'si
[`API-SETUP.md`](API-SETUP.md) dosyasındadır. Master bu dosyayı `api.md` ile
birleştirecek.

İkisi de `CGO_ENABLED=0` ile derlenen statik ikili dosyalardır. Bağımlılıklar:
`coder/websocket`, `creack/pty`, `godbus/dbus/v5`. Gerisi standart kütüphanedir.

## Derleme ve test (yalnızca Docker)

Host'a Go kurmanız gerekmez. Tüm hedefler `golang:1.26.10-alpine` içinde çalışır.
Modül önbelleği `makerd-gomod`, derleme önbelleği `makerd-gocache` adlı Docker
birimlerinde tutulur.

```sh
make test          # go vet + go test ./...
make build-arm64   # bin/linux-arm64/{makerd,makerd-priv,makerd-user}
make build-amd64   # bin/linux-amd64/...
make image         # makeros/makerd:dev (mock önizleme imajı)
```

Önizleme için depo kökünde `docker compose up` (ya da `docker-compose up`)
çalıştırın. Ardından http://localhost/ adresini açın ve `maker` / `maker`
ile giriş yapın. `shell/` dizini `/web` olarak salt okunur bağlanır.

İlk açılış sihirbazını denemek için önizlemeyi kurulum yapılmamış durumda başlatın:

```sh
docker-compose run --rm -e MAKERD_MOCK_FRESH=1 -p 80:8080 makerd
```

Bu modda hiç kullanıcı yoktur. Hesap `POST /api/v1/setup/complete` ile oluşturulur.

## Bayraklar

| Bayrak | Varsayılan | Açıklama |
|---|---|---|
| `--listen` | `:80` | Dinlenecek adresler (virgülle ayrılır). systemd soket devri varsa bu bayrak yok sayılır. |
| `--web` | `/usr/share/makeros/shell` | Kabuk dizini |
| `--mock` | kapalı | Sahte arka uçlar kullanılır: kök, NetworkManager ve D-Bus gerekmez. |
| `--data-dir` | `/var/lib/makerd` | Kalıcı veriler. Mock modda geçici bir dizin kullanılır. |
| `--token-file` | `/run/makeros/token` | Yerel kabuk belirteci |
| `--priv-socket` | `/run/makeros/priv.sock` | makerd-priv soketi |
| `--user-socket-dir` | `/run/makeros/user` | makerd-user soketleri (`<uid>.sock`) |
| `--log-level`, `--log-format` | `info`, `text` | `log/slog` günlük ayarları |

Önerilen ortam değişkenleri: `GOMEMLIMIT=48MiB GOGC=50 GOMAXPROCS=2`.
Mock modda `MAKERD_MOCK_FRESH=1`, kurulum yapılmamış durumla başlatır.
Hazır olunca systemd'ye `READY=1` bildirilir (`Type=notify`).

## Modüller

| Paket | Görev |
|---|---|
| `internal/httpd` | Yönlendirici, CSRF denetimi (`X-Maker: 1`), kimlik ara katmanı, hata gövdesi, statik sunum (ETag, `immutable`, `.br`/`.gz`) |
| `internal/auth` | Bellekte tutulan oturumlar. Çerez HMAC ile imzalanır ve 7 gün geçerlidir. Giriş hız sınırı: IP başına dakikada 5 başarısız deneme. Ayrıca yerel belirteç ve yönetici grubu denetimi. |
| `internal/events` | Konu abonelikli dağıtıcı. Üreticiler yalnızca abone varken çalışır. |
| `internal/jobs`, `internal/notify` | Bellekte tutulan işler ve bildirimler. Her güncelleme events'e yayımlanır. |
| `internal/system` | `/proc`, `/sys`, statfs ve `get_throttled` okumaları. Ayarlar hostnamed, timedated, localed ve logind üzerinden D-Bus ile yapılır. |
| `internal/network` | NetworkManager D-Bus API'si. NM yoksa durum `/sys` üzerinden okunur. |
| `internal/apps` | Yerleşik web uygulamaları ve `.desktop` dosyalarından okunan yerel uygulamalar. Yerel uygulamalar `systemd-run --user --slice=apps.slice` ile başlatılır. |
| `internal/activity` | `/proc` ayrıştırma. CPU % iki ölçüm arasındaki farktan hesaplanır, cgroup dilimi de gösterilir. |
| `internal/term` | Terminal oturumları: bilet, `Origin` denetimi, 256 KB halka tampon, 5 dk bekleme, en fazla 6 oturum. Süreç yerelde (`Local`) ya da makerd-user üzerinden (`Remote`) açılır. |
| `internal/termproc` | PTY süreç arayüzü, makerd ↔ makerd-user çerçeveli protokolü (istemci ve sunucu) ve giriş kabuğu |
| `internal/setup` | İlk açılış durumu (`setup-done`), tek seferlik kilit (`flock`), yerel adres denetimi |
| `internal/prefs` | Kullanıcı başına kabuk tercihleri (`<data-dir>/prefs/<kullanıcı>.json`, en fazla 16 KB, atomik yazım) |
| `internal/ucred` | `SO_PEERCRED` okuma |
| `internal/priv` | makerd-priv protokolü, SO_PEERCRED denetimi, SHA-512 crypt ve shadow dosyası yazımı. |
| `internal/mock` | `--mock` için sahte veri arka uçları |

## makerd-priv protokolü

Protokol, `/run/makeros/priv.sock` üzerinde çalışır. Her bağlantıda bir satır
JSON istek gönderilir ve bir satır JSON yanıt alınır:

```json
{"cmd":"auth.check","args":{"user":"maker","password":"..."}}
{"ok":true,"data":{"ok":true}}
```

Komutlar: `auth.check`, `system.password` (yalnızca çağıranın kendi parolası),
`system.hostname`, `power`, `user.create`.

`user.create` ilk açılışta yönetici hesabını oluşturur. `--setup-flag` ile
verilen dosya (`/var/lib/makerd/setup-done`) varsa komut reddedilir. Böylece
ele geçirilmiş bir makerd bile kurulumdan sonra yeni bir wheel kullanıcısı
ekleyemez. Komut `useradd` gerektirmez. passwd, group, gshadow ve shadow
dosyaları Go ile ve atomik olarak yazılır (`/etc/.pwd.lock` fcntl kilidiyle).
Hesap sırasıyla şunları alır:

- UID ≥ 1000 ve aynı adla birincil grup,
- ek gruplar: wheel, video, gpio, i2c, spi, dialout, input, render, audio,
  plugdev (sistemde olmayan gruplar atlanır),
- `/etc/skel` kopyasıyla oluşturulan bir ev dizini,
- `systemctl enable --now makerd-user@<ad>.service`.

UID'si ≥ 1000 olan aynı adlı bir hesap varsa (ör. imajdaki kilitli `maker`),
o hesap devralınır. Çağıran SO_PEERCRED ile denetlenir: yalnızca
`--allow-user` ile izin verilen kullanıcılar ve root kabul edilir. Her çağrı
günlüğe yazılır, ama argümanlar yazılmaz.

**Parola özeti:** Saf Go ile yalnızca SHA-512 crypt (`$6$`) uygulanmıştır.
yescrypt (`$y$`) desteklenmez. Bu yüzden imajda `/etc/login.defs` dosyasında
`ENCRYPT_METHOD SHA512` ayarlı olmalıdır (bkz. `dist/makeros.login.defs.snippet`).
Yeni parolalar `$6$rounds=65536$` ile yazılır.

## Örnek systemd birimleri

`dist/` dizinindeki birimler:

- `makerd.service`: sıkılaştırılmıştır (`User=maker`, `CAP_NET_BIND_SERVICE`).
  M2'de `NoNewPrivileges`, `MemoryDenyWriteExecute`, `RestrictNamespaces` ve
  `ProtectHome=read-only` yeniden açıldı.
- `makerd-priv.socket` ve `makerd-priv.service`: soketle etkinleşir. `user.create`
  için `ReadWritePaths=/etc /home` gerekir.
- `makerd-user@.service`: kullanıcının terminal hizmetidir. Kasıtlı olarak
  kum havuzu yoktur, böylece `sudo` çalışır.
- `makeros.tmpfiles.conf`: iki dizin oluşturur:
  - `/run/makeros` (0751, `maker`),
  - `/run/makeros/user` (1733, root).

os/ tarafı bunları uyarlayabilir.

## Terminal: makerd ↔ makerd-user

makerd, oturum kullanıcısının `/run/makeros/user/<uid>.sock` soketine bağlanır.
Önce karşı ucun UID'sini `SO_PEERCRED` ile denetler, sonra her terminal için bir
bağlantı açar. Bağlantıdaki çerçeve biçimi şöyledir: 1 bayt tür, 4 bayt
big-endian uzunluk, ardından yük.

| Yön | Tür | Yük |
|---|---|---|
| makerd → makerd-user | `o` (aç) | `{cols, rows}` |
| makerd → makerd-user | `d` | girdi |
| makerd → makerd-user | `r` | `{cols, rows}` |
| makerd → makerd-user | `k` | SIGHUP |
| makerd-user → makerd | `p` | `{pid}` |
| makerd-user → makerd | `e` | hata metni |
| makerd-user → makerd | `d` | çıktı |
| makerd-user → makerd | `x` | `{code}` |

makerd-user da bağlanan tarafı `SO_PEERCRED` ile denetler. Yalnızca root,
`--allow-user` ile izin verilen kullanıcılar ve kendi kullanıcısı kabul edilir.
Halka tampon, bilet ve yeniden bağlanma makerd'de kalır. Soket yoksa oturum
yalnızca makerd'nin kendi kullanıcısı için süreç içinde açılır. Diğer
kullanıcılar `503` alır. Mock modda her oturum süreç içinde açılır.

## Ağ geri dönüşü

Wi-Fi bağlantısı başarısız olursa v1'deki davranış korunur. Önce önceki
bağlantı geri açılır. O da olmazsa `makeros-hotspot` profili açılır
(SSID `MakerOS`, `10.42.0.1/24`, `ipv4.method=shared`). Yeni bir parola
denenirken eski profil silinmez; yalnızca bağlantı başarılı olursa silinir.
