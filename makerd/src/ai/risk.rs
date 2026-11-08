//! Komut risk sınıflandırması (kurallı, modelden bağımsız).
//!
//! Model komut önerir ama risk düzeyine model karar vermez: 0.5B bir model "rm -rf" için "güvenli" diyebilir.
//! Burada komut kabuk sözdizimine göre parçalanır (| ; && || & $(…) `…`), her parçanın komut adı ve
//! argümanları kurallarla değerlendirilir; sonuç en riskli parçadır. Bilinmeyen komut WARNING sayılır.
//!
//! - SAFE: salt okunur (durum gösterir, hiçbir şeyi değiştirmez).
//! - WARNING: sistem durumunu değiştirebilir (servis yeniden başlatma, dosya yazma, paket kurma, sudo…).
//! - DANGEROUS: geri alınamaz ya da sistemi bozabilir (silme, biçimlendirme, izin/kullanıcı değişikliği…).
//!
//! Hiçbir komut burada ya da başka bir yerde çalıştırılmaz.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum Risk {
    Safe,
    Warning,
    Dangerous,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Safe => "SAFE",
            Risk::Warning => "WARNING",
            Risk::Dangerous => "DANGEROUS",
        }
    }
    pub fn tr(self) -> &'static str {
        match self {
            Risk::Safe => "güvenli (salt okunur)",
            Risk::Warning => "dikkat (sistemi değiştirebilir)",
            Risk::Dangerous => "tehlikeli (geri alınamayabilir)",
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Assessment {
    pub command: String,
    pub risk: Risk,
    pub reasons: Vec<String>,
}

struct Acc {
    risk: Risk,
    reasons: Vec<String>,
}

impl Acc {
    fn add(&mut self, r: Risk, why: impl Into<String>) {
        let why = why.into();
        if r > self.risk {
            self.risk = r;
        }
        if r > Risk::Safe && !self.reasons.contains(&why) {
            self.reasons.push(why);
        }
    }
}

/// `<servis>`, `<dosya adı>` gibi yer tutucuları düz sözcüğe çevirir (yönlendirme sanılmasın).
fn unplaceholder(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('<') {
        return s.into();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(a) = rest.find('<') {
        let after = &rest[a + 1..];
        let end = after.find('>');
        let ok = end.is_some_and(|e| e > 0 && e <= 40 && after[..e].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' || c == '.') && after[..e].chars().next().is_some_and(char::is_alphabetic));
        if ok {
            let e = end.unwrap_or(0);
            out.push_str(&rest[..a]);
            out.push_str(&after[..e].replace(' ', "_"));
            rest = &after[e + 1..];
        } else {
            out.push_str(&rest[..=a]);
            rest = after;
        }
    }
    out.push_str(rest);
    out.into()
}

/// Komutta `<servis>` gibi doldurulmamış yer tutucu var mı?
pub fn has_placeholder(cmd: &str) -> bool {
    unplaceholder(cmd) != cmd
}

/// Komutu değerlendirir.
pub fn classify(cmd: &str) -> Assessment {
    let mut acc = Acc { risk: Risk::Safe, reasons: Vec::new() };
    let original = cmd.trim();
    let un = unplaceholder(original);
    let c = un.as_ref();
    if c.is_empty() {
        return Assessment { command: String::new(), risk: Risk::Safe, reasons: Vec::new() };
    }
    let compact: String = c.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.contains(":(){") || compact.contains(":|:&") {
        acc.add(Risk::Dangerous, "fork bombası: sistemi kilitler");
    }
    let (segs, subs, pipe_to_shell) = split(c);
    if pipe_to_shell {
        acc.add(Risk::Dangerous, "çıktı bir kabuğa aktarılıp çalıştırılıyor (ör. curl … | sh): içeriği görmeden kod çalıştırır");
    }
    for s in subs {
        let inner = classify(&s);
        for r in inner.reasons {
            acc.add(inner.risk, r);
        }
        if inner.risk == Risk::Safe {
            acc.add(Risk::Safe, "");
        }
    }
    for seg in segs {
        segment(&seg, &mut acc);
    }
    Assessment { command: original.to_string(), risk: acc.risk, reasons: acc.reasons }
}

/// Kabuk satırını komut parçalarına böler; tırnak içini korur. (parçalar, $(…)/`…` içerikleri, "| sh" var mı)
fn split(s: &str) -> (Vec<Vec<String>>, Vec<String>, bool) {
    let mut segs = Vec::new();
    let mut subs = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut w = String::new();
    let mut has_w = false;
    let (mut sq, mut dq) = (false, false);
    let mut piped = false;
    let mut pipe_to_shell = false;
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    let flush_w = |words: &mut Vec<String>, w: &mut String, has_w: &mut bool| {
        if *has_w {
            words.push(std::mem::take(w));
            *has_w = false;
        }
    };
    let end_seg = |words: &mut Vec<String>, piped_next: bool, piped: &mut bool, pipe_to_shell: &mut bool, segs: &mut Vec<Vec<String>>| {
        if !words.is_empty() {
            if *piped && is_shell(&words[strip_prefix_cmds(words)..]) {
                *pipe_to_shell = true;
            }
            segs.push(std::mem::take(words));
        }
        *piped = piped_next;
    };
    while i < ch.len() {
        let c = ch[i];
        if sq {
            if c == '\'' { sq = false } else { w.push(c) }
            i += 1;
            continue;
        }
        if c == '\\' && i + 1 < ch.len() {
            if ch[i + 1] == '\n' { i += 2; continue; }
            w.push(ch[i + 1]);
            has_w = true;
            i += 2;
            continue;
        }
        // Komut yerine koyma: içerik ayrıca değerlendirilir.
        if c == '$' && ch.get(i + 1) == Some(&'(') {
            let (inner, j) = take_balanced(&ch, i + 2);
            subs.push(inner);
            w.push_str("$(…)");
            has_w = true;
            i = j;
            continue;
        }
        if c == '`' {
            let j = ch[i + 1..].iter().position(|&x| x == '`').map(|p| i + 1 + p).unwrap_or(ch.len());
            subs.push(ch[i + 1..j.min(ch.len())].iter().collect());
            w.push_str("`…`");
            has_w = true;
            i = j + 1;
            continue;
        }
        if dq {
            if c == '"' { dq = false } else { w.push(c) }
            i += 1;
            continue;
        }
        match c {
            '\'' => { sq = true; has_w = true; }
            '"' => { dq = true; has_w = true; }
            ' ' | '\t' => flush_w(&mut words, &mut w, &mut has_w),
            '\n' | ';' => { flush_w(&mut words, &mut w, &mut has_w); end_seg(&mut words, false, &mut piped, &mut pipe_to_shell, &mut segs); }
            '|' => {
                flush_w(&mut words, &mut w, &mut has_w);
                let or = ch.get(i + 1) == Some(&'|');
                if or { i += 1; }
                if !or && ch.get(i + 1) == Some(&'&') { i += 1; } // |&
                end_seg(&mut words, !or, &mut piped, &mut pipe_to_shell, &mut segs);
            }
            '&' => {
                // 2>&1, &> , >& yönlendirmeleri kelimenin parçasıdır.
                let prev = if w.is_empty() { None } else { w.chars().last() };
                if prev == Some('>') || ch.get(i + 1) == Some(&'>') {
                    w.push(c);
                    has_w = true;
                } else {
                    flush_w(&mut words, &mut w, &mut has_w);
                    if ch.get(i + 1) == Some(&'&') { i += 1; }
                    end_seg(&mut words, false, &mut piped, &mut pipe_to_shell, &mut segs);
                }
            }
            '>' | '<' => {
                // Yönlendirme ayrı kelime: ">dosya" → ">", "dosya"
                let mut op = String::new();
                if has_w && w.chars().all(|x| x.is_ascii_digit() || x == '&') { op.push_str(&w); w.clear(); has_w = false; }
                flush_w(&mut words, &mut w, &mut has_w);
                op.push(c);
                while i + 1 < ch.len() && (ch[i + 1] == '>' || ch[i + 1] == '&' || (c == '<' && ch[i + 1] == '<')) { i += 1; op.push(ch[i]); }
                words.push(op);
            }
            _ => { w.push(c); has_w = true; }
        }
        i += 1;
    }
    flush_w(&mut words, &mut w, &mut has_w);
    end_seg(&mut words, false, &mut piped, &mut pipe_to_shell, &mut segs);
    (segs, subs, pipe_to_shell)
}

fn take_balanced(ch: &[char], start: usize) -> (String, usize) {
    let mut depth = 1;
    let mut j = start;
    while j < ch.len() {
        match ch[j] {
            '(' => depth += 1,
            ')' => { depth -= 1; if depth == 0 { return (ch[start..j].iter().collect(), j + 1); } }
            _ => {}
        }
        j += 1;
    }
    (ch[start..].iter().collect(), ch.len())
}

fn base(w: &str) -> &str {
    w.rsplit('/').next().unwrap_or(w)
}

/// Başa eklenen ve asıl komutu değiştirmeyen sözcükler (sudo, env, nice…) atlandığında komutun başladığı yer.
fn strip_prefix_cmds(words: &[String]) -> usize {
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        let b = base(w);
        if w.contains('=') && !w.starts_with('-') && !w.starts_with('=') && i == 0 || (i > 0 && w.contains('=') && !w.starts_with('-') && is_assign(w)) {
            i += 1;
            continue;
        }
        match b {
            "sudo" | "doas" => {
                i += 1;
                while i < words.len() && words[i].starts_with('-') {
                    let f = words[i].as_str();
                    i += 1;
                    if matches!(f, "-u" | "-g" | "-C" | "-p" | "-U") { i += 1; }
                }
            }
            "env" | "nice" | "ionice" | "nohup" | "time" | "stdbuf" | "timeout" | "exec" | "command" | "builtin" => {
                i += 1;
                while i < words.len() && (words[i].starts_with('-') || (b == "timeout" && words[i].chars().next().is_some_and(|c| c.is_ascii_digit())) || (b == "env" && is_assign(&words[i]))) {
                    let f = words[i].clone();
                    i += 1;
                    if b == "nice" && f == "-n" { i += 1; }
                }
            }
            _ => break,
        }
    }
    i
}

fn is_assign(w: &str) -> bool {
    w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn is_shell(words: &[String]) -> bool {
    words.first().is_some_and(|w| matches!(base(w), "sh" | "bash" | "zsh" | "dash" | "ksh" | "python" | "python3" | "perl" | "ruby" | "node"))
        && !words.iter().skip(1).any(|a| a == "-n")
}

const DEVICE_PREFIXES: [&str; 6] = ["/dev/sd", "/dev/mmcblk", "/dev/nvme", "/dev/hd", "/dev/vd", "/dev/disk"];

fn segment(words: &[String], acc: &mut Acc) {
    // Yönlendirmeler: dosyaya yazma WARNING; diske yazma DANGEROUS. /dev/null, /dev/stderr ve &1 zararsız.
    let mut args: Vec<&str> = Vec::new();
    let mut k = 0;
    while k < words.len() {
        let w = words[k].as_str();
        if w.starts_with('>') || (w.len() > 1 && w.ends_with('>') && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit() || c == '&')) || w.contains(">>") || w.starts_with("&>") || (w.contains('>') && w.chars().next().is_some_and(|c| c.is_ascii_digit())) {
            let target = if w.ends_with('>') || w == "&>" || w == ">|" || w == ">>" || w == "&>>" { words.get(k + 1).map(String::as_str).unwrap_or("") } else { "" };
            let inline = w.trim_start_matches(|c: char| c.is_ascii_digit() || c == '&' || c == '>' || c == '|');
            let t = if target.is_empty() { inline } else { target };
            if !(t.is_empty() || t.starts_with('&') || w.ends_with("&1") || w.ends_with("&2") || t == "/dev/null" || t == "/dev/stderr" || t == "/dev/stdout") {
                if DEVICE_PREFIXES.iter().any(|p| t.starts_with(p)) {
                    acc.add(Risk::Dangerous, format!("doğrudan diske ({t}) yazar: veriler kaybolur"));
                } else if w.contains(">>") {
                    acc.add(Risk::Warning, format!("{t} dosyasının sonuna ekler"));
                } else {
                    acc.add(Risk::Warning, format!("{t} dosyasının üzerine yazar"));
                }
            }
            if !target.is_empty() { k += 1; }
            k += 1;
            continue;
        }
        if w.starts_with('<') {
            if w == "<" || w == "<<" || w == "<<<" { k += 1; }
            k += 1;
            continue;
        }
        args.push(w);
        k += 1;
    }
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let start = strip_prefix_cmds(&owned);
    if owned[..start].iter().any(|w| matches!(base(w), "sudo" | "doas")) {
        acc.add(Risk::Warning, "sudo: yönetici (root) yetkisiyle çalışır");
    }
    let Some(name) = owned.get(start) else { return };
    let name = base(name);
    let a: Vec<&str> = owned[start + 1..].iter().map(String::as_str).collect();
    let has = |f: &str| a.iter().any(|x| *x == f);
    let has_any = |fs: &[&str]| a.iter().any(|x| fs.contains(x));
    let short = |c: char| a.iter().any(|x| x.starts_with('-') && !x.starts_with("--") && x.contains(c));
    let sub = a.iter().find(|x| !x.starts_with('-')).copied().unwrap_or("");
    let w = Risk::Warning;
    let d = Risk::Dangerous;
    match name {
        // --- tehlikeli: silme, biçimlendirme, disk, izin, kullanıcı
        "rm" | "unlink" => {
            let rec = short('r') || short('R') || has("--recursive");
            let force = short('f') || has("--force");
            let root = a.iter().any(|x| matches!(*x, "/" | "/*" | "~" | "~/" | "*" | "/home" | "/etc" | "/usr" | "/boot" | "/var"));
            acc.add(d, if root { "kritik bir dizini siler: sistem çalışmaz hâle gelebilir".to_string() } else if rec && force { "dosya/klasörleri sormadan, alt klasörleriyle siler (geri alınamaz)".to_string() } else { "dosya siler (çöp kutusu yok, geri alınamaz)".to_string() });
        }
        "rmdir" => acc.add(d, "klasör siler"),
        "shred" | "srm" | "wipe" => acc.add(d, "dosyayı kurtarılamayacak şekilde siler"),
        "dd" => {
            let of = a.iter().find_map(|x| x.strip_prefix("of=")).unwrap_or("");
            if DEVICE_PREFIXES.iter().any(|p| of.starts_with(p)) { acc.add(d, format!("diske ({of}) ham yazar: tüm veriler silinir")) } else { acc.add(d, "dd ham veri kopyalar; yanlış hedef veri kaybına yol açar") }
        }
        n if n.starts_with("mkfs") || n == "mke2fs" || n == "mkswap" => acc.add(d, "diski/bölümü biçimlendirir: üzerindeki tüm veriler silinir"),
        "fdisk" | "sfdisk" | "gdisk" | "sgdisk" | "cfdisk" | "parted" | "wipefs" | "blkdiscard" => {
            if name == "fdisk" && (has("-l") || has("--list")) || name == "parted" && has("-l") || name == "sfdisk" && (has("-l") || has("--list")) { return; }
            acc.add(d, "bölüm tablosunu/diski değiştirir: veri kaybı olabilir")
        }
        "fsck" | "e2fsck" => acc.add(d, "dosya sistemini onarır; bağlı (mount) diskte çalıştırmak veriyi bozar"),
        "chmod" | "chown" | "chgrp" | "chattr" | "setfacl" => acc.add(d, if short('R') || has("--recursive") { "dosya izinlerini/sahipliğini alt klasörlerle birlikte değiştirir" } else { "dosya izinlerini/sahipliğini değiştirir" }),
        "useradd" | "userdel" | "usermod" | "adduser" | "deluser" | "groupadd" | "groupdel" | "groupmod" | "addgroup" | "delgroup" | "passwd" | "chpasswd" | "chage" | "visudo" | "vipw" => acc.add(d, "kullanıcı/grup ya da parola ayarlarını değiştirir"),
        "truncate" => acc.add(d, "dosyanın içeriğini keser/siler"),
        "mv" => {
            if a.iter().any(|x| *x == "/dev/null") { acc.add(d, "dosyayı /dev/null'a taşır: dosya yok olur") } else { acc.add(w, "dosya taşır/yeniden adlandırır (hedefte aynı adlı dosya varsa üzerine yazar)") }
        }
        "find" => {
            if has("-delete") { acc.add(d, "find -delete: bulunan dosyaları siler") }
            if let Some(p) = a.iter().position(|x| matches!(*x, "-exec" | "-execdir" | "-ok" | "-okdir")) {
                let inner: Vec<String> = a[p + 1..].iter().take_while(|x| **x != ";" && **x != "+" && **x != "\\;").map(|s| s.to_string()).collect();
                let sub = classify(&inner.join(" "));
                acc.add(sub.risk.max(Risk::Warning), format!("find -exec her dosya için komut çalıştırır: {}", inner.join(" ")));
                for r in sub.reasons { acc.add(sub.risk, r); }
            }
        }
        "xargs" => {
            let i = a.iter().position(|x| !x.starts_with('-')).unwrap_or(a.len());
            let inner = a[i..].join(" ");
            if inner.is_empty() { return; }
            let sub = classify(&inner);
            for r in sub.reasons { acc.add(sub.risk, format!("xargs: {r}")); }
        }
        "kill" => {
            if a.iter().any(|x| *x == "1" || *x == "-1") { acc.add(d, "PID 1'i (init) ya da tüm işlemleri sonlandırır: sistem çöker") } else if has("-l") || has("-L") { } else { acc.add(w, "işlem sonlandırır (kaydedilmemiş veri kaybolabilir)") }
        }
        "pkill" | "killall" | "skill" => acc.add(w, "adıyla eşleşen tüm işlemleri sonlandırır"),
        "crontab" => {
            if has("-r") { acc.add(d, "crontab -r: tüm zamanlanmış görevleri siler") } else if has("-l") { } else { acc.add(w, "zamanlanmış görevleri değiştirir") }
        }
        "git" => {
            if matches!(sub, "status" | "log" | "diff" | "show" | "branch" | "remote" | "blame" | "describe" | "rev-parse" | "ls-files" | "tag" | "config") && !has("--global") { return; }
            if sub == "clean" || (sub == "reset" && has("--hard")) || (sub == "push" && (has("--force") || short('f'))) { acc.add(d, "git: yerel değişiklikleri kalıcı olarak siler/üzerine yazar") } else { acc.add(w, "git deposunu değiştirir") }
        }
        // --- uyarı: durum değiştirir
        "systemctl" => {
            let ro = ["status", "list-units", "list-unit-files", "list-timers", "list-sockets", "list-dependencies", "list-jobs", "is-active", "is-enabled", "is-failed", "is-system-running", "show", "cat", "get-default", "help", "--version"];
            if sub.is_empty() || ro.contains(&sub) { return; }
            match sub {
                "poweroff" | "reboot" | "halt" | "kexec" | "rescue" | "emergency" | "isolate" | "default" => acc.add(w, "sistemi kapatır/yeniden başlatır ya da çalışma hedefini değiştirir"),
                "mask" | "disable" => acc.add(w, format!("servisi kalıcı olarak devre dışı bırakır ({sub})")),
                _ => acc.add(w, format!("servis durumunu değiştirir (systemctl {sub})")),
            }
        }
        "service" => { if !has("status") && !has("--status-all") { acc.add(w, "servis durumunu değiştirir") } }
        "journalctl" => { if a.iter().any(|x| x.starts_with("--vacuum") || *x == "--rotate" || *x == "--flush" || *x == "--relinquish-var") { acc.add(w, "günlük dosyalarını siler/döndürür") } }
        "dmesg" => { if short('c') || short('C') || has("--clear") || has("--read-clear") || has("-n") || has("--console-level") { acc.add(w, "çekirdek günlüğünü temizler/ayarını değiştirir") } }
        "apt" | "apt-get" | "aptitude" | "dpkg" | "snap" | "flatpak" | "pacman" | "dnf" | "yum" | "apk" => {
            let ro = ["list", "show", "search", "policy", "showpkg", "depends", "rdepends", "-l", "-L", "-s", "--list", "--status", "--get-selections", "info", "changelog"];
            if ro.contains(&sub) || (name == "dpkg" && has_any(&["-l", "-L", "-s", "-S", "--list", "--status", "--search"])) { return; }
            if matches!(sub, "purge" | "autoremove" | "remove" | "-r" | "-P" | "--purge" | "--remove") { acc.add(w, "paket kaldırır (bağlı programlar da kalkabilir)") } else { acc.add(w, "paket kurar/günceller (sistemi değiştirir, internet ister)") }
        }
        "pip" | "pip3" | "uv" | "npm" | "cargo" | "gem" => {
            if matches!(sub, "list" | "show" | "freeze" | "--version" | "-V" | "check" | "search" | "help") { return; }
            acc.add(w, "paket kurar/kaldırır")
        }
        "reboot" | "shutdown" | "poweroff" | "halt" | "init" | "telinit" => acc.add(w, "sistemi kapatır ya da yeniden başlatır"),
        "cp" | "ln" | "install" | "rsync" | "scp" | "mkdir" | "touch" | "mktemp" => acc.add(w, if name == "mkdir" || name == "touch" { "dosya/klasör oluşturur" } else { "dosya kopyalar/oluşturur (hedefin üzerine yazabilir)" }),
        "tee" => { if a.iter().any(|x| !x.starts_with('-') && *x != "/dev/null") { acc.add(w, "girdiyi dosyaya yazar") } }
        "sed" | "perl" => { if a.iter().any(|x| x.starts_with("-i") || *x == "--in-place") { acc.add(w, "dosyayı yerinde düzenler") } }
        "awk" | "gawk" | "mawk" => { if a.iter().any(|x| x.contains("system(") || x.contains("| \"") || x.contains("> \"")) { acc.add(w, "awk betiği komut çalıştırır/dosyaya yazar") } }
        "tar" | "unzip" | "gunzip" | "gzip" | "bzip2" | "xz" | "zip" | "7z" => {
            let list = has("-t") || has("--list") || short('t') && name == "tar" || has("-l") && name == "unzip";
            if !list { acc.add(w, "arşiv açar/oluşturur: dosya yazar") }
        }
        "mount" | "umount" | "swapon" | "swapoff" => { if !(name == "mount" && a.is_empty()) && !(name == "swapon" && (has("--show") || has("-s"))) { acc.add(w, "dosya sistemi/takas bağlar ya da ayırır") } }
        "modprobe" | "insmod" | "rmmod" | "depmod" => acc.add(w, "çekirdek modülü yükler/kaldırır"),
        "sysctl" => { if has("-w") || a.iter().any(|x| x.contains('=')) || has("-p") { acc.add(w, "çekirdek ayarını değiştirir") } }
        "ip" => {
            let obj = sub;
            let rest: Vec<&str> = a.iter().copied().filter(|x| !x.starts_with('-')).skip(1).collect();
            let verb = rest.first().copied().unwrap_or("show");
            let _ = obj;
            if !matches!(verb, "show" | "list" | "ls" | "get" | "lst" | "sh") { acc.add(w, format!("ağ ayarını değiştirir (ip … {verb})")) }
        }
        "ifconfig" | "iwconfig" => { if a.len() > 1 { acc.add(w, "ağ arayüzü ayarını değiştirir") } }
        "nmcli" => {
            let j = a.join(" ");
            if ["up", "down", "delete", "modify", "add", "connect", "disconnect", "radio", "reload", "edit", "networking off", "hotspot"].iter().any(|k| a.contains(k) || j.contains(k)) { acc.add(w, "ağ bağlantısını değiştirir (bağlantı kopabilir)") }
        }
        "iptables" | "ip6tables" | "nft" | "ufw" | "firewall-cmd" => {
            if has_any(&["-L", "-S", "--list", "list", "status", "--list-all"]) && !has_any(&["-A", "-D", "-I", "-F", "-X", "-P", "add", "delete", "flush"]) { return; }
            acc.add(w, "güvenlik duvarı kurallarını değiştirir (erişim kesilebilir)")
        }
        "hostnamectl" | "timedatectl" | "localectl" => { if a.iter().any(|x| x.starts_with("set-")) || has("set-ntp") { acc.add(w, "sistem ayarını değiştirir") } }
        "date" => { if short('s') || has("--set") { acc.add(w, "sistem saatini değiştirir") } }
        "hostname" => { if a.iter().any(|x| !x.starts_with('-')) { acc.add(w, "makine adını değiştirir") } }
        "raspi-config" | "rpi-eeprom-update" | "rpi-update" | "rpi-eeprom-config" => { if !(name == "rpi-eeprom-update" && a.is_empty()) { acc.add(w, "Raspberry Pi ayarlarını/önyükleyiciyi değiştirir") } }
        "docker" | "podman" => { if !matches!(sub, "ps" | "images" | "logs" | "inspect" | "stats" | "version" | "info" | "top" | "port") { acc.add(w, "kapsayıcı başlatır/durdurur/siler") } }
        "curl" => { if has_any(&["-o", "-O", "--output", "--remote-name", "-T", "--upload-file", "-X", "--request", "-d", "--data"]) { acc.add(w, "dosya indirir ya da sunucuya veri gönderir") } }
        "wget" => { if !(has("--spider") || a.windows(2).any(|p| p[0] == "-O" && p[1] == "-") || has("-qO-")) { acc.add(w, "dosya indirip diske yazar") } }
        "nano" | "vim" | "vi" | "emacs" | "ed" | "micro" => acc.add(w, "dosyayı düzenler (kaydedilirse değişir)"),
        "chroot" | "unshare" | "nsenter" => acc.add(w, "farklı ortamda komut çalıştırır"),
        "watch" => {
            let mut i = 0;
            while i < a.len() && a[i].starts_with('-') {
                if matches!(a[i], "-n" | "--interval" | "-q" | "--equexit") { i += 1; }
                i += 1;
            }
            let s = classify(&a[i.min(a.len())..].join(" "));
            for r in s.reasons { acc.add(s.risk, r); }
        }
        "sh" | "bash" | "zsh" | "dash" | "source" | "." | "eval" => {
            if let Some(p) = a.iter().position(|x| *x == "-c") { let s = classify(a.get(p + 1).copied().unwrap_or("")); for r in s.reasons { acc.add(s.risk, r); } if s.risk == Risk::Safe { } } else if a.is_empty() { } else { acc.add(w, "betik çalıştırır: içeriği bilinmiyor") }
        }
        "python" | "python3" | "perl5" | "node" | "ruby" | "lua" => { if !(has("--version") || has("-V")) { acc.add(w, "program/betik çalıştırır: ne yaptığı içeriğine bağlı") } }
        "vcgencmd" => { if !a.first().is_some_and(|s| s.starts_with("measure_") || s.starts_with("get_") || *s == "version" || *s == "commands" || *s == "otp_dump" || s.starts_with("read_ring_osc")) && !a.is_empty() { acc.add(w, "VideoCore ayarını değiştirebilir") } }
        // --- salt okunur (bilinen)
        "ls" | "ll" | "la" | "dir" | "cat" | "tac" | "less" | "more" | "head" | "tail" | "grep" | "egrep" | "fgrep" | "rg" | "zgrep" | "zcat" | "wc" | "sort" | "uniq"
        | "cut" | "tr" | "column" | "nl" | "fold" | "paste" | "join" | "comm" | "diff" | "cmp" | "df" | "du" | "free" | "top" | "htop" | "btop" | "atop" | "ps" | "pstree" | "pgrep" | "pidof"
        | "uptime" | "uname" | "whoami" | "id" | "groups" | "w" | "who" | "last" | "lastlog" | "cal" | "ss" | "netstat" | "ping" | "ping6" | "traceroute" | "tracepath" | "mtr"
        | "dig" | "nslookup" | "host" | "resolvectl" | "lsblk" | "blkid" | "lsusb" | "lspci" | "lscpu" | "lsmod" | "lsof" | "findmnt" | "stat" | "file" | "which" | "whereis" | "type"
        | "printenv" | "echo" | "printf" | "sensors" | "tree" | "realpath" | "readlink" | "basename" | "dirname" | "md5sum" | "sha1sum" | "sha256sum" | "sha512sum" | "b2sum" | "test" | "["
        | "true" | "false" | "sleep" | "seq" | "vmstat" | "iostat" | "mpstat" | "pidstat" | "sar" | "nproc" | "arch" | "loginctl" | "hostid" | "lsb_release" | "iw" | "iwgetid" | "iwlist"
        | "jq" | "strings" | "hexdump" | "xxd" | "od" | "man" | "info" | "help" | "whatis" | "apropos" | "locale" | "tput" | "clear" | "history" | "alias" | "env" | "networkctl" | "busctl"
        | "systemd-analyze" | "coredumpctl" | "lslocks" | "lsipc" | "lsns" | "dmidecode" | "ethtool" | "arp" | "route" | "nmap" | "nc" | "pwd" | "cd" | "getent" | "timedatectl_" | "fc-list"
        | "gpioinfo" | "gpiodetect" | "pinctrl" | "i2cdetect" | "libcamera-hello" | "rpicam-hello" | "mos" => {
            if name == "i2cdetect" && !short('y') { acc.add(w, "i2cdetect -y olmadan sorar; bazı aygıtları etkileyebilir") }
        }
        n => acc.add(w, format!("bilinmeyen komut ({n}): ne yaptığını çalıştırmadan önce inceleyin")),
    }
}

/// Model yanıtındaki komutlar: ```bash / ```sh / ``` blokları (her satır bir komut, `\` devam satırları birleşir);
/// blok yoksa satır içi `…` parçalarından komut olabilenler.
pub fn extract_commands(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut lang_ok = true;
    let mut cont = String::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("```") {
            if in_block {
                in_block = false;
                if !cont.is_empty() { out.push(std::mem::take(&mut cont)); }
            } else {
                in_block = true;
                let l = rest.trim().to_ascii_lowercase();
                lang_ok = matches!(l.as_str(), "" | "bash" | "sh" | "shell" | "console" | "zsh" | "terminal" | "komut" | "linux");
            }
            continue;
        }
        if !in_block || !lang_ok {
            continue;
        }
        let mut t = t.to_string();
        if let Some(r) = t.strip_prefix("$ ") { t = r.to_string(); }
        if t.is_empty() || t.starts_with('#') { continue; }
        // Bloktaki örnek çıktı satırları komut değildir ("Disk 2: 7.5GB …", "Filesystem Size Used").
        let first = t.split_whitespace().next().unwrap_or("");
        if cont.is_empty() && (first.ends_with(':') || first.chars().next().is_some_and(|c| c.is_uppercase()) || first.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '%') || first.starts_with("/dev/")) { continue; }
        if let Some(stripped) = t.strip_suffix('\\') {
            cont.push_str(stripped);
            cont.push(' ');
            continue;
        }
        cont.push_str(&t);
        out.push(std::mem::take(&mut cont));
    }
    if !cont.is_empty() { out.push(cont); }
    if out.is_empty() {
        // Satır içi: `df -h` gibi; yalnız bilinen bir komut adıyla başlayanlar.
        let mut rest = text;
        while let Some(a) = rest.find('`') {
            let after = &rest[a + 1..];
            let Some(b) = after.find('`') else { break };
            let span = after[..b].trim();
            rest = &after[b + 1..];
            let first = span.split_whitespace().next().unwrap_or("");
            if !span.is_empty() && span.len() < 300 && !span.contains('\n') && looks_like_command(first) && !out.iter().any(|x| x == span) {
                out.push(span.to_string());
            }
        }
    }
    out.dedup();
    out
}

