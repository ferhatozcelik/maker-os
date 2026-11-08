//! Python sürümleri (uv) ve şablonlu Python projeleri. Web arayüzü (makerd →
//! makerd-user) ve `mos` CLI (süreç içi) aynı kodu kullanır; tüm durum
//! kullanıcının ev dizinindedir (~/Projeler, uv önbelleği). Uzun işler
//! (kurulum, çalıştırma) terminal oturumunda görünür çalışır: bu modül yalnız
//! komutu üretir, çalıştıran taraf onu bir oturuma bağlar.

use crate::apierr::{ApiError, ApiResult};
use crate::termproc::fsops::Listing;
use crate::termproc::Open;
use crate::uagent::{self, Agent};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/py-templates");

/// Kullanıcının proje kökü (ev dizinine göre).
pub const PROJECTS_DIR: &str = "Projeler";
/// İmajdaki Python (libcamera, picamera2, gpiozero, lgpio burada).
pub const SYSTEM_PYTHON: &str = "/usr/bin/python3";

/// İki dilli metin.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Text {
    #[serde(default)]
    pub tr: String,
    #[serde(default)]
    pub en: String,
}

/// Proje şablonu.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Template {
    pub id: String,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub title: Text,
    #[serde(default)]
    pub desc: Text,
    #[serde(default)]
    pub deps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run: String,
    /// Projenin web arayüzü portu (0: yok). Kabuk, proje çalışınca arayüzü Tarayıcı penceresinde açar.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ui: u16,
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

/// Gömülü şablonlar (sıralı).
pub fn templates() -> Vec<Template> {
    let mut out: Vec<Template> = TEMPLATES
        .dirs()
        .filter_map(|d| {
            let id = d.path().file_name()?.to_str()?;
            let t: Template = serde_json::from_slice(d.get_file(d.path().join("template.json"))?.contents()).ok()?;
            (t.id == id).then_some(t)
        })
        .collect();
    out.sort_by_key(|t| t.order);
    out
}

fn template_by_id(id: &str) -> Option<Template> {
    templates().into_iter().find(|t| t.id == id)
}

/// Proje adı: küçük harf veya rakamla başlar; en fazla 40 karakter (a-z, 0-9, - ve _).
pub fn valid_name(n: &str) -> ApiResult<()> {
    let b = n.as_bytes();
    let ok = !b.is_empty()
        && b.len() <= 40
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-');
    if ok { Ok(()) } else { Err(ApiError::invalid("Proje adı küçük harf veya rakamla başlamalı; en fazla 40 karakter (a-z, 0-9, - ve _).")) }
}

/// Python sürümü: "3.12" veya "3.12.7".
pub fn valid_version(v: &str) -> ApiResult<()> {
    let p: Vec<&str> = v.split('.').collect();
    let num = |s: &str| (1..=2).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit());
    if (p.len() == 2 || p.len() == 3) && p[0] == "3" && p[1..].iter().all(|s| num(s)) {
        Ok(())
    } else {
        Err(ApiError::invalid("Geçersiz Python sürümü (örnek: 3.12)."))
    }
}

// --- Python sürümleri ------------------------------------------------------

/// Kurulu veya indirilebilir bir Python.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Version {
    pub version: String,
    pub minor: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub system: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
}

#[derive(Deserialize)]
struct UvParts {
    #[serde(default)]
    major: u32,
    #[serde(default)]
    minor: u32,
}

#[derive(Deserialize)]
struct UvPython {
    version: String,
    path: Option<String>,
    #[serde(default)]
    implementation: String,
    #[serde(default)]
    variant: String,
    version_parts: UvParts,
}

