# MakerOS 2 — Master Plan

MakerOS 2, Pardus/Debian tabanını bırakıp doğrudan Linux çekirdeği üzerine
kurulan bağımsız bir işletim sistemidir. Hedef: Raspberry Pi 4 / 400 (4 GB RAM),
web tabanlı macOS benzeri masaüstü, telefondan/PC'den aynı arayüzle erişim.

Araştırma raporları: [`research/01-base-os.md`](research/01-base-os.md),
[`research/02-graphics-shell.md`](research/02-graphics-shell.md),
[`research/03-services-perf.md`](research/03-services-perf.md).

## Kararlar (ADR özeti)

| # | Konu | Karar | Gerekçe |
|---|---|---|---|
| 1 | Build sistemi | **Buildroot 2026.02 LTS**, `BR2_EXTERNAL` ağacı `os/` | Mac (16 GB / 100 GB) üzerinde Docker'da derlenir; küçük, hızlı imaj. Yocto 32 GB RAM + 150-300 GB ister |
| 2 | libc | **glibc** (Bootlin aarch64 glibc toolchain) | torch / onnxruntime / mediapipe yalnızca `manylinux_2_28` wheel yayımlıyor |
| 3 | Init | **systemd** (sade) | oomd, repart, growfs, sysext, boot-complete, journald volatile |
| 4 | Çekirdek | `raspberrypi/linux` **rpi-6.18.y**, commit sabit; `bcm2711_defconfig` + fragment | unicam/ISP, tryboot, vcio; mainline yetersiz |
| 5 | Kök FS | **EROFS (lz4hc) salt-okunur**, A/B slot; `/data` ext4 (ilk açılışta büyür) | Bozulmaya dayanıklı, güvenli OTA |
| 6 | OTA | **RAUC** + Pi `tryboot`/`autoboot.txt`, imzalı bundle, watchdog | Bugünkü sha256-aynı-kaynak açığını kapatır |
| 7 | Ağ | **NetworkManager** (+wpa_supplicant), resolved (mDNS) | Mevcut AP/captive/eduroam mantığı kanıtlı; iwd M5'te değerlendirilecek |
| 8 | Backend | **makerd (Rust: tokio + axum)**: `makerd` (yetkisiz) + `makerd-priv` (dar root yardımcı) + `makerd-user@` (kullanıcı PTY) | Kullanıcı kararı (2026-11-01): kendi sistem kodumuzun tamamı Rust. Bellek güvenliği, ~5 MB RSS. M1/M2'deki Go sürümü referans; API sözleşmesi aynı |
| 9 | Compositor | **labwc 0.9.x** (wlroots 0.19, Raspberry Pi OS varsayılanı), kendi `rc.xml` ile: cog altta tam ekran, yerel pencereler üstte. `LABWC_FALLBACK_OUTPUT` ile monitörsüz sanal ekran | (2026-11-03) wayvnc yalnızca wlroots protokolleriyle çalışır. Smithay ile özel WM, VNC desteğini yeniden yazmayı gerektirirdi; labwc bakımlı ve hafif, Pi Ltd destekli. Özel Rust WM (`wm/`) ileride isteğe bağlı |
| 9b | Dil politikası | Kendi sistem kodumuz **Rust**; web arayüzü JS; kernel/glibc/Mesa/WPE gibi üçüncü parti bileşenler olduğu gibi | C/C++ yalnızca upstream bileşenlerde |
| 10 | Kabuk motoru | **WPE WebKit + cog** (~70-130 MB) | Chromium ~2-3x RAM; Buildroot'ta Chromium yok |
| 11 | Arayüz | Derleme adımsız **ES modülleri + Web Components** (`<mk-*>`), `tokens.json` | Sıfır bağımlılık, düşük bellek, Pi'de hızlı |
| 12 | Uzak erişim | **İki yol, ikisi de birinci sınıf:** (1) Web: kabuk + uygulamalar tarayıcıda doğrudan (telefon dahil). (2) VNC: tüm masaüstü **wayvnc 0.10 (RFB, yalnız 127.0.0.1:5900) → makerd `/api/v1/vnc` (WebSocket↔TCP köprüsü) → noVNC 1.7**; standart VNC istemcileri için isteğe bağlı TCP 5900 | Kullanıcı hedefi: makerlara yönelik, VNC ve web tabanlı. Kimlik doğrulama makerd oturumunda; wayvnc dışarı açılmaz |
| 12b | Ekransız kullanım | HDMI yoksa labwc sanal ekran (1280x720, 30 Hz) açar; Pi yalnızca ağ + tarayıcı ile kullanılır | Maker senaryosu: monitörsüz Pi |
| 13 | Geliştirici araçları | **Docker uyumlu kutu**: podman + `docker` CLI uyumluluğu, `mos box` (Debian arm64): gcc, apt, Chromium | Host sade kalır; öğrenci tam apt deneyimi alır |
| 14 | Host'ta hazır | python3 + **uv**, Node.js (resmi tarball), git, libcamera + rpicam-apps, libgpiod v2 | Wheel'ler, AI CLI'lar, kamera, GPIO kutu gerektirmez |
| 15 | Build/Test | **Her şey Docker'da**: `os/build.sh` (Buildroot), `makerd` Go derleme/test, `docker compose` önizleme | Host'a araç kurulmaz, tekrarlanabilir |
| 16 | Güvenlik | Varsayılan parola yok (ilk açılış sihirbazı), cihaz başına yerel CA + HTTPS, nftables default-deny | |

