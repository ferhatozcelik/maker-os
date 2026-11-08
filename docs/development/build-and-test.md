# Derleme ve Test Rehberi

[Türkçe](build-and-test.md) · [English](build-and-test.en.md)

MakerOS'un tüm derleme, önizleme ve duman testi süreçleri Docker içinde çalışır; geliştirici makinesine ek araç zinciri kurulmasına gerek yoktur.

## Hızlı Geliştirme ve Önizleme

```bash
# 1. Kabuk geliştirme sunucusu (sahte API ile hızlı arayüz testi)
make -C shell dev

# 2. Gerçek Rust arka ucu (makerd --mock) + kabuk önizlemesi
make -C makerd preview

# 3. Kabuk uçtan uca duman testi (başsız Chromium)
make -C shell smoke
make -C shell smoke-makerd
```

## Raspberry Pi İmajını Derleme (`os/`)

```bash
cd os
./build.sh defconfig
./build.sh build
# Çıktı: os/out/makeros-1.0.0-rpi4.img.xz (+ .sha256)
```

USB-C üzerinden bağlı bir Raspberry Pi üzerindeki geliştirme imajına anında güncelleme göndermek için [`os/debug/push.sh`](../../os/debug/push.sh) betiği kullanılır (`os/debug/push.sh all`).
