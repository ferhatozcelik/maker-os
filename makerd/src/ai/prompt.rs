//! Türkçe MakerOS ve Linux asistanı: sistem istemleri, MakerOS bilgi bankası ve doğrulanmış komut kartları.
//!
//! Asistan üç modda doğal olarak yanıt verir:
//! 1. **Sohbet & MakerOS & Genel Konular (`CHAT_SYSTEM`):** Kullanıcı MakerOS özelliklerini, uygulamalarını,
//!    Python/elektronik/yazılım konularını sorduğunda ya da günlük sohbet ettiğinde kalıp komut şablonuna
//!    girmeden doğal, samimi ve bilgilendirici Türkçe yanıt verir.
//! 2. **Linux Komut İstemi (`SYSTEM`):** Kullanıcı sistem durumu, dosya/ağ/servis işlemi veya terminal komutu
//!    istediğinde çalıştırılabilir ```bash bloğu ve açıklamasıyla yanıt verir.
//! 3. **Hata Çözümleme & Olay Özeti:** Hata mesajlarını neden-çözüm olarak açıklar; bildirim sorularında
//!    son olayları özetler.

/// Komut ve hata çözümleme için sistem istemi (Türkçe).
pub const SYSTEM: &str = "Sen MakerOS 2.0'ın yerleşik Türkçe yapay zeka asistanısın; Raspberry Pi 4 üzerinde çalışıyorsun.
MakerOS Hakkında:
- MakerOS; Dosyalar, Terminal, Python Yöneticisi (uv tabanlı sanal ortam ve GPIO/Kamera/Web/AI proje şablonları), AI Ajanları (Claude Code, Codex, Gemini, OpenCode, Ollama), Kamera, GPIO (40-pin kontrolü), Eklentiler, Etkinlik Monitörü, Tarayıcı ve Ayarlar (Wi-Fi, Ağ/Hotspot, Görünüm/Sistem UI gizleme, Otomatik Başlatma & Cron Zamanlayıcı, OTA Güncelleme, Kullanıcı, Güç) uygulamalarını içeren modern bir masaüstü işletim sistemidir.
Kurallar:
- Her zaman Türkçe, net ve yardımcı yanıt ver.
- Komutları asla kendin çalıştırmazsın; ```bash bloğu içinde önerirsin, arayüz çalıştırır.
- Önce salt okunur komutları öner. Silme, biçimlendirme veya izin değişikliği gerekiyorsa açıkça uyar.
- \"Bilgi\" bölümü verilmişse oradaki doğrulanmış bilgileri ve komutları kullan.
Komut istenirse örnekteki biçimi kullan: Komut, Ne yapar, Parametreler, Beklenen çıktı, Olası hatalar, Alternatif, Risk.
Hata mesajı verilirse: **Anlamı**, **Olası nedenler** (madde madde), **Çözüm** (güvenli komutlarla).";

/// Doğal sohbet, MakerOS soruları ve genel konular için sistem istemi.
pub const CHAT_SYSTEM: &str = "Sen MakerOS 2.0'ın yerleşik Türkçe yapay zeka asistanısın; Raspberry Pi 4 üzerinde çalışıyorsun.
MakerOS 2.0 Özellikleri:
- Masaüstü Uygulamaları: Dosyalar (~/Projeler, ~/Belgeler, ~/Resimler, ~/İndirilenler), Terminal (sekmeli kabuk), Python Yöneticisi (uv ile hızlı sanal ortam, paket yönetimi, hazır proje şablonları: LED, Buton+LED, I2C tarayıcı, Kamera fotoğraf/web, Web uygulaması, Nesne tespiti), AI Ajanları (Claude Code, Codex CLI, Gemini CLI, OpenCode, Ollama), Kamera (canlı MJPEG yayın ve fotoğraf çekimi), GPIO (40-pin Raspberry Pi pinlerini canlı izleme ve HIGH/LOW sürme), Eklentiler (.zip eklenti mağazası ve Konya HackerSpace teması), Etkinlik Monitörü (CPU/RAM/sıcaklık ve süreç yönetimi), Tarayıcı (Raspberry Pi internetini kullanan yerleşik web tarayıcısı) ve Ayarlar.
- Ayarlar Sayfaları: Wi-Fi, Ağ (Ethernet, Hotspot erişim noktası, varsayılan yerel IP 10.42.0.0, USB-C bağlantısı 10.55.0.1 / makeros.local), Genel (cihaz adı, OTA yazılım güncellemesi), Tarih ve Saat, Dil ve Klavye, Görünüm (açık/koyu tema, duvar kâğıdı, Üst Menü Çubuğu / Raf / AI Asistan / Sıcaklık / Bildirim gizleme), Otomatik Başlatma (açılışta uygulama/Python projesi/betik çalıştırma ve Cron zamanlayıcı), Kullanıcı ve Parola, Güç (yeniden başlat / kapat).
Kurallar:
- Her zaman doğal, akıcı ve samimi Türkçe ile yanıt ver.
- Kullanıcı MakerOS, yazılım, Python, elektronik, Raspberry Pi veya genel konular hakkında sohbet ediyorsa doğrudan sohbet ederek açıkla; gereksiz yere 'Komut / Parametreler / Risk' şablonu kullanma.
- Eğer sorunun yanıtında bir kod veya komut örneği yararlı olacaksa kısa bir kod bloğu ekleyebilirsin.
- \"Bilgi\" bölümü verilmişse yanıtında o bilgileri esas al.";

/// Sohbet modu için kısa örnek çifti (modelin kalıp komut şablonuna kaymadan doğal konuşmasını sağlar).
pub const CHAT_EXAMPLE_Q: &str = "Merhaba! MakerOS nedir ve neler yapabilirim?";
pub const CHAT_EXAMPLE_A: &str = "Merhaba! **MakerOS**, Raspberry Pi 4 için geliştirilmiş Türkçe masaüstü ve maker işletim sistemidir.\n\nMakerOS ile şunları yapabilirsiniz:\n- **Python Yöneticisi** ile `uv` tabanlı sanal ortamlarda GPIO, kamera, web ve yapay zeka projeleri oluşturup çalıştırabilirsiniz.\n- **GPIO** ve **Kamera** uygulamalarıyla devrelerinizi ve kameranızı doğrudan arayüzden kontrol edebilirsiniz.\n- **AI Ajanları** ile Claude Code, Gemini, Codex, OpenCode veya Ollama'yı tek tıkla başlatabilirsiniz.\n- **Ayarlar › Otomatik Başlatma** bölümünden açılışta veya belirli saatlerde (Cron) çalışacak Python projeleri ve betikler tanımlayabilirsiniz.\n\nBana istediğiniz zaman bir uygulama açtırabilir, sistem komutu çalıştırtabilir ya da Python, elektronik ve genel konularda soru sorabilirsiniz!";

/// Biçim örneği (few-shot): komut istemleri için.
pub const EXAMPLE_Q: &str = "Disk alanını kontrol et.";
pub const EXAMPLE_A: &str = "**Komut:**
```bash
df -h
```
**Ne yapar:** Bağlı disklerin toplam, kullanılan ve boş alanını gösterir.
**Parametreler:** `-h` boyutları okunabilir birimlerle (G, M) yazar.
**Beklenen çıktı:** Her disk için bir satır: Size (toplam), Used (kullanılan), Avail (boş), Use% (doluluk), Mounted on (bağlama yeri).
**Olası hatalar:** Erişilemeyen ağ diskleri varsa komut bekleyebilir; `df -h /` yalnız kök diski gösterir.
**Alternatif:** `lsblk` (diskleri ve bölümleri listeler).
Risk: güvenli, yalnızca okur.";

/// Hata çözümleme örneği (ikinci few-shot çifti).
pub const EXAMPLE2_Q: &str = "Şu hatayı açıkla: cat: /var/log/syslog: Permission denied";
pub const EXAMPLE2_A: &str = "**Anlamı:** `cat` komutu /var/log/syslog dosyasını okuyamadı; kullanıcınızın bu dosyayı okuma izni yok.
**Olası nedenler:**
- Dosya yalnız root ya da adm grubu tarafından okunabilir.
- Kullanıcınız adm grubunda değil.
**Çözüm:**
```bash
ls -l /var/log/syslog
```
Dosyanın sahibini ve izinlerini gösterir (güvenli, yalnızca okur). Okumak için yönetici yetkisi gerekir:
```bash
sudo cat /var/log/syslog
```
Risk: dikkat, sudo yönetici yetkisi kullanır. İzinleri chmod 777 ile herkese açmayın.";

/// Doğrulanmış bilgi/komut kartı: anahtar sözcükler (küçük harf, Türkçe) ve modele verilecek kısa bilgi.
struct Card {
    keys: &'static [&'static str],
    text: &'static str,
}

/// MakerOS'a özgü uygulama, özellik ve kullanım kartları.
const MAKEROS_CARDS: &[Card] = &[
    Card {
        keys: &["makeros", "maker os", "işletim sistemi", "masaüstü", "özellik", "neler var", "ne işe yarar"],
        text: "MakerOS 2.0: Raspberry Pi 4 üzerinde çalışan Türkçe masaüstü işletim sistemidir. Yerleşik uygulamaları: Dosyalar, Terminal, Python Yöneticisi (uv tabanlı), AI Ajanları (Claude Code, Codex, Gemini, OpenCode, Ollama), Kamera, GPIO, Eklentiler, Etkinlik Monitörü, Tarayıcı ve Ayarlar. USB-C ile bilgisayara takıldığında 10.55.0.1 (makeros.local) adresinden doğrudan masaüstü açılır.",
    },
    Card {
        keys: &["python yöneticisi", "proje oluştur", "yeni proje", "şablon", "uv ", "pyproject", "sanal ortam", "projeler"],
        text: "MakerOS Python Yöneticisi: Projeler `~/Projeler/<ad>` dizininde `uv` ile sanal ortamda oluşturulur. Hazır şablonlar: Boş Proje, LED Yak-Söndür (GPIO), Buton ve LED, I2C Aygıt Tarayıcı, Kamera Fotoğraf Çekici, Kamera Web Yayını, Web Uygulaması (HTTP sunucu) ve Nesne Tespiti. Paket eklemek için Python Yöneticisi'ndeki Paketler sekmesi veya `uv add <paket>` kullanılır.",
    },
    Card {
        keys: &["otomatik başlat", "başlangıç", "cron", "zamanlayıcı", "zamanlanmış", "açılışta", "autostart"],
        text: "MakerOS Otomatik Başlatma & Zamanlayıcı (Ayarlar › Otomatik Başlatma): 1) Açılışta masaüstü uygulaması başlatma (tam ekran seçeneğiyle), 2) Python projelerinin sistem açılışında otomatik çalışması, 3) Sistem açılışında (@reboot) veya belirli zamanlarda (5 alanlı Cron ifadesiyle, ör. `*/5 * * * *` veya `0 9 * * *`) Python projesi ya da kabuk/Python betiği çalıştırma.",
    },
    Card {
        keys: &["gpio", "pin", "led", "buton", "sensör", "i2c", "spi", "pwm", "donanım"],
        text: "MakerOS GPIO Uygulaması: Raspberry Pi 40-pin başlığını canlı gösterir; pinleri giriş/çıkış olarak ayarlayıp HIGH (3.3V) veya LOW (0V) sürebilirsiniz. Python'da `gpiod` kütüphanesi (ör. GPIO 17 = Fiziksel Pin 11) kullanılır. I2C aygıtlarını taramak için `i2cdetect -y 1`, pin durumu için `gpioinfo` veya `pinctrl` kullanılabilir.",
    },
    Card {
        keys: &["kamera", "fotoğraf", "mjpeg", "video", "rpicam", "picamera", "csi"],
        text: "MakerOS Kamera Uygulaması: Bağlı CSI veya USB kameradan canlı MJPEG görüntü akışı sağlar ve tek tıkla fotoğraf çeker. Çekilen fotoğraflar `~/Resimler/Kamera/` klasörüne tarih-saat adıyla kaydedilir. Asistana 'Fotoğraf çek' yazarak da doğrudan fotoğraf çekebilirsiniz.",
    },
    Card {
        keys: &["ajan", "claude", "codex", "gemini", "opencode", "ollama", "yapay zeka"],
        text: "MakerOS AI Ajanları: Claude Code, Codex CLI, Gemini CLI, OpenCode ve Ollama desteklenir. AI Ajanları uygulamasından veya asistana 'Claude Code'u aç' yazarak kurabilir ve doğrudan Terminal sekmesinde başlatabilirsiniz.",
    },
    Card {
        keys: &["eklenti", "plugin", "mağaza", "khs", "hackerspace", "konya"],
        text: "MakerOS Eklentiler: `~/.local/share/makeros/plugins/<id>` altında çalışan `.zip` paketleridir (`plugin.json` + `main.py`). Web arayüzü, masaüstü widget'ı, menü çubuğu durumu ve özel tema getirebilir (ör. Konya HackerSpace eklentisi kapı durumunu ve temasını sağlar).",
    },
    Card {
        keys: &["görünüm", "tema", "duvar kağıdı", "duvar kâğıdı", "üst çubuk", "menü çubuğu", "raf", "dock", "gizle"],
        text: "MakerOS Görünüm Ayarları (Ayarlar › Görünüm): Açık/Koyu mod, vurgu rengi, tema ve duvar kâğıdı (masaüstü ve kilit ekranı ortak) değiştirilebilir. 'Sistem Arayüzü (UI) Öğeleri' bölümünden Üst Menü Çubuğu, Uygulama Rafı (Dock), Masaüstü AI Asistan Widget'ı, Tema Widget'ı, CPU Sıcaklığı ve Bildirim Balonları ayrı ayrı gizlenip gösterilebilir.",
    },
    Card {
        keys: &["tarayıcı", "browser", "chromium", "internet sitesi", "web sitesi"],
        text: "MakerOS Tarayıcı: Tüm web sayfalarını, alt kaynakları (resim, CSS, JS, form) ve aramaları host bilgisayarın internetini kullanmadan doğrudan Raspberry Pi'nin internet bağlantısı (`/api/v1/browse`) üzerinden MakerOS penceresi içinde açar.",
    },
    Card {
        keys: &["güncelleme", "ota", "yeni sürüm", "rauc", "sistem güncelle"],
        text: "MakerOS Yazılım Güncellemesi (Ayarlar › Genel): A/B çift bölümlü RAUC güncelleme yapısıyla GitHub Releases üzerinden yeni sürümü denetler, indirir ve güvenli şekilde uygular.",
    },
];

const CARDS: &[Card] = &[
    Card { keys: &["sıcaklı", "sicakli", "ısı", "derece", "temperature", "ısın", "soğu"], text: "CPU sıcaklığı: `cat /sys/class/thermal/thermal_zone0/temp` (milli-°C; 1000'e bölün, ör. 48312 = 48,3 °C). Raspberry Pi OS'ta alternatif: `vcgencmd measure_temp`." },
    Card { keys: &["ram", "bellek", "hafıza", "memory", "swap", "takas"], text: "RAM kullanımı: `free -h` (-h: okunabilir birimler; 'available' sütunu kullanılabilir bellek). Ayrıntı: `cat /proc/meminfo`." },
    Card { keys: &["disk", "depolama", "alan", "doluluk", "boş yer", "kart", "storage"], text: "Disk alanı: `df -h` (-h: GB/MB). Diskler ve bölümler: `lsblk`." },
    Card { keys: &["klasör", "dizin", "boyut", "büyüklü", "kaplıyor", "folder"], text: "Klasör boyutu: `du -sh <klasör>` (-s: toplam, -h: okunabilir). Alt klasörler: `du -h --max-depth=1 <klasör> | sort -h`." },
    Card { keys: &["servis", "hizmet", "service", "daemon"], text: "Çalışan servisler: `systemctl list-units --type=service --state=running`. Hatalı servisler: `systemctl --failed`. Bir servisin durumu: `systemctl status <servis>`." },
    Card { keys: &["log", "logla", "günlük", "günlü", "kayıt", "journal", "servis"], text: "Servis logları: `journalctl -u <servis> -n 50 --no-pager` (-u: servis, -n: son satır sayısı). Canlı izleme: `journalctl -u <servis> -f`." },
    Card { keys: &["ip", "adres", "ağ", "network", "arayüz"], text: "IP adresi: `ip -brief address` (arayüz başına IP). Kısa: `hostname -I`. Varsayılan ağ geçidi: `ip route`." },
    Card { keys: &["internet", "bağlantı", "ping", "çevrimiçi", "online", "dns"], text: "İnternet bağlantısı: `ping -c 4 1.1.1.1` (-c 4: 4 paket). DNS de denemek için: `ping -c 4 google.com`. DNS çalışmıyorsa ilki başarılı, ikincisi 'Temporary failure in name resolution' verir." },
    Card { keys: &["hata", "error", "sorun", "arıza", "problem"], text: "Son sistem hataları: `journalctl -p err -b -n 50 --no-pager` (-p err: yalnız hatalar, -b: bu açılıştan beri). Çekirdek: `dmesg --level=err,warn | tail -n 30`." },
    Card { keys: &["işlem", "process", "program", "uygulama"], text: "Çalışan işlemler: `ps aux --sort=-%cpu | head -n 15` (en çok CPU kullananlar). Anlık: `top -b -n 1 | head -n 20`." },
    Card { keys: &["cpu", "işlemci", "yük", "load"], text: "CPU yükü: `uptime` (load average: 1, 5, 15 dk; 4 çekirdekte 4,0 tam yük). İşlemci bilgisi: `lscpu`. Anlık kullanım: `top -b -n 1 | head -n 5`." },
    Card { keys: &["durum", "rapor", "özet", "sağlık", "status"], text: "Sistem durumu raporu: `uptime; free -h; df -h /; cat /sys/class/thermal/thermal_zone0/temp`. Raspberry Pi OS'ta düşük voltaj/ısınma kısıtlaması: `vcgencmd get_throttled` (0x0 = sorun yok)." },
    Card { keys: &["port", "dinle", "listen", "soket"], text: "Dinleyen portlar: `ss -tulpn` (t: TCP, u: UDP, l: dinleyen, p: program, n: sayısal). Program adları için sudo gerekebilir." },
    Card { keys: &["çekirdek", "kernel", "sürüm", "versiyon", "işletim"], text: "Çekirdek sürümü: `uname -r`; tümü: `uname -a`. İşletim sistemi: `cat /etc/os-release`." },
    Card { keys: &["usb", "aygıt", "cihaz", "takılı"], text: "USB aygıtları: `lsusb`. Takıldığında çekirdek mesajları: `dmesg | tail -n 20`." },
    Card { keys: &["wifi", "wi-fi", "kablosuz", "wlan"], text: "Wi-Fi ağları: `nmcli device wifi list`. Bağlı ağ ve sinyal: `nmcli -f IN-USE,SSID,SIGNAL device wifi`." },
    Card { keys: &["bul", "ara", "find", "nerede"], text: "Dosya bulma: `find <dizin> -name \"<desen>\"` (ör. `find ~ -name \"*.py\"`). İçerikte arama: `grep -rn \"metin\" <dizin>`." },
    Card { keys: &["sil", "kaldır", "temizle", "delete", "remove"], text: "Dosya silme: `rm <dosya>` TEHLİKELİ: geri alınamaz, çöp kutusu yok. Önce `ls <dosya>` ile kontrol edin; sorarak silmek için `rm -i <dosya>`. `rm -rf` asla / veya ~ ile kullanılmamalı." },
    Card { keys: &["yeniden başlat", "restart", "başlat", "durdur", "stop"], text: "Servisi yeniden başlatma: `sudo systemctl restart <servis>` (sistemi değiştirir, sudo ister). Sonra durum: `systemctl status <servis>`." },
    Card { keys: &["paket", "kur", "yükle", "install", "apt"], text: "Raspberry Pi OS'ta paket kurma: `sudo apt update` sonra `sudo apt install <paket>` (internet ve sudo ister). MakerOS'ta apt yoktur: Python paketleri proje içinde `uv add <paket>` ile kurulur." },
    Card { keys: &["açık kal", "uptime", "ne zamandır", "çalışma süresi"], text: "Açık kalma süresi: `uptime -p`. Son açılış zamanı: `uptime -s`." },
    Card { keys: &["kullanıcı", "user", "oturum", "kim"], text: "Oturum açan kullanıcılar: `who`. Kendi kullanıcınız ve grupları: `id`." },
    Card { keys: &["voltaj", "voltage", "güç", "adaptör", "throttl", "under-voltage"], text: "'Under-voltage detected' güç kaynağının yetersiz olduğunu gösterir (Pi 4 için 5,1 V / 3 A önerilir). Kontrol: `dmesg | grep -i voltage`; Raspberry Pi OS'ta `vcgencmd get_throttled`." },
    // Hata mesajları
    Card { keys: &["permission denied", "izin reddedildi", "erişim engellendi"], text: "'Permission denied': dosyaya/aygıta yetkiniz yok. Sahipliği görmek: `ls -l <dosya>`; grubunuz: `id`. Gerekirse komutu `sudo` ile çalıştırın (yetkiyi körü körüne chmod 777 ile herkese açmayın: TEHLİKELİ)." },
    Card { keys: &["no space left", "yer kalmadı", "disk dolu"], text: "'No space left on device': disk dolu. Kontrol: `df -h`; büyük klasörler: `sudo du -h --max-depth=1 / 2>/dev/null | sort -h | tail`; günlük boyutu: `journalctl --disk-usage`." },
    Card { keys: &["command not found", "komut bulunamadı", "bulunamadı"], text: "'command not found': program kurulu değil ya da PATH'te değil. Kontrol: `which <komut>`; PATH: `echo $PATH`." },
    Card { keys: &["could not resolve", "name resolution", "temporary failure"], text: "'Temporary failure in name resolution / Could not resolve host': DNS çalışmıyor ya da internet yok. Kontrol: `ping -c 3 1.1.1.1` (IP ile), `resolvectl status`." },
    Card { keys: &["failed to start", "başlatılamadı", "failed with result", "exit-code"], text: "'Failed to start <servis>': servis açılışta hata verdi. Neden: `systemctl status <servis>` ve `journalctl -xeu <servis> --no-pager | tail -n 40`." },
    Card { keys: &["dpkg was interrupted", "configure -a"], text: "'dpkg was interrupted': yarım kalan kurulum. Çözüm: `sudo dpkg --configure -a` (paket yapılandırmasını tamamlar)." },
    Card { keys: &["could not get lock", "lock-frontend", "kilit"], text: "'Could not get lock /var/lib/dpkg/lock': başka bir apt işlemi çalışıyor. Kontrol: `ps aux | grep -i apt`; bitmesini bekleyin (kilit dosyasını silmeyin)." },
    Card { keys: &["read-only file system", "salt okunur"], text: "'Read-only file system': dosya sistemi salt okunur bağlı (MakerOS'ta / bilerek salt okunurdur; yazılabilir yerler /home, /data, /var). Bağlama seçenekleri: `findmnt -o TARGET,OPTIONS /`. SD kart hatası da olabilir: `dmesg | grep -i -E 'mmc|ext4'`." },
    Card { keys: &["out of memory", "oom", "killed process"], text: "'Out of memory: Killed process': RAM bitti, çekirdek işlemi sonlandırdı. Kontrol: `free -h`, `journalctl -k | grep -i oom`; en çok bellek kullananlar: `ps aux --sort=-%mem | head`." },
    Card { keys: &["connection refused", "bağlantı reddedildi"], text: "'Connection refused': hedef portta dinleyen program yok. Kontrol: `ss -tlnp | grep <port>` ve servis durumu `systemctl status <servis>`." },
    Card { keys: &["segmentation fault", "segfault", "core dumped"], text: "'Segmentation fault': program geçersiz belleğe erişip çöktü (program hatası ya da bozuk kütüphane). Ayrıntı: `dmesg | tail`, `coredumpctl list`." },
    Card { keys: &["ext4-fs error", "i/o error", "mmc0", "mmcblk", "input/output error"], text: "'EXT4-fs error / I/O error / mmc0 timeout': SD kart ya da disk sorunu (bozulma, gevşek bağlantı, yetersiz güç). Kontrol: `dmesg | grep -i -E 'mmc|ext4|i/o'`. Önemli verileri yedekleyin." },
    Card { keys: &["unable to locate package", "paket bulunamadı"], text: "'Unable to locate package': paket listesi eski ya da ad yanlış. Önce `sudo apt update`, sonra `apt search <ad>`." },
    Card { keys: &["no such file", "böyle bir dosya", "dosya yok"], text: "'No such file or directory': yol yanlış ya da dosya yok. Kontrol: `ls -l <yol>`, bulunduğunuz yer: `pwd`." },
];

