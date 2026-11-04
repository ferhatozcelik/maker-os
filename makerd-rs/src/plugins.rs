//! Kullanıcı eklentileri: zip olarak paylaşılan küçük Python uygulamaları.
//! Dosyalar kullanıcının evinde (~/.local/share/makeros/plugins/<id>) durur;
//! süreç kullanıcının hesabıyla (systemd --user varsa apps.slice içinde, yoksa
//! arka plan süreci) çalışır. Web arayüzü olan eklentiler 127.0.0.1:<web_port>
//! dinler; makerd oturum doğrulamasından sonra /plugin/<id>/ adresinden yönlendirir.

use crate::apierr::{ApiError, ApiResult};
use crate::termproc::fsops::Listing;
use crate::termproc::Open;
use crate::uagent::{self, Agent};
use crate::util::{quote, shq};
use futures_util::future::BoxFuture;
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};

static TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/plugin-template");
static DEFAULTS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/plugin-defaults");

/// Eklentilerin kökü (ev dizinine göre).
pub const DIR: &str = ".local/share/makeros/plugins";
pub const MAX_ZIP: usize = 15 << 20;
pub const MAX_UNPACKED: u64 = 64 << 20;
pub const MAX_FILES: usize = 2000;

/// Eklenti kimliği: küçük harfle başlar; en fazla 40 küçük harf, rakam veya tire.
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty() && b.len() <= 40 && b[0].is_ascii_lowercase() && b.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn check_id(id: &str) -> ApiResult<()> {
    if valid_id(id) { Ok(()) } else { Err(ApiError::invalid("Geçersiz eklenti kimliği.")) }
}

/// plugin.json
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Manifest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default)]
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub web_port: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// İsteğe bağlı tema paketi (duvar kâğıtları, logo, işaret, vurgu, uygulamalar); kabuk tema seçicide gösterir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<serde_json::Value>,
    /// İsteğe bağlı durum göstergesi ({"api":"/api/...","label":"..."}); kabuk menü çubuğunda gösterir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<serde_json::Value>,
    /// İsteğe bağlı masaüstü kartı ({"path":"/widget","w":340,"h":220}); kabuk masaüstünde gösterir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<serde_json::Value>,
    /// false ise eklentinin kendi web arayüzü ayrı uygulama olarak listelenmez.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<bool>,
    /// Eklenti gerektiğinde (tema/durum) kabuk tarafından otomatik başlatılır.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub autostart: bool,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

/// Listede gösterilen eklenti.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Plugin {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub running: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub pid: i64,
}

fn json_len(v: &Option<serde_json::Value>) -> usize {
    v.as_ref().map(|v| v.to_string().len()).unwrap_or(0)
}

fn bad_rel(p: &str) -> bool {
    p.contains("..") || p.starts_with('/')
}

impl Manifest {
    /// Alanları denetler (boş entrypoint "main.py" olur).
    pub fn validate(&mut self) -> ApiResult<()> {
        if !valid_id(&self.id) {
            return Err(ApiError::invalid("plugin.json: id küçük harfle başlamalı; en fazla 40 küçük harf, rakam veya tire."));
        }
        for (k, v) in [("name", &self.name), ("version", &self.version)] {
            if v.is_empty() || v.len() > 100 {
                return Err(ApiError::invalid(format!("plugin.json: {k} eksik veya çok uzun.")));
            }
        }
        if self.description.len() > 500 {
            return Err(ApiError::invalid("plugin.json: description en fazla 500 karakter."));
        }
        if self.entrypoint.is_empty() {
            self.entrypoint = "main.py".into();
        }
        if !self.entrypoint.ends_with(".py") || bad_rel(&self.entrypoint) {
            return Err(ApiError::invalid("plugin.json: entrypoint eklenti içindeki bir .py dosyası olmalı."));
        }
        if self.web_port != 0 && !(1024..=65535).contains(&self.web_port) {
            return Err(ApiError::invalid("plugin.json: web_port 1024–65535 arasında olmalı."));
        }
        if !self.icon.is_empty() && bad_rel(&self.icon) {
            return Err(ApiError::invalid("plugin.json: icon eklenti içinde olmalı."));
        }
        if json_len(&self.theme) > 8 << 10 || json_len(&self.status) > 2 << 10 || json_len(&self.widget) > 1 << 10 {
            return Err(ApiError::invalid("plugin.json: theme/status alanı çok büyük."));
        }
        if self.args.len() > 64 {
            return Err(ApiError::invalid("plugin.json: en fazla 64 argüman."));
        }
        if self.args.iter().any(|a| a.len() > 1024 || a.contains('\0')) {
            return Err(ApiError::invalid("plugin.json: argüman geçersiz."));
        }
        Ok(())
    }
}

