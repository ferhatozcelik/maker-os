//! user.create: ilk açılış sihirbazının yönetici hesabı. Hesap dosyaları
//! (passwd, group, shadow, gshadow) shadow araçlarıyla uyumlu kilitle güncellenir.

use super::{PrivError, UserCreateArgs, UserCreateResult, MIN_PASSWORD_LEN};
use crate::util::replace_file;
use std::collections::HashSet;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const RESERVED: &[&str] = &[
    "root", "daemon", "bin", "sys", "sync", "nobody", "adm", "lp", "mail", "news", "uucp", "operator", "man", "sshd", "messagebus", "polkitd", "avahi", "dbus", "admin",
    "wheel", "users", "makerd",
];

/// İlk kullanıcının ek grupları (sistemde olmayanlar atlanır).
pub const DEFAULT_USER_GROUPS: &[&str] = &["wheel", "video", "gpio", "i2c", "spi", "dialout", "input", "render", "audio", "plugdev"];

/// Küçük harfle başlar, en fazla 32; ayrılmış adlar ve systemd-* değil.
pub fn valid_username(s: &str) -> bool {
    let b = s.as_bytes();
    let first = |c: &u8| c.is_ascii_lowercase() || *c == b'_';
    let rest = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_' || *c == b'-';
    !b.is_empty() && b.len() <= 32 && first(&b[0]) && b[1..].iter().all(rest) && !RESERVED.contains(&s) && !s.starts_with("systemd-")
}

/// GECOS alanına yazılabilir mi (':' ',' '=' ve satır sonu yok, en fazla 64 karakter).
pub fn valid_fullname(s: &str) -> bool {
    s.chars().count() <= 64 && !s.contains([':', '\n', '\r', '\0', ',', '='])
}

/// En az 6 karakter (bayt), en fazla 512; NUL/satır sonu yok.
pub fn valid_new_password(s: &str) -> bool {
    s.len() >= MIN_PASSWORD_LEN && s.len() <= 512 && !s.contains(['\0', '\n', '\r'])
}

/// Hesap dosyalarının yolları (testte geçici dizin).
#[derive(Clone)]
pub struct UserFiles {
    pub passwd: String,
    pub group: String,
    pub gshadow: String,
    /// lckpwdf uyumlu kilit (/etc/.pwd.lock)
    pub lock: String,
    pub home_base: String,
    pub skel: String,
    /// Varsa user.create reddedilir (makerd'nin <data-dir>/setup-done dosyası).
    pub setup_flag: String,
    /// Boş değilse makerd-user@<user>.service etkinleştirilip başlatılır.
    pub systemctl: String,
    pub shells: Vec<String>,
}

impl Default for UserFiles {
    fn default() -> Self {
        Self {
            passwd: "/etc/passwd".into(),
            group: "/etc/group".into(),
            gshadow: "/etc/gshadow".into(),
            lock: "/etc/.pwd.lock".into(),
            home_base: "/home".into(),
            skel: "/etc/skel".into(),
            setup_flag: "/var/lib/makerd/setup-done".into(),
            systemctl: "/usr/bin/systemctl".into(),
            shells: vec!["/bin/bash".into(), "/bin/sh".into()],
        }
    }
}

struct PwLock(std::fs::File);

impl Drop for PwLock {
    fn drop(&mut self) {
        let mut lk: libc::flock = unsafe { std::mem::zeroed() };
        lk.l_type = libc::F_UNLCK as _;
        // SAFETY: geçerli tanıtıcı ve flock yapısı.
        unsafe { libc::fcntl(self.0.as_raw_fd(), libc::F_SETLK, &lk) };
    }
}

