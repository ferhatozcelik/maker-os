# MakerOS 2.1: istekler, kararlar, ilerleme

Kullanıcı istekleri (2026-11-03) tek yerde. Her madde Docker içinde test edilmeden "bitti" sayılmaz.
Gerçek Raspberry Pi ölçümü gerektiren maddeler ayrıca işaretlidir (🍓). Bu ölçümleri yalnız Pi üzerinde
`mos bench` alabilir; README'ye uydurma sayı yazılmaz.

## İlkeler

- Tamamen Raspberry Pi'ye (Pi 4 / Pi 400 / Pi 5, 4 GB hedef) yönelik; geliştirici makinesinin (Mac, M serisi)
  hiçbir izi arayüzde görünmez.
- Arayüzde "Yakında" yok: dock'taki her uygulama çalışır (mock modda sahte veriyle, Pi'de gerçek donanımla).
- Her işlev hem web arayüzünden hem arayüzsüz `mos` komutundan kullanılabilir.
- "Raspberry Pi" yalnızca uyumluluğu belirtmek için kullanılır (marka beyanı NOTICE'ta).
- Tüm derleme ve test Docker içinde.

## Aşamalar

| # | Konu | Kapsam | Durum |
|---|---|---|---|
| F1 | Görsel kimlik | Maker logosu (grafit, devre izi "M"); 6 tema: **Maker** (grafit, varsayılan), **Hackerspace** (retro terminal), **Donanım** (PCB), **Siber Güvenlik**, **3D Baskı**, **Akış** (eski renkli); tema + duvar kâğıdı seçimi; arayüzden aşama kodları (M3/M4) ve "Yakında" kaldırılır | ⏳ |
| F2 | Uygulamalar | Dosyalar, Python (uv sürümleri, projeler, şablonlar), AI Agent'lar (Claude Code, Codex, Gemini CLI, OpenCode, Ollama), Kamera, GPIO, **Eklentiler** (Mağaza yerine: şablon, zip içe/dışa aktar), Uzak Ekran | ⏳ |
| F3 | Ortak terminal | makerd terminal oturumları birden çok istemciye açık: web'de ve Pi ekranında açılan terminal aynı oturum, eşzamanlı; oturum listesi, yeniden bağlanma | ⏳ |
| F4 | Web ↔ Ekran geçişi | Dock'ta küçük düğme: sayfayı tamamen VNC ekranına (Pi'nin yerel ekranı) çevirir, tekrar basınca web arayüzüne döner | ⏳ |
| F5 | Raspberry Pi ekosistemi | Pi 5 (BCM2712 DTB, `[pi5]`, RP1), Pi kamera (libcamera vc4 + **pisp**/libpisp, rpicam-apps), GPIO (libgpiod v2, gpiozero, lgpio, smbus2, spidev, pyserial), kamera takılınca picamera2 yığınını arka planda kuran hizmet | ⏳ |
| F6 | AI: nesne tespiti | Pi 4 CPU'da 30 FPS hedefi: NanoDet-m 320 (Apache-2.0) + ncnn (BSD-3), picamera2 düşük çözünürlüklü akış, iş parçacığı ayrımı; örnek proje + `mos bench ai` | ⏳ 🍓 |
| F7 | Sistem optimizasyonu | Çekirdek (PREEMPT, HZ), cgroup ağırlıkları (kabuk korunur, AI çekirdek 1–3), zram/MGLRU/swappiness, CPU yöneticisi, SD I/O, WPE/V3D ayarları; arayüz AI yükü altında da akıcı | ⏳ 🍓 |
| F9 | Chromium | Chromium tarayıcı hazır kurulu gelir (kullanıcı isteği 2026-11-04). Buildroot Chromium paketlemiyor; yöntem (sistem uzantısı imajı veya kutu) bu aşamada seçilecek | ⏳ |
| F10 | Güncelleme (OTA) | GitHub Releases'taki son sürüm: makerd sürüm denetimi, imzalı RAUC paketi indir → boştaki slota yaz → tryboot ile dene → başarısızsa eski slota dön; Ayarlar → Güncelleme ve `mos update` (kullanıcı isteği 2026-11-04) | ⏳ |
| F8 | Belgeler | README (TR/EN) yeniden, CHANGELOG, NOTICE/lisans (Raspberry Pi marka beyanı), ekran görüntüleri, `mos bench` sonuç şablonu | ✅ |

## Kararlar

