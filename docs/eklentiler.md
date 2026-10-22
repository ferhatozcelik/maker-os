# Python eklentileri

Merkez → **Eklentiler**, yerel Python projelerini uygulama gibi listeler.
Her uygulama ayrı bir systemd hizmetinde `pardus` kullanıcısıyla çalışır.
Merkez sayfasını kapatmak uygulamayı durdurmaz. **Durdur**, uygulamayı ve
alt süreçlerini kapatır; **Günlük** son 200 satırı gösterir. Web arayüzü
olan uygulamalarda **Uygulamayı aç** düğmesi bulunur.

## Kendi uygulamanı ekle

1. **Python şablonunu indir** düğmesinden ZIP'i indir ve aç.
2. `main.py` yerine kendi kodunu yaz. Yardımcı `.py` dosyaları ve veri
   klasörleri ekleyebilirsin. Bağımlılıkları `pyproject.toml` dosyasına yaz.
3. `plugin.json` içindeki uygulama bilgilerini düzenle:

```json
{
  "id": "benim-eklentim",
  "name": "Benim Eklentim",
  "version": "1.0.0",
  "description": "Uygulamamın kısa açıklaması",
  "entrypoint": "main.py",
  "args": [],
  "web_port": 8765
}
```

4. `plugin.json`, `pyproject.toml`, `main.py` ve diğer dosyalar ZIP'in
   **kökünde** olacak şekilde sıkıştır. Üstte ek bir proje klasörü bulunmasın.
   `.venv`, `.git` ve `__pycache__` klasörlerini ekleme.
5. **ZIP içe aktar** ile yükle; listeden **Çalıştır** düğmesine bas.

İlk çalıştırmada `uv run` proje ortamını ve bağımlılıklarını hazırlar;
önbellekte bulunmayan paketler için internet gerekir. Örnek şablon Python
3.12'yi seçer; kendi projenin sürümünü `pyproject.toml` ile belirleyebilirsin.
`uv.lock` dosyası eklenebilir. Proje dosyaları `/var/lib/pios/plugins/<id>`
altındadır ve `pardus` tarafından düzenlenebilir.

`id`, küçük harfle başlar; en fazla 40 küçük harf, rakam veya tire içerir.
`entrypoint`, proje içindeki bir `.py` dosyasıdır. `args`, isteğe bağlı komut
satırı argümanları listesidir. Web uygulaması yoksa `web_port` alanını
kaldır. Varsa 1024–65535 arasında farklı ve boş bir port seç; örnek kod
`PIOS_PLUGIN_PORT` ortam değişkenini okuyarak bu portu kullanır.
Uygulama web sunucusu `0.0.0.0` adresinde dinlerse telefondan erişilebilir.
Etkileşimli terminal girdisi isteyen kod yerine web, masaüstü veya arka
plan akışı kullan.

ZIP en fazla 15 MB, açılmış içerik en fazla 64 MB ve 500 giriş olabilir.
İçe aktarmada dosya yolları ve sembolik bağlantılar kontrol edilir.
Aynı kimlikle içe aktarma mevcut kodu ezmez. Yeni sürüm yüklemek için önce
durdurup **Kaldır** ile listeden çıkar; eski klasör `.silinenler` altında
saklanır. Ardından yeni ZIP'i içe aktar. Kaldırılan uygulamanın bu klasör
dışına yazdığı kişisel dosyalar korunur.

## MediaPipe projesini uygulamaya çevir

Merkez'de bir MediaPipe projesi oluştur. Projedeki `main.py`,
`pios_kamera.py`, `pyproject.toml`, varsa `uv.lock` ve `models/` klasörünün
yanına `plugin.json` ekle. Örneğin:

```json
{
  "id": "el-takibi",
  "name": "El Takibi",
  "version": "1.0.0",
  "description": "Kameradan el analizi",
  "entrypoint": "main.py",
  "args": ["--profil", "tasarruf", "--pencere-yok"],
  "web_port": 8001
}
```

Yeni MediaPipe yardımcısı `PIOS_PLUGIN_PORT` değerini kullanır. Kamera ve
GPIO aynı anda birden çok uygulama tarafından kullanılamayabilir; ilgili
diğer uygulamayı durdur. Büyük modeller ZIP sınırına sığmıyorsa projenin
Pi'deki `/usr/share/pios/mediapipe/` modellerini okumasını sağlayabilirsin.
Resim analizi için `args` listesine `--kaynak`, resim yolu ve `--cikti`,
`/home/pardus/analiz-sonuclari` eklenebilir. Tek resim veya klasör analizi
tamamlandığında uygulama normal şekilde kapanır.

Uygulamalar root olarak çalışmaz; normal kullanıcının dosya ve donanım
izinleri geçerlidir. Yeniden başlatmada otomatik başlatılmazlar; Merkez'den
istenildiğinde çalıştırılırlar. Başarısız başlangıçta çıkış koduna ve
günlüğe bak. Eklentiler Merkez güncellemesi sırasında kapatılmaz.
