# Sorun giderme

## Pi açılmıyor (ekranda görüntü yok, yeşil LED yanıp sönmüyor)

- Kartı bilgisayara takın. `bootfs` (FAT32) bölümü görünmüyor veya
  "dosya sistemi yok" deniyorsa yazma işlemi yarım kalmıştır: imajı Raspberry
  Pi Imager ile **Use custom** seçerek yeniden yazın ve doğrulamanın
  bitmesini bekleyin.
- İndirilen dosyayı doğrulayın: `sha256sum -c maker-os-*.img.xz.sha256`.
- Resmi 5V/3A güç kaynağı kullanın; zayıf kablo veya adaptör açılışı engeller.
- Pi 5 henüz desteklenmez.
- Kök bölüm ilk açılışta kartın tamamına genişler. Kartı bilgisayarda
  incelediğinizde kök bölüm hâlâ ~6,5 GB ise Pi ilk açılışı hiç tamamlamamıştır.

## Pi'nin adresini bulamıyorum

1. `http://maker-os.local/` açılmıyorsa (bazı Windows ve Android
   sürümleri `.local` adlarını çözmez) modemin bağlı cihazlar listesinden IP'ye bakın.
2. Kartı bilgisayara takıp `bootfs/pardus-status.txt` dosyasını açın:
   ana makine adı, mod (istemci/erişim noktası) ve IP adresleri yazar.
3. Ağ yoksa 45 sn sonra şifresiz `MakerOS` ağı açılır. Telefonu bağlayın; kurulum sayfası
   kendiliğinden açılır (açılmazsa `http://10.42.0.1/kurulum`). Ağınızı seçip parolasını girin.

## Wi-Fi'ye bağlanmıyor

- Parolayı kontrol edin; yanlışsa Merkez "Parola yanlış" der ve erişim noktasını geri açar.
- Okul/kurum ağlarında kullanıcı adı + parola gerekir (kurumsal ağ olarak görünür).
- 5 GHz ağlar Pi 4'te desteklenir; ülke kodu `TR` ayarlıdır. Kanal 12-13 kullanan ağlarda sorun olursa modemi farklı kanala alın.
- **Merkez → Wi-Fi ve Ağ**'da "NetworkManager çalışmıyor" uyarısı varsa Pi'yi yeniden başlatın.

## İnternet var ama siteler açılmıyor, güncellemeler sertifika hatası veriyor

Pi'de pilli saat yoktur; saat yanlışsa HTTPS bağlantıları reddedilir.
**Merkez → Tarih ve Saat → Bu cihazın saatini kullan** ile düzeltin. Kurum
ağları NTP'yi (UDP 123) engelleyebilir.

## Kamera görünmüyor

- Kabloyu Pi kapalıyken takın; yönünü kontrol edin ([Donanım rehberi](hardware.md#kamera)).
- Terminal: `libcamera-hello --list-cameras` kamerayı listelemeli.
- Üçüncü taraf kamerada **Merkez → Kamera → Pi kamera modeli**'nden sensörü seçip yeniden başlatın.
- "Kamera meşgul": başka bir uygulama (Python betiği, MediaPipe örneği, ikinci bir sekme) kamerayı kullanıyordur.

## Monitörde görüntü yok veya yanlış çözünürlük

- Monitörü **HDMI 0**'a (USB-C'nin yanındaki) takın.
- **Merkez → Ekran ve Donanım → Çözünürlük → Otomatik** seçip yeniden başlatın.
- Sonradan takılan monitör için **Masaüstünü yeniden başlat**.

## USB bellek görünmüyor veya çıkarılamıyor

- **Merkez → USB Aygıtları** bölümünde disk listelenmeli. "biçimlendirilmemiş" görünüyorsa bilgisayarda FAT32/exFAT ile biçimlendirin.
- "ayrılamadı (açık dosya olabilir)": bellekteki dosyayı kullanan pencereyi veya terminali kapatın.
- Harici diskler fazla akım çekiyorsa beslemeli USB hub kullanın.

## Günlükler

```bash
journalctl -u pios-center -b          # Merkez (terminal oturumları dahil)
journalctl -u pios-network-fallback -b
journalctl -u NetworkManager -b
dmesg | grep -iE 'brcmfmac|unicam|imx|ov5647|usb'
```

## Build kesildi; hangi aşamadan devam etmeliyim?

`image/` dizininde `./build-docker.sh 04` gibi bir başlangıç aşaması verin.
Bu komut 04'ten başlayıp 09'a kadar çalışır; yalnızca tek aşama çalıştırmaz.
Kök dosya sistemi ve indirme önbelleği `pios-work` Docker volume'unda kalır.

- **04'te firmware paketi bulunamıyor:** Paketler kuruluysa 04 kurulumu atlar.
  Paketler kurulu değilse ve `/var/cache/pios-debs/` de boşsa
  `./build-docker.sh 03` ile donanım paketlerini yeniden indirin.
- **Merkez veya overlay değişti:** `./build-docker.sh 06` ile yapılandırmayı,
  temizliği, imajı ve sağlama toplamlarını yeniden üretin.
- **Taban veya çekirdek pini değişti:** Tam build başlatın
  (`./build-docker.sh`); eski kök dosya sistemiyle devam etmeyin.
- **Disk doldu:** Hem Docker/Colima diskinde hem `image/out/` için host
  diskinde boş alan kontrol edin. `pios-work` volume'unu silmek mevcut kök
  dosya sistemini ve önbelleği kaybettirir.

Sıkıştırma sırasında `.img.xz` dosyasının görünmesi build'in bittiği anlamına
gelmez. 09 aşamasının başarılı bitmesini ve sağlama toplamlarının oluşmasını
bekleyin. Donanım doğrulaması için imajı gerçek Pi üzerinde açın.
