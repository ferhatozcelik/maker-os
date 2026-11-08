# Genel Mimari

[Türkçe](overview.md) · [English](overview.en.md)

MakerOS, hazır bir Linux dağıtımının üzerine masaüstü kurmak yerine Buildroot ile sıfırdan üretilen bağımsız bir işletim sistemidir. Sistem üç ana katmandan oluşur: işletim sistemi tabanı ([`os/`](../../os/)), Rust sistem servisleri ([`makerd/`](../../makerd/)) ve web masaüstü kabuğu ([`shell/`](../../shell/)).

## Temel Bileşenler

| Bileşen | Teknoloji | Kaynak |
| --- | --- | --- |
| Derleme sistemi | Buildroot 2026.02 LTS | [`os/`](../../os/) |
| Çekirdek | Raspberry Pi Linux 6.18 (ARM64) | [`os/board/rpi4/`](../../os/board/rpi4/) |
| Başlatma ve servisler | systemd | [`os/board/rpi4/rootfs-overlay/`](../../os/board/rpi4/rootfs-overlay/) |
| Kök dosya sistemi | EROFS (salt okunur) + ext4 (`/data`) | [`os/board/rpi4/genimage.cfg.in`](../../os/board/rpi4/genimage.cfg.in) |
| OTA güncelleme | RAUC + Raspberry Pi `tryboot` | [`makerd/src/ota.rs`](../../makerd/src/ota.rs) |
| Arka uç servisleri | Rust (`makerd`, `makerd-priv`, `makerd-user`, `makeros-ai`) | [`makerd/src/`](../../makerd/src/) |
| Yerel ekran | Wayland (`labwc`) + Cog (WPE WebKit) | [`os/package/labwc/`](../../os/package/labwc/) |
| Masaüstü ve Web kabuğu | Bağımlılıksız ES modülleri ve CSS | [`shell/`](../../shell/) |

Hem HDMI ekrana bağlı yerel oturum hem de ağ veya USB-C üzerinden bağlanan uzak tarayıcılar aynı `makerd` API'sini ve `shell/` arayüzünü paylaşır.
