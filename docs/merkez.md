# Pardus Maker Merkezi

Merkez, Pi'nin tüm ayarlarını ve geliştirme araçlarını tek bir web
uygulamasında toplar. Backend yalnızca Python 3.9 standart kütüphanesiyle
yazılmıştır (`/usr/lib/pios/center/`), arayüz derleme adımı olmayan bir
JavaScript modülüdür (`web/`). Dış CDN kullanılmaz; Pi internetsizken de çalışır.

## Erişim

| Nereden | Nasıl |
| --- | --- |
| Pi'nin masaüstü | Dock'taki Ayarlar, Python, Agent'lar, Kamera, GPIO simgeleri (`pios-open <bölüm>`, Chromium uygulama penceresi, parola sorulmaz) |
| Aynı ağdaki telefon/bilgisayar | `http://pardus-maker.local/` veya `http://<IP>/`, kullanıcı `pardus` |
| Erişim noktası | `Pardus-Maker-XXXX` ağına bağlanıp `http://10.42.0.1/` |

Ağdan ilk girişte uzak masaüstü açılır; üstteki **‹ Merkez** düğmesi ayarlara
geçer. Telefonda menü sol üstteki ☰ düğmesindedir.

## Bölümler

| Grup | Bölüm | Neler yapılır |
| --- | --- | --- |
| Ayarlar | Genel | Sıcaklık, bellek, disk, IP, çekirdek, sürüm; masaüstünü/Pi'yi yeniden başlat, kapat |
| | Wi-Fi ve Ağ | İnternet/DNS durumu, Ethernet (kablo, hız, IP, ağ geçidi), Wi-Fi tarama ve bağlanma, gizli ağ, kurumsal ağ (PEAP/MSCHAPv2), kayıtlı ağları unutma, erişim noktası |
| | Görünüm | Pardus 100. Yıl teması, açık/koyu |
| | Dil ve Klavye | Sistem dili (gerekirse `locale-gen` ile üretilir), klavye düzeni (Türkçe Q/F, İngilizce...), Alt+Shift ile ikinci düzen |
| | Tarih ve Saat | Canlı saat, saat dilimi, NTP aç/kapat, şimdi eşitle, tarayıcının saatini kullan, elle ayarla |
| | Ekran ve Donanım | HDMI 0/1 ve DSI durumu, monitör adı (EDID), çözünürlük (otomatik dahil), ses aygıtları, I2C/SPI/UART |
| | Uzak Erişim | Uzak ekranı aç/kapat, adresler (web, VNC, SSH) |
| | Kullanıcı | Parola (giriş, SSH, sudo, web, VNC birlikte), cihaz adı |
| | Güncellemeler | `apt` ile denetle ve güncelle (canlı günlük) |
| Geliştirme | Python | uv ile sürüm kur/kaldır/varsayılan yap, hazır AI ortamı (Terminal, Jupyter Lab) |
| | Projeler | Şablondan proje (boş, GPIO, yapay zeka, Flask, 7 MediaPipe örneği), VS Code/Terminal'de aç, paket ekle, sil |
| | Agent'lar | Claude Code, Codex CLI, Gemini CLI, OpenCode, Antigravity, Ollama: kur, güncelle, kaldır, başlat |
| | Terminal | Tarayıcıda kabuk (xterm.js); telefon için Esc/Tab/Ctrl/ok tuşları; sayfalar arası geçişte oturum korunur |
| Donanım | Kamera | Pi kamera ve USB kamera, canlı görüntü, fotoğraf, galeri, çözünürlük, 180° döndürme, kamera modeli |
| | USB Aygıtları | Bellek/disk bağla, güvenle çıkar, seri portlar, tüm USB aygıtları |
| | GPIO Pinleri | 40 pinli başlık şeması, çıkış/giriş, I2C tarama |
| Uzaktan | Ekran | Tarayıcıda masaüstü (noVNC); telefonda yatay tam ekran, ekran klavyesi |

## API

Tüm uç noktalar kimlik doğrulaması ister. `GET` istekleri JSON döndürür;
durum değiştiren istekler `POST`, `Content-Type: application/json` ve
`X-Pios: 1` başlığıyla gönderilir. Uzun işler (`job`) `/api/jobs/<id>?since=<n>`
ile izlenir.

```bash
curl -u pardus:PAROLA http://pardus-maker.local/api/status
curl -u pardus:PAROLA -H 'X-Pios: 1' -H 'Content-Type: application/json' \
     -d '{"timezone":"Europe/Istanbul"}' http://pardus-maker.local/api/time/timezone
```

