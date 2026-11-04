//! Kullanıcı adına dosya işlemleri. makerd-user (kullanıcının kendi hesabı)
//! bunları soket üzerinden yapar; mock modda aynı işlevler süreç içinde çağrılır.

use super::Open;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: i64,
    pub mtime: jiff::Timestamp,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub link: bool,
    pub mode: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Listing {
    pub path: String,
    pub home: String,
    pub entries: Vec<Entry>,
}

/// Kullanıcıya gösterilecek dosya sistemi hatası.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError(pub String);

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FsError {}

pub fn fs_err(e: std::io::Error) -> FsError {
    use std::io::ErrorKind::*;
    FsError(
        match e.kind() {
            NotFound => "Dosya veya klasör bulunamadı.",
            PermissionDenied => "Bu konuma erişim izniniz yok.",
            AlreadyExists => "Bu adla bir dosya veya klasör zaten var.",
            _ => return FsError(e.to_string()),
        }
        .into(),
    )
}

/// filepath.Clean eşdeğeri: "." ve ".." bileşenlerini çözer.
pub fn clean(p: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) => {}
                _ => out.push(c),
            },
            other => out.push(other),
        }
    }
    let r: PathBuf = out.iter().collect();
    if r.as_os_str().is_empty() { PathBuf::from(".") } else { r }
}

/// Yolu ev dizinine göre çözer ("~", "~/x", göreli yollar).
pub fn resolve(home: &str, p: &str) -> PathBuf {
    if p.is_empty() || p == "~" {
        return PathBuf::from(home);
    }
    if let Some(r) = p.strip_prefix("~/") {
        return clean(&Path::new(home).join(r));
    }
    if !p.starts_with('/') {
        return clean(&Path::new(home).join(p));
    }
    clean(Path::new(p))
}

/// Go FileMode.Perm().String() biçimi: "-rwxr-xr-x".
pub fn perm_string(mode: u32) -> String {
    let mut s = String::from("-");
    for (i, c) in "rwxrwxrwx".chars().enumerate() {
        s.push(if mode & (1 << (8 - i)) != 0 { c } else { '-' });
    }
    s
}

fn entry(name: String, md: &std::fs::Metadata) -> Entry {
    Entry {
        name,
        dir: md.is_dir(),
        size: md.len() as i64,
        mtime: md.modified().map(crate::util::ts_from_system).unwrap_or(jiff::Timestamp::UNIX_EPOCH),
        link: false,
        mode: perm_string(md.permissions().mode()),
    }
}

/// Dizini listeler (klasörler önce, sonra ada göre).
pub fn list(home: &str, p: &str) -> Result<Listing, FsError> {
    let dir = resolve(home, p);
    let rd = std::fs::read_dir(&dir).map_err(fs_err)?;
    let mut out = Listing { path: dir.to_string_lossy().into_owned(), home: home.into(), entries: Vec::new() };
    for de in rd.flatten() {
        let Ok(md) = de.path().symlink_metadata() else { continue };
        let name = de.file_name().to_string_lossy().into_owned();
        let mut e = entry(name, &md);
        if md.file_type().is_symlink() {
            e.link = true;
            if let Ok(st) = std::fs::metadata(de.path()) {
                e.dir = st.is_dir();
                e.size = st.len() as i64;
            }
        }
        out.entries.push(e);
    }
    out.entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// mkdir, rename, remove, stat ve list; JSON yanıt.
pub fn fs_op(home: &str, o: &Open) -> Result<Vec<u8>, FsError> {
    let p = resolve(home, &o.path);
    match o.mode.as_str() {
        "list" => return serde_json::to_vec(&list(home, &o.path)?).map_err(|e| FsError(e.to_string())),
        "mkdir" => std::fs::create_dir_all(&p).map_err(fs_err)?,
        "rename" => {
            let to = resolve(home, &o.to);
            if to.symlink_metadata().is_ok() {
                return Err(FsError("Hedefte aynı adla bir dosya var.".into()));
            }
            std::fs::rename(&p, &to).map_err(fs_err)?;
        }
        "remove" => {
            if p == Path::new(home) || p == Path::new("/") {
                return Err(FsError("Ev dizini silinemez.".into()));
            }
            match p.symlink_metadata() {
                Ok(md) if md.is_dir() => std::fs::remove_dir_all(&p).map_err(fs_err)?,
                Ok(_) => std::fs::remove_file(&p).map_err(fs_err)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fs_err(e)),
            }
        }
        "stat" => {
            let md = std::fs::metadata(&p).map_err(fs_err)?;
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into());
            return serde_json::to_vec(&entry(name, &md)).map_err(|e| FsError(e.to_string()));
        }
        m => return Err(FsError(format!("Bilinmeyen işlem: {m}"))),
    }
    Ok(br#"{"ok":true}"#.to_vec())
}

