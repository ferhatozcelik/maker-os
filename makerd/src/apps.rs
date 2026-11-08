//! Kabuğun uygulama kaydı: yerleşik web uygulamaları ve
//! /usr/share/applications altındaki .desktop dosyalarından yerel uygulamalar.

use crate::apierr::{ApiError, ApiResult};
use futures_util::future::BoxFuture;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// GET /apps öğesi.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// web | native
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub exec: String,
    pub categories: Vec<String>,
    #[serde(skip)]
    pub argv: Vec<String>,
    #[serde(skip)]
    pub terminal: bool,
}

fn web(name: &str, title: &str, cats: &[&str]) -> App {
    App {
        id: format!("makeros.{name}"),
        name: title.into(),
        icon: format!("/shell/icons/{name}.svg"),
        kind: "web".into(),
        url: format!("/shell/apps/{name}/"),
        categories: cats.iter().map(|c| c.to_string()).collect(),
        ..Default::default()
    }
}

/// Yerleşik web uygulamaları.
pub fn builtin() -> Vec<App> {
    vec![
        web("settings", "Ayarlar", &["System", "Settings"]),
        web("terminal", "Terminal", &["System", "Utility"]),
        web("activity", "Etkinlik Monitörü", &["System", "Monitor"]),
        web("files", "Dosyalar", &["System", "FileManager"]),
        web("python", "Python Yöneticisi", &["Development"]),
        web("agents", "AI Ajanları", &["Development", "AI"]),
        web("camera", "Kamera", &["AudioVideo", "Development"]),
        web("gpio", "GPIO", &["Development", "Electronics"]),
        web("plugins", "Eklentiler", &["System", "Plugin"]),
        web("browser", "Tarayıcı", &["Network", "WebBrowser"]),
    ]
}

/// Yerel uygulamayı başlatır (mock'ta yalnız günlük).
pub type Launcher = Arc<dyn Fn(App, Vec<String>) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

pub struct Registry {
    /// .desktop dizinleri.
    pub desktop_dirs: Vec<PathBuf>,
    /// İkon varlığı denetimi için web kökü.
    pub web_dir: String,
    /// Mock yerel uygulamalar.
    pub extra: Vec<App>,
    pub launch: Launcher,
    cache: Mutex<Option<(Vec<App>, Instant)>>,
}

impl Registry {
    pub fn new(desktop_dirs: Vec<PathBuf>, web_dir: &str, extra: Vec<App>, launch: Launcher) -> Self {
        Self { desktop_dirs, web_dir: web_dir.into(), extra, launch, cache: Mutex::new(None) }
    }

    /// Uygulamalar (.desktop taraması 10 sn önbellekli).
    pub fn list(&self) -> Vec<App> {
        let mut c = self.cache.lock().unwrap();
        if let Some((l, at)) = c.as_ref() {
            if at.elapsed() < Duration::from_secs(10) {
                return l.clone();
            }
        }
        let mut out = builtin();
        out.extend(self.extra.iter().cloned());
        let mut native = Vec::new();
        let mut seen = HashSet::new();
        for d in &self.desktop_dirs {
            let Ok(rd) = std::fs::read_dir(d) else { continue };
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "desktop")).collect();
            files.sort();
            for f in files {
                let Some(mut a) = parse_desktop(&f) else { continue };
                if !seen.insert(a.id.clone()) {
                    continue;
                }
                a.icon = self.icon(&a.icon);
                native.push(a);
            }
        }
        native.sort_by_key(|a| a.name.to_lowercase());
        out.extend(native);
        *c = Some((out.clone(), Instant::now()));
        out
    }

    fn icon(&self, name: &str) -> String {
        // Kabuğun kendi simgesi tam yol olarak verilmiş olabilir (ör. chromium.desktop: Icon=/shell/icons/chromium.svg).
        if let Some(rest) = name.strip_prefix("/shell/icons/") {
            if !rest.contains("..") && !self.web_dir.is_empty() && Path::new(&self.web_dir).join("icons").join(rest).exists() {
                return name.into();
            }
        }
        if !name.is_empty() && !name.contains(['/', '.']) && !self.web_dir.is_empty() && Path::new(&self.web_dir).join("icons").join(format!("{name}.svg")).exists() {
            return format!("/shell/icons/{name}.svg");
        }
        "/shell/icons/app.svg".into()
    }

    /// Uygulamayı başlatır: web → invalid (kabuk açar), yok → not_found.
    pub async fn start(&self, id: &str) -> ApiResult<()> {
        self.start_with(id, None).await
    }

    /// Uygulamayı bir adresle başlatır (ör. Chromium'da sayfa). Adres yalnız http(s) olabilir ve
    /// yalnız Exec satırında %u/%U alan kodu olan uygulamalara verilir (bayrak olarak yorumlanamaz).
    pub async fn start_with(&self, id: &str, url: Option<&str>) -> ApiResult<()> {
        let a = self.list().into_iter().find(|a| a.id == id).ok_or_else(|| ApiError::not_found("Uygulama bulunamadı."))?;
        if a.kind == "web" {
            return Err(ApiError::invalid("Web uygulamalarını kabuk kendisi açar."));
        }
        let mut argv = if a.argv.is_empty() { split_exec(&a.exec) } else { a.argv.clone() };
        if argv.is_empty() {
            return Err(ApiError::invalid("Uygulamanın Exec satırı boş."));
        }
        if let Some(u) = url.map(str::trim).filter(|u| !u.is_empty()) {
            let parsed = url::Url::parse(u).map_err(|_| ApiError::invalid("Geçersiz adres."))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(ApiError::invalid("Yalnız http(s) adresleri açılabilir."));
            }
            if !accepts_url(&a.exec) {
                return Err(ApiError::invalid("Bu uygulama adres açamaz."));
            }
            argv.push(parsed.to_string());
        }
        if a.terminal {
            argv.insert(0, "foot".into());
        }
        (self.launch)(a, argv).await.map_err(|e| ApiError::internal(format!("Uygulama başlatılamadı: {e}")))
    }
}