fn root(id: &str) -> String {
    format!("{DIR}/{id}")
}

fn embedded_files<'a>(d: &'a Dir<'a>, out: &mut Vec<&'a include_dir::File<'a>>) {
    out.extend(d.files());
    for s in d.dirs() {
        embedded_files(s, out);
    }
}

fn zip_opts() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

/// Şablon eklenti (zip).
pub fn template_zip() -> ApiResult<Vec<u8>> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut files = Vec::new();
    embedded_files(&TEMPLATE, &mut files);
    files.sort_by_key(|f| f.path());
    for f in files {
        zw.start_file(f.path().to_string_lossy(), zip_opts()).map_err(|e| ApiError::internal(e.to_string()))?;
        zw.write_all(f.contents())?;
    }
    Ok(zw.finish().map_err(|e| ApiError::internal(e.to_string()))?.into_inner())
}

/// path.Clean (göreli yol için): ".", ".." ve fazla eğik çizgileri çözer.
fn clean(p: &str) -> String {
    let rooted = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for c in p.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    let s = out.join("/");
    match (rooted, s.is_empty()) {
        (true, _) => format!("/{s}"),
        (false, true) => ".".into(),
        _ => s,
    }
}

/// Zip'i doğrular; manifest ve (göreli yol → içerik) döner. Zip kökünde
/// plugin.json olmalı veya tek üst klasör içinde bulunmalı.
pub fn unpack(data: &[u8]) -> ApiResult<(Manifest, BTreeMap<String, Vec<u8>>)> {
    if data.len() > MAX_ZIP {
        return Err(ApiError::invalid("Zip en fazla 15 MB olabilir."));
    }
    let mut zr = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|_| ApiError::invalid("Geçerli bir zip dosyası değil."))?;
    let names: Vec<String> = (0..zr.len()).filter_map(|i| zr.name_for_index(i).map(String::from)).collect();
    let mut prefix = String::new();
    for n in &names {
        let base = n.rsplit('/').next().unwrap_or(n);
        if base == "plugin.json" && n.trim_matches('/').matches('/').count() <= 1 {
            match n.rfind('/') {
                None => {
                    prefix.clear();
                    break;
                }
                Some(i) => {
                    let p = &n[..i];
                    if prefix.is_empty() || p.len() < prefix.len() {
                        prefix = format!("{p}/");
                    }
                }
            }
        }
    }
    let mut files = BTreeMap::new();
    let mut total: u64 = 0;
    for i in 0..zr.len() {
        let mut f = zr.by_index(i).map_err(|_| ApiError::invalid("Zip okunamadı."))?;
        if f.is_dir() {
            continue;
        }
        let raw = f.name().to_string();
        if f.is_symlink() {
            return Err(ApiError::invalid(format!("Zip sembolik bağ içeremez: {raw}")));
        }
        let name = raw.strip_prefix(&prefix);
        let c = clean(name.unwrap_or(&raw));
        if name.is_none() || c == "." || c == ".." || c.starts_with("../") || c.starts_with('/') || c.contains('\\') {
            return Err(ApiError::invalid(format!("Zip eklenti klasörü dışına yazamaz: {raw}")));
        }
        if c.starts_with(".venv/") || c.starts_with("__pycache__/") || c.contains("/__pycache__/") {
            continue;
        }
        if files.len() >= MAX_FILES {
            return Err(ApiError::invalid("Zip en fazla 2000 dosya içerebilir."));
        }
        total += f.size();
        if total > MAX_UNPACKED {
            return Err(ApiError::invalid("Açılmış eklenti en fazla 64 MB olabilir."));
        }
        let mut b = Vec::new();
        // Bildirilen boyuta güvenilmez: okuma da sınırlıdır.
        if (&mut f).take(MAX_UNPACKED + 1).read_to_end(&mut b).is_err() || b.len() as u64 > MAX_UNPACKED {
            return Err(ApiError::invalid("Zip okunamadı."));
        }
        files.insert(c, b);
    }
    let mj = files.get("plugin.json").ok_or_else(|| ApiError::invalid("Zip kökünde plugin.json bulunmalı."))?;
    let mut m: Manifest = (mj.len() <= 16 << 10)
        .then(|| serde_json::from_slice(mj).ok())
        .flatten()
        .ok_or_else(|| ApiError::invalid("plugin.json okunamadı (en fazla 16 KB geçerli JSON)."))?;
    m.validate()?;
    if !files.contains_key(&m.entrypoint) {
        return Err(ApiError::invalid(format!("Başlangıç dosyası bulunamadı: {}", m.entrypoint)));
    }
    if !files.contains_key("pyproject.toml") {
        return Err(ApiError::invalid("pyproject.toml gerekli (bağımlılıklar uv ile kurulur)."));
    }
    Ok((m, files))
}