/// Türkçe küçük harf (İ→i, I→ı) ve ASCII olmayan büyük harfler.
pub fn lower_tr(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'İ' => o.push('i'),
            'I' => o.push('ı'),
            _ => o.extend(c.to_lowercase()),
        }
    }
    o.replace("i̇", "i")
}

fn score_cards(pool: &[Card], question: &str, max: usize) -> Vec<&'static str> {
    let q = lower_tr(question);
    let words: Vec<&str> = q.split(|c: char| !c.is_alphanumeric() && c != '-').filter(|w| !w.is_empty()).collect();
    let hit = |k: &str| if k.chars().count() <= 3 { words.iter().any(|w| *w == k) } else { q.contains(k) };
    let mut scored: Vec<(usize, usize, &'static str)> = pool
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let n: usize = c.keys.iter().map(|k| if hit(k) { k.chars().count() } else { 0 }).sum();
            (n > 0).then_some((n, i, c.text))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(max).map(|x| x.2).collect()
}

/// Soruyla en ilgili en fazla `max` Linux komut kartı.
pub fn cards_for(question: &str, max: usize) -> Vec<&'static str> {
    score_cards(CARDS, question, max)
}

/// Soruyla en ilgili en fazla `max` MakerOS bilgi kartı.
pub fn makeros_cards_for(question: &str, max: usize) -> Vec<&'static str> {
    score_cards(MAKEROS_CARDS, question, max)
}

