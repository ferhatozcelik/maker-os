//! Kullanıcının giriş kabuğu ve ortamı.

use std::path::Path;

/// Başlatılacak komutun tanımı.
#[derive(Clone, Debug)]
pub struct CmdSpec {
    pub path: String,
    pub argv0: String,
    pub args: Vec<String>,
    pub dir: String,
    pub env: Vec<(String, String)>,
}

impl CmdSpec {
    /// "kabuk -l -c cmd" biçimine çevirir; cmd boşsa aynen döner.
    pub fn with_cmd(mut self, cmd: &str) -> Self {
        if !cmd.is_empty() {
            self.argv0 = self.path.clone();
            self.args = vec!["-l".into(), "-c".into(), cmd.into()];
        }
        self
    }

    pub fn std_command(&self) -> std::process::Command {
        use std::os::unix::process::CommandExt;
        let mut c = std::process::Command::new(&self.path);
        c.arg0(&self.argv0).args(&self.args).current_dir(&self.dir).env_clear().envs(self.env.iter().map(|(k, v)| (k, v)));
        c
    }
}

/// /etc/passwd'den kullanıcının (kabuk, ev, ad) bilgisi.
pub fn user_shell(passwd: &str, uid: u32) -> (String, String, String) {
    crate::util::passwd_entries(passwd).into_iter().find(|e| e.uid == uid).map(|e| (e.shell, e.home, e.name)).unwrap_or_default()
}

/// Çalışan sürecin kullanıcısının ev dizini.
pub fn home_of(passwd: &str) -> String {
    let (_, home, _) = user_shell(passwd, crate::util::getuid());
    if !home.is_empty() {
        return home;
    }
    std::env::var("HOME").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "/".into())
}

#[derive(Debug)]
pub struct NoShell;

impl std::fmt::Display for NoShell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("kabuk bulunamadı")
    }
}

/// Çalışan sürecin kullanıcısının giriş kabuğunu ev dizininde hazırlar.
pub fn login_command(passwd: &str, fallback_shell: &str) -> Result<CmdSpec, NoShell> {
    let (mut shell, mut home, mut name) = user_shell(passwd, crate::util::getuid());
    if shell.is_empty() || shell.ends_with("nologin") || shell.ends_with("false") {
        shell = std::env::var("SHELL").unwrap_or_default();
    }
    if shell.is_empty() {
        shell = fallback_shell.to_string();
    }
    if !Path::new(&shell).exists() {
        shell = crate::util::look_path("sh").map(|p| p.to_string_lossy().into_owned()).ok_or(NoShell)?;
    }
    if home.is_empty() {
        home = std::env::var("HOME").unwrap_or_default();
    }
    if !Path::new(&home).is_dir() {
        home = "/".into();
    }
    if name.is_empty() {
        name = std::env::var("USER").unwrap_or_default();
    }
    let base = Path::new(&shell).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "sh".into());
    let mut env: Vec<(String, String)> = vec![
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
        ("HOME".into(), home.clone()),
        ("SHELL".into(), shell.clone()),
        ("USER".into(), name.clone()),
        ("LOGNAME".into(), name),
        ("PATH".into(), format!("{home}/.local/bin:/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin")),
    ];
    for k in ["LANG", "LC_ALL", "TZ", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"] {
        if let Ok(v) = std::env::var(k) {
            env.push((k.into(), v));
        }
    }
    if std::env::var("LANG").map(|v| v.is_empty()).unwrap_or(true) {
        env.push(("LANG".into(), "C.UTF-8".into()));
    }
    Ok(CmdSpec { path: shell, argv0: format!("-{base}"), args: Vec::new(), dir: home, env })
}

/// Giriş kabuğu üreteci (her çağrıda güncel /etc/passwd okunur).
pub type Command = std::sync::Arc<dyn Fn() -> Result<CmdSpec, NoShell> + Send + Sync>;

pub fn login(passwd: &str, fallback: &str) -> Command {
    let (p, f) = (passwd.to_string(), fallback.to_string());
    std::sync::Arc::new(move || login_command(&p, &f))
}