| Konu | Karar | Gerekçe |
|---|---|---|
| Nesne tespiti modeli | **NanoDet-m 320** (COCO 80 sınıf, Apache-2.0); 30 FPS için gerekirse 256 girdi; sensör üstü 30 FPS için Raspberry Pi AI Camera (IMX500) desteği | Yolo-FastestV2 daha hızlı (Pi 4: 38 FPS) ama **lisanssız**, dağıtılamaz. NanoDet 320 Pi 4: 26,2 FPS (1950 MHz, yalnız çıkarım). Kamera akışı 30 FPS, tespit ayrı iş parçacığında; çekirdek 1–3 |
| Çıkarım motoru | ncnn (PyPI `ncnn`, cp314 aarch64 tekerleği var) | BSD-3; ARM NEON için optimize; onnxruntime'dan ~2 kat hızlı (ARM CPU) |
| Pi 5 çekirdeği | Ortak `kernel8.img` (bcm2711_defconfig; RP1, PiSP, CFE sürücüleri içinde) | Tek imaj Pi 4 ve 5'te açılır. 16K sayfalı `kernel_2712.img` ileride (ölçüm sonrası) |
| Pi 5 kamera | libpisp 1.3.0 paketi + libcamera `rpi/pisp` hattı | Buildroot'ta yok; `os/package/libpisp` ve `external.mk` ile eklenir |
| picamera2 | İmajda: libcamera Python, numpy, pillow, piexif, prctl, pidng; arka planda (PyPI tekerlek): picamera2, simplejpeg, av, videodev2 | Kök dosya sistemi salt okunur; tekerlekler `/usr/local` (yazılabilir katman) altına |
| Eklenti biçimi | `plugin.json` + `pyproject.toml` + `main.py`, isteğe bağlı `web_port`; zip ≤ 15 MB | Topluluk ekosistemi; uv ile yalıtılmış ortam |

## Üçüncü taraf referans ölçümleri (MakerOS üzerinde doğrulanmadı)

