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

// --- kopyala, çöp kutusu, arama, konumlar ------------------------------------------------

/// Hedef varsa "ad (2).uzantı" biçiminde boş bir ad bulur (Finder/Dosyalar gibi).
fn free_name(p: &Path) -> PathBuf {
    if p.symlink_metadata().is_err() {
        return p.to_path_buf();
    }
    let dir = p.parent().unwrap_or(Path::new("/"));
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..10_000).map(|i| dir.join(format!("{stem} ({i}){ext}"))).find(|c| c.symlink_metadata().is_err()).unwrap_or_else(|| p.to_path_buf())
}

/// Klasörü (bağlantılar bağlantı olarak) özyinelemeli kopyalar.
fn copy_tree(src: &Path, dst: &Path) -> Result<(), FsError> {
    let md = src.symlink_metadata().map_err(fs_err)?;
    if md.file_type().is_symlink() {
        let t = std::fs::read_link(src).map_err(fs_err)?;
        std::os::unix::fs::symlink(t, dst).map_err(fs_err)?;
    } else if md.is_dir() {
        std::fs::create_dir(dst).map_err(fs_err)?;
        for e in std::fs::read_dir(src).map_err(fs_err)?.flatten() {
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
        let _ = std::fs::set_permissions(dst, md.permissions());
    } else {
        std::fs::copy(src, dst).map_err(fs_err)?;
    }
    Ok(())
}

/// freedesktop.org çöp kutusu: ~/.local/share/Trash/{files,info}.
fn trash_dirs(home: &str) -> (PathBuf, PathBuf) {
    let t = Path::new(home).join(".local/share/Trash");
    (t.join("files"), t.join("info"))
}

fn pct_encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Çöpteki bir öğe (liste için).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrashItem {
    /// Çöp içindeki ad (geri yükleme/silme anahtarı).
    pub id: String,
    pub name: String,
    /// Asıl konumu.
    pub origin: String,
    pub deleted: String,
    pub dir: bool,
    pub size: i64,
}

fn trash_put(home: &str, p: &Path) -> Result<(), FsError> {
    if p == Path::new(home) || p == Path::new("/") {
        return Err(FsError("Ev dizini çöpe atılamaz.".into()));
    }
    let (files, info) = trash_dirs(home);
    if p.starts_with(&files) {
        return Err(FsError("Zaten çöp kutusunda.".into()));
    }
    std::fs::create_dir_all(&files).map_err(fs_err)?;
    std::fs::create_dir_all(&info).map_err(fs_err)?;
    let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).ok_or_else(|| FsError("Geçersiz yol.".into()))?;
    let dst = free_name(&files.join(&name));
    let id = dst.file_name().unwrap().to_string_lossy().into_owned();
    let when = jiff::Zoned::now().strftime("%Y-%m-%dT%H:%M:%S").to_string();
    std::fs::write(info.join(format!("{id}.trashinfo")), format!("[Trash Info]\nPath={}\nDeletionDate={when}\n", pct_encode(&p.to_string_lossy()))).map_err(fs_err)?;
    if let Err(e) = std::fs::rename(p, &dst) {
        let _ = std::fs::remove_file(info.join(format!("{id}.trashinfo")));
        // Başka bir dosya sistemi (USB bellek): çöpe taşınamaz.
        if e.raw_os_error() == Some(18) {
            return Err(FsError("Bu sürücüdeki öğeler çöpe atılamaz; kalıcı olarak silin.".into()));
        }
        return Err(fs_err(e));
    }
    Ok(())
}

fn trash_list(home: &str) -> Vec<TrashItem> {
    let (files, info) = trash_dirs(home);
    let mut out = Vec::new();
    for e in std::fs::read_dir(&files).into_iter().flatten().flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        let txt = std::fs::read_to_string(info.join(format!("{id}.trashinfo"))).unwrap_or_default();
        let field = |k: &str| txt.lines().find_map(|l| l.strip_prefix(k)).unwrap_or("").to_string();
        let origin = pct_decode(&field("Path="));
        let md = e.path().symlink_metadata().ok();
        out.push(TrashItem {
            name: Path::new(&origin).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| id.clone()),
            id,
            origin,
            deleted: field("DeletionDate="),
            dir: md.as_ref().is_some_and(|m| m.is_dir()),
            size: md.map(|m| m.len() as i64).unwrap_or(0),
        });
    }
    out.sort_by(|a, b| b.deleted.cmp(&a.deleted));
    out
}

fn trash_id_ok(id: &str) -> Result<(), FsError> {
    if id.is_empty() || id.contains('/') || id == "." || id == ".." {
        return Err(FsError("Geçersiz çöp öğesi.".into()));
    }
    Ok(())
}