/// shadow araçlarıyla uyumlu fcntl kilidi (en fazla 15 sn bekler).
fn lock_passwd(path: &str) -> Result<Option<PwLock>, PrivError> {
    if path.is_empty() {
        return Ok(None);
    }
    let f = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(path)?;
    let mut lk: libc::flock = unsafe { std::mem::zeroed() };
    lk.l_type = libc::F_WRLCK as _;
    lk.l_whence = libc::SEEK_SET as _;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        // SAFETY: geçerli tanıtıcı ve flock yapısı.
        if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETLK, &lk) } == 0 {
            return Ok(Some(PwLock(f)));
        }
        if Instant::now() > deadline {
            return Err(PrivError::new("internal", "Hesap dosyaları kilitli."));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn read_lines(path: &str) -> std::io::Result<Vec<String>> {
    let s = std::fs::read_to_string(path)?;
    let s = s.trim_end_matches('\n');
    if s.is_empty() {
        return Ok(Vec::new());
    }
    Ok(s.split('\n').map(String::from).collect())
}

fn join_lines(l: &[String]) -> Vec<u8> {
    let mut s = l.join("\n");
    s.push('\n');
    s.into_bytes()
}

/// "ad:x:gid:üyeler" satırının üye listesine kullanıcıyı ekler.
fn add_member(line: &str, user: &str) -> String {
    let mut f: Vec<String> = line.split(':').map(String::from).collect();
    if f.len() < 4 || f[3].split(',').any(|m| m == user) {
        return line.to_string();
    }
    if f[3].is_empty() {
        f[3] = user.to_string();
    } else {
        f[3] = format!("{},{user}", f[3]);
    }
    f.join(":")
}

/// Hesabı oluşturur veya (kurulumdan önce var olan, UID ≥ min_uid) devralır.
pub fn user_create(shadow: &str, min_uid: u32, uf: &UserFiles, a: &UserCreateArgs) -> Result<UserCreateResult, PrivError> {
    if !valid_username(&a.user) {
        return Err(PrivError::new("invalid", "Kullanıcı adı küçük harfle başlamalı; yalnızca a-z, 0-9, '-' ve '_' içerebilir (en fazla 32)."));
    }
    if !valid_new_password(&a.password) {
        return Err(PrivError::new("invalid", "Parola en az 6 karakter olmalıdır."));
    }
    if !valid_fullname(&a.fullname) {
        return Err(PrivError::new("invalid", "Ad soyad geçersiz karakter içeriyor."));
    }
    if !uf.setup_flag.is_empty() && Path::new(&uf.setup_flag).exists() {
        return Err(PrivError::new("forbidden", "Kurulum tamamlandı; bu işlem yalnızca ilk açılışta yapılabilir."));
    }
    let hash = super::crypt::hash_password(&a.password);
    let _lock = lock_passwd(&uf.lock)?;

    let mut passwd = read_lines(&uf.passwd)?;
    let mut groups = read_lines(&uf.group)?;
    let mut shadow_l = read_lines(shadow)?;
    let gs = read_lines(&uf.gshadow);
    let has_gshadow = gs.is_ok();
    let mut gshadow = gs.unwrap_or_default();

    let mut res = UserCreateResult::default();
    let (mut uid, mut gid): (i64, i64) = (-1, -1);
    let mut home = Path::new(&uf.home_base).join(&a.user).to_string_lossy().into_owned();
    let mut used_uid = HashSet::new();
    let mut used_gid = HashSet::new();
    for l in passwd.iter_mut() {
        let mut f: Vec<String> = l.split(':').map(String::from).collect();
        if f.len() < 7 {
            continue;
        }
        let n: i64 = f[2].parse().unwrap_or(0);
        used_uid.insert(n);
        if f[0] == a.user {
            uid = n;
            gid = f[3].parse().unwrap_or(0);
            home = f[5].clone();
            if !a.fullname.is_empty() {
                f[4] = a.fullname.clone();
                *l = f.join(":");
            }
        }
    }
    for l in &groups {
        let f: Vec<&str> = l.split(':').collect();
        if f.len() >= 3 {
            used_gid.insert(f[2].parse::<i64>().unwrap_or(0));
            if f[0] == a.user && uid < 0 {
                return Err(PrivError::new("conflict", "Bu ad bir grup tarafından kullanılıyor."));
            }
        }
    }
    let min = i64::from(min_uid);
    if uid >= 0 {
        if uid < min || uid >= 60000 {
            return Err(PrivError::new("forbidden", "Bu ad bir sistem hesabına ait."));
        }
    } else {
        res.created = true;
        uid = min;
        while used_uid.contains(&uid) {
            uid += 1;
        }
        gid = uid;
        while used_gid.contains(&gid) {
            gid += 1;
        }
        if uid >= 60000 || gid >= 60000 {
            return Err(PrivError::new("internal", "Boş kullanıcı numarası kalmadı."));
        }
        let shell = uf.shells.iter().find(|s| Path::new(s).exists()).cloned().unwrap_or_else(|| "/bin/sh".into());
        passwd.push([a.user.as_str(), "x", &uid.to_string(), &gid.to_string(), &a.fullname, &home, &shell].join(":"));
        groups.push(format!("{}:x:{gid}:", a.user));
        let day = jiff::Timestamp::now().as_second() / 86400;
        shadow_l.push(format!("{}:!:{day}:0:99999:7:::", a.user));
        if has_gshadow {
            gshadow.push(format!("{}:!::", a.user));
        }
    }
    let want: HashSet<&str> = DEFAULT_USER_GROUPS.iter().copied().collect();
    for l in groups.iter_mut().chain(gshadow.iter_mut()) {
        let name = l.split(':').next().unwrap_or("");
        if want.contains(name) {
            *l = add_member(l, &a.user);
        }
    }
    // Sıra: shadow → group/gshadow → passwd (passwd'de görünen hesabın shadow kaydı hep vardır).
    replace_file(Path::new(shadow), &join_lines(&shadow_l), 0o600)?;
    if has_gshadow {
        replace_file(Path::new(&uf.gshadow), &join_lines(&gshadow), 0o600)?;
    }
    replace_file(Path::new(&uf.group), &join_lines(&groups), 0o644)?;
    replace_file(Path::new(&uf.passwd), &join_lines(&passwd), 0o644)?;
    super::shadow::set_shadow_hash(shadow, &a.user, &hash, jiff::Timestamp::now().as_second()).map_err(|e| PrivError::new("internal", e.to_string()))?;
    make_home(Path::new(&home), &uf.skel, uid as u32, gid as u32)?;
    res.uid = uid as u32;
    if !uf.systemctl.is_empty() && Path::new(&uf.systemctl).exists() {
        let unit = format!("makerd-user@{}.service", a.user);
        res.service = run_timeout(&uf.systemctl, &["enable", "--now", &unit], Duration::from_secs(20));
    }
    Ok(res)
}

fn run_timeout(cmd: &str, args: &[&str], d: Duration) -> bool {
    let Ok(mut c) = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).spawn() else { return false };
    let end = Instant::now() + d;
    loop {
        match c.try_wait() {
            Ok(Some(st)) => return st.success(),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = c.kill();
                let _ = c.wait();
                return false;
            }
        }
    }
}

