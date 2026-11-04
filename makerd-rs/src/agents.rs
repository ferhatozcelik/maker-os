//! Terminal tabanlı yapay zeka kodlama ajanları ve yerel model çalıştırıcı
//! kataloğu. Kurulum/güncelleme/kaldırma kullanıcının hesabıyla bir terminal
//! oturumunda görünür çalışır (npm önekleri ~/.local).

use crate::apierr::{ApiError, ApiResult};
use crate::uagent::{self, Agent as UserAgent};
use serde::Serialize;
use std::collections::HashMap;

/// Katalog girdisi.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub command: String,
    pub summary: String,
    pub tip: String,
    pub account: String,
    pub docs: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip)]
    install: String,
    #[serde(skip)]
    update: String,
    #[serde(skip)]
    remove: String,
    #[serde(skip)]
    run: String,
}

/// npm komutları: bildirimsiz (fund/audit/güncelleme uyarısı yok) ve paketin kendi
/// kurulum betiklerine açık izinle (npm 11 betikleri varsayılan engeller; ör.
/// OpenCode platform ikilisini, Gemini keytar'ı kurar).
fn npm(pkg: &str, allow: &[&str]) -> (String, String, String) {
    let a = std::iter::once(pkg).chain(allow.iter().copied()).collect::<Vec<_>>().join(",");
    let flags = format!("--no-fund --no-audit --loglevel=error --allow-scripts={a} ");
    (format!("npm install -g {flags}{pkg}"), format!("npm install -g {flags}{pkg}@latest"), format!("npm uninstall -g --loglevel=error {pkg}"))
}

#[allow(clippy::too_many_arguments)]
fn entry(id: &str, name: &str, vendor: &str, command: &str, summary: &str, tip: &str, account: &str, docs: &str, ops: (String, String, String), run: &str) -> Agent {
    Agent {
        id: id.into(),
        name: name.into(),
        vendor: vendor.into(),
        command: command.into(),
        summary: summary.into(),
        tip: tip.into(),
        account: account.into(),
        docs: docs.into(),
        install: ops.0,
        update: ops.1,
        remove: ops.2,
        run: run.into(),
        ..Default::default()
    }
}

/// Sabit liste (kurulum yöntemleri sağlayıcıların resmi belgelerinden).
pub fn catalog() -> Vec<Agent> {
    let ollama_install = "curl -fsSL https://ollama.com/install.sh | sudo sh".to_string();
    vec![
        entry(
            "claude", "Claude Code", "Anthropic", "claude",
            "Proje klasöründe kod yazan, düzenleyen ve komut çalıştıran ajan.",
            "Çok dosyalı Python/GPIO projelerini adım adım kurmak ve hata ayıklamak için.",
            "Claude hesabı (Pro/Max) veya Anthropic API anahtarı", "https://docs.claude.com/claude-code",
            (
                "curl -fsSL https://claude.ai/install.sh | bash".into(),
                "claude update".into(),
                "rm -rf ~/.local/bin/claude ~/.local/share/claude && echo kaldırıldı".into(),
            ),
            "",
        ),
        entry(
            "codex", "Codex CLI", "OpenAI", "codex",
            "Terminalden çalışan, yerel dosyalarla çalışan kodlama ajanı.",
            "ChatGPT hesabınız varsa ek ücret olmadan denemek için.",
            "ChatGPT hesabı veya OpenAI API anahtarı", "https://github.com/openai/codex",
            npm("@openai/codex", &[]),
            "",
        ),
        entry(
            "gemini", "Gemini CLI", "Google", "gemini",
            "Google hesabıyla kullanılan açık kaynak terminal ajanı.",
            "Kişisel Google hesabıyla ücretsiz kullanım kotası vardır.",
            "Google hesabı veya Gemini API anahtarı", "https://github.com/google-gemini/gemini-cli",
            npm("@google/gemini-cli", &["@github/keytar"]),
            "",
        ),
        entry(
            "opencode", "OpenCode", "Açık kaynak", "opencode",
            "Birçok model sağlayıcısıyla (Anthropic, OpenAI, Google, yerel Ollama) çalışan açık kaynak ajan.",
            "Tek araçta farklı modelleri denemek veya Ollama ile internetsiz çalışmak için.",
            "Seçtiğiniz sağlayıcının hesabı/API anahtarı veya yerel Ollama", "https://opencode.ai/docs",
            npm("opencode-ai", &[]),
            "",
        ),
        entry(
            "ollama", "Ollama", "Yerel modeller", "ollama",
            "Küçük dil modellerini internetsiz, Pi'nin kendisinde çalıştırır.",
            "4 GB RAM için 1-3B parametreli modeller (llama3.2:1b, qwen2.5:1.5b, gemma3:1b).",
            "Hesap gerekmez", "https://ollama.com/library",
            (
                ollama_install.clone(),
                ollama_install,
                "sudo systemctl disable --now ollama 2>/dev/null; sudo rm -rf /usr/local/bin/ollama /usr/local/lib/ollama /etc/systemd/system/ollama.service && echo kaldırıldı".into(),
            ),
            "ollama run llama3.2:1b",
        ),
    ]
}