| Uç nokta | Yöntem | Açıklama |
| --- | --- | --- |
| `/api/status` | GET | Cihaz durumu (model, sıcaklık, bellek, disk, IP, bağlantılar) |
| `/api/settings` | GET | Tema, donanım arayüzleri, çözünürlük, uzak ekran durumu |
| `/api/settings/theme`, `/api/settings/hostname`, `/api/settings/password`, `/api/settings/resolution`, `/api/settings/interface`, `/api/settings/remote` | POST | Ayarlar |
| `/api/network` | GET | Ethernet, kayıtlı ağlar, internet/DNS, NetworkManager durumu |
| `/api/wifi` | GET | Wi-Fi tarama sonuçları |
| `/api/wifi/connect` | POST | `{ssid, password, hidden, identity}` (identity: kurumsal ağ kullanıcı adı) |
| `/api/wifi/forget`, `/api/hotspot` | POST | Kayıtlı ağı unut, erişim noktasını aç/kapat |
| `/api/time` | GET | Saat, saat dilimi, NTP durumu, saat dilimleri |
| `/api/time/timezone`, `/api/time/ntp`, `/api/time/sync`, `/api/time/set` | POST | Saat ayarları (`set`: `{epoch}`) |
| `/api/language` | GET/POST | Dil durumu / `{locale}` |
| `/api/keyboard` | POST | `{keyboard: "tr:f", switch: true}` |
| `/api/displays` | GET | Ekran bağlayıcıları, etkin modlar, ses aygıtları |
| `/api/devices` | GET | USB aygıtları, depolama, seri portlar |
| `/api/devices/mount`, `/api/devices/eject`, `/api/devices/open` | POST | `{path}` |
| `/api/camera` | GET | Kameralar, ayarlar, hata, fotoğraf listesi |
| `/api/camera/stream?cam=<id>` | GET | MJPEG canlı görüntü (`multipart/x-mixed-replace`) |
| `/api/camera/photo/<ad>` | GET | Fotoğraf (`?thumb=1` küçük resim, `?download=1` indirme) |
| `/api/camera/photo`, `/api/camera/delete`, `/api/camera/settings`, `/api/camera/model` | POST | Fotoğraf çek, sil, ayarla, kamera modeli |
| `/api/terminal/ticket` | GET | Tek kullanımlık WebSocket bileti |
| `/api/terminal/ws?ticket=<bilet>` | WebSocket | İkili çerçeve: terminal verisi; metin çerçeve: `{"type":"resize","cols":..,"rows":..}` |
| `/api/python/versions`, `/api/python/projects`, `/api/python/ai` | GET | Python sürümleri, projeler ve şablonlar, AI ortamı |
| `/api/python/install`, `/api/python/uninstall`, `/api/python/default` | POST | `{minor}` |
| `/api/python/projects/create` | POST | `{name, template, python}` → job |
| `/api/python/projects/add`, `/api/devices/open`, `/api/camera/delete` | POST | Proje işlemleri |
| `/api/agents` | GET | Agent listesi |
| `/api/agents/action`, `/api/agents/launch` | POST | Kur/güncelle/kaldır, başlat |
| `/api/gpio`, `/api/gpio/i2c` | GET | Pin durumları, I2C tarama |
| `/api/gpio/set` | POST | `{bcm, mode: out\|in\|release, value, bias}` |
| `/api/updates/check`, `/api/updates/install` | POST | apt → job |
| `/api/power`, `/api/desktop/restart` | POST | `{action: reboot\|poweroff}`, masaüstünü yeniden başlat |
| `/api/vnc` | GET | Uzak ekran parolası ve portu |
| `/api/jobs/<id>` | GET | Arka plan işinin durumu ve çıktısı |

## Geliştirme

Merkez'i değiştirip denemek için kök dosya sistemini yeniden üretmek gerekmez:

```bash
cd image && ./test/run-test.sh                         # bir kez
docker cp overlay/usr/lib/pios/center/. pardus-maker-test:/usr/lib/pios/center/
docker exec pardus-maker-test sh -c 'pkill -f center/server.py'
docker exec -d pardus-maker-test /usr/bin/python3 -I /usr/lib/pios/center/server.py
```

Sistem servisleri `/usr/bin/python3` (3.9) ile çalışır; kullanıcı `PATH`'indeki
`python3` uv ile kurulan en güncel sürümdür ve `crypt` modülü içermez.