/// Soru son olaylarla/bildirimlerle mi ilgili?
pub fn wants_events(question: &str) -> bool {
    let q = lower_tr(question);
    ["bildirim", "olay", "ne oldu", "neler oldu", "son durum", "uyarı", "haber", "ne değişti", "son gelen"].iter().any(|k| q.contains(k))
}

/// Olay (bildirim) soruları için kısa sistem istemi: komut biçimi örnekleri kullanılmaz.
pub const EVENTS_SYSTEM: &str = "Sen MakerOS'un Türkçe asistanısın. Kullanıcıya son sistem olaylarını (bildirimleri) Türkçe, kısa ve madde madde özetle. Önemli olanı başa yaz. Komut önerme.";

/// Olay dosyasının son `n` satırı bağlam olarak (yoksa boş).
pub fn events_context(lines: &[String], n: usize) -> String {
    if lines.is_empty() {
        return "\n\nSon olaylar: (bildirim yok)\n".into();
    }
    let mut s = String::from("\n\nSon olaylar (MakerOS bildirimleri, eskiden yeniye):\n");
    for l in lines.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev() {
        s.push_str("- ");
        s.push_str(l);
        s.push('\n');
    }
    s
}

/// Soru bir hata mesajı mı (açıklama biçimi) yoksa komut/sohbet isteği mi?
pub fn is_error_report(question: &str) -> bool {
    let q = lower_tr(question);
    const ERR: [&str; 22] = ["hata", "error", "failed", "denied", "not found", "cannot", "can't", "could not", "unable", "no such", "refused", "segmentation", "killed", "timeout", "warning:", "e: ", "fatal", "bulunamadı", "başarısız", "reddedildi", "çöktü", "invalid"];
    ERR.iter().any(|k| q.contains(k)) && !q.starts_with("son sistem hata") && !q.contains("hataları göster") && !q.contains("hatalarını göster")
}

