# MakerOS Bench · Raspberry Pi 4 Rev 1.2 4 GB · MakerOS 1.0.0

**Genel puan: 98**  ·  referans (Apple M1 Pro · macOS) = 1000  ·  2026-11-26 23:34 UTC  ·  MakerOS Bench 1.1.0

| Kategori | Puan | |
|---|---:|---|
| CPU tek çekirdek | 144 | `█░░░░░░░░░░░░░░░░░░░` |
| CPU çok çekirdek | 112 | `█░░░░░░░░░░░░░░░░░░░` |
| Bellek | 97 | `█░░░░░░░░░░░░░░░░░░░` |
| Disk | 59 | `█░░░░░░░░░░░░░░░░░░░` |

## Sistem

| | |
|---|---|
| İşletim sistemi | MakerOS 1.0.0 |
| Model | Raspberry Pi 4 Model B Rev 1.2 |
| İşlemci | aarch64 |
| Çekirdek | 4 |
| En yüksek frekans | 1500 MHz |
| Bellek | 3.7 GB |
| Mimari | aarch64 |
| Çekirdek (kernel) | 6.18.59-v8-makeros |
| Python | 3.14.5 (CPython) |
| CPU yöneticisi | ondemand |
| Sıcaklık (başta → sonda) | 57.9 °C → 67.7 °C |

## Testler

| Test | Sonuç | Puan |
|---|---:|---:|
| Tamsayı | 0.85 M işlem/sn | 143 |
| Kayan nokta (Mandelbrot) | 1.20 M yineleme/sn | 199 |
| SHA-256 | 143 MB/s | 60 |
| zlib sıkıştırma | 12.2 MB/s | 231 |
| JSON kodla/çöz | 1.43 bin/sn | 126 |
| Düzenli ifade | 5.30 MB/s | 177 |
| Tamsayı · tüm çekirdekler | 3.31 M işlem/sn | 97 |
| Kayan nokta · tüm çekirdekler | 4.50 M yineleme/sn | 129 |
| Bellek kopyalama | 2.39 GB/s | 54 |
| Nesne ayırma | 0.26 M nesne/sn | 175 |
| Sıralı yazma (fsync) | 20.9 MB/s | 6 |
| Sıralı okuma | 46.4 MB/s | 19 |
| Rastgele 4K yazma (fsync) | 130 IOPS | 634 |
| Rastgele 4K okuma | 2,049 IOPS | 171 |

Çok çekirdek ölçeklenmesi: **3.9×** (4 çekirdekte, tamsayı).

## Yayımlanmış Raspberry Pi 4 sonuçlarıyla

| Test | Raspberry Pi 4 (kaynak) | Bu sistem | Fark | |
|---|---:|---:|---:|---|
| Geekbench 5 · tek çekirdek | 233 puan | 215 puan | -8% | `█████████░░░░░░░░░░░` |
| Geekbench 5 · çok çekirdek | 686 puan | 634 puan | -8% | `█████████░░░░░░░░░░░` |
| Zstd sıkıştırma (seviye 19) | 291 sn | 686 sn | -58% | `████░░░░░░░░░░░░░░░░` |
| Speedometer 2.0 (tarayıcı) | 16.9 çalışma/dk | — |  |  |

Fark: + bu sistem daha hızlı. Çubuk: kaynaktaki değer = yarım çubuk.

Kaynaklar ve koşullar:

- Geekbench 5 · tek çekirdek: Pi 4 4 GB · Linux 5.10.17-v8+ aarch64 · Geekbench 5.4.0 — https://browser.geekbench.com/v5/cpu/9697615
- Geekbench 5 · çok çekirdek: Pi 4 4 GB · Linux 5.10.17-v8+ aarch64 · Geekbench 5.4.0 — https://browser.geekbench.com/v5/cpu/9697615
- Zstd sıkıştırma (seviye 19): Pi 4 4 GB · Raspbian 10 armv7l · Linux 4.19.50 · zstd 1.3.4 — https://openbenchmarking.org/result/2202182-NE-2202156EI32
- Speedometer 2.0 (tarayıcı): Pi 4 · Raspbian (Haziran 2019) — https://magazine.raspberrypi.com/articles/raspberry-pi-4-specs-benchmarks
- Bu sistemde zstd: *** Zstandard CLI (64-bit) v1.5.7, by Yann Collet *** (`zstd -19 -T1`, 3 ölçümün ortancası)
- Bu sistemin Geekbench sonucu: https://browser.geekbench.com/v5/cpu/24849878

Süre: 3088 sn · kip: tam · disk testi: 54 GB birim

_Python iş yükü başarımıdır; aynı MakerOS Bench sürümüyle alınan sonuçlar karşılaştırılabilir._