/// Bir .desktop dosyası. Gizli/uygulama dışı girdiler None döner.
pub fn parse_desktop(path: &Path) -> Option<App> {
    let s = std::fs::read_to_string(path).ok()?;
    let mut kv: HashMap<&str, &str> = HashMap::new();
    let mut inside = false;
    for l in s.lines().map(str::trim) {
        if l.starts_with('[') {
            inside = l == "[Desktop Entry]";
            continue;
        }
        if !inside || l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            kv.insert(k.trim(), v.trim());
        }
    }
    let get = |k: &str| kv.get(k).copied().unwrap_or("");
    if get("Type") != "Application" || get("NoDisplay") == "true" || get("Hidden") == "true" || get("Exec").is_empty() {
        return None;
    }
    let name = if get("Name[tr]").is_empty() { get("Name") } else { get("Name[tr]") };
    if name.is_empty() {
        return None;
    }
    let exec = get("Exec").to_string();
    Some(App {
        id: path.file_name()?.to_string_lossy().trim_end_matches(".desktop").to_string(),
        name: name.into(),
        icon: get("Icon").into(),
        kind: "native".into(),
        categories: get("Categories").split(';').map(str::trim).filter(|c| !c.is_empty()).map(String::from).collect(),
        argv: split_exec(&exec),
        terminal: get("Terminal") == "true",
        exec,
        url: String::new(),
    })
}

/// Exec satırı adres kabul ediyor mu (%u / %U alan kodu)?
pub fn accepts_url(exec: &str) -> bool {
    split_raw(exec).iter().any(|a| a == "%u" || a == "%U")
}

/// Exec satırını (çift tırnak ve ters bölü kaçışlarıyla) argümanlara böler ve
/// alan kodlarını (%f %U ...) kaldırır. Kabuk kullanılmaz.
pub fn split_exec(s: &str) -> Vec<String> {
    split_raw(s)
        .into_iter()
        .filter_map(|a| {
            let b = a.as_bytes();
            if b.len() == 2 && b[0] == b'%' {
                return (b[1] == b'%').then(|| "%".to_string()); // %f %F %u %U %i %c %k ...
            }
            Some(a.replace("%%", "%"))
        })
        .collect()
}