## Ürün ilkeleri (2026-11-03)

1. **Tamamen makerlara yönelik:** Python/AI, kamera, GPIO, seri port, proje şablonları ilk açılışta hazır.
2. **VNC + web tabanlı:** Pi ister monitörle ister monitörsüz kullanılsın, telefon/PC tarayıcısından aynı arayüz; tam masaüstü VNC ile.
3. **Pardus/Debian bağımlılığı yok:** doğrudan Linux çekirdeği + Buildroot; tüm bileşenler upstream açık kaynak.
4. **Basit:** SD karta yaz → aç → tarayıcıdan `makeros.local`. Komut satırı bilmek gerekmez.
5. **Açık kaynak, geniş kullanım:** GPL-3.0, Docker ile herkesin tekrar üretebileceği derleme, TR/EN arayüz.
6. **Pi 4 (4 GB) üzerinde hızlı:** boşta < 500 MB RAM, kabuğa < 10 sn açılış, VNC 30 FPS / 720p.
7. **Tüm derleme ve testler Docker içinde;** geliştirici makinesine araç kurulmaz.

## Mimari

```
 Telefon/PC tarayıcı ──HTTPS──┐                 Pi HDMI
                              ▼                    ▲
 ┌──────────────── makerd (Rust, :443/:80) ───────┐│
 │ REST /api/v1 · events WS · term WS · vnc WS    ││
 │ static: /usr/share/makeros/shell (web kabuk)   ││
 └──▲──────────── D-Bus / unix soket ─────────▲───┘│
    │ makerd-priv (root, sabit komutlar)       │    │
    │ NetworkManager · logind · timedated ·    │  ┌─┴────────────────────────┐
    │ udisks2 · RAUC                           └──┤ labwc + cog (WPE)        │
    │                                             │ → web kabuk (tam ekran)  │
 ┌──┴──────────────────────────────────────┐      │ wayvnc :5900 · foot, GTK │
 │ systemd · zram · oomd · slices          │      └──────────────────────────┘
 │ EROFS rootfs A/B + /data (ext4)         │
 │ Linux rpi-6.18.y · Pi firmware          │
 └─────────────────────────────────────────┘
```

## Bellek bütçesi (4 GB, hedef)

| Bileşen | Boşta | Tipik |
|---|---|---|
| Çekirdek + CMA (256 MB) | ~300 MB | ~300 MB |
| systemd + NM + journald + resolved | ~45 MB | ~50 MB |
| makerd | ~12 MB | ~25 MB |
| labwc + cog/WPE kabuk | ~110 MB | ~170 MB |
| wayvnc (yalnızca bağlantı varken kodlar) | ~8 MB | ~25 MB |
| Kullanıcı uygulamaları (Chromium kutu, code-server, MediaPipe) | – | ~1.5 GB |
| **Toplam** | **~480 MB** | **~2.1 GB** (≥1.5 GB boş + zram) |

cgroup dilimleri: `shell.slice` (MemoryLow=256M, korunur), `apps.slice`
(MemoryHigh=2.2G), `ai.slice` (MemoryHigh=1.6G). systemd-oomd PSI ile.

## Depo düzeni (v2)

