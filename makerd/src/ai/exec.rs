//! Önerilen komutları kullanıcı izniyle çalıştırma kuralları (web: makerd `POST /api/v1/asistan/calistir`,
//! terminal: `makeros-ai sohbet --mod …`).
//!
//! İzin modları (kullanıcı tercihi `aiMode`):
//! - `oner`     yalnız öneri; hiçbir komut çalıştırılmaz (varsayılan).
//! - `sor`      her komut kullanıcı "Çalıştır" deyince çalışır.
//! - `otomatik` SAFE (salt okunur) komutlar sormadan çalışır.
//! Her modda WARNING ve DANGEROUS komutlar açık onay ister; DANGEROUS onayında komut yeniden yazılır.
//!
//! Komutlar kullanıcının kendi yetkisiyle (root değil), stdin kapalı, 30 sn zaman aşımı ve 64 KB çıktı sınırıyla
//! çalışır. `sudo` parola sormaz (`sudo -n`): parola gerekiyorsa hata verir, kullanıcı Terminal'de çalıştırır.

use super::risk::{self, Assessment, Risk};

pub const TIMEOUT_S: u32 = 30;
pub const MAX_OUTPUT: usize = 64 << 10;
const EXIT_MARK: &str = "\n__MK_CIKIS=";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Suggest,
    Ask,
    Auto,
    Bypass,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s {
            "bypass" | "full" => Mode::Bypass,
            "sor" | "izin" | "ask" => Mode::Ask,
            "otomatik" | "oto" | "auto" => Mode::Auto,
            "oner" | "suggest" => Mode::Suggest,
            _ => Mode::Bypass,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Mode::Suggest => "oner",
            Mode::Ask => "sor",
            Mode::Auto => "otomatik",
            Mode::Bypass => "bypass",
        }
    }
}

/// Karar: komut çalıştırılabilir mi, onay gerekir mi?
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Çalıştır.
    Run,
    /// Kullanıcı onayı gerekiyor (risk düzeyiyle).
    Confirm(Risk),
    /// Çalıştırılamaz (neden).
    Deny(String),
}

/// Etkileşimli ya da sonu gelmeyen komutlar (stdin kapalıyken asılı kalır/anlamsız): Terminal'de çalıştırılmalı.
pub fn interactive(cmd: &str) -> Option<String> {
    // Boru ve ayraçlara göre parçalar; her parçada önekler (sudo, env, nice, time, bayraklar) atlanınca gelen sözcük komuttur.
    for seg in cmd.split(['|', ';', '&', '\n']) {
        let words: Vec<&str> = seg.split_whitespace().collect();
        let mut i = 0;
        while i < words.len() && (matches!(words[i], "sudo" | "doas" | "env" | "nice" | "time" | "nohup" | "stdbuf") || words[i].starts_with('-') || (words[i].contains('=') && i == 0)) {
            i += 1;
        }
        let Some(first) = words.get(i) else { continue };
        let args = &words[i + 1..];
        let b = first.rsplit('/').next().unwrap_or(first);
        let why = match b {
            "nano" | "vim" | "vi" | "emacs" | "micro" | "mc" => Some("düzenleyici açar"),
            "less" | "more" | "man" => Some("sayfalayıcı açar"),
            "htop" | "btop" | "atop" | "nmtui" | "raspi-config" | "alsamixer" | "ssh" | "telnet" | "ftp" | "passwd" | "su" | "screen" | "tmux" => Some("etkileşimli program"),
            "python" | "python3" | "bash" | "sh" | "zsh" | "node" if args.is_empty() => Some("etkileşimli kabuk açar"),
            "top" if !args.iter().any(|w| w.starts_with("-b")) => Some("top etkileşimlidir (top -b -n 1 kullanın)"),
            "watch" => Some("sürekli yeniler"),
            "tail" | "journalctl" if args.iter().any(|w| matches!(*w, "-f" | "--follow" | "-F")) => Some("sürekli izleme (-f) bitmez"),
            "dmesg" if args.iter().any(|w| matches!(*w, "-w" | "--follow")) => Some("sürekli izleme bitmez"),
            _ => None,
        };
        if let Some(why) = why {
            return Some(format!("{b}: {why}; Terminal'de çalıştırın"));
        }
    }
    None
}