/// Ad içinde arar (büyük/küçük harf duyarsız), en fazla 300 sonuç, 8 düzey derinlik; gizli klasörlere girmez.
fn search(home: &str, dir: &Path, q: &str) -> Listing {
    let q = q.to_lowercase();
    let mut out = Listing { path: dir.to_string_lossy().into_owned(), home: home.into(), entries: Vec::new() };
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(md) = e.path().symlink_metadata() else { continue };
            if name.to_lowercase().contains(&q) {
                // Sonucun klasöre göre yolu: kabuk tıklayınca açabilsin.
                let rel = e.path().strip_prefix(dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or(name.clone());
                out.entries.push(entry(rel, &md));
                if out.entries.len() >= 300 {
                    return out;
                }
            }
            if md.is_dir() && !name.starts_with('.') && depth < 8 && name != "node_modules" && name != "__pycache__" {
                stack.push((e.path(), depth + 1));
            }
        }
    }
    out.entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

/// Kenar çubuğu konumları: ev altındaki standart klasörler (varsa) ve takılı sürücüler (USB bellek, SD).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Place {
    pub id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drive: bool,
}

fn places(home: &str) -> Vec<Place> {
    // Standart klasörler yoksa oluşturulur (yeni hesapta da kenar çubuğu dolu olsun).
    for d in ["Masaüstü", "Belgeler", "İndirilenler", "Resimler", "Videolar"] {
        let _ = std::fs::create_dir(Path::new(home).join(d));
    }
    let mut out = Vec::new();
    for (id, d) in [("home", ""), ("desktop", "Masaüstü"), ("documents", "Belgeler"), ("downloads", "İndirilenler"), ("pictures", "Resimler"), ("videos", "Videolar"), ("projects", "Projeler")] {
        let p = if d.is_empty() { PathBuf::from(home) } else { Path::new(home).join(d) };
        if id == "home" || p.is_dir() {
            out.push(Place { id: id.into(), path: p.to_string_lossy().into_owned(), drive: false });
        }
    }
    let user = Path::new(home).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    for base in [format!("/run/media/{user}"), format!("/media/{user}"), "/media".into(), "/mnt".into()] {
        for e in std::fs::read_dir(&base).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && std::fs::read_dir(&p).is_ok() && !out.iter().any(|x| x.path == p.to_string_lossy()) && p.to_string_lossy() != format!("/media/{user}") {
                out.push(Place { id: e.file_name().to_string_lossy().into_owned(), path: p.to_string_lossy().into_owned(), drive: true });
            }
        }
    }
    out
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
        // Kopyala: hedef klasöre (ya da tam hedef yola); ad çakışırsa "ad (2)".
        "copy" => {
            let mut to = resolve(home, &o.to);
            if to.is_dir() {
                to = to.join(p.file_name().ok_or_else(|| FsError("Geçersiz yol.".into()))?);
            }
            let to = free_name(&to);
            if to.starts_with(&p) {
                return Err(FsError("Klasör kendi içine kopyalanamaz.".into()));
            }
            copy_tree(&p, &to)?;
            return serde_json::to_vec(&serde_json::json!({ "path": to.to_string_lossy() })).map_err(|e| FsError(e.to_string()));
        }
        // Taşı: hedef klasöre; ad çakışırsa hata (kabuk sorar).
        "move" => {
            let mut to = resolve(home, &o.to);
            if to.is_dir() {
                to = to.join(p.file_name().ok_or_else(|| FsError("Geçersiz yol.".into()))?);
            }
            if to == p {
                return Ok(br#"{"ok":true}"#.to_vec());
            }
            if to.starts_with(&p) {
                return Err(FsError("Klasör kendi içine taşınamaz.".into()));
            }
            if to.symlink_metadata().is_ok() {
                return Err(FsError("Hedefte aynı adla bir dosya var.".into()));
            }
            if let Err(e) = std::fs::rename(&p, &to) {
                if e.raw_os_error() != Some(18) {
                    return Err(fs_err(e));
                }
                // Farklı sürücü: kopyala + sil.
                copy_tree(&p, &to)?;
                if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) }.map_err(fs_err)?;
            }
        }
        "trash" => trash_put(home, &p)?,
        "trash-list" => return serde_json::to_vec(&trash_list(home)).map_err(|e| FsError(e.to_string())),
        // path = çöp öğesinin kimliği
        "restore" => {
            trash_id_ok(&o.path)?;
            let (files, info) = trash_dirs(home);
            let id = &o.path;
            let txt = std::fs::read_to_string(info.join(format!("{id}.trashinfo"))).map_err(fs_err)?;
            let origin = pct_decode(txt.lines().find_map(|l| l.strip_prefix("Path=")).unwrap_or(""));
            if origin.is_empty() {
                return Err(FsError("Asıl konum bilinmiyor.".into()));
            }
            let dst = free_name(Path::new(&origin));
            if let Some(d) = dst.parent() {
                std::fs::create_dir_all(d).map_err(fs_err)?;
            }
            std::fs::rename(files.join(id), &dst).map_err(fs_err)?;
            let _ = std::fs::remove_file(info.join(format!("{id}.trashinfo")));
            return serde_json::to_vec(&serde_json::json!({ "path": dst.to_string_lossy() })).map_err(|e| FsError(e.to_string()));
        }
        // path = çöp öğesinin kimliği; boşsa tüm çöp boşaltılır
        "trash-delete" => {
            let (files, info) = trash_dirs(home);
            let ids: Vec<String> = if o.path.is_empty() {
                std::fs::read_dir(&files).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()
            } else {
                trash_id_ok(&o.path)?;
                vec![o.path.clone()]
            };
            for id in ids {
                let f = files.join(&id);
                match f.symlink_metadata() {
                    Ok(md) if md.is_dir() => std::fs::remove_dir_all(&f).map_err(fs_err)?,
                    Ok(_) => std::fs::remove_file(&f).map_err(fs_err)?,
                    Err(_) => {}
                }
                let _ = std::fs::remove_file(info.join(format!("{id}.trashinfo")));
            }
        }
        // path = aranacak klasör, to = sorgu
        "search" => {
            if o.to.trim().is_empty() {
                return serde_json::to_vec(&list(home, &o.path)?).map_err(|e| FsError(e.to_string()));
            }
            return serde_json::to_vec(&search(home, &p, o.to.trim())).map_err(|e| FsError(e.to_string()));
        }
        "places" => return serde_json::to_vec(&places(home)).map_err(|e| FsError(e.to_string())),
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

    #[test]
    fn copy_move_trash_search() {
        let h = std::env::temp_dir().join(format!("mk-fs2-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&h).unwrap();
        let home = h.to_str().unwrap();
        let op = |mode: &str, path: &str, to: &str| fs_op(home, &Open { mode: mode.into(), path: path.into(), to: to.into(), ..Default::default() });
        op("mkdir", "Belgeler/rapor", "").unwrap();
        write_file_atomic(home, "Belgeler/rapor/not ğ.txt", b"x").unwrap();
        // Kopya: aynı klasöre → "ad (2)"
        let r: serde_json::Value = serde_json::from_slice(&op("copy", "Belgeler/rapor", "Belgeler").unwrap()).unwrap();
        assert!(r["path"].as_str().unwrap().ends_with("Belgeler/rapor (2)"), "{r}");
        assert!(h.join("Belgeler/rapor (2)/not ğ.txt").is_file());
        assert!(op("copy", "Belgeler", "Belgeler/rapor").is_err(), "kendi içine kopya");
        // Taşı
        op("mkdir", "Arsiv", "").unwrap();
        op("move", "Belgeler/rapor (2)", "Arsiv").unwrap();
        assert!(h.join("Arsiv/rapor (2)/not ğ.txt").is_file());
        assert!(op("move", "Arsiv", "Arsiv/rapor (2)").is_err());
        // Arama
        let s: Listing = serde_json::from_slice(&op("search", "", "NOT").unwrap()).unwrap();
        assert_eq!(s.entries.len(), 2, "{:?}", s.entries.iter().map(|e| &e.name).collect::<Vec<_>>());
        // Çöp: at, listele, geri yükle, boşalt
        op("trash", "Belgeler/rapor/not ğ.txt", "").unwrap();
        assert!(!h.join("Belgeler/rapor/not ğ.txt").exists());
        let t: Vec<TrashItem> = serde_json::from_slice(&op("trash-list", "", "").unwrap()).unwrap();
        assert_eq!((t.len(), t[0].name.as_str()), (1, "not ğ.txt"));
        assert!(t[0].origin.ends_with("Belgeler/rapor/not ğ.txt"), "{}", t[0].origin);
        op("restore", &t[0].id, "").unwrap();
        assert!(h.join("Belgeler/rapor/not ğ.txt").is_file());
        assert!(op("restore", "../x", "").is_err());
        op("trash", "Arsiv", "").unwrap();
        op("trash-delete", "", "").unwrap();
        let t: Vec<TrashItem> = serde_json::from_slice(&op("trash-list", "", "").unwrap()).unwrap();
        assert!(t.is_empty());
        assert!(op("trash", "", "").is_err(), "ev dizini");
        let pl: Vec<Place> = serde_json::from_slice(&op("places", "", "").unwrap()).unwrap();
        assert!(pl.iter().any(|p| p.id == "documents"));
        let _ = std::fs::remove_dir_all(h);
    }
}