/// uv'nin kurulu ve indirilebilir CPython sürümleri (her küçük sürümün en yenisi).
pub async fn versions(a: &dyn Agent, user: &str) -> ApiResult<Vec<Version>> {
    let raw: Vec<UvPython> = uagent::json(a, user, "uv python list --all-versions --output-format json 2>/dev/null || uv python list --output-format json")
        .await
        .map_err(|e| ApiError::unavailable(format!("uv ile Python sürümleri okunamadı: {e}")))?;
    let (def, _) = uagent::output(a, user, r#"readlink -f "$HOME/.local/bin/python3" 2>/dev/null || true"#).await?;
    Ok(merge_versions(raw, String::from_utf8_lossy(&def).trim()))
}

fn dir2(p: &str) -> &str {
    let up = |s: &str| -> usize { s.rfind('/').unwrap_or(0) };
    let a = &p[..up(p)];
    &a[..up(a)]
}

fn merge_versions(raw: Vec<UvPython>, def_path: &str) -> Vec<Version> {
    let mut best: HashMap<String, Version> = HashMap::new();
    let mut out = Vec::new();
    for r in raw {
        if r.implementation != "cpython" || (!r.variant.is_empty() && r.variant != "default") || r.version.contains(['a', 'b', 'r', 'c']) {
            continue;
        }
        let minor = format!("{}.{}", r.version_parts.major, r.version_parts.minor);
        let mut v = Version { version: r.version.clone(), minor: minor.clone(), ..Default::default() };
        if let Some(p) = r.path {
            v.installed = true;
            v.system = !p.contains("/uv/python/");
            v.default = !def_path.is_empty() && (def_path == p || def_path.starts_with(&format!("{}/", dir2(&p))));
            v.path = p;
            out.push(v);
            continue;
        }
        if best.get(&minor).is_none_or(|b| newer(&r.version, &b.version)) {
            best.insert(minor, v);
        }
    }
    let have: HashSet<String> = out.iter().map(|v| v.minor.clone()).collect();
    out.extend(best.into_iter().filter(|(m, _)| !have.contains(m)).map(|(_, v)| v));
    out.sort_by(|a, b| b.installed.cmp(&a.installed).then_with(|| if newer(&a.version, &b.version) { std::cmp::Ordering::Less } else if newer(&b.version, &a.version) { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Equal }));
    out
}

fn newer(a: &str, b: &str) -> bool {
    let parts = |s: &str| -> [u32; 3] {
        let mut r = [0; 3];
        for (i, p) in s.split('.').take(3).enumerate() {
            r[i] = p.bytes().take_while(u8::is_ascii_digit).fold(0u32, |n, c| n * 10 + u32::from(c - b'0'));
        }
        r
    };
    parts(a) > parts(b)
}

/// Sürüm işlemi için terminal komutu.
pub fn python_cmd(op: &str, version: &str) -> ApiResult<String> {
    valid_version(version)?;
    Ok(match op {
        "install" => format!("uv python install {version}"),
        "remove" => format!("uv python uninstall {version}"),
        // python/python3 komutları ~/.local/bin altında bu sürüme bağlanır.
        "default" => format!("uv python install --default {version} && python3 --version"),
        _ => return Err(ApiError::invalid("Geçersiz işlem.")),
    })
}

// --- Projeler --------------------------------------------------------------

/// Proje kökündeki .makeros.json.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Meta {
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub python: String,
    pub created: jiff::Timestamp,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run: String,
    /// Web arayüzü portu (0: yok; şablondan gelir, .makeros.json'da değiştirilebilir).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ui: u16,
}

/// Listede gösterilen proje.
#[derive(Serialize, Clone, Debug)]
pub struct Project {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub python: String,
    pub created: jiff::Timestamp,
    pub modified: jiff::Timestamp,
    pub venv: bool,
}

async fn list(a: &dyn Agent, user: &str, path: &str) -> ApiResult<Listing> {
    let raw = a.fs(user, Open { mode: "list".into(), path: path.into(), ..Default::default() }).await?;
    Ok(serde_json::from_slice(&raw)?)
}

async fn read_meta(a: &dyn Agent, user: &str, dir: &str) -> Option<Meta> {
    serde_json::from_slice(&uagent::read_all(a, user, &format!("{dir}/.makeros.json")).await.ok()?).ok()
}

