# Kablosuz güncelleme (OTA)

Kurulu bir Pardus Maker, SD kartı yeniden yazmadan yeni sürüme yükseltilir.
Güncellemeyi her zaman kullanıcı başlatır; cihaz yalnızca haber verir.

## Nasıl çalışır

1. **Denetim:** `pios-ota-check.timer` açılıştan 10 dk sonra ve 6 saatte bir
   GitHub'daki son yayını (`releases/latest`) sorgular. Yeni sürüm varsa Pi'nin
   masaüstünde bildirim, Merkez'de uyarı görünür. Merkez → Güncellemeler →
   **Denetle** ile elle de denetlenir.
2. **Kurulum** (Güncelle düğmesi): `pardus-maker-X.Y.Z-ota.tar.xz` indirilir,
   yayındaki `.sha256` ile doğrulanır, açılır.
3. **Yedek:** üzerine yazılacak her dosya `/var/lib/pios/ota/backup/` altına
   alınır; yeni oluşan dosyalar kaydedilir.
4. **Kurulum ve kanca:** dosyalar atomik olarak yerleştirilir; kurulum betiği
   yeni servisleri etkinleştirir ve paketin içindeki `.deb`'leri kurar. Cihazın
   dış paket deposu yoktur: OTA paketi, ilk sürümden bu yana değişen tüm
   sistem paketlerini taşır (bkz. `vendor/README.md`). Cihaz ayarları
   (`config.txt`, `cmdline.txt`, kullanıcı, Wi-Fi, parolalar) değişmez.
5. **Sağlık kontrolü:** Merkez yeniden başlatılır ve 45 sn içinde yanıt vermezse
   yedek **kendiliğinden** geri yüklenir.
6. **Çekirdek:** yeni çekirdek doğrudan yazılmaz; `pios-ota/` altına konur ve
   Pi'nin `tryboot` özelliğiyle **bir kez** denenir. Açılırsa kalıcı olur;
   açılmazsa Pi'yi kapatıp açın, eski çekirdekle başlar.
7. **Onay:** Merkez'de açılan pencerede **Onayla ve temizle** yedeği ve geçici
   dosyaları siler. **Geri al** önceki sürüme döner.

Kurulum sırasında elektrik kesilirse bir sonraki açılışta `pios-ota-boot.service`
yedekten geri döner.

## Komut satırı

```bash
sudo python3 /usr/lib/pios/center/ota.py check      # yeni sürüm var mı
cat /var/lib/pios/ota/state.json                    # durum
cat /var/lib/pios/ota/ota.log                       # son işlemin günlüğü
```

## Yayın hazırlama

`VERSION` dosyasını artırıp `CHANGELOG.md`'ye aynı başlıkla (`## 1.3.0`) bölüm
ekleyin ve `main`'e gönderin. GitHub Actions imajı build eder, imaj ve OTA
paketini sürüm notlarıyla birlikte yayına ekler. `VERSION` değişmeyen push'lar
yalnızca build ile test edilir.

OTA, 24 Ekim 2026'dan önce indirilen ön sürüm imajlarda yoktur; bu cihazlar bir
kez güncel 1.0.0 imajıyla yeniden yazılmalıdır.