/// Mod, risk ve kullanıcı onayına göre karar. `confirmed`: kullanıcı onay düğmesine bastı;
/// `typed`: DANGEROUS için yeniden yazılan komut.
pub fn decide(mode: Mode, a: &Assessment, confirmed: bool, typed: Option<&str>) -> Decision {
    if mode == Mode::Suggest {
        return Decision::Deny("Komut çalıştırma kapalı (izin modu: yalnız öneri). Ayarlar › Görünüm › AI Asistan izin modu.".into());
    }
    if a.command.trim().is_empty() {
        return Decision::Deny("boş komut".into());
    }
    if let Some(why) = interactive(&a.command) {
        return Decision::Deny(why);
    }
    if risk::has_placeholder(&a.command) {
        return Decision::Deny("komutta doldurulmamış yer tutucu var (ör. <servis>); kopyalayıp düzenleyin, Terminal'de çalıştırın".into());
    }
    if mode == Mode::Bypass {
        return Decision::Run;
    }
    match a.risk {
        Risk::Safe if mode == Mode::Auto || confirmed => Decision::Run,
        Risk::Safe => Decision::Confirm(Risk::Safe),
        Risk::Warning if confirmed => Decision::Run,
        Risk::Dangerous if confirmed && (mode == Mode::Ask || typed.map(str::trim) == Some(a.command.trim())) => Decision::Run,
        r => Decision::Confirm(r),
    }
}

/// Çalıştırılacak kabuk satırı: sudo parola sormaz, stdin kapalı, zaman aşımı, çıktı sınırı, çıkış kodu işareti.
pub fn wrap(cmd: &str) -> String {
    let c = sudo_noninteractive(cmd.trim());
    format!(
        "{{ timeout -k 2 {TIMEOUT_S} sh -c {} < /dev/null 2>&1; printf '{}%s\\n' \"$?\"; }} | head -c {}",
        crate::util::shq(&c),
        EXIT_MARK.replace('\n', "\\n"),
        MAX_OUTPUT + 64
    )
}

/// Baştaki ya da boru/; sonrası `sudo` → `sudo -n`.
fn sudo_noninteractive(cmd: &str) -> String {
    let mut out = String::with_capacity(cmd.len() + 8);
    let mut at_start = true;
    let mut rest = cmd;
    while !rest.is_empty() {
        let trimmed = rest.trim_start();
        let lead = &rest[..rest.len() - trimmed.len()];
        out.push_str(lead);
        rest = trimmed;
        if at_start && (rest.starts_with("sudo ") || rest == "sudo") && !rest.starts_with("sudo -n") {
            out.push_str("sudo -n");
            rest = &rest[4..];
        }
        at_start = false;
        let Some(ch) = rest.chars().next() else { break };
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
        if matches!(ch, '|' | ';' | '&' | '(' | '\n') {
            at_start = true;
        }
    }
    out
}

/// Sarılmış komutun çıktısından (metin, çıkış kodu, kesildi mi).
pub fn parse_output(raw: &[u8]) -> (String, Option<i32>, bool) {
    let s = String::from_utf8_lossy(raw);
    if let Some(p) = s.rfind(EXIT_MARK.trim_start_matches('\n')) {
        let code = s[p..].trim_start_matches(EXIT_MARK.trim_start_matches('\n')).trim().parse::<i32>().ok();
        let body = s[..p].strip_suffix('\n').unwrap_or(&s[..p]).to_string();
        let truncated = body.len() > MAX_OUTPUT;
        let body = if truncated { cut(&body, MAX_OUTPUT) } else { body };
        // 124: timeout
        return (body, code, truncated);
    }
    (cut(&s, MAX_OUTPUT), None, true)
}

fn cut(s: &str, n: usize) -> String {
    let mut e = n.min(s.len());
    while !s.is_char_boundary(e) {
        e -= 1;
    }
    s[..e].to_string()
}

/// Sonucu açıklayan kısa Türkçe not (çıkış kodlarına göre).
pub fn note(code: Option<i32>, out: &str) -> Option<String> {
    match code {
        Some(124) | Some(137) => Some(format!("{TIMEOUT_S} sn içinde bitmedi, durduruldu.")),
        Some(_) if out.contains("sudo: a password is required") || out.contains("sudo: a terminal is required") => Some("sudo parola istiyor: bu komutu Terminal'de çalıştırın.".into()),
        Some(127) => Some("Komut bulunamadı (kurulu değil).".into()),
        Some(126) => Some("Komut çalıştırılamadı (izin yok).".into()),
        None => Some("Çıktı çok uzun; kısaltıldı.".into()),
        _ => None,
    }
}