/// ~/Projeler altındaki Python projeleri (pyproject.toml olanlar), yeniden eskiye.
pub async fn projects(a: &dyn Agent, user: &str) -> ApiResult<Vec<Project>> {
    let Ok(l) = list(a, user, PROJECTS_DIR).await else { return Ok(Vec::new()) }; // klasör henüz yok
    let mut out = Vec::new();
    for e in l.entries.iter().filter(|e| e.dir && !e.name.starts_with('.')) {
        let dir = format!("{PROJECTS_DIR}/{}", e.name);
        let Ok(il) = list(a, user, &dir).await else { continue };
        let mut p = Project { name: e.name.clone(), path: format!("{}/{}", l.path, e.name), template: String::new(), python: String::new(), created: e.mtime, modified: e.mtime, venv: false };
        let mut ok = false;
        for f in &il.entries {
            match f.name.as_str() {
                "pyproject.toml" => ok = true,
                ".venv" => p.venv = true,
                ".makeros.json" => {
                    if let Some(m) = read_meta(a, user, &dir).await {
                        p.template = m.template;
                        p.python = m.python;
                        p.created = m.created;
                    }
                }
                _ => {}
            }
        }
        if ok {
            out.push(p);
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

/// TOML temel dizgesi (çift tırnaklı, kaçışlı).
fn toml_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if c.is_control() => o.push_str(&format!("\\u{:04X}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn run_cmd(r: &str) -> &str {
    if r.is_empty() { "uv run main.py" } else { r }
}

/// Şablondan projeyi yazar ve ortamı kuracak terminal komutunu döner.
/// python: "system" (varsayılan; kamera/GPIO kütüphaneleri hazır) veya "3.12" gibi.
pub async fn create(a: &dyn Agent, user: &str, name: &str, tmpl_id: &str, python: &str) -> ApiResult<String> {
    valid_name(name)?;
    let t = template_by_id(tmpl_id).ok_or_else(|| ApiError::invalid("Şablon bulunamadı."))?;
    let python = if python.is_empty() { "system" } else { python };
    if python != "system" {
        valid_version(python)?;
    }
    let dir = format!("{PROJECTS_DIR}/{name}");
    if a.fs(user, Open { mode: "stat".into(), path: dir.clone(), ..Default::default() }).await.is_ok() {
        return Err(ApiError::conflict("Bu adla bir proje zaten var."));
    }
    let root = TEMPLATES.get_dir(&t.id).ok_or_else(|| ApiError::internal("şablon dizini yok"))?;
    let mut files = Vec::new();
    collect(root, &mut files);
    for f in files {
        let rel = f.path().strip_prefix(&t.id).unwrap_or(f.path());
        if rel.file_name().is_some_and(|n| n == "template.json") {
            continue;
        }
        let body = String::from_utf8_lossy(f.contents()).replace("{{name}}", name);
        uagent::write_all(a, user, &format!("{dir}/{}", rel.display()), body).await?;
    }
    let deps = t.deps.iter().map(|d| toml_str(d)).collect::<Vec<_>>().join(", ");
    let pyproject = format!(
        "[project]\nname = {}\nversion = \"0.1.0\"\ndescription = {}\nrequires-python = \">=3.11\"\ndependencies = [{deps}]\n\n[tool.uv]\npackage = false\n",
        toml_str(name),
        toml_str(&t.title.tr)
    );
    uagent::write_all(a, user, &format!("{dir}/pyproject.toml"), pyproject).await?;
    let meta = Meta { template: t.id.clone(), python: python.into(), created: jiff::Timestamp::now(), run: t.run.clone(), ui: t.ui };
    let mut mb = serde_json::to_vec_pretty(&meta)?;
    mb.push(b'\n');
    uagent::write_all(a, user, &format!("{dir}/.makeros.json"), mb).await?;
    let _ = uagent::write_all(a, user, &format!("{dir}/.gitignore"), ".venv/\n__pycache__/\n").await;
    let py = if python == "system" { SYSTEM_PYTHON } else { python };
    // --system-site-packages: imajdaki libcamera/picamera2/gpiozero/lgpio/numpy projede de görünür.
    Ok(format!(
        "cd ~/{dir} && echo '» {name}: ortam hazırlanıyor (uv)' && uv venv --quiet --system-site-packages --python {py} && uv sync && echo && echo '✔ Hazır. Çalıştırmak için: {}'",
        run_cmd(&t.run)
    ))
}

fn collect<'a>(d: &'a Dir<'a>, out: &mut Vec<&'a include_dir::File<'a>>) {
    out.extend(d.files());
    for sub in d.dirs() {
        collect(sub, out);
    }
}

/// Projeyi çalıştıracak terminal komutu.
pub async fn run_command(a: &dyn Agent, user: &str, name: &str) -> ApiResult<String> {
    valid_name(name)?;
    let dir = format!("{PROJECTS_DIR}/{name}");
    let run = read_meta(a, user, &dir).await.map(|m| m.run).unwrap_or_default();
    Ok(format!("cd ~/{dir} && {}", run_cmd(&run)))
}

/// Projeyi siler.
pub async fn delete(a: &dyn Agent, user: &str, name: &str) -> ApiResult<()> {
    valid_name(name)?;
    a.fs(user, Open { mode: "remove".into(), path: format!("{PROJECTS_DIR}/{name}"), ..Default::default() }).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termproc::login::CmdSpec;
    use std::sync::Arc;

    #[tokio::test]
    async fn templates_and_create() {
        let ts = templates();
        assert!(ts.len() >= 8 && ts[0].id == "bos", "şablonlar: {}", ts.len());
        for t in &ts {
            assert!(!t.title.tr.is_empty() && !t.title.en.is_empty() && !t.desc.tr.is_empty(), "{}: başlık/açıklama eksik", t.id);
            assert!(TEMPLATES.get_file(format!("{}/main.py", t.id)).is_some(), "{}: main.py yok", t.id);
        }
        let home = std::env::temp_dir().join(format!("mk-py-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&home).unwrap();
        let a = crate::uagent::Local {
            command: Arc::new(|| Ok(CmdSpec { path: "/bin/sh".into(), argv0: "sh".into(), args: vec![], dir: "/".into(), env: vec![] })),
            home: home.to_string_lossy().into_owned(),
        };
        let cmd = create(&a, "u", "ilk-proje", "nesne-tespiti", "system").await.unwrap();
        assert!(cmd.contains("uv venv --quiet --system-site-packages --python /usr/bin/python3") && cmd.contains("uv sync"), "{cmd}");
        let main = std::fs::read_to_string(home.join("Projeler/ilk-proje/main.py")).unwrap();
        assert!(main.contains("ilk-proje") && !main.contains("{{name}}"));
        let pp = std::fs::read_to_string(home.join("Projeler/ilk-proje/pyproject.toml")).unwrap();
        assert!(pp.contains(r#"dependencies = ["ncnn", "opencv-python-headless"]"#), "{pp}");
        assert_eq!(create(&a, "u", "ilk-proje", "bos", "").await.unwrap_err().code, "conflict");
        assert_eq!(create(&a, "u", "Kötü Ad", "bos", "").await.unwrap_err().code, "invalid");
        let ps = projects(&a, "u").await.unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!((ps[0].template.as_str(), ps[0].python.as_str()), ("nesne-tespiti", "system"));
        assert_eq!(run_command(&a, "u", "ilk-proje").await.unwrap(), "cd ~/Projeler/ilk-proje && uv run main.py");
        delete(&a, "u", "ilk-proje").await.unwrap();
        assert!(projects(&a, "u").await.unwrap().is_empty());
        assert!(python_cmd("install", "3.12; rm -rf /").is_err());
        assert_eq!(python_cmd("default", "3.12").unwrap(), "uv python install --default 3.12 && python3 --version");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn validation_and_versions() {
        for v in ["3.12", "3.9", "3.12.7"] {
            assert!(valid_version(v).is_ok(), "{v}");
        }
        for v in ["3", "2.7", "3.123", "3.12.", "3.12.7.1", "3.x"] {
            assert!(valid_version(v).is_err(), "{v}");
        }
        assert!(valid_name("a").is_ok() && valid_name("-a").is_err() && valid_name(&"a".repeat(41)).is_err());
        assert!(newer("3.12.10", "3.12.9") && !newer("3.11.9", "3.12.0"));
        let raw: Vec<UvPython> = serde_json::from_str(r#"[
          {"version":"3.13.1","path":null,"implementation":"cpython","variant":"default","version_parts":{"major":3,"minor":13,"patch":1}},
          {"version":"3.13.2","path":null,"implementation":"cpython","variant":"default","version_parts":{"major":3,"minor":13,"patch":2}},
          {"version":"3.14.0rc1","path":null,"implementation":"cpython","variant":"default","version_parts":{"major":3,"minor":14,"patch":0}},
          {"version":"3.13.2","path":null,"implementation":"cpython","variant":"freethreaded","version_parts":{"major":3,"minor":13,"patch":2}},
          {"version":"3.12.7","path":"/home/m/.local/share/uv/python/cpython-3.12.7-linux/bin/python3.12","implementation":"cpython","variant":"default","version_parts":{"major":3,"minor":12,"patch":7}},
          {"version":"3.11.2","path":"/usr/bin/python3.11","implementation":"cpython","variant":"","version_parts":{"major":3,"minor":11,"patch":2}},
          {"version":"7.3.17","path":null,"implementation":"pypy","variant":"","version_parts":{"major":3,"minor":10,"patch":14}}
        ]"#).unwrap();
        let v = merge_versions(raw, "/home/m/.local/share/uv/python/cpython-3.12.7-linux/bin/python3.12");
        let got: Vec<(&str, bool, bool, bool)> = v.iter().map(|v| (v.version.as_str(), v.installed, v.system, v.default)).collect();
        assert_eq!(got, vec![("3.12.7", true, false, true), ("3.11.2", true, true, false), ("3.13.2", false, false, false)]);
    }
}

// --- Python yöneticisi: arka planda çalıştırma, günlük, paketler, ortam, otomatik başlatma ----------
// Proje "Çalıştır" ile kendi oturumunda (setsid) arka planda çalışır; çıktı .makeros-run.log'a yazılır ve
// uygulama onu canlı izler. Otomatik başlatma: ~/.config/makeros/autostart/<proje> işareti; makerd açılışta
// birincil kullanıcının işaretli projelerini başlatır. Hepsi kullanıcının kendi hesabıyla (makerd-user).

pub const RUN_LOG: &str = ".makeros-run.log";
pub const RUN_PID: &str = ".makeros-run.pid";
pub const AUTOSTART_DIR: &str = ".config/makeros/autostart";

/// Paket adı / gereksinim belirteci (ör. "requests", "numpy>=2", "uvicorn[standard]").
pub fn valid_pkg(p: &str) -> ApiResult<()> {
    if p.is_empty() || p.len() > 120 || !p.chars().all(|c| c.is_ascii_alphanumeric() || "._-[],<>=!~ ".contains(c)) || p.starts_with('-') {
        return Err(ApiError::invalid("Geçersiz paket adı."));
    }
    Ok(())
}

async fn project_dir(a: &dyn Agent, user: &str, name: &str) -> ApiResult<String> {
    valid_name(name)?;
    let dir = format!("{PROJECTS_DIR}/{name}");
    a.fs(user, Open { mode: "stat".into(), path: format!("{dir}/pyproject.toml"), ..Default::default() })
        .await
        .map_err(|_| ApiError::not_found("Proje bulunamadı."))?;
    Ok(dir)
}

/// Arka planda başlatan komut (zaten çalışıyorsa bir şey yapmaz).
pub async fn start_command(a: &dyn Agent, user: &str, name: &str) -> ApiResult<String> {
    let dir = project_dir(a, user, name).await?;
    let run = read_meta(a, user, &dir).await.map(|m| m.run).unwrap_or_default();
    let run = crate::util::shq(&format!("exec {}", run_cmd(&run)));
    Ok(format!(
        "cd ~/{dir} || exit 1; p=$(cat {RUN_PID} 2>/dev/null); if [ -n \"$p\" ] && kill -0 \"$p\" 2>/dev/null && [ \"$(cut -d\\) -f2 /proc/$p/stat 2>/dev/null | cut -c2)\" != Z ]; then exit 0; fi; \
         printf '» %s  %s\\n' \"$(date '+%Y-%m-%d %H:%M:%S')\" {name} > {RUN_LOG}; \
         PYTHONUNBUFFERED=1 setsid sh -c {run} >> {RUN_LOG} 2>&1 < /dev/null & echo $! > {RUN_PID}"
    ))
}

pub async fn stop(a: &dyn Agent, user: &str, name: &str) -> ApiResult<()> {
    let dir = project_dir(a, user, name).await?;
    let cmd = format!(
        "cd ~/{dir} || exit 0; p=$(cat {RUN_PID} 2>/dev/null) || exit 0; [ -n \"$p\" ] || exit 0; kill -TERM -- -\"$p\" 2>/dev/null || kill -TERM \"$p\" 2>/dev/null; \
         for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 \"$p\" 2>/dev/null || break; sleep 0.3; done; kill -KILL -- -\"$p\" 2>/dev/null; rm -f {RUN_PID}; printf '\\n» durduruldu\\n' >> {RUN_LOG}; true"
    );
    uagent::output(a, user, &cmd).await.map(|_| ())
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Status {
    pub running: bool,
    pub autostart: bool,
    pub python: String,
    pub template: String,
    /// Web arayüzü portu (0: yok).
    pub ui: u16,
}

pub async fn status(a: &dyn Agent, user: &str, name: &str) -> ApiResult<Status> {
    let dir = project_dir(a, user, name).await?;
    let cmd = format!("cd ~/{dir} || exit 1; p=$(cat {RUN_PID} 2>/dev/null); if [ -n \"$p\" ] && kill -0 \"$p\" 2>/dev/null && [ \"$(cut -d\\) -f2 /proc/$p/stat 2>/dev/null | cut -c2)\" != Z ]; then echo run; fi; [ -e ~/{AUTOSTART_DIR}/{name} ] && echo auto; true");
    let (out, _) = uagent::output(a, user, &cmd).await?;
    let out = String::from_utf8_lossy(&out);
    let m = read_meta(a, user, &dir).await.unwrap_or_default();
    // Eski projelerin .makeros.json'unda ui yok: şablondan.
    let ui = if m.ui > 0 { m.ui } else { template_by_id(&m.template).map(|t| t.ui).unwrap_or(0) };
    Ok(Status { running: out.lines().any(|l| l == "run"), autostart: out.lines().any(|l| l == "auto"), python: m.python, template: m.template, ui })
}

/// Günlüğün `from` baytından sonrası (en fazla 64 KB) ve toplam boyut.
pub async fn log(a: &dyn Agent, user: &str, name: &str, from: u64) -> ApiResult<serde_json::Value> {
    let dir = project_dir(a, user, name).await?;
    let cmd = format!("cd ~/{dir} || exit 1; s=$(stat -c %s {RUN_LOG} 2>/dev/null || echo 0); echo \"$s\"; [ \"$s\" -gt {from} ] && tail -c +{} {RUN_LOG} | head -c 65536; true", from + 1);
    let (out, _) = uagent::output(a, user, &cmd).await?;
    let txt = String::from_utf8_lossy(&out);
    let (size, rest) = txt.split_once('\n').unwrap_or((&txt, ""));
    let size: u64 = size.trim().parse().unwrap_or(0);
    // Günlük kısaldıysa (yeniden başlatma) baştan.
    let (from, text) = if size < from { (0, String::new()) } else { (from, rest.to_string()) };
    Ok(serde_json::json!({ "size": size, "from": from, "text": text, "reset": size < from }))
}

/// Doğrudan bağımlılıklar (pyproject) ve ortamda kurulu paketler.
pub async fn packages(a: &dyn Agent, user: &str, name: &str) -> ApiResult<serde_json::Value> {
    let dir = project_dir(a, user, name).await?;
    let cmd = format!(
        "cd ~/{dir} || exit 1; printf '{{\"deps\":'; python3 -c \"import tomllib,json;print(json.dumps(tomllib.load(open('pyproject.toml','rb')).get('project',{{}}).get('dependencies',[])),end='')\" 2>/dev/null || printf '[]'; \
         printf ',\"installed\":'; if [ -d .venv ]; then uv pip list --format json 2>/dev/null || printf '[]'; else printf '[]'; fi; printf '}}'"
    );
    uagent::json(a, user, &cmd).await
}

/// Paket işlemi için terminal komutu (çıktı uygulamada canlı görünür).
pub async fn package_cmd(a: &dyn Agent, user: &str, name: &str, op: &str, pkg: &str) -> ApiResult<String> {
    let dir = project_dir(a, user, name).await?;
    let q = crate::util::shq(pkg);
    let c = match op {
        "add" => { valid_pkg(pkg)?; format!("uv add {q}") }
        "remove" => { valid_pkg(pkg)?; format!("uv remove {q}") }
        "upgrade" if pkg.is_empty() => "uv lock --upgrade && uv sync".into(),
        "upgrade" => { valid_pkg(pkg)?; format!("uv lock --upgrade-package {q} && uv sync") }
        "sync" => "uv sync".into(),
        _ => return Err(ApiError::invalid("Bilinmeyen paket işlemi.")),
    };
    Ok(format!("cd ~/{dir} && {c} && echo && echo '✔ Tamamlandı'"))
}

/// Projenin Python sürümünü değiştirir: ortam yeniden kurulur ("system" ya da "3.12").
pub async fn python_switch_cmd(a: &dyn Agent, user: &str, name: &str, python: &str) -> ApiResult<String> {
    let dir = project_dir(a, user, name).await?;
    let python = if python.is_empty() { "system" } else { python };
    if python != "system" {
        valid_version(python)?;
    }
    let mut m = read_meta(a, user, &dir).await.unwrap_or_else(|| Meta { template: String::new(), python: String::new(), created: jiff::Timestamp::now(), run: String::new(), ui: 0 });
    m.python = python.into();
    let mut mb = serde_json::to_vec_pretty(&m)?;
    mb.push(b'\n');
    uagent::write_all(a, user, &format!("{dir}/.makeros.json"), mb).await?;
    let py = if python == "system" { SYSTEM_PYTHON } else { python };
    Ok(format!("cd ~/{dir} && echo '» Python ortamı yeniden kuruluyor: {python}' && uv venv --clear --quiet --system-site-packages --python {py} && uv sync && echo && echo '✔ Tamamlandı'"))
}

pub async fn set_autostart(a: &dyn Agent, user: &str, name: &str, on: bool) -> ApiResult<()> {
    project_dir(a, user, name).await?;
    let mark = format!("{AUTOSTART_DIR}/{name}");
    if on {
        uagent::write_all(a, user, &mark, "").await
    } else {
        a.fs(user, Open { mode: "remove".into(), path: mark, ..Default::default() }).await.map(|_| ())
    }
}

/// Açılışta: kullanıcının otomatik başlatılacak projelerini arka planda başlatır.
pub async fn autostart_all(a: &dyn Agent, user: &str) {
    let Ok(l) = list(a, user, AUTOSTART_DIR).await else { return };
    for e in l.entries.iter().filter(|e| !e.dir) {
        match start_command(a, user, &e.name).await {
            Ok(cmd) => match uagent::output(a, user, &cmd).await {
                Ok(_) => tracing::info!(user, project = %e.name, "proje otomatik başlatıldı"),
                Err(err) => tracing::warn!(user, project = %e.name, err = %err.message, "proje başlatılamadı"),
            },
            Err(err) => tracing::warn!(user, project = %e.name, err = %err.message, "otomatik başlatma atlandı"),
        }
    }
}