fn looks_like_command(first: &str) -> bool {
    let b = base(first);
    if b.is_empty() || !b.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '.' || c == '/') { return false; }
    let a = classify(b);
    // Bilinmeyen tek sözcük WARNING döner ("bilinmeyen komut"): satır içi metinde komut sayılmaz.
    !(a.risk == Risk::Warning && a.reasons.iter().any(|r| r.starts_with("bilinmeyen komut"))) || first.starts_with("sudo")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(c: &str) -> Risk {
        classify(c).risk
    }

    #[test]
    fn safe_readonly() {
        for c in [
            "df -h", "free -h", "cat /sys/class/thermal/thermal_zone0/temp", "ps aux --sort=-%cpu | head -n 15", "systemctl list-units --type=service --state=running",
            "journalctl -u ssh -n 50 --no-pager", "journalctl -p err -b", "ip -brief address", "ip a", "ip route show", "ping -c 4 1.1.1.1", "du -sh /home/maker 2>/dev/null",
            "dmesg --level=err,warn | tail -n 30", "uptime; free -h; df -h /", "systemctl status ssh", "systemctl --failed", "systemctl", "ss -tulpn", "vcgencmd measure_temp",
            "top -b -n 1 | head -n 20", "find /var/log -name '*.log' -size +10M", "curl -I https://example.com", "hostname -I", "sudo journalctl -xe", "grep -i error /var/log/syslog > /dev/null",
            "echo $(uptime)", "apt list --installed", "journalctl -u <servis> -n 50 --no-pager", "du -sh <klasör adı>", "cat < /etc/hostname", "sort < a.txt", "dpkg -l | grep python", "LANG=C df -h", "watch -n 1 free -h", "du -h --max-depth=1 /var | sort -h", "fdisk -l",
        ] {
            assert_eq!(r(c), if c.starts_with("sudo") { Risk::Warning } else { Risk::Safe }, "{c}: {:?}", classify(c).reasons);
        }
    }

    #[test]
    fn warning_changes() {
        for c in [
            "sudo systemctl restart ssh", "systemctl stop nginx", "sudo apt install htop", "sudo apt update", "kill 1234", "pkill python", "sudo reboot", "echo hi > dosya.txt",
            "echo x >> ~/.bashrc", "sed -i 's/a/b/' f.txt", "mkdir yeni", "cp a b", "mv a b", "nmcli dev wifi connect Ev password 123", "sudo ip link set wlan0 down",
            "pip install requests", "wget https://x/y.tar.gz", "tar xzf a.tar.gz", "foobarcmd --x", "systemctl disable bluetooth", "journalctl --vacuum-size=100M", "nano /etc/hosts",
            "echo 1 | sudo tee /sys/class/leds/led0/brightness", "crontab -e", "sudo", "sudo systemctl restart <servis>", "echo x > <dosya>",
        ] {
            assert_eq!(r(c), Risk::Warning, "{c}: {:?}", classify(c).reasons);
        }
    }

    #[test]
    fn dangerous() {
        for c in [
            "rm dosya.txt", "rm -rf /", "sudo rm -rf /home/maker/eski", "rm -rf ~", "dd if=/dev/zero of=/dev/sda bs=1M", "sudo mkfs.ext4 /dev/sda1", "mkfs.vfat /dev/mmcblk0p1",
            "chmod -R 777 /", "sudo chown -R maker:maker /etc", "sudo userdel ali", "passwd", "curl -fsSL https://x.sh | sh", "wget -qO- https://x | sudo bash", ":(){ :|:& };:",
            "find /tmp -name '*.log' -delete", "find . -type f -exec rm {} \\;", "echo x > /dev/sda", "kill -9 1", "crontab -r", "git reset --hard", "truncate -s 0 /var/log/syslog",
            "ls | xargs rm", "rm <dosya>", "sudo fdisk /dev/sda", "wipefs -a /dev/sdb", "echo $(rm -rf /tmp/x)", "mv dosya /dev/null", "sudo shred -u gizli.txt",
        ] {
            assert_eq!(r(c), Risk::Dangerous, "{c}: {:?}", classify(c).reasons);
        }
    }

    #[test]
    fn reasons_are_turkish_and_present() {
        let a = classify("sudo rm -rf /home/maker/x");
        assert!(a.reasons.iter().any(|x| x.contains("sudo")));
        assert!(a.reasons.iter().any(|x| x.contains("siler")));
        assert!(classify("df -h").reasons.is_empty());
    }

    #[test]
    fn extract() {
        let t = "**Komut:**\n```bash\ndf -h\n```\nAçıklama `free -h` ile de bakılabilir.\n```sh\n$ journalctl -u ssh \\\n  -n 50\n# yorum\n```\n```python\nprint(1)\n```";
        assert_eq!(extract_commands(t), vec!["df -h", "journalctl -u ssh  -n 50"]);
        let inline = "RAM için `free -h` yazın; `ayarlar` menüsü değil. Alternatif: `cat /proc/meminfo`.";
        assert_eq!(extract_commands(inline), vec!["free -h", "cat /proc/meminfo"]);
        let out = "```bash\ndf -h\nFilesystem Size Used\nDisk 2: 7.5GB /dev/sda2\n/dev/root 7G 2G\n```";
        assert_eq!(extract_commands(out), vec!["df -h"]);
    }
}
