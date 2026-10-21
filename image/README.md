# image

Imaj build hatti. `stages/` icindeki betikler sirayla calisir (01-09);
`build.sh` orkestrator, `build-docker.sh` onu ayricalikli bir builder
konteynerinde calistirir. Kullanim ve asama listesi icin depo kokundeki
[README.md](../README.md#kaynaktan-derleme) dosyasina bakin.

## Build ilkeleri

- Her imaj bir surum damgasi (`/etc/pios-build-version`) ve paket/surum
  manifesti tasir.
- Kernel/firmware commit'i `pins/base.env.example` ile sabitlenir.
- Imaja indirilen dis dosyalar (xterm.js, MediaPipe modelleri) sabit surum
  ve sha256 ile dogrulanir.
- Imaj icine ortak SSH anahtari veya cihaz kimligi gomulmez; cihaza ozel
  kimlikler ilk acilista uretilir.
- Docker testi donanim testlerinin yerine gecmez: kamera, Wi-Fi, GPIO ve
  ekran gercek Pi uzerinde denenmelidir.