```
os/                 Buildroot BR2_EXTERNAL ağacı (MAKEROS) + Docker build
  configs/makeros_rpi4_defconfig
  board/rpi4/{config.txt,cmdline.txt,autoboot.txt,genimage.cfg,linux.fragment,post-*.sh}
  board/rpi4/rootfs-overlay/      systemd birimleri, sysctl, zram, slices, udev
  package/{makerd,makeros-shell,uv-bin,nodejs-bin,makeros-firstboot,...}
  docker/Dockerfile  build.sh
makerd/             Rust backend (cargo workspace: makerd, makerd-priv, makerd-user) — Go M2 sürümü geçişte makerd-go/ olarak referans
wm/                 makeros-wm: Rust + Smithay Wayland compositor (M3)
shell/              Web masaüstü kabuğu ve sistem uygulamaları
docs/v2/            Plan, API sözleşmesi, araştırma
compose.yaml        Yerel önizleme: makerd (mock) + kabuk → http://localhost/
image/              v1 (Pardus) — v2 yayınlanana kadar korunur
```

## Yol haritası

| Faz | Hedef | Çıktı | Ajan dağılımı (≤3 paralel) |
|---|---|---|---|
| **M0** Araştırma | Kararlar | `docs/v2/research/*`, bu plan | 3 araştırma ajanı ✅ |
| **M1** İskelet | Docker'da derlenen her parça | `os/` ağacı + defconfig; `makerd` çekirdeği (auth, sistem, ağ, terminal, events); web kabuk (menü çubuğu, dock, pencereler, Spotlight, Mission Control, kısayollar, Ayarlar/Terminal/Etkinlik) ; `compose.yaml` önizleme | A: os · B: makerd · C: shell |
| **M2** İlk boot | Pi'de cage+cog kabuğa < 10 sn | Tam Buildroot imajı, firstboot sihirbazı, NM fallback AP, mDNS, zram/oomd | A: os build · B: makerd-priv + firstboot · C: kurulum sihirbazı UI |
| **M3** Native pencereler + VNC | labwc oturumu, wayvnc, noVNC "Ekran" uygulaması | `os/package/{labwc,wayvnc,neatvnc,aml}` ✅ (2026-11-03), makerd `/api/v1/vnc` köprüsü ✅, kabukta noVNC, Xwayland on-demand (OpenCV imshow) | A: os · B: makerd · C: shell |
| **M4** Donanım | Kamera, GPIO, USB, ses, ekran | WebRTC/MJPEG kamera, libgpiod v2, udisks2, displays | A: os paketleri · B: makerd modülleri · C: uygulama UI'ları |
| **M5** Geliştirici | Python/AI, agent'lar, `mos box` (Docker uyumlu) | uv/ai-env, Node, AI CLI'lar, podman+docker CLI, Chromium kutuda, code-server | A · B · C |
| **M6** OTA + Güvenlik | RAUC A/B, imzalı bundle, HTTPS/CA, nftables | `os/` RAUC + tryboot betiği, makerd ota | A · B · C (test) |
| **M3.5** Sanal kıyas | Raspberry Pi OS 2026-10-06 ile Docker + QEMU raspi4b kıyası ✅ (2026-11-03) | [`BENCHMARK.md`](BENCHMARK.md): imaj 5,2 kat küçük, çekirdek masaüstü + VNC 3,8 kat az bellek; açık risk: WPE boşta maliyeti (Pi'de ölçülecek) | |
| **M7** Sürüm | 2.0.0 | CI (GitHub Actions arm64), SBOM, dokümantasyon, v1→v2 geçiş notu | |

## Orkestrasyon kuralları

- Master ajan planlar, sözleşmeleri (`docs/v2/api.md`) yazar, işleri dağıtır, sonuçları birleştirir.
- Aynı anda en fazla **3 alt ajan**; her ajan yalnızca kendi dizinine yazar (`os/`, `makerd/`, `shell/`).
- Ortak sözleşme değişikliği yalnızca master tarafından yapılır.
- Her faz sonunda: derleme/test Docker'da yeşil → master incelemesi → sonraki faz.

## Riskler

| Risk | Önlem |
|---|---|
| WPE'nin V3D performans gerilemeleri | Sürüm sabitleme, Pi'de fps/RAM benchmark (M2) |
| Tryboot A/B betiği kritik yol | Güç kesintisi / bozuk bundle testleri, watchdog |
| Debian ekosisteminden kopuş | `mos box` (Docker uyumlu Debian kutusu) |
| labwc config/tema sınırları | Pencere davranışları kabukta (web) çözülür; gerekirse `wm/` Rust WM sonraya |
| wayvnc/neatvnc Buildroot paketi değil | `os/package/` altında kendi paketlerimiz; hash dosyaları Docker içinde üretilir |
| pip OpenCV `imshow` X11 ister | Xwayland on-demand (M3) |
| CVE bakımı | Buildroot LTS, `make show-info` → SBOM/CVE kontrolü CI'da |