/// Dosyayı okumak için açar (klasör değil).
pub fn open_read(home: &str, p: &str) -> Result<std::fs::File, FsError> {
    let f = std::fs::File::open(resolve(home, p)).map_err(fs_err)?;
    if f.metadata().map(|m| m.is_dir()).unwrap_or(false) {
        return Err(FsError("Klasör indirilemez.".into()));
    }
    Ok(f)
}

/// Parça parça yazılıp sonunda yerine taşınan dosya (yarım dosya kalmaz).
pub struct AtomicWriter {
    tmp: PathBuf,
    dst: PathBuf,
    f: Option<std::fs::File>,
}

impl AtomicWriter {
    pub fn create(home: &str, p: &str) -> Result<Self, FsError> {
        let dst = resolve(home, p);
        let dir = dst.parent().unwrap_or(Path::new("/")).to_path_buf();
        std::fs::create_dir_all(&dir).map_err(fs_err)?;
        let tmp = dir.join(format!(".mk-yukleme-{}", crate::util::random_hex(6)));
        let f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).map_err(fs_err)?;
        Ok(Self { tmp, dst, f: Some(f) })
    }

    pub fn write(&mut self, b: &[u8]) -> Result<(), FsError> {
        self.f.as_mut().ok_or_else(|| FsError("kapalı".into()))?.write_all(b).map_err(fs_err)
    }

    pub fn commit(mut self) -> Result<(), FsError> {
        let f = self.f.take().ok_or_else(|| FsError("kapalı".into()))?;
        drop(f);
        std::fs::set_permissions(&self.tmp, std::fs::Permissions::from_mode(0o644)).map_err(fs_err)?;
        std::fs::rename(&self.tmp, &self.dst).map_err(fs_err)
    }
}

impl Drop for AtomicWriter {
    fn drop(&mut self) {
        if self.f.is_some() {
            let _ = std::fs::remove_file(&self.tmp);
        }
    }
}

/// Tüm içeriği atomik yazar.
pub fn write_file_atomic(home: &str, p: &str, data: &[u8]) -> Result<(), FsError> {
    let mut w = AtomicWriter::create(home, p)?;
    w.write(data)?;
    w.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_paths() {
        assert_eq!(resolve("/home/a", ""), PathBuf::from("/home/a"));
        assert_eq!(resolve("/home/a", "~/x/../y"), PathBuf::from("/home/a/y"));
        assert_eq!(resolve("/home/a", "Projeler/p"), PathBuf::from("/home/a/Projeler/p"));
        assert_eq!(resolve("/home/a", "/etc/../tmp"), PathBuf::from("/tmp"));
        assert_eq!(perm_string(0o755), "-rwxr-xr-x");
        assert_eq!(perm_string(0o640), "-rw-r-----");
    }

    #[test]
    fn ops() {
        let h = std::env::temp_dir().join(format!("mk-fs-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&h).unwrap();
        let home = h.to_str().unwrap();
        let op = |mode: &str, path: &str, to: &str| fs_op(home, &Open { mode: mode.into(), path: path.into(), to: to.into(), ..Default::default() });
        op("mkdir", "a/b", "").unwrap();
        write_file_atomic(home, "a/b/c.txt", b"selam").unwrap();
        write_file_atomic(home, "z.txt", b"1").unwrap();
        let l = list(home, "a/b").unwrap();
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].name, "c.txt");
        let top = list(home, "").unwrap();
        assert_eq!(top.entries[0].name, "a"); // klasörler önce
        assert!(op("rename", "z.txt", "a/b/c.txt").is_err());
        op("rename", "z.txt", "y.txt").unwrap();
        let st: Entry = serde_json::from_slice(&op("stat", "y.txt", "").unwrap()).unwrap();
        assert_eq!(st.size, 1);
        assert_eq!(op("remove", "", "").unwrap_err().0, "Ev dizini silinemez.");
        op("remove", "a", "").unwrap();
        assert!(open_read(home, "a/b/c.txt").is_err());
        assert_eq!(open_read(home, "yok").unwrap_err().0, "Dosya veya klasör bulunamadı.");
        let _ = std::fs::remove_dir_all(h);
    }
}
