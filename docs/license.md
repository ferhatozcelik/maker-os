# Lisans ve dağıtım notları

Bu belge projenin lisans kapsamını ve marka atıflarını açıklar. Bir izin
belgesi, hukuk görüşü veya bütün bileşenler için tamamlanmış bir lisans
uyumluluğu denetimi değildir. Esas olan ilgili lisans metinleri ve yürürlükteki
hukuktur; gerektiğinde somut dağıtım için uzman görüşü alınmalıdır.

## Projenin kendi kaynakları

Pardus Maker için geliştirilen özgün kaynaklar `GPL-3.0-or-later` kapsamındadır;
tam metin [LICENSE](../LICENSE) dosyasındadır. Başka bir projeden alınan
kodun mevcut telif ve lisans bildirimleri korunur. Bu deponun lisansı üçüncü
taraf kodun lisansını değiştirmez.

## İmaj ve kaynak kodu

İmaj birbirinden bağımsız birçok bileşen içerir. [NOTICE.md](../NOTICE.md)
bir başlangıç envanteridir; paketlerin kurulu lisans metinleri de
`/usr/share/doc/<paket>/copyright` altında bulunur. GPL kapsamındaki ikili
bileşenlerin dağıtımında, ilgili lisansın kabul ettiği yöntemle o sürüme
karşılık gelen kaynak sağlanmalıdır. Buna uygulanan yamalar ve gerekli
derleme/kurulum betikleri de dahil olabilir. Projenin Git deposu bütün
üçüncü taraf kaynaklarını içermez; yalnızca üst proje ana sayfalarını
listelemek kaynak sağlama koşullarını kendiliğinden karşılamaz.

Bu açıklama GPL metninin yerine geçmez. Dağıtım biçimi için özellikle
[GPL v3 bölüm 6](https://www.gnu.org/licenses/gpl-3.0.html#section6) ve
[GNU lisans SSS](https://www.gnu.org/licenses/gpl-faq.en.html) incelenmelidir.
Linux çekirdeği gibi farklı lisans sürümü kullanan bileşenlerin kendi
koşulları ayrıca geçerlidir.

## Ayrı koşulları olan bileşenler

Firmware ve modellerin yeniden dağıtım koşulları kendi lisans dosyalarından
kontrol edilmelidir. Açık kaynak olmayan bir bileşen, bu imaja eklenmekle
GPL kapsamına girmez.

Özellikle imaja kurulan **Microsoft Visual Studio Code derlemesi**, MIT
lisanslı kaynak deposuyla aynı lisans kapsamında değildir. Yeniden dağıtım
hakları [Microsoft ürün lisansı](https://code.visualstudio.com/license)
üzerinden ayrıca doğrulanmalıdır. Paketin indirilebilir olması, imaj içinde
yeniden dağıtılmasına izin verildiği anlamına gelmez; bu belge böyle bir
iznin alındığını beyan etmez.

## Markalar ve bağımsızlık

Pardus Maker bağımsız bir topluluk çalışmasıdır. TÜBİTAK, Pardus projesi
veya Raspberry Pi Ltd tarafından çıkarılmış ya da onaylanmış bir ürün
olduğu iddia edilmez. Projeye özgü destek, bu deponun
[Issues](https://github.com/ferhatozcelik/pardus-maker/issues) bölümündedir.
Pardus'un resmî projesi için [pardus.org.tr](https://pardus.org.tr/hakkimizda/)
ziyaret edilebilir.

Pardus adı ve görsel kimliğinin hakları ilgili hak sahiplerine aittir.
Raspberry Pi is a trademark of Raspberry Pi Ltd. Donanım adı, hedef platformu
tanımlamak için kullanılır; ilgili
[marka kuralları](https://www.raspberrypi.com/trademark-rules/) ayrıca geçerlidir.
Yazılım lisansı, proje adındaki veya dağıtılan tema ve logolardaki markalar
için kullanım izni vermez. Bağımsızlık açıklaması, gerekli marka izninin
yerini tutmaz; izin gerektiren kullanımlar hak sahibiyle ayrıca çözülmelidir.

## Garanti

Uygulanabilir hukukun izin verdiği ölçüde ilgili lisansların garanti ve
sorumluluk hükümleri geçerlidir. Bu belge kanunun emredici hükümlerini veya
bunlardan doğan hakları bertaraf etmez; her kullanım ya da dağıtım için
hukuki sorunsuzluk taahhüt etmez.
