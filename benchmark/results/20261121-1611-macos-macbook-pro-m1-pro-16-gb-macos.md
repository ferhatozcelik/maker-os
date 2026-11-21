# MakerOS Bench · MacBook Pro M1 Pro 16 GB · macOS

**Genel puan: 1,025**  ·  referans (Apple M1 Pro · macOS) = 1000  ·  2026-11-21 16:11 UTC  ·  MakerOS Bench 1.0.0

| Kategori | Puan | |
|---|---:|---|
| CPU tek çekirdek | 1,014 | `██████████░░░░░░░░░░` |
| CPU çok çekirdek | 1,018 | `██████████░░░░░░░░░░` |
| Bellek | 988 | `██████████░░░░░░░░░░` |
| Disk | 1,081 | `███████████░░░░░░░░░` |

## Sistem

| | |
|---|---|
| İşletim sistemi | macOS 27.2.1 |
| Model | MacBookPro18,3 |
| İşlemci | Apple M1 Pro |
| Çekirdek | 8 (6 performans + 2 verimlilik) |
| Bellek | 16.0 GB |
| Mimari | arm64 |
| Çekirdek (kernel) | 27.2.0 |
| Python | 3.9.6 (CPython) |

## Testler

| Test | Sonuç | Puan |
|---|---:|---:|
| Tamsayı | 5.93 M işlem/sn | 1,004 |
| Kayan nokta (Mandelbrot) | 6.11 M yineleme/sn | 1,018 |
| SHA-256 | 2,478 MB/s | 1,033 |
| zlib sıkıştırma | 53.8 MB/s | 1,015 |
| JSON kodla/çöz | 11.4 bin/sn | 1,006 |
| Düzenli ifade | 30.3 MB/s | 1,009 |
| Tamsayı · tüm çekirdekler | 34.8 M işlem/sn | 1,024 |
| Kayan nokta · tüm çekirdekler | 35.5 M yineleme/sn | 1,013 |
| Bellek kopyalama | 42.9 GB/s | 976 |
| Nesne ayırma | 1.48 M nesne/sn | 1,001 |
| Sıralı yazma (fsync) | 4,869 MB/s | 1,391 |
| Sıralı okuma | 2,459 MB/s | 984 |
| Rastgele 4K yazma (fsync) | 208 IOPS | 1,015 |
| Rastgele 4K okuma | 11,781 IOPS | 982 |

Çok çekirdek ölçeklenmesi: **5.9×** (8 çekirdekte, tamsayı).

Süre: 15 sn · kip: tam · disk testi: 494 GB birim

_Python iş yükü başarımıdır; aynı MakerOS Bench sürümüyle alınan sonuçlar karşılaştırılabilir._
