# Ekran görüntüleri

Bu görüntüler `20261021T202330Z-ddd9692b` build'inin kök dosya sisteminden
oluşturulan Docker test ortamında çekildi. Merkez, XFCE masaüstü ve web
terminali gerçekten çalışır; API yanıtları veya ekran içerikleri taklit edilmez.
Masaüstü tarayıcı alanı 1440 × 1000, telefon görünümü 390 × 844 pikseldir.
Telefon görüntüleri tarayıcının mobil emülasyonuyla alınmıştır.

## Pardus Maker Merkezi

Cihaz durumu, sistem bilgileri ve yönetim bölümleri. Docker ortamında
çekirdek, bellek ve ağ bilgileri Raspberry Pi yerine konteynerin çalıştığı
Linux ortamını yansıtır.

![Pardus Maker Merkezi genel bakış](screenshots/merkez.png)

## Telefon: Wi-Fi ve ağ

Docker'da fiziksel Wi-Fi radyosu bulunmaz; bu görüntü telefon düzenini ve
ağ sayfasını gösterir. Gerçek Wi-Fi tarama ve bağlantı testi Pi üzerinde yapılır.

<img src="screenshots/telefon-wifi.png" alt="Telefon boyutunda Wi-Fi ve ağ sayfası" width="390">

## Telefon: web terminali

`pardus` kullanıcısıyla çalışan kabuk ve telefon için ek tuş çubuğu.

<img src="screenshots/telefon-terminal.png" alt="Telefonda çalışan web terminali" width="390">

## Tarayıcıda uzak masaüstü

XFCE ve Plank, test ortamında sanal X ekranında çalışır; görüntü noVNC ile
Merkez'in uzak ekran bölümünde açılır.

![Tarayıcıdaki Pardus masaüstü](screenshots/masaustu.png)

## Python ve hazır yapay zeka ortamı

Python sürüm yönetimi ve imajla birlikte gelen AI ortamı.

![Python ve yapay zeka ortamı](screenshots/python.png)

## MediaPipe proje şablonları

El takibi şablonunun seçildiği yeni proje formu. Bu bir kamera çıkarımı
veya GPIO testi değildir; kamera ve bağlı devreler gerçek Pi gerektirir.

![MediaPipe el takibi projesi oluşturma formu](screenshots/projeler.png)

## Yerelde açma

```bash
cd image
./test/run-test.sh
```

Merkez: <http://localhost:8080/> · kullanıcı: `pardus` · parola: `pardus`.
Konteyner arka planda çalışır. Günlükler: `docker logs pardus-maker-test`.
Durdurmak için: `docker stop pardus-maker-test`; tekrar başlatmak için:
`docker start pardus-maker-test`.
