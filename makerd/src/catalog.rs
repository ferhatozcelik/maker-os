//! Eklenti kataloğu (mağaza): GitHub'daki eklenti deposunun `plugins/<id>/` klasörleri.
//! Depoya bir klasör eklemek yeterlidir; sürüm, derleme veya yayın adımı yoktur.
//!
//! - Liste: tek GitHub API çağrısı (`git/trees/<dal>?recursive=1`) klasörleri ve dosyaları verir;
//!   her eklentinin `plugin.json`'u ve simgesi raw.githubusercontent.com'dan okunur. 10 dk önbellek.
//! - Kur: klasördeki dosyalar raw'dan indirilir, her biri ağaçtaki git blob özetiyle (SHA-1) denetlenir,
//!   zip'lenir ve [`plugins::import`] kurallarıyla kurulur.

use crate::apierr::{ApiError, ApiResult};
use crate::plugins::{self, Manifest};
use crate::uagent::Agent;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Varsayılan katalog: `sahip/depo@dal`.
pub const DEFAULT_SOURCE: &str = "ferhatozcelik/makeros-plugins@main";
const DIR: &str = "plugins/";
const TTL: Duration = Duration::from_secs(10 * 60);
const MAX_TREE: u64 = 8 << 20;
const MAX_MANIFEST: u64 = 16 << 10;
const MAX_ICON: u64 = 32 << 10;
const MAX_PLUGINS: usize = 200;

/// Klasördeki bir dosya (ağaçtan).
#[derive(Clone, Debug)]
struct File {
    rel: String,
    sha: String,
    size: u64,
}

/// Mağazada gösterilen eklenti.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// data: URI (svg/png/webp) ya da boş.
    pub icon: String,
    pub tags: Vec<String>,
    /// Klasördeki dosyaların toplam boyutu.
    pub size: u64,
    /// GitHub'daki klasör.
    pub url: String,
    #[serde(skip)]
    files: Vec<File>,
    /// Dosyaların indirileceği commit (raw önbelleği dal adresini birkaç dakika eski tutabilir).
    #[serde(skip)]
    commit: String,
}

#[derive(Serialize, Debug)]
pub struct Item {
    #[serde(flatten)]
    pub entry: Entry,
    /// Kurulu sürüm (yoksa null).
    pub installed: Option<String>,
    /// Depodaki sürüm kurulu olandan yeni.
    pub update: bool,
}

#[derive(Serialize, Debug)]
pub struct Listing {
    /// `sahip/depo@dal`
    pub source: String,
    /// Deponun GitHub adresi.
    pub repo_url: String,
    pub plugins: Vec<Item>,
}

#[derive(Deserialize)]
struct Branch {
    commit: BranchCommit,
}

#[derive(Deserialize)]
struct BranchCommit {
    sha: String,
}

#[derive(Deserialize)]
struct Tree {
    #[serde(default)]
    tree: Vec<Node>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct Node {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: String,
    #[serde(default)]
    size: u64,
}

pub struct Catalog {
    repo: String,
    branch: String,
    api: String,
    raw: String,
    ua: String,
    cache: Mutex<Option<(Instant, Vec<Entry>)>>,
}

/// Git blob özeti: sha1("blob <boyut>\0" + içerik).
fn blob_sha(b: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", b.len()).as_bytes());
    h.update(b);
    hex::encode(h.finalize())
}

/// Pakete girmeyen dosyalar (gizli, .venv, önbellek).
fn skipped(rel: &str) -> bool {
    rel.split('/').any(|p| p.starts_with('.') || p == "__pycache__")
}

fn icon_type(rel: &str) -> Option<&'static str> {
    match rel.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

impl Catalog {
    /// `source`: `sahip/depo` veya `sahip/depo@dal` (varsayılan dal main).
    pub fn new(source: &str, version: &str) -> Result<Self, String> {
        let (repo, branch) = source.split_once('@').unwrap_or((source, "main"));
        let ok = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'));
        let parts: Vec<&str> = repo.split('/').collect();
        if parts.len() != 2 || !parts.iter().all(|p| ok(p)) || !branch.split('/').all(ok) {
            return Err(format!("geçersiz eklenti kataloğu: {source} (beklenen sahip/depo[@dal])"));
        }
        Ok(Self {
            repo: repo.into(),
            branch: branch.into(),
            api: "https://api.github.com".into(),
            raw: "https://raw.githubusercontent.com".into(),
            ua: format!("makerd/{version}"),
            cache: Mutex::new(None),
        })
    }