/// Ev dizinini yoksa oluşturur, /etc/skel'i kopyalar ve sahipliği verir.
fn make_home(home: &Path, skel: &str, uid: u32, gid: u32) -> std::io::Result<()> {
    if home.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(p) = home.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::DirBuilder::new().mode(0o700).create(home)?;
    let is_root = nix::unistd::geteuid().is_root();
    let chown = |p: &Path| -> std::io::Result<()> {
        match std::os::unix::fs::lchown(p, Some(uid), Some(gid)) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !is_root => Ok(()),
            r => r,
        }
    };
    chown(home)?;
    if skel.is_empty() {
        return Ok(());
    }
    copy_tree(Path::new(skel), home, &chown)
}

fn copy_tree(src: &Path, dst: &Path, chown: &dyn Fn(&Path) -> std::io::Result<()>) -> std::io::Result<()> {
    let Ok(rd) = std::fs::read_dir(src) else { return Ok(()) };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(md) = e.path().symlink_metadata() else { continue };
        let to: PathBuf = dst.join(e.file_name());
        let ft = md.file_type();
        if ft.is_dir() {
            std::fs::DirBuilder::new().mode(md.permissions().mode() & 0o7777).create(&to)?;
            chown(&to)?;
            copy_tree(&e.path(), &to, chown)?;
        } else if ft.is_file() {
            let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(md.permissions().mode() & 0o7777).open(&to)?;
            std::io::copy(&mut std::fs::File::open(e.path())?, &mut out)?;
            chown(&to)?;
        } else if ft.is_symlink() {
            if let Ok(t) = std::fs::read_link(e.path()) {
                std::os::unix::fs::symlink(t, &to)?;
                chown(&to)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (PathBuf, UserFiles, String) {
        let d = std::env::temp_dir().join(format!("mk-users-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(d.join("skel/.config")).unwrap();
        std::fs::write(d.join("skel/.profile"), "export X=1\n").unwrap();
        std::fs::write(d.join("passwd"), "root:x:0:0:root:/root:/bin/sh\nmaker:x:999:999::/var/lib/makerd:/sbin/nologin\n").unwrap();
        std::fs::write(d.join("group"), "root:x:0:\nwheel:x:10:root\nvideo:x:44:\nmaker:x:999:\n").unwrap();
        std::fs::write(d.join("shadow"), "root:*:1::::::\nmaker:!:1::::::\n").unwrap();
        std::fs::write(d.join("gshadow"), "wheel:!::root\nvideo:!::\n").unwrap();
        let uf = UserFiles {
            passwd: d.join("passwd").to_string_lossy().into(),
            group: d.join("group").to_string_lossy().into(),
            gshadow: d.join("gshadow").to_string_lossy().into(),
            lock: d.join(".pwd.lock").to_string_lossy().into(),
            home_base: d.join("home").to_string_lossy().into(),
            skel: d.join("skel").to_string_lossy().into(),
            setup_flag: d.join("setup-done").to_string_lossy().into(),
            systemctl: String::new(),
            shells: vec!["/bin/sh".into()],
        };
        let shadow = d.join("shadow").to_string_lossy().into_owned();
        (d, uf, shadow)
    }

    #[test]
    fn creates_user() {
        let (d, uf, shadow) = setup();
        let a = UserCreateArgs { user: "ali".into(), password: "parola1".into(), fullname: "Ali Veli".into() };
        let r = user_create(&shadow, 1000, &uf, &a).unwrap();
        assert!(r.created);
        assert_eq!(r.uid, 1000);
        let p = std::fs::read_to_string(&uf.passwd).unwrap();
        assert!(p.contains("ali:x:1000:1000:Ali Veli:"));
        let g = std::fs::read_to_string(&uf.group).unwrap();
        assert!(g.contains("wheel:x:10:root,ali"));
        assert!(g.contains("video:x:44:ali"));
        assert!(g.contains("ali:x:1000:"));
        assert!(std::fs::read_to_string(&uf.gshadow).unwrap().contains("wheel:!::root,ali"));
        assert!(super::super::shadow::check_shadow(&shadow, "ali", "parola1").unwrap());
        assert!(d.join("home/ali/.profile").exists());
        // ikinci kez: devralma (created=false)
        let r2 = user_create(&shadow, 1000, &uf, &UserCreateArgs { password: "baska12".into(), ..a.clone() }).unwrap();
        assert!(!r2.created);
        assert!(super::super::shadow::check_shadow(&shadow, "ali", "baska12").unwrap());
        // sistem hesabı devralınamaz; kurulum bittiyse reddedilir
        assert_eq!(user_create(&shadow, 1000, &uf, &UserCreateArgs { user: "maker2".into(), ..a.clone() }).unwrap().uid, 1001);
        std::fs::write(&uf.setup_flag, "{}").unwrap();
        assert_eq!(user_create(&shadow, 1000, &uf, &a).unwrap_err().code, "forbidden");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn validators() {
        assert!(valid_username("ali_1"));
        assert!(!valid_username("root"));
        assert!(!valid_username("systemd-x"));
        assert!(!valid_username("1ali"));
        assert!(valid_fullname("Ayşe Yılmaz"));
        assert!(!valid_fullname("a:b"));
        assert!(valid_new_password("123456"));
        assert!(!valid_new_password("12345"));
    }
}