/// Soru doğrudan bir Linux sistem komutu / sistem ölçümü mü istiyor, yoksa sohbet / MakerOS / genel soru mu?
pub fn wants_command(question: &str) -> bool {
    if is_error_report(question) || wants_events(question) {
        return false;
    }
    let q = lower_tr(question);
    // Selamlaşma, genel sohbet veya "MakerOS nedir / nasıl kullanılır" tarzı kavramsal sorular sohbet modunda yanıtlanır.
    let chat_markers = [
        "merhaba", "selam", "nasılsın", "nasilsin", "kimsin", "sen kimsin", "ne yapabilirsin", "neler yapabilirsin",
        "makeros nedir", "maker os nedir", "nedir", "ne işe yarar", "anlat", "hakkında bilgi", "farkı nedir",
        "nasıl yazılır", "örnek kod", "python'da", "pythonda", "teşekkür", "sağ ol", "iyi günler", "günaydın",
    ];
    if chat_markers.iter().any(|k| q.contains(k)) && !q.contains("komut") {
        return false;
    }
    // MakerOS'a özgü kavramsal kart eşleşiyorsa ve açıkça terminal/bash komutu istenmiyorsa sohbet modunu tercih et.
    if !makeros_cards_for(question, 1).is_empty() && !q.contains("komut") && !q.contains("terminal") {
        let sys_check = ["sıcaklı", "ram ", "disk", "ip adres", "ping", "journalctl", "systemctl", "df ", "free "];
        if !sys_check.iter().any(|k| q.contains(k)) {
            return false;
        }
    }
    // Açık komut isteği ya da sistem ölçüm kartı eşleşmesi:
    let cmd_verbs = ["komut", "kontrol et", "listele", "göster", "öğren", "raporla", "ölç", "boyutunu", "loglarını", "hatalarını"];
    !cards_for(question, 1).is_empty() || cmd_verbs.iter().any(|k| q.contains(k))
}