    #[cfg(test)]
    fn with_bases(mut self, api: &str, raw: &str) -> Self {
        (self.api, self.raw) = (api.into(), raw.into());
        self
    }

    pub fn source(&self) -> String {
        format!("{}@{}", self.repo, self.branch)
    }

    fn raw_url(&self, commit: &str, id: &str, rel: &str) -> String {
        format!("{}/{}/{commit}/{DIR}{id}/{rel}", self.raw, self.repo)
    }

    async fn fetch(&self, url: String, limit: u64) -> Result<Vec<u8>, String> {
        let ua = self.ua.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let local = url.starts_with("http://127.0.0.1:");
            if !url.starts_with("https://") && !local {
                return Err("yalnız https adresleri desteklenir".into());
            }
            let ag: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(60))).build().into();
            let mut r = ag.get(&url).header("User-Agent", &ua).header("Accept", "application/vnd.github+json").call().map_err(|e| match e {
                ureq::Error::StatusCode(403 | 429) => "GitHub istek sınırı aşıldı; biraz sonra yeniden deneyin".to_string(),
                ureq::Error::StatusCode(404) => "depo veya dal bulunamadı".to_string(),
                ureq::Error::StatusCode(c) => format!("sunucu yanıtı {c}"),
                e => e.to_string(),
            })?;
            r.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    fn unavailable(e: String) -> ApiError {
        ApiError::unavailable(format!("Eklenti kataloğuna ulaşılamadı: {e}"))
    }

    /// Ağaçtaki `plugins/<id>/...` dosyaları → id başına dosya listesi.
    fn group(tree: Tree) -> BTreeMap<String, Vec<File>> {
        let mut by: BTreeMap<String, Vec<File>> = BTreeMap::new();
        for n in tree.tree.into_iter().filter(|n| n.kind == "blob") {
            let Some(rest) = n.path.strip_prefix(DIR) else { continue };
            let Some((id, rel)) = rest.split_once('/') else { continue };
            if !plugins::valid_id(id) || rel.is_empty() || skipped(rel) || n.sha.len() != 40 {
                continue;
            }
            by.entry(id.into()).or_default().push(File { rel: rel.into(), sha: n.sha, size: n.size });
        }
        by.retain(|_, f| f.iter().any(|f| f.rel == "plugin.json"));
        by
    }