/// Zip'i kullanıcının eklenti dizinine yazar (aynı id varsa değiştirir).
pub async fn import(a: &dyn Agent, user: &str, data: &[u8]) -> ApiResult<Manifest> {
    let (m, files) = unpack(data)?;
    let _ = stop(a, user, &m.id).await;
    // Yoksa sorun değil.
    let _ = a.fs(user, Open { mode: "remove".into(), path: root(&m.id), ..Default::default() }).await;
    for (n, b) in files {
        uagent::write_all(a, user, &format!("{}/{n}", root(&m.id)), b).await?;
    }
    Ok(m)
}

/// Kurulu eklentiler ve çalışma durumları.
pub async fn list(a: &dyn Agent, user: &str) -> ApiResult<Vec<Plugin>> {
    seed(a, user).await;
    let Ok(raw) = a.fs(user, Open { mode: "list".into(), path: DIR.into(), ..Default::default() }).await else { return Ok(Vec::new()) };
    let l: Listing = serde_json::from_slice(&raw).unwrap_or_default();
    let mut out = Vec::new();
    for e in l.entries.iter().filter(|e| e.dir && valid_id(&e.name)) {
        if let Ok(p) = get(a, user, &e.name).await {
            out.push(p);
        }
    }
    Ok(out)
}