/// Katalog, kurulu durumla.
pub async fn list(a: &dyn UserAgent, user: &str) -> ApiResult<Vec<Agent>> {
    let mut cat = catalog();
    let script: String = cat.iter().map(|x| format!("printf '%s=%s\\n' {} \"$(command -v {} 2>/dev/null)\"; ", x.id, x.command)).collect();
    let (out, _) = uagent::output(a, user, &script).await?;
    let paths: HashMap<String, String> =
        String::from_utf8_lossy(&out).lines().filter_map(|l| l.trim().split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    for x in &mut cat {
        x.path = paths.get(&x.id).cloned().unwrap_or_default();
        x.installed = !x.path.is_empty();
    }
    Ok(cat)
}

/// İşlem için terminal komutu ve başlığı.
pub fn cmd(id: &str, op: &str) -> ApiResult<(String, String)> {
    let x = catalog().into_iter().find(|x| x.id == id).ok_or_else(|| ApiError::not_found("Ajan bulunamadı."))?;
    Ok(match op {
        "install" => (x.install, format!("{} · kurulum", x.name)),
        "update" => (x.update, format!("{} · güncelleme", x.name)),
        "remove" => (x.remove, format!("{} · kaldırma", x.name)),
        "run" => {
            let r = if x.run.is_empty() { x.command.clone() } else { x.run };
            (format!("mkdir -p ~/Projeler && cd ~/Projeler && {r}"), x.name)
        }
        _ => return Err(ApiError::invalid("Geçersiz işlem.")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        let (c, t) = cmd("gemini", "install").unwrap();
        assert_eq!(c, "npm install -g --no-fund --no-audit --loglevel=error --allow-scripts=@google/gemini-cli,@github/keytar @google/gemini-cli");
        assert_eq!(t, "Gemini CLI · kurulum");
        assert_eq!(cmd("codex", "update").unwrap().0, "npm install -g --no-fund --no-audit --loglevel=error --allow-scripts=@openai/codex @openai/codex@latest");
        assert_eq!(cmd("ollama", "run").unwrap().0, "mkdir -p ~/Projeler && cd ~/Projeler && ollama run llama3.2:1b");
        assert_eq!(cmd("claude", "run").unwrap(), ("mkdir -p ~/Projeler && cd ~/Projeler && claude".into(), "Claude Code".into()));
        assert_eq!(cmd("claude", "x").unwrap_err().code, "invalid");
        assert_eq!(cmd("yok", "run").unwrap_err().code, "not_found");
        // İç komutlar JSON'a sızmaz.
        let j = serde_json::to_value(&catalog()[0]).unwrap();
        assert!(j.get("install").is_none() && j.get("path").is_none());
        assert_eq!(j["installed"], false);
    }

    #[tokio::test]
    async fn installed_state() {
        let dir = std::env::temp_dir().join(format!("mk-ag-{}", crate::util::random_hex(4)));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("ollama");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        // "-l" kabuğu /etc/profile ile PATH'i sıfırlayabilir: PATH'i kendisi kuran sarmalayıcı.
        let wrap = dir.join("kabuk");
        std::fs::write(&wrap, format!("#!/bin/sh\nexport PATH={}:/usr/bin:/bin\nexec /bin/sh -c \"$3\"\n", dir.display())).unwrap();
        std::fs::set_permissions(&wrap, std::fs::Permissions::from_mode(0o755)).unwrap();
        let w = wrap.to_string_lossy().into_owned();
        let a = crate::uagent::Local {
            command: std::sync::Arc::new(move || {
                Ok(crate::termproc::login::CmdSpec { path: w.clone(), argv0: "kabuk".into(), args: vec![], dir: "/".into(), env: vec![] })
            }),
            home: dir.to_string_lossy().into_owned(),
        };
        let l = list(&a, "u").await.unwrap();
        let o = l.iter().find(|x| x.id == "ollama").unwrap();
        assert!(o.installed);
        assert_eq!(o.path, fake.to_string_lossy());
        assert!(!l.iter().find(|x| x.id == "claude").unwrap().installed);
        let _ = std::fs::remove_dir_all(dir);
    }
}
