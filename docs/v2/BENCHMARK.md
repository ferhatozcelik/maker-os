# MakerOS 2 ve Raspberry Pi OS: sanal karşılaştırma (2026-11-03)

Bu rapordaki tüm ölçümler Docker içinde alındı (Mac'te colima, aarch64, 6 CPU / 10 GB).
Gerçek Raspberry Pi donanımında ölçüm **yapılmadı**. Mutlak süreler Pi 4'ü temsil etmez.
Anlamlı olan, aynı düzenekte ölçülen iki sistemin birbirine göre durumudur.

| | MakerOS | Raspberry Pi OS |
|---|---|---|
| Sürüm | 2.0.0-dev (Buildroot 2026.02.3, çekirdek 6.18.59) | Desktop / Lite **2026-10-06** trixie arm64 (çekirdek 6.18.50+rpt) |
| Masaüstü + VNC | labwc 0.9.8 + wayvnc 0.10.2 + makerd + cog/WPE 2.50 | labwc 0.20.2 + wayvnc 0.10.2 + wf-panel-pi + pcmanfm-pi + lxsession |

Betikler: `bench/scripts/` (`run-one.sh`, `measure.sh`, `rfb-bench.mjs`,
`qemu-boot.sh`). Ham veriler: `bench/out/`.

## 1. İmaj ve dosya sistemi

| | MakerOS 2.0.0-dev | Pi OS Desktop | Pi OS Lite |
|---|---|---|---|
| İndirme (`.img.xz`) | **248 MiB** | 1302 MiB | 525 MiB |
| Ham imaj | 5,8 GiB ¹ | 6,3 GiB | 2,9 GiB |
| Kök dosya sistemi (açılmış) | **816 MiB** (karttaki EROFS lz4hc: 383 MiB) | 4,69 GiB | 1,83 GiB |
| Paket | 177 (Buildroot) ² | 1645 (dpkg) | 633 (dpkg) |
| Dosya | **13 039** | 117 460 | 61 939 |
| Açılışta etkin sistem birimi | 35 | 49 | 37 |
| Kök dosya sistemi | salt okunur, A/B (OTA) | yazılabilir ext4 | yazılabilir ext4 |

¹ İmajın 2,5 GiB'ı OTA için boş bırakılan ROOT_B bölümüdür. `.xz` hâlinde yer kaplamaz.
² Buildroot paketleri dpkg paketlerinden daha iri taneli olduğu için bu iki sayı
birebir karşılaştırılamaz. Dosya sayısı ve boyut daha adil bir ölçüdür.

**Sonuç:** MakerOS, Pi OS Desktop'a göre indirmede 5,2 kat, açılmış kök dosya sisteminde 5,9 kat,
dosya sayısında 9 kat daha küçük. Kurulu Python/AI, kamera ve GPIO araçlarına rağmen
Pi OS **Lite**'tan bile küçük.

## 2. Masaüstü + VNC yığını (aynı kap düzeneği)

Her sistem kendi kök dosya sisteminden Docker imajı olarak çalıştırıldı.
Koşullar: Pi 4 benzeri sınır (4 CPU, 4 GB), monitörsüz labwc (headless + pixman),
her ölçümden önce sayfa önbelleği boşaltıldı. Her yapılandırma 3 kez ölçüldü; tabloda ortanca değerler var.
Bellek PSS (paylaşılan sayfalar paylaşanlara bölünür) olarak verildi.

| | Pi OS Desktop | **MakerOS çekirdek** ³ | MakerOS tam ⁴ |
|---|---|---|---|
| Oturum başlangıcından VNC hazır olana | 1330 ms | **480 ms** | 510 ms |
| Süreç sayısı | 21 | **5** | 8 |
| Toplam PSS | 134,9 MB | **35,7 MB** | 511,5 MB |
| Toplam RSS | 268,8 MB | **41,7 MB** | 593,6 MB |
| Boşta CPU (tek çekirdeğin %'si) | 0,17 | **0,05** | 100,3 ⚠ |
| İlk tam VNC karesi (1280x720, Tight/JPEG) | 35,9 ms | **32,1 ms** | 71 ms |
| wayvnc PSS | 30,6 MB | **5,3 MB** | 5,0 MB |

³ labwc + wayvnc + makerd. Web kabuğunun tamamı telefon/PC tarayıcısında çalışır,
Pi'de hiçbir şey çizilmez. Pi OS'un karşılığı panel + masaüstü + dosya yöneticisidir.
⁴ Pi'nin ekranında web kabuğu da açık (cog/WPE). Kapta GPU olmadığı için
**yazılımla çizim (Mesa softpipe)** kullanıldı. Bu değerler en kötü durumu gösterir, bkz. §4.

Raspberry Pi OS'ta belleğin büyük kısmı wf-panel-pi (34 MB), pcmanfm (20 MB),
iki xdg-desktop-portal (16 MB) ve 7 gvfs sürecinden geliyor. wayvnc'nin PSS'i 30 MB,
çünkü Pi OS sürümü ffmpeg ile derlenmiş. MakerOS'unki libjpeg-turbo ile derlenmiş ve 5 MB.

## 3. Tam açılış (QEMU `raspi4b`)

Docker içinde QEMU 10.0.13, `-M raspi4b -smp 4 -m 2G` (QEMU bu makinede 2 GB ile sınırlı).
Emülasyon TCG ile yapıldı, yani gerçek Pi'den birkaç kat yavaş. Saat seri konsoldan okundu.
İki sistem için de değişen tek şeyler şunlar: DTB'den Bluetooth düğümü silindi (yoksa seri konsol çalışmıyor),
`systemd.watchdog_sec=off`, `systemd.log_target=kmsg`.

| | MakerOS | Raspberry Pi OS Desktop |
|---|---|---|
| Kök dosya sistemi bağlandı | 6,6 sn | 5,7 sn (initramfs'siz) / 11,3 sn (initramfs ile) |
| makerd (web + VNC erişimi) hazır | **36,6 sn** | (yok) |
| Yerel kabuk (labwc + cog) başladı | 36,6 sn | |
| `systemd: Startup finished` | **35,3 sn** (çekirdek 7,6 + kullanıcı alanı 27,7) | **açılış tamamlanmadı** ⁵ |

⁵ Raspberry Pi OS QEMU'da iki şekilde denendi. initramfs ile: `Kernel panic - Attempted to kill init`.
initramfs'siz: `systemd-remount-fs` adımından sonra 15 dakika boyunca ilerlemedi; günlük diske hiç yazılmadı.
Bu, Raspberry Pi OS'un gerçek Pi'deki açılış hızı hakkında bir şey **söylemez**. QEMU'nun raspi4b
emülasyonu eksik (firmware mailbox, PCIe/USB, GENET yok). Açılış süresi karşılaştırması
gerçek Pi'de yapılmalı.

MakerOS'un kendi açılışı ise ölçüm sayesinde düzeldi: ilk ölçümde 83,8 sn idi, düzeltmeyle 35,3 sn oldu. Ayrıntı §5'te.

## 4. Ana risk: web kabuğunun boşta maliyeti

GPU olmadan (softpipe) WPE, ekrandaki her değişiklikte tüm pencereyi yeniden birleştiriyor:

| Sayfa (cog, softpipe) | Boşta CPU | WPE PSS |
|---|---|---|
| `about:blank` | %0 | 330 MB |
| Düz sayfa / duvar kâğıdı / bulanık duvar kâğıdı | %0–1 | 333–367 MB |
| `backdrop-filter: blur` | %3 | 346 MB |
| Yanıp sönen metin imleci (odaklı `<input>`) | **%27** | 278 MB |
| Sonsuz CSS animasyonu | **%102** | 335 MB |
| MakerOS giriş ekranı | **%100–106** (`?fx=flat` ile de) | 463–481 MB |
| Giriş ekranı, `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1` | %75 | 294 MB |

Aynı giriş ekranı Chromium'da boştayken neredeyse hiçbir şey yapmıyor (6 sn'de 1 layout, çalışan animasyon yok).
Yani sorun kabuk kodunda değil. Sorun, WPE'nin yazılımla çizim yolunun her kareyi baştan birleştirmesi.
Pi 4'ün V3D GPU'sunda bu işin çok ucuz olması beklenir, ama bu henüz **ölçülmedi**.

## 5. Ölçümlerin ortaya çıkardığı ve düzeltilen hatalar

| Sorun | Etki | Düzeltme |
|---|---|---|
| `makeros-network-fallback` `Type=oneshot` idi; ağ için 45 sn bekleyip multi-user/graphical hedeflerini blokluyordu | Açılış 83,8 sn | `Type=simple`; açılış **35,3 sn** |
| GPU/DRM yokken kiosk her ~13 sn'de çöküp yeniden başlıyordu | Ekran ve VNC hiç açılmıyordu | `/usr/lib/makeros/session`: DRM yoksa headless + pixman + Skia CPU; wayvnc `--gpu`'yu yalnız GPU varsa kullanıyor |
| `makeros-zram.service` açıklamasında kaçışsız `%` | systemd belirteç hatası | `%%` |
| systemd 258 `DefaultCPUAccounting=` seçeneğini kaldırmış | Her açılışta uyarı | Satır silindi |
| genimage: firmware paketinin `cmdline.txt` dosyası slotlara özel `cmdline.txt` ile çakışıyordu | İmaj üretilemiyordu | `post-image.sh` firmware'inkini atlıyor |
| labwc `rc.xml` içinde boş `<name>` | Her açılışta hata satırı | Kaldırıldı |

## 6. Uçtan uca Uzak Ekran testi

`make -C shell vnc-e2e`: başsız Chromium → kabuk "Uzak Ekran" (noVNC 1.7) → makerd
`/api/v1/vnc` → wayvnc → labwc. Test giriş yapıyor, bağlanıyor (1,1–1,2 sn), 1280x720 kareyi
doğruluyor, VNC üzerinden terminale bir komut yazıyor ve komutun uzak masaüstünde gerçekten
çalıştığını kontrol ediyor. Sonuç: **geçti** (Debian tabanlı sanal masaüstü kabında).
MakerOS'un kendi kök dosya sisteminde (cog/WPE giriş ekranı) de bağlantı ve görüntü
adımları geçti. Web kabuğu WPE'de doğru görüntüleniyor.

## 7. Sonraki adımlar

1. **Gerçek Pi 4 (4 GB) üzerinde ölçüm:** WPE + V3D ile boşta CPU/PSS, açılış süresi ve Raspberry Pi OS'la aynı karttan açılış karşılaştırması. En kritik açık bu.
2. Kiosk kilit ekranında parola kutusunun otomatik odaklanmaması (imleç kaynaklı yeniden çizim) ve kilit ekranı katmanlarının sadeleştirilmesi.
3. Pi 4 donanım H.264 VNC kodlaması için ffmpeg (neatvnc `-Dh264`). Şu an Tight/JPEG kullanılıyor.
4. Yazılımla çizim yedeğinin (Skia CPU) gerçek Pi'de GPU'suz senaryoda doğrulanması.