Kaynak: [Qengineering/YoloFastestV2-ncnn-Raspberry-Pi-4](https://github.com/Qengineering/YoloFastestV2-ncnn-Raspberry-Pi-4)
README tablosu, ncnn, "RPi 4 1950" sütunu (1950 MHz, yalnız çıkarım süresi). Sayfa sütunları açıklamıyor;
bu yüzden yalnız model seçimi için kullanılır, README kıyaslamasına girmez.

| Model | Girdi | mAP | Pi 4 (1950 MHz) FPS | Lisans |
|---|---|---|---|---|
| NanoDet | 320 | 20,6 | 26,2 | Apache-2.0 ✅ |
| PP-PicoDet | 320 | 27,0 | 24,0 | Apache-2.0 ✅ |
| YoloX nano | 416 | 25,8 | 22,6 | Apache-2.0 ✅ |
| NanoDet-Plus | 416 | 30,4 | 18,5 | Apache-2.0 ✅ |
| YoloFastestV2 | 352 | 24,1 | 38,4 | lisans yok ❌ |
| YOLOv8n | 640 | 37,3 | 14,5 | AGPL-3.0 (dikkat) |

## İlerleme günlüğü

- 2026-11-03: Plan yazıldı.
- 2026-11-03: F1 logo + 6 tema duvar kâğıdı + Ayarlar tema seçici (Docker'da ekran görüntüsüyle doğrulandı).
- 2026-11-03: Kullanıcı temsilcisi (makerd-user: exec + dosya işlemleri; mock'ta süreç içi) ve testleri.
- 2026-11-03: F3 ortak terminal: çoklu istemci, `GET /term` listesi, komutla oturum; iki istemcili test geçti.
- 2026-11-03: Python/proje/AI ajan API'leri; 8 şablon (gömülü). Nesne tespiti şablonu MakerOS kök FS'inde uçtan uca
  çalıştı (uv venv + ncnn, Python 3.14). Bulgu: ncnn iş parçacığı > sabitlenen çekirdek → 20 kat yavaşlama; şablon sınırlıyor.
- 2026-11-04: GPIO (gpiochip / benzetim), Kamera (rpicam MJPEG tek üretici çok izleyici / sentetik), Eklentiler (zip içe/dışa
  aktarma, şablon, yol aşımı ve boyut denetimleri, /plugin/<id>/ yönlendirmesi) arka uçları ve testleri.
- 2026-11-04: Uygulamalar: Dosyalar, Python (projeler/şablonlar/sürümler), AI Ajanları, Eklentiler, Kamera, GPIO; Terminal ortak oturumlar.
  "Yakında" metinleri ve aşama kodları arayüzden kaldırıldı; Mağaza kaldırıldı.
- 2026-11-04: Kullanıcı isteğiyle iki mod: **Web** (uzak tarayıcı varsayılanı; kenar çubuğu + uygulama sayfaları, masaüstü yok) ve
  **Ekran** (Pi'nin gerçek ekranı, tam sayfa VNC, kendiliğinden gizlenen "Web | Ekran" düğmesi). Pi'nin kendi ekranı makerd
  yerel girişiyle (`?ui=desktop&local=1`) masaüstünü açar. Akış teması kaldırıldı. Pencere içi tasarım düzenlendi.
- 2026-11-04: Kullanıcı kararı: yalnız iki adres. `/` = masaüstü (tarayıcıda doğrudan çizilir; VNC'den hızlı, giriş/kurulum
  burada), `/web` = Web arayüzü. `?full=1` Ekran modu ve `/ui/switch` köprüsü kaldırıldı; raftaki düğme `/web`'e, Web
  arayüzündeki "Ekran" düğmesi `/`'e gider. Uzak Ekran (VNC) uygulaması yerel pencereler için uygulama olarak kalır.
  Tüm sayfaların sekme simgesi `/shell/icons/logo.svg`; `/favicon.ico` oraya yönlenir.
- 2026-11-04: "Hackerspace" teması → **Konya HackerSpace** (kullanıcı isteği; KHS işareti ve renkleri konyahackerspace.org'dan,
  kullanıcının verdiği SVG). Tema açıkken: menü çubuğu işareti, giriş/açılış ve Web arayüzü logosu KHS; raf'ta
  konyahackerspace.org uygulaması (dış site penceresi, ayrı başlık şeridi); lime vurgu (üzerindeki yazı mürekkep rengi);
  koyu görünüm. NOTICE'ta KHS adı ve işaretinin sahibine ait olduğu belirtilecek.
- 2026-11-04: F9 Chromium: Debian trixie arm64 Chromium + glibc dışı kütüphaneler `/opt/chromium` (os/chromium/build-bundle.sh,
  Docker), Buildroot paketi `chromium-bin`, `chromium.desktop`, MakerOS tarzı simge (Chromium logosu kullanılmaz), raf'ta.
- 2026-11-04: Konya HackerSpace artık **varsayılan yüklü eklenti** (makerd'ye gömülü, her kullanıcıya ilk listede kurulur;
  kullanıcı kaldırırsa işaret dosyasıyla bir daha kurulmaz; gömülü sürüm yeniyse güncellenir). Eklenti biçimine eklenenler:
  `theme` (duvar kâğıdı, logo, işaret, favicon, simge paketi, uygulamalar), `status` (menü çubuğu göstergesi), `widget`
  (masaüstü/Web ana sayfa kartı, yükseklik içerikten), `autostart`, `app:false`. Kapı durumu kaynağı: sitenin kendi
  kullandığı Firebase RTDB `presence/state.json` (canlı) + `konyahackerspace.org/api/space.json` (since, haftalık);
  eklenti 30 sn / 5 dk önbellekler. Giriş ekranı için marka veri URL'si olarak önbelleklenir.
- 2026-11-04: Chromium 156 paketi (Debian trixie) MakerOS kök FS'inde başsız çalıştı (NSS modülleri dlopen ile eklendi).
- 2026-11-04: AI ajanları MakerOS kabında kuruldu ve çalıştı: Claude Code 2.1.317, Codex 0.166.0, Gemini 0.66.0,
  OpenCode 1.19.5. npm bildirimsiz (`--no-fund --no-audit --loglevel=error`) ve paket betiklerine açık izinle
  (`--allow-scripts`); terminal bitişi "✔ Tamamlandı / ✘ Hata (kod)". Foot ve Thonny kaldırıldı.
- 2026-11-04: Chromium raftan açılmıyordu: `chromium-bin` paketi defconfig'te seçili değildi (imajda `/usr/bin/chromium`
  yoktu) → `BR2_PACKAGE_CHROMIUM_BIN=y`. Dosyalar'da `.py` dosyasına tıklamak betiği doğrudan terminalde çalıştırır
  (`POST /files/run`; klasörde `pyproject.toml` varsa `uv run`, yoksa `python3`); önizleme ⋯ menüsünde "Görüntüle".
  Bildirim Merkezi: "Bildirimleri temizle" (`DELETE /notifications`, tüm açık oturumlara `{"cleared":true}`).
- 2026-11-04: Chromium gerçekten açılıyor (Docker'da doğrulandı): Wayland açıkça seçilir (`systemd-run` biriminde
  `XDG_SESSION_TYPE` yok → "auto" X11'e düşüp çıkıyordu); paketteki eski libwayland/Mesa kaldırıldı, imajın kendi Mesa
  yığını kullanılır (`undefined symbol wl_display_dispatch_queue_timeout`); başlatıcı `os/package/chromium-bin/chromium-launcher`.
  Mock makerd, Wayland ekranı varsa uygulamaları gerçekten başlatır.
- 2026-11-04: Simgeler boş görünüyordu: makerd `vendor/`'u bir yıl `immutable` sunuyordu, üretilen `lucide/icons.js` eski
  tarayıcılarda hiç yenilenmiyordu → yalnız `.woff2` değişmez; simge kümesi `shell/js/icon-set.js`'e taşındı (yeni adres).
- 2026-11-04: Pencere başlık şeridi `title:false` uygulamaların üst çubuğunu (sekmeler, düğmeler) örtüyordu (Python
  Şablonlar/Sürümler açılmıyordu): şeridin altında tıklanabilir öğe varsa tıklama ona geçer, boş yerden sürüklenir.
- 2026-11-04: Uzak Ekran → **Ekran Modu**; telefonda yatay (yön kilidi, yoksa CSS 90° + dokunma koordinatı çevirisi,
  `apps/screen/rotate.js`). Web arayüzü PWA (manifest, `/sw.js` ağ-önce + çevrimdışı ekranı, PNG/maskable/iOS simgeleri;
  SW yalnız HTTPS/localhost'ta), telefonda alt sekme çubuğu ve çentik boşlukları. Docker önizlemesi 80 portunda
  (`os/run-docker.sh`, `compose.yaml`).
- 2026-11-04: Ekran Modu yeniden tasarlandı: pencere değil **tam sayfa** (iç içe masaüstü yok); arayüzde VNC/WayVNC ibaresi
  yok, yalnız üstte kendiliğinden gizlenen kontrol hapı (MakerOS'a dön, klavye, döndür, tam ekran, ⋯ kalite /
  Ctrl+Alt+Del / yeniden bağlan). Uzak tarayıcıdan yerel uygulama (Chromium) açılınca sayfa Ekran Modu'na geçer; Pi'nin
  kendi ekranında Ekran Modu listelenmez. Docker'da doğrulandı: raftan Chromium → Ekran Modu → adres yazıldı, sayfa açıldı.
- 2026-11-04: Ekran Modu ayrı adrese gitmez: aynı sayfanın içinde tüm görünümü kaplayan katman (`shell/js/screen-overlay.js`;
  adres `http://<pi>/` ya da `/web` kalır). "‹ MakerOS" veya tarayıcının Geri tuşu katmanı kapatır.
- 2026-11-04: **Ekran Modu varsayılan kapalı** (Ayarlar › Ekran Modu, yalnız yönetici): kapalıyken wayvnc hiç çalışmaz,
  `/api/v1/vnc` 403, Ekran Modu arayüzün hiçbir yerinde görünmez. Açınca makerd wayvnc'yi başlatır (`internal/screen`;
  systemd kullanıcı birimi, yoksa doğrudan), aynı HTTP portundan. labwc autostart da işarete bakar. Uzaktan **Chromium**
  tıklanınca kullanıcının kendi tarayıcısında yeni sekme açılır (Pi'de tarayıcı başlamaz). Monitör yoksa Pi'nin sanal
  ekranı tarayıcı penceresinin boyutuna geçer (noVNC `resizeSession` → wayvnc; Docker'da 1280×720 → 1440×900 doğrulandı).
- 2026-11-08: **VNC tamamen kaldırıldı** (Ekran Modu, wayvnc/neatvnc/aml paketleri, noVNC, makerd `/vnc` ve
  `/screen`). Uzaktan "Chromium" artık bağlanan bilgisayarın tarayıcısında çizilen Tarayıcı penceresi
  (`/shell/apps/browser/`, iframe; iframe'i reddeden siteler için "Yeni sekmede aç"). Pi'nin kendi ekranında
  Chromium yerel uygulama olarak kalır.
- 2026-11-07: **Chromium yalnız Pi'nin gerçek ekranında** (HDMI/labwc). 11-04'teki ayrı monitörsüz tarayıcı oturumu
  (ikinci labwc + wayvnc 5901, `browser-session`, makerd `browser`) kaldırıldı: VNC içinde "sanal makine" gibi görünüyor,
  Pi 4'te ikinci bir bileşik + yazılım kodlama yüzünden kasıyordu. Uzaktan Chromium tıklanınca Pi'nin ekranında açılır ve
  Ekran Modu gelir (kapalıysa yöneticiye açılsın mı diye sorulur): web'de görünen HDMI'daki ekranın kendisidir.
- 2026-11-04: Web arayüzünde **Web ↔ Ekran** geçişi her zaman görünür: "Ekran" MakerOS masaüstüne (`/`, pencereler, raf,
  Chromium penceresi) geçer, Ekran Modu (VNC) ayarından bağımsızdır; masaüstünden dönüş raftaki Web düğmesiyle.
- 2026-11-04: **2.0.0 sürümü.** Ayarlar'da Ekran Modu sayfası yok (kullanıcı isteği): Ekran Modu kapalı kalır ve
  arayüzde görünmez (arka uç `/api/v1/screen` duruyor). Yazılım güncelleme (Ayarlar › Genel): GitHub Releases →
  imzalı RAUC paketi → boştaki slot → tryboot (`internal/ota`, `makerd-priv` `ota.install|ota.status`,
  `rauc-hook.sh`). Sürümler GitHub Actions ile (`.github/workflows/release.yml`).