/// Son kullanıcı iletisine eklenen biçim ipucu ve bilgi bölümü.
pub fn knowledge(question: &str) -> String {
    if is_error_report(question) {
        let mut s = String::from("\n\n(Hata açıklaması biçiminde yanıtla: Anlamı, Olası nedenler, Çözüm.)");
        for t in cards_for(question, 1) {
            s.push_str("\nBilgi (doğrulanmış, bunu kullan):\n- ");
            s.push_str(t);
            s.push('\n');
        }
        return s;
    }
    if wants_command(question) {
        let mut s = String::from("\n\n(Komut biçiminde yanıtla: Komut, Ne yapar, Parametreler, Beklenen çıktı, Olası hatalar, Alternatif, Risk.)");
        for t in cards_for(question, 1) {
            s.push_str("\nBilgi (doğrulanmış, bunu kullan):\n- ");
            s.push_str(t);
            s.push('\n');
        }
        return s;
    }
    // Doğal sohbet / MakerOS / genel konu modu:
    let mut s = String::new();
    let mk = makeros_cards_for(question, 2);
    let lx = cards_for(question, 1);
    if !mk.is_empty() || !lx.is_empty() {
        s.push_str("\n\nBilgi (MakerOS bağlamı, yanıtında kullan):\n");
        for t in mk.into_iter().chain(lx.into_iter()) {
            s.push_str("- ");
            s.push_str(t);
            s.push('\n');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower() {
        assert_eq!(lower_tr("IP ADRESİMİ"), "ıp adresimi");
        assert_eq!(lower_tr("İnternet"), "internet");
    }

    #[test]
    fn picks_cards() {
        let first = |q: &str| cards_for(q, 2).first().copied().unwrap_or("");
        assert!(first("CPU sıcaklığını göster").contains("thermal_zone0"));
        assert!(first("RAM kullanımını kontrol et").contains("free -h"));
        assert!(first("Disk alanını kontrol et").contains("df -h"));
        assert!(first("Çalışan servisleri listele").contains("list-units"));
        assert!(first("ssh servisinin loglarını göster").contains("journalctl -u"));
        assert!(first("IP adresimi öğren").contains("ip -brief"));
        assert!(first("İnternet bağlantısını kontrol et").contains("ping"));
        assert!(first("Son sistem hatalarını göster").contains("-p err"));
        assert!(first("/var/log klasörünün boyutunu öğren").contains("du -sh"));
        assert!(first("Çalışan işlemleri listele").contains("ps aux"));
        assert!(cards_for("Raspberry Pi sistem durumunu raporla", 2).iter().any(|c| c.contains("uptime; free")));
        assert!(first("bash: htop: command not found").contains("command not found"));
        assert!(first("E: Could not get lock /var/lib/dpkg/lock-frontend").contains("lock"));
        assert!(cards_for("merhaba nasılsın", 2).is_empty());
        assert!(cards_for("bir ipucu ver", 2).is_empty()); // "ip" tam sözcük değil
    }

    #[test]
    fn makeros_chat_and_cards() {
        assert!(!wants_command("Merhaba nasılsın?"));
        assert!(!wants_command("MakerOS nedir ve neler yapabilirim?"));
        assert!(!wants_command("Otomatik başlatma ve cron nasıl ayarlanır?"));
        assert!(wants_command("Disk alanını kontrol et."));
        assert!(wants_command("CPU sıcaklığını göster."));
        assert!(!makeros_cards_for("MakerOS nedir?", 1).is_empty());
        assert!(!makeros_cards_for("Otomatik başlatma cron görevi", 1).is_empty());
    }

    #[test]
    fn events() {
        assert!(wants_events("Son bildirimler neler?"));
        assert!(wants_events("Bugün neler oldu"));
        assert!(!wants_events("Disk alanını kontrol et"));
        let l = vec!["05:40 camera: Fotoğraf".to_string(), "05:41 khs: Kapı açıldı".to_string(), "05:42 ota: Güncelleme".to_string()];
        let c = events_context(&l, 2);
        assert!(c.contains("Kapı açıldı") && c.contains("Güncelleme") && !c.contains("Fotoğraf"));
    }

    #[test]
    fn error_or_command() {
        assert!(is_error_report("Şu hatayı açıkla: bash: htop: command not found"));
        assert!(is_error_report("E: Could not get lock /var/lib/dpkg/lock-frontend"));
        assert!(is_error_report("Failed to start nginx.service"));
        assert!(!is_error_report("CPU sıcaklığını göster."));
        assert!(!is_error_report("Son sistem hatalarını göster."));
        assert!(!is_error_report("IP adresimi öğren."));
    }

    #[test]
    fn cards_are_safe_or_marked() {
        // Kartlardaki her komut sınıflandırılabilir; tehlikeli olanlar kartta "TEHLİKELİ" diye işaretli.
        for c in CARDS {
            for cmd in crate::ai::risk::extract_commands(c.text) {
                let a = crate::ai::risk::classify(&cmd.replace(['<', '>'], ""));
                if a.risk == crate::ai::risk::Risk::Dangerous {
                    assert!(c.text.contains("TEHLİKELİ"), "{cmd}");
                }
            }
        }
    }
}
