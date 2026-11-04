//! Ortak yardımcılar: rastgele belirteç, atomik dosya yazma, kabuk tırnaklama, zaman.

use base64::Engine;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// n rastgele baytın base64url (dolgusuz) gösterimi.
pub fn random_token(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::fill(&mut b[..]);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// n rastgele baytın onaltılık gösterimi.
pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::fill(&mut b[..]);
    hex::encode(b)
}

/// Tek tırnaklı POSIX kabuk sözcüğü.
pub fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Go'nun %q biçimine yakın: çift tırnaklı, ters bölü ve tırnak kaçışlı (TOML/JSON dizesi olarak da geçerli).
pub fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// Şimdiki zaman, yerel saat diliminde RFC 3339 (ör. 2026-11-04T22:30:00+03:00).
pub fn now_rfc3339_local() -> String {
    jiff::Zoned::now().strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

pub fn now() -> jiff::Timestamp {
    jiff::Timestamp::now()
}

/// Saniyeye yuvarlanmış UTC zaman damgası.
pub fn now_secs() -> jiff::Timestamp {
    jiff::Timestamp::from_second(jiff::Timestamp::now().as_second()).unwrap_or_else(|_| jiff::Timestamp::now())
}

pub fn ts_from_system(t: std::time::SystemTime) -> jiff::Timestamp {
    jiff::Timestamp::try_from(t).unwrap_or(jiff::Timestamp::UNIX_EPOCH)
}

/// Dosyayı atomik olarak (geçici dosya + fsync + rename) değiştirir. Var olan
/// dosyanın izinleri ve sahipliği korunur; yoksa `perm` kullanılır.
pub fn replace_file(path: &Path, data: &[u8], perm: u32) -> std::io::Result<()> {
    let (mode, owner) = match fs::metadata(path) {
        Ok(m) => (m.permissions().mode() & 0o7777, Some((m.uid(), m.gid()))),
        Err(_) => (perm, None),
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let base = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{base}-{}", random_hex(6)));
    let res = (|| {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(data)?;
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        if let Some((u, g)) = owner {
            let _ = std::os::unix::fs::fchown(&f, Some(u), Some(g));
        }
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// Basit atomik yazma (geçici dosya + fsync + rename), verilen izinle.
pub fn write_atomic(path: &Path, data: &[u8], perm: u32) -> std::io::Result<()> {
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    let res = (|| {
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(perm).open(&tmp)?;
        f.write_all(data)?;
        f.set_permissions(fs::Permissions::from_mode(perm))?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// /etc/passwd satırları: (ad, uid, gid, gecos, ev, kabuk).
pub struct PasswdEntry {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
}

pub fn passwd_entries(path: &str) -> Vec<PasswdEntry> {
    let Ok(s) = fs::read_to_string(path) else { return Vec::new() };
    s.lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split(':').collect();
            if p.len() < 7 {
                return None;
            }
            Some(PasswdEntry {
                name: p[0].into(),
                uid: p[2].parse().ok()?,
                gid: p[3].parse().ok()?,
                home: p[5].into(),
                shell: p[6].into(),
            })
        })
        .collect()
}

/// Kullanıcı adından UID (NSS: önce libc, sonra /etc/passwd).
pub fn uid_of(name: &str) -> Option<u32> {
    if let Ok(Some(u)) = nix::unistd::User::from_name(name) {
        return Some(u.uid.as_raw());
    }
    passwd_entries("/etc/passwd").into_iter().find(|e| e.name == name).map(|e| e.uid)
}

/// UID'den kullanıcı adı.
pub fn name_of(uid: u32) -> Option<String> {
    if let Ok(Some(u)) = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid)) {
        return Some(u.name);
    }
    passwd_entries("/etc/passwd").into_iter().find(|e| e.uid == uid).map(|e| e.name)
}

/// Grup adından GID.
pub fn gid_of(name: &str) -> Option<u32> {
    nix::unistd::Group::from_name(name).ok().flatten().map(|g| g.gid.as_raw())
}

pub fn getuid() -> u32 {
    nix::unistd::getuid().as_raw()
}

/// PATH içinde çalıştırılabilir dosya arar.
pub fn look_path(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return is_exec(&p).then_some(p);
    }
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    path.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join(name)).find(|p| is_exec(p))
}

fn is_exec(p: &Path) -> bool {
    fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Dosya içeriği, kırpılmış; okunamazsa boş.
pub fn read_trim(p: impl AsRef<Path>) -> String {
    fs::read(p).map(|b| String::from_utf8_lossy(&b).trim_end_matches('\0').trim().to_string()).unwrap_or_default()
}

/// Son n baytı (UTF-8 kayıpsız) kırpılmış metin olarak döndürür.
pub fn tail(b: &[u8], n: usize) -> String {
    let b = if b.len() > n { &b[b.len() - n..] } else { b };
    String::from_utf8_lossy(b).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote() {
        assert_eq!(shq("a b"), "'a b'");
        assert_eq!(shq("it's"), r"'it'\''s'");
        assert_eq!(quote("a\"b"), r#""a\"b""#);
    }

    #[test]
    fn tokens_are_urlsafe() {
        let t = random_token(18);
        assert_eq!(t.len(), 24);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