    /// Ağaçtan ve plugin.json'lardan liste (önbellekli).
    async fn index(&self, refresh: bool) -> ApiResult<Vec<Entry>> {
        if !refresh {
            if let Some((at, l)) = self.cache.lock().unwrap().as_ref() {
                if at.elapsed() < TTL {
                    return Ok(l.clone());
                }
            }
        }
        let url = format!("{}/repos/{}/branches/{}", self.api, self.repo, self.branch);
        let b = self.fetch(url, 1 << 20).await.map_err(Self::unavailable)?;
        let commit = serde_json::from_slice::<Branch>(&b).map_err(|_| ApiError::unavailable("Eklenti kataloğu okunamadı."))?.commit.sha;
        if commit.len() != 40 || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(ApiError::unavailable("Eklenti kataloğu okunamadı."));
        }
        let url = format!("{}/repos/{}/git/trees/{commit}?recursive=1", self.api, self.repo);
        let b = self.fetch(url, MAX_TREE).await.map_err(Self::unavailable)?;
        let tree: Tree = serde_json::from_slice(&b).map_err(|_| ApiError::unavailable("Eklenti kataloğu okunamadı."))?;
        if tree.truncated {
            tracing::warn!(repo = %self.repo, "eklenti deposu ağacı kesik geldi; bazı eklentiler listelenmeyebilir");
        }
        let mut out = Vec::new();
        for (id, files) in Self::group(tree).into_iter().take(MAX_PLUGINS) {
            match self.entry(&commit, &id, files).await {
                Ok(e) => out.push(e),
                Err(e) => tracing::warn!(plugin = %id, err = %e, "katalogdaki eklenti atlandı"),
            }
        }
        out.sort_by_key(|e| e.name.to_lowercase());
        *self.cache.lock().unwrap() = Some((Instant::now(), out.clone()));
        Ok(out)
    }

    async fn entry(&self, commit: &str, id: &str, files: Vec<File>) -> Result<Entry, String> {
        let mf = files.iter().find(|f| f.rel == "plugin.json").ok_or("plugin.json yok")?;
        let b = self.fetch(self.raw_url(commit, id, "plugin.json"), MAX_MANIFEST).await?;
        if blob_sha(&b) != mf.sha {
            return Err("plugin.json özeti uyuşmuyor".into());
        }
        let mut m: Manifest = serde_json::from_slice(&b).map_err(|e| format!("plugin.json bozuk: {e}"))?;
        m.validate().map_err(|e| e.message)?;
        if m.id != id {
            return Err("plugin.json id klasör adıyla aynı değil".into());
        }
        for need in [m.entrypoint.as_str(), "pyproject.toml"] {
            if !files.iter().any(|f| f.rel == need) {
                return Err(format!("{need} yok"));
            }
        }
        let mut icon = String::new();
        if let (Some(f), Some(ct)) = (files.iter().find(|f| !m.icon.is_empty() && f.rel == m.icon), icon_type(&m.icon)) {
            if f.size <= MAX_ICON {
                if let Ok(b) = self.fetch(self.raw_url(commit, id, &f.rel), MAX_ICON + 1).await {
                    if blob_sha(&b) == f.sha {
                        icon = format!("data:{ct};base64,{}", base64::engine::general_purpose::STANDARD.encode(&b));
                    }
                }
            }
        }
        let mut tags = Vec::new();
        for (t, on) in [("theme", m.theme.is_some()), ("status", m.status.is_some()), ("widget", m.widget.is_some()), ("web", m.web_port != 0)] {
            if on {
                tags.push(t.to_string());
            }
        }
        Ok(Entry {
            id: id.into(),
            name: m.name,
            version: m.version,
            description: m.description,
            author: m.author,
            icon,
            tags,
            size: files.iter().map(|f| f.size).sum(),
            url: format!("https://github.com/{}/tree/{}/{DIR}{id}", self.repo, self.branch),
            files,
            commit: commit.into(),
        })
    }

    /// Katalog + kullanıcının kurulu sürümleri.
    pub async fn list(&self, a: &dyn Agent, user: &str, refresh: bool) -> ApiResult<Listing> {
        let list = self.index(refresh).await?;
        let have: HashMap<String, String> = plugins::list(a, user).await.unwrap_or_default().into_iter().map(|p| (p.manifest.id, p.manifest.version)).collect();
        let plugins = list
            .into_iter()
            .map(|e| {
                let installed = have.get(&e.id).cloned();
                let update = installed.as_deref().is_some_and(|v| plugins::newer_version(&e.version, v));
                Item { entry: e, installed, update }
            })
            .collect();
        Ok(Listing { source: self.source(), repo_url: format!("https://github.com/{}", self.repo), plugins })
    }

    /// Klasörü indirir (her dosya git özetiyle denetlenir) ve kurar; güncelleme de budur.
    /// Çalışıyorsa yeni sürümle yeniden başlatılır.
    pub async fn install(&self, a: &dyn Agent, user: &str, id: &str) -> ApiResult<Manifest> {
        if !plugins::valid_id(id) {
            return Err(ApiError::invalid("Geçersiz eklenti kimliği."));
        }
        // Kurarken her zaman güncel ağaç: önbellekteki eski özetlerle yeni dosyalar uyuşmaz.
        let e = self.index(true).await?.into_iter().find(|e| e.id == id).ok_or_else(|| ApiError::not_found("Eklenti katalogda yok."))?;
        if e.files.len() > plugins::MAX_FILES || e.size > plugins::MAX_UNPACKED {
            return Err(ApiError::invalid("Eklenti çok büyük (en fazla 2000 dosya, 64 MB)."));
        }
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for f in &e.files {
            // Sınır bir fazla: boyutu tutmayan (değiştirilmiş) dosya ağ hatası değil, uyuşmazlık olarak raporlanır.
            let b = self.fetch(self.raw_url(&e.commit, id, &f.rel), f.size + 2).await.map_err(Self::unavailable)?;
            if b.len() as u64 != f.size || blob_sha(&b) != f.sha {
                return Err(ApiError::invalid(format!("İndirilen dosya depodakiyle uyuşmuyor: {}; kurulmadı.", f.rel)));
            }
            zw.start_file(f.rel.as_str(), opts).map_err(|e| ApiError::internal(e.to_string()))?;
            zw.write_all(&b)?;
        }
        let data = zw.finish().map_err(|e| ApiError::internal(e.to_string()))?.into_inner();
        if data.len() > plugins::MAX_ZIP {
            return Err(ApiError::invalid("Eklenti sıkıştırılmış hâliyle 15 MB'ı aşıyor."));
        }
        let was_running = plugins::get(a, user, id).await.is_ok_and(|p| p.running);
        let m = plugins::import(a, user, &data).await?;
        if was_running {
            let _ = plugins::start(a, user, id).await;
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termproc::login::CmdSpec;
    use std::sync::Arc;

    #[test]
    fn source_and_sha() {
        assert!(Catalog::new("o/r", "t").is_ok_and(|c| c.source() == "o/r@main"));
        assert!(Catalog::new("o/r@dev", "t").is_ok_and(|c| c.branch == "dev"));
        for bad in ["o", "o/r/x", "o/../r", "o/r@a b", ""] {
            assert!(Catalog::new(bad, "t").is_err(), "{bad}");
        }
        // git hash-object: boş blob
        assert_eq!(blob_sha(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        assert!(skipped(".venv/x") && skipped("a/__pycache__/b.pyc") && skipped(".log") && !skipped("icons/a.svg"));
    }

    /// Sahte GitHub: dal → commit, commit ağacı ve raw dosyalar (/o/r/<commit>/plugins/...).
    async fn github(files: Vec<(&'static str, Vec<u8>)>, corrupt: Option<&'static str>) -> String {
        use axum::extract::Path;
        use axum::routing::get;
        let tree: Vec<serde_json::Value> = files
            .iter()
            .map(|(p, b)| serde_json::json!({"path": p, "type": "blob", "sha": blob_sha(b), "size": b.len()}))
            .chain([serde_json::json!({"path": "plugins/ornek", "type": "tree", "sha": "x"}), serde_json::json!({"path": "README.md", "type": "blob", "sha": "a".repeat(40), "size": 1})])
            .collect();
        let tree = serde_json::json!({"sha": "t", "tree": tree, "truncated": false}).to_string();
        let raw: Arc<HashMap<String, Vec<u8>>> = Arc::new(
            files.into_iter().map(|(p, mut b)| {
                if corrupt == Some(p) {
                    b.push(b'!');
                }
                (p.to_string(), b)
            }).collect(),
        );
        let ln = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", ln.local_addr().unwrap());
        const C: &str = "c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ff";
        let branch = serde_json::json!({"name": "main", "commit": {"sha": C}}).to_string();
        let app = axum::Router::new()
            .route("/repos/o/r/branches/main", get(move || async move { branch }))
            .route(&format!("/repos/o/r/git/trees/{C}"), get(move || async move { tree }))
            .route(&format!("/o/r/{C}/{{*p}}"),
            get(move |Path(p): Path<String>| {
                let raw = raw.clone();
                async move { raw.get(&p).cloned().ok_or(axum::http::StatusCode::NOT_FOUND) }
            }),
        );
        tokio::spawn(async move { axum::serve(ln, app).await.unwrap() });
        base
    }

    fn agent(home: &std::path::Path) -> crate::uagent::Local {
        let h = home.to_string_lossy().into_owned();
        crate::uagent::Local {
            command: Arc::new(move || {
                Ok(CmdSpec { path: "/bin/sh".into(), argv0: "sh".into(), args: vec![], dir: "/".into(), env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("HOME".into(), h.clone())] })
            }),
            home: home.to_string_lossy().into_owned(),
        }
    }

    fn files() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("plugins/ornek/plugin.json", br#"{"id":"ornek","name":"Ornek","version":"2.0.0","icon":"icon.svg","web_port":8800}"#.to_vec()),
            ("plugins/ornek/main.py", b"print(1)".to_vec()),
            ("plugins/ornek/pyproject.toml", b"[project]\n".to_vec()),
            ("plugins/ornek/icon.svg", b"<svg/>".to_vec()),
            ("plugins/ornek/.venv/x", b"".to_vec()),
            // id klasör adıyla aynı değil → atlanır
            ("plugins/kotu/plugin.json", br#"{"id":"baska","name":"x","version":"1"}"#.to_vec()),
            ("plugins/kotu/main.py", b"".to_vec()),
            ("plugins/kotu/pyproject.toml", b"".to_vec()),
        ]
    }

    #[tokio::test]
    async fn list_and_install() {
        let home = std::env::temp_dir().join(format!("mk-cat-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&home).unwrap();
        let a = agent(&home);
        let base = github(files(), None).await;
        let c = Catalog::new("o/r", "t").unwrap().with_bases(&base, &base);
        let l = c.list(&a, "u", false).await.unwrap();
        let ids: Vec<&str> = l.plugins.iter().map(|i| i.entry.id.as_str()).collect();
        assert!(ids.contains(&"ornek") && !ids.contains(&"kotu"), "{ids:?}");
        let it = l.plugins.iter().find(|i| i.entry.id == "ornek").unwrap();
        assert_eq!((it.installed.as_deref(), it.update, it.entry.tags.clone()), (None, false, vec!["web".to_string()]));
        assert!(it.entry.icon.starts_with("data:image/svg+xml;base64,"));
        let m = c.install(&a, "u", "ornek").await.unwrap();
        assert_eq!(m.version, "2.0.0");
        let dir = home.join(".local/share/makeros/plugins/ornek");
        assert!(dir.join("main.py").exists() && !dir.join(".venv/x").exists());
        let l = c.list(&a, "u", false).await.unwrap();
        assert_eq!(l.plugins.iter().find(|i| i.entry.id == "ornek").unwrap().installed.as_deref(), Some("2.0.0"));
        assert_eq!(c.install(&a, "u", "yok").await.unwrap_err().code, "not_found");
        std::fs::remove_dir_all(&home).ok();
    }

    #[tokio::test]
    async fn install_rejects_tampered_file() {
        let home = std::env::temp_dir().join(format!("mk-cat-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&home).unwrap();
        let base = github(files(), Some("plugins/ornek/main.py")).await;
        let c = Catalog::new("o/r", "t").unwrap().with_bases(&base, &base);
        let e = c.install(&agent(&home), "u", "ornek").await.unwrap_err();
        assert!(e.message.contains("uyuşmuyor"), "{}", e.message);
        assert!(!home.join(".local/share/makeros/plugins/ornek").exists());
        std::fs::remove_dir_all(&home).ok();
    }
}