/// Tek eklenti.
pub async fn get(a: &dyn Agent, user: &str, id: &str) -> ApiResult<Plugin> {
    check_id(id)?;
    let b = uagent::read_all(a, user, &format!("{}/plugin.json", root(id))).await.map_err(|_| ApiError::not_found("Eklenti bulunamadı."))?;
    let mut m: Manifest = serde_json::from_slice(&b).map_err(|_| ApiError::invalid("plugin.json bozuk."))?;
    if m.validate().is_err() || m.id != id {
        return Err(ApiError::invalid("plugin.json bozuk."));
    }
    let mut p = Plugin { manifest: m, ..Default::default() };
    let cmd = format!(r#"p=$(cat ~/{}/.pid 2>/dev/null) && kill -0 "$p" 2>/dev/null && echo "$p""#, root(id));
    if let Ok((out, _)) = uagent::output(a, user, &cmd).await {
        if let Ok(pid) = String::from_utf8_lossy(&out).trim().parse::<i64>() {
            if pid > 0 {
                p.running = true;
                p.pid = pid;
            }
        }
    }
    Ok(p)
}

/// Eklentiyi arka planda başlatır (uv run; ilk seferde bağımlılıklar kurulur).
pub async fn start(a: &dyn Agent, user: &str, id: &str) -> ApiResult<Plugin> {
    let p = get(a, user, id).await?;
    if p.running {
        return Ok(p);
    }
    let m = &p.manifest;
    let args: String = m.args.iter().map(|x| format!(" {}", shq(x))).collect();
    let dir = format!("$HOME/{}", root(id));
    let run = format!("cd {} && exec uv run --quiet {}{args} >> .log 2>&1", quote(&dir), shq(&m.entrypoint));
    let env = format!("MAKEROS_PLUGIN_ID={id} MAKEROS_PLUGIN_PORT={}", m.web_port);
    // systemd --user varsa apps.slice içinde (bellek sınırı, kabuk korunur); yoksa ayrık süreç.
    let cmd = format!(
        "cd {qd} && : > .log && if command -v systemd-run >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
  systemd-run --user --quiet --collect --unit makeros-plugin-{id} --slice apps --setenv=MAKEROS_PLUGIN_ID={id} --setenv=MAKEROS_PLUGIN_PORT={port} sh -c {qr} && sleep 0.3 && systemctl --user show -p MainPID --value makeros-plugin-{id} > .pid
else
  env {env} setsid sh -c {qr} </dev/null >/dev/null 2>&1 & echo $! > .pid
fi",
        qd = quote(&dir),
        port = m.web_port,
        qr = shq(&run),
    );
    let (out, code) = uagent::output(a, user, &cmd).await?;
    if code != 0 {
        return Err(ApiError::unavailable(format!("Eklenti başlatılamadı: {}", String::from_utf8_lossy(&out).trim())));
    }
    get(a, user, id).await
}

/// Eklentiyi durdurur.
pub async fn stop(a: &dyn Agent, user: &str, id: &str) -> ApiResult<Plugin> {
    check_id(id)?;
    let r = root(id);
    let cmd = format!(r#"systemctl --user stop makeros-plugin-{id} 2>/dev/null; p=$(cat ~/{r}/.pid 2>/dev/null) && kill -TERM -- -"$p" 2>/dev/null || kill -TERM "$p" 2>/dev/null; rm -f ~/{r}/.pid; true"#);
    let _ = uagent::output(a, user, &cmd).await;
    get(a, user, id).await
}

/// Eklentiyi durdurup siler. Varsayılan eklentiler bir daha kurulmaz.
pub async fn remove(a: &dyn Agent, user: &str, id: &str) -> ApiResult<()> {
    stop(a, user, id).await?;
    if is_default(id) {
        let _ = uagent::write_all(a, user, &removed_mark(id), "kullanıcı kaldırdı\n").await;
    }
    a.fs(user, Open { mode: "remove".into(), path: root(id), ..Default::default() }).await.map(|_| ())
}

/// Son günlük satırları (en fazla 64 KB).
pub async fn logs(a: &dyn Agent, user: &str, id: &str) -> ApiResult<String> {
    check_id(id)?;
    let (out, _) = uagent::output(a, user, &format!("tail -c 65536 ~/{}/.log 2>/dev/null; true", root(id))).await?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Eklentiyi paylaşılabilir zip olarak döner (.venv, günlük, pid hariç).
pub async fn export(a: &dyn Agent, user: &str, id: &str) -> ApiResult<Vec<u8>> {
    get(a, user, id).await?;
    let mut files = Vec::new();
    walk(a, user, id, String::new(), &mut files).await?;
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for name in files {
        let b = uagent::read_all(a, user, &format!("{}/{name}", root(id))).await?;
        zw.start_file(name.as_str(), zip_opts()).map_err(|e| ApiError::internal(e.to_string()))?;
        zw.write_all(&b)?;
    }
    Ok(zw.finish().map_err(|e| ApiError::internal(e.to_string()))?.into_inner())
}

fn walk<'a>(a: &'a dyn Agent, user: &'a str, id: &'a str, rel: String, out: &'a mut Vec<String>) -> BoxFuture<'a, ApiResult<()>> {
    Box::pin(async move {
        let raw = a.fs(user, Open { mode: "list".into(), path: format!("{}{rel}", root(id)), ..Default::default() }).await?;
        let l: Listing = serde_json::from_slice(&raw).unwrap_or_default();
        for e in l.entries {
            let name = format!("{rel}/{}", e.name).trim_start_matches('/').to_string();
            if e.link || name == ".venv" || name == ".log" || name == ".pid" || e.name == "__pycache__" {
                continue;
            }
            if e.dir {
                walk(a, user, id, format!("{rel}/{}", e.name), out).await?;
            } else {
                out.push(name);
            }
        }
        Ok(())
    })
}

// --- varsayılan eklentiler -----------------------------------------------------

/// Kullanıcı varsayılan bir eklentiyi kaldırdığında yazılır; bir daha kurulmaz.
fn removed_mark(id: &str) -> String {
    format!("{DIR}/.kaldirildi-{id}")
}

/// Eklenti MakerOS ile birlikte mi geliyor.
pub fn is_default(id: &str) -> bool {
    DEFAULTS.get_file(format!("{id}/plugin.json")).is_some()
}

/// MakerOS ile gelen eklentileri (ör. Konya HackerSpace) kullanıcıda yoksa kurar;
/// kuruluysa ve gömülü sürüm daha yeniyse dosyaları yeniler (.venv ve günlük korunur).
/// Kullanıcı kaldırdıysa (işaret dosyası) dokunmaz.
pub async fn seed(a: &dyn Agent, user: &str) {
    for d in DEFAULTS.dirs() {
        let Some(id) = d.path().file_name().and_then(|n| n.to_str()) else { continue };
        if !valid_id(id) {
            continue;
        }
        if let Ok(cur) = get(a, user, id).await {
            let emb: Option<Manifest> = d.get_file(d.path().join("plugin.json")).and_then(|f| serde_json::from_slice(f.contents()).ok());
            if !emb.is_some_and(|e| newer_version(&e.version, &cur.manifest.version)) {
                continue;
            }
        }
        if a.fs(user, Open { mode: "stat".into(), path: removed_mark(id), ..Default::default() }).await.is_ok() {
            continue;
        }
        let mut files = Vec::new();
        embedded_files(d, &mut files);
        for f in files {
            let rel = f.path().strip_prefix(id).unwrap_or(f.path());
            if uagent::write_all(a, user, &format!("{}/{}", root(id), rel.display()), f.contents()).await.is_err() {
                break;
            }
        }
    }
}

/// Eklentinin içindeki bir dosya (duvar kâğıdı, logo); süreç çalışmak zorunda değil.
/// Gizli dosyalar (.venv, .log, .pid) verilmez.
pub fn asset_path(id: &str, rel: &str) -> ApiResult<String> {
    check_id(id)?;
    let c = clean(&format!("/{rel}"));
    let c = c.trim_start_matches('/');
    if c.is_empty() || c.starts_with('.') || c.contains("/.") {
        return Err(ApiError::not_found("Dosya bulunamadı."));
    }
    Ok(format!("{}/{c}", root(id)))
}

/// a > b mi (noktalı sayısal sürüm; "1.0.10" > "1.0.9").
pub fn newer_version(a: &str, b: &str) -> bool {
    let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('.').collect(), b.split('.').collect());
    for i in 0..pa.len().max(pb.len()) {
        let x: i64 = pa.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
        let y: i64 = pb.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termproc::login::CmdSpec;
    use std::sync::Arc;

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (n, b) in entries {
            zw.start_file(*n, zip_opts()).unwrap();
            zw.write_all(b).unwrap();
        }
        zw.finish().unwrap().into_inner()
    }

    const MANI: &[u8] = br#"{"id":"deneme","name":"Deneme","version":"1.0","description":"x"}"#;

    #[test]
    fn unpack_rules() {
        let (m, f) = unpack(&zip_of(&[("plugin.json", MANI), ("main.py", b"print(1)"), ("pyproject.toml", b""), ("__pycache__/x.pyc", b"")])).unwrap();
        assert_eq!((m.id.as_str(), m.entrypoint.as_str()), ("deneme", "main.py"));
        assert_eq!(f.keys().collect::<Vec<_>>(), vec!["main.py", "plugin.json", "pyproject.toml"]);
        // Tek üst klasör.
        let (_, f) = unpack(&zip_of(&[("d/plugin.json", MANI), ("d/main.py", b""), ("d/pyproject.toml", b""), ("d/a/b.txt", b"")])).unwrap();
        assert!(f.contains_key("a/b.txt"));
        let e = |z: Vec<u8>| unpack(&z).unwrap_err().message;
        assert!(e(zip_of(&[("plugin.json", MANI), ("main.py", b""), ("pyproject.toml", b""), ("../kac.txt", b"")])).contains("dışına"));
        assert!(e(zip_of(&[("plugin.json", MANI), ("pyproject.toml", b"")])).contains("Başlangıç"));
        assert!(e(zip_of(&[("plugin.json", MANI), ("main.py", b"")])).contains("pyproject"));
        assert!(e(zip_of(&[("plugin.json", br#"{"id":"Kotu","name":"a","version":"1"}"#)])).contains("id"));
        assert!(e(b"zip degil".to_vec()).contains("zip"));
        let mut m: Manifest = serde_json::from_slice(MANI).unwrap();
        m.web_port = 80;
        assert!(m.validate().is_err());
        m.web_port = 8080;
        m.entrypoint = "../x.py".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn misc() {
        assert!(newer_version("1.0.10", "1.0.9") && !newer_version("1.0", "1.0.0") && newer_version("2", "1.9.9"));
        assert_eq!(clean("a/./b/../c"), "a/c");
        assert_eq!(clean("../a"), "../a");
        assert_eq!(asset_path("khs", "wallpapers/a.jpg").unwrap(), ".local/share/makeros/plugins/khs/wallpapers/a.jpg");
        assert!(asset_path("khs", "../../.ssh/id").is_err(), "eklenti dışına ve gizli dosyaya çıkılamaz");
        assert_eq!(asset_path("khs", "../icons/a.svg").unwrap(), ".local/share/makeros/plugins/khs/icons/a.svg");
        assert!(asset_path("khs", ".pid").is_err() && asset_path("khs", "a/.venv/x").is_err() && asset_path("khs", "").is_err());
        assert!(is_default("khs") && !is_default("yok"));
        let z = template_zip().unwrap();
        let (m, _) = unpack(&z).unwrap();
        assert!(valid_id(&m.id));
        // Plugin JSON'u manifest alanlarını düz taşır.
        let j = serde_json::to_value(Plugin { manifest: m, running: true, pid: 7 }).unwrap();
        assert_eq!((j["running"].as_bool(), j["pid"].as_i64()), (Some(true), Some(7)));
        assert!(j["id"].is_string());
    }

    #[tokio::test]
    async fn seed_import_export_remove() {
        let home = std::env::temp_dir().join(format!("mk-pl-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&home).unwrap();
        // Kabuk komutları "~" kullanır: giriş kabuğundaki gibi HOME ayarlı olmalı.
        let h = home.to_string_lossy().into_owned();
        let a = crate::uagent::Local {
            command: Arc::new(move || {
                Ok(CmdSpec { path: "/bin/sh".into(), argv0: "sh".into(), args: vec![], dir: "/".into(), env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("HOME".into(), h.clone())] })
            }),
            home: home.to_string_lossy().into_owned(),
        };
        let l = list(&a, "u").await.unwrap();
        assert!(l.iter().any(|p| p.manifest.id == "khs"), "varsayılan eklenti kurulmalı");
        let m = import(&a, "u", &zip_of(&[("plugin.json", MANI), ("main.py", b"print(1)"), ("pyproject.toml", b"[project]\n"), ("lib/x.py", b"")])).await.unwrap();
        assert_eq!(m.id, "deneme");
        std::fs::write(home.join(".local/share/makeros/plugins/deneme/.log"), "gunluk").unwrap();
        assert_eq!(logs(&a, "u", "deneme").await.unwrap(), "gunluk");
        let z = export(&a, "u", "deneme").await.unwrap();
        let (_, f) = unpack(&z).unwrap();
        assert_eq!(f.keys().collect::<Vec<_>>(), vec!["lib/x.py", "main.py", "plugin.json", "pyproject.toml"]);
        assert!(!get(&a, "u", "deneme").await.unwrap().running);
        remove(&a, "u", "khs").await.unwrap();
        // Kaldırılan varsayılan eklenti yeniden kurulmaz.
        let l = list(&a, "u").await.unwrap();
        assert_eq!(l.iter().map(|p| p.manifest.id.as_str()).collect::<Vec<_>>(), vec!["deneme"]);
        assert_eq!(get(&a, "u", "yok").await.unwrap_err().code, "not_found");
        let _ = std::fs::remove_dir_all(home);
    }
}