/// Exec satırını alan kodlarına dokunmadan argümanlara böler.
fn split_raw(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_q, mut has, mut esc) = (false, false, false);
    for r in s.chars() {
        if esc {
            cur.push(r);
            esc = false;
        } else if r == '\\' {
            esc = true;
        } else if r == '"' {
            in_q = !in_q;
            has = true;
        } else if (r == ' ' || r == '\t') && !in_q {
            if has || !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            has = false;
        } else {
            cur.push(r);
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Uygulamayı varsa `systemd-run --user` ile apps.slice'ta, yoksa ayrı oturumda
/// doğrudan başlatır. Wayland ortamı eksikse tamamlanır.
pub fn system_launcher() -> Launcher {
    Arc::new(|a: App, argv: Vec<String>| {
        Box::pin(async move {
            let mut env: Vec<(String, String)> = Vec::new();
            if std::env::var("XDG_RUNTIME_DIR").map(|v| v.is_empty()).unwrap_or(true) {
                env.push(("XDG_RUNTIME_DIR".into(), format!("/run/user/{}", crate::util::getuid())));
            }
            if std::env::var("WAYLAND_DISPLAY").map(|v| v.is_empty()).unwrap_or(true) {
                env.push(("WAYLAND_DISPLAY".into(), "wayland-0".into()));
            }
            if let Some(run) = crate::util::look_path("systemd-run") {
                let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() % 1_000_000).unwrap_or(0);
                let unit = format!("--unit=app-{}-{nanos}", sanitize(&a.id));
                let out = tokio::process::Command::new(run)
                    .args(["--user", "--collect", "--quiet", "--slice=apps.slice", &unit, "--"])
                    .args(&argv)
                    .envs(env.iter().map(|(k, v)| (k, v)))
                    .output()
                    .await;
                match out {
                    Ok(o) if o.status.success() => {
                        tracing::info!(id = %a.id, via = "systemd-run", "uygulama başlatıldı");
                        return Ok(());
                    }
                    Ok(o) => {
                        let mut b = o.stdout;
                        b.extend_from_slice(&o.stderr);
                        tracing::warn!(out = %String::from_utf8_lossy(&b).trim(), "systemd-run başarısız, doğrudan başlatılıyor");
                    }
                    Err(e) => tracing::warn!(err = %e, "systemd-run başarısız, doğrudan başlatılıyor"),
                }
            }
            let path = crate::util::look_path(&argv[0]).ok_or_else(|| format!("{} bulunamadı", argv[0]))?;
            let mut cmd = std::process::Command::new(path);
            cmd.args(&argv[1..]).envs(env.iter().map(|(k, v)| (k, v))).current_dir(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
            // SAFETY: fork sonrası yalnız setsid (async-signal-safe).
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
            let mut child = tokio::process::Command::from(cmd).spawn().map_err(|e| e.to_string())?;
            tracing::info!(id = %a.id, pid = child.id(), "uygulama başlatıldı");
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split() {
        for (i, w) in [
            ("foot", vec!["foot"]),
            ("thonny %F", vec!["thonny"]),
            (r#"sh -c "echo a b" %u"#, vec!["sh", "-c", "echo a b"]),
            (r#"app --pct 100%% \"q\""#, vec!["app", "--pct", "100%", r#""q""#]),
            (r#""/opt/My App/bin" --x """#, vec!["/opt/My App/bin", "--x", ""]),
        ] {
            assert_eq!(split_exec(i), w, "{i}");
        }
    }

    #[test]
    fn url_field_codes() {
        assert!(accepts_url("chromium %U"));
        assert!(accepts_url("firefox %u"));
        assert!(!accepts_url("thonny %F"));
        assert!(!accepts_url("app 100%%U"));
    }

    #[tokio::test]
    async fn registry() {
        let dir = std::env::temp_dir().join(format!("mk-apps-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let w = |n: &str, b: &str| std::fs::write(dir.join(n), b).unwrap();
        w("foot.desktop", "[Desktop Entry]\nType=Application\nName=Foot\nName[tr]=Foot Uçbirim\nExec=foot\nIcon=foot\nCategories=System;TerminalEmulator;\n");
        w("hidden.desktop", "[Desktop Entry]\nType=Application\nName=H\nExec=h\nNoDisplay=true\n");
        w("link.desktop", "[Desktop Entry]\nType=Link\nName=L\nURL=x\n");
        let launched = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = launched.clone();
        let r = Registry::new(
            vec![dir.clone()],
            "",
            vec![],
            Arc::new(move |_a, argv| {
                *l2.lock().unwrap() = argv;
                Box::pin(async { Ok(()) })
            }),
        );
        let list = r.list();
        assert_eq!(list.len(), builtin().len() + 1);
        let foot = list.last().unwrap();
        assert_eq!((foot.id.as_str(), foot.name.as_str(), foot.kind.as_str(), foot.icon.as_str(), foot.categories.len()), ("foot", "Foot Uçbirim", "native", "/shell/icons/app.svg", 2));
        for a in builtin() {
            assert_eq!(a.url, format!("/shell/apps/{}/", a.id.strip_prefix("makeros.").unwrap()));
        }
        r.start("foot").await.unwrap();
        assert_eq!(*launched.lock().unwrap(), vec!["foot".to_string()]);
        assert_eq!(r.start("makeros.settings").await.unwrap_err().code, "invalid");
        assert_eq!(r.start("nope").await.unwrap_err().code, "not_found");
        let j = serde_json::to_value(foot).unwrap();
        assert!(j.get("argv").is_none() && j.get("url").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