/// Değerlendirme + karar (tek yerde).
pub fn check(mode: Mode, cmd: &str, confirmed: bool, typed: Option<&str>) -> (Assessment, Decision) {
    let a = risk::classify(cmd);
    let d = decide(mode, &a, confirmed, typed);
    (a, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions() {
        let c = |m, cmd: &str, ok, t: Option<&str>| check(m, cmd, ok, t).1;
        // yalnız öneri: hiçbir şey
        assert!(matches!(c(Mode::Suggest, "df -h", true, None), Decision::Deny(_)));
        // full bypass: her komutu sormadan çalıştırır
        assert_eq!(c(Mode::Bypass, "df -h", false, None), Decision::Run);
        assert_eq!(c(Mode::Bypass, "sudo systemctl restart ssh", false, None), Decision::Run);
        assert_eq!(c(Mode::Bypass, "rm -rf /tmp/x", false, None), Decision::Run);
        // izin iste: güvenli komut da onay ister, onaylanınca çalışır
        assert_eq!(c(Mode::Ask, "df -h", false, None), Decision::Confirm(Risk::Safe));
        assert_eq!(c(Mode::Ask, "df -h", true, None), Decision::Run);
        assert_eq!(c(Mode::Ask, "rm -rf /tmp/x", true, None), Decision::Run);
        // otomatik: yalnız SAFE sormadan
        assert_eq!(c(Mode::Auto, "free -h", false, None), Decision::Run);
        assert_eq!(c(Mode::Auto, "sudo systemctl restart ssh", false, None), Decision::Confirm(Risk::Warning));
        assert_eq!(c(Mode::Auto, "sudo systemctl restart ssh", true, None), Decision::Run);
        // tehlikeli: onay + komutun yeniden yazılması
        assert_eq!(c(Mode::Auto, "rm -rf /tmp/x", true, None), Decision::Confirm(Risk::Dangerous));
        assert_eq!(c(Mode::Auto, "rm -rf /tmp/x", true, Some("rm -rf /tmp/y")), Decision::Confirm(Risk::Dangerous));
        assert_eq!(c(Mode::Auto, "rm -rf /tmp/x", true, Some(" rm -rf /tmp/x ")), Decision::Run);
        // yer tutucu
        assert!(matches!(c(Mode::Auto, "journalctl -u <servis> -n 50", true, None), Decision::Deny(_)));
        assert_eq!(c(Mode::Auto, "cat < /etc/hostname", false, None), Decision::Run);
        // etkileşimli
        for i in ["nano /etc/hosts", "top", "tail -f /var/log/syslog", "journalctl -u ssh -f", "htop", "sudo nano x", "watch free -h", "less a.txt"] {
            assert!(matches!(c(Mode::Auto, i, true, Some(i)), Decision::Deny(_)), "{i}");
        }
        for ok in ["top -b -n 1 | head -n 20", "journalctl -u ssh -n 50 --no-pager", "ps aux | less"] {
            assert!(!matches!(c(Mode::Auto, ok, true, Some(ok)), Decision::Deny(_)) || ok.contains("less"), "{ok}");
        }
    }

    #[test]
    fn wrapping() {
        assert_eq!(sudo_noninteractive("sudo systemctl restart ssh"), "sudo -n systemctl restart ssh");
        assert_eq!(sudo_noninteractive("echo 1 | sudo tee /x"), "echo 1 | sudo -n tee /x");
        assert_eq!(sudo_noninteractive("sudo -n ls"), "sudo -n ls");
        assert_eq!(sudo_noninteractive("pseudo x; sudo ls"), "pseudo x; sudo -n ls");
        let w = wrap("df -h");
        assert!(w.contains("timeout -k 2 30 sh -c 'df -h' < /dev/null 2>&1"), "{w}");
        let (o, code, tr) = parse_output(b"Filesystem Size\n/dev/root 7G\n__MK_CIKIS=0\n");
        assert_eq!((o.as_str(), code, tr), ("Filesystem Size\n/dev/root 7G", Some(0), false));
        let (_, code, _) = parse_output(b"x\n__MK_CIKIS=124\n");
        assert_eq!(code, Some(124));
        assert_eq!(note(Some(124), ""), Some("30 sn içinde bitmedi, durduruldu.".into()));
        let (_, code, tr) = parse_output(b"cok uzun ve isaretsiz");
        assert_eq!((code, tr), (None, true));
    }
}
