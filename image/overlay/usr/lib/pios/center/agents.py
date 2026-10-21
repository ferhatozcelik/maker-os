# Agent Yoneticisi: terminal tabanli yapay zeka kodlama ajanlarini ve
# yerel model calistiriciyi kurar, gunceller, kaldirir, baslatir.
import os
import shutil

from common import as_user, jobs, open_terminal, run, user_info

NPM_ENV = {"PATH": "/opt/pios/node/bin:/usr/local/bin:/usr/bin:/bin", "HOME": "/root",
           "npm_config_update_notifier": "false", "npm_config_fund": "false"}

# kind: "user" -> kullanici olarak curl betigi (~/.local/bin),
#       "npm"  -> /opt/pios/node altina global npm paketi (root),
#       "root" -> sistem geneli betik (root).
AGENTS = [
    {
        "id": "claude", "name": "Claude Code", "vendor": "Anthropic",
        "commands": ["claude"], "kind": "user",
        "install": "curl -fsSL https://claude.ai/install.sh | bash",
        "update": "claude update",
        "remove_paths": [".local/bin/claude", ".local/share/claude"],
        "summary": "Proje klasöründe kod yazan, düzenleyen ve komut çalıştıran ajan.",
        "tip": "Çok dosyalı Python/GPIO projelerini adım adım kurmak ve hata ayıklamak için.",
        "account": "Claude hesabı (Pro/Max) veya Anthropic API anahtarı",
        "docs": "https://docs.claude.com/claude-code",
    },
    {
        "id": "codex", "name": "Codex CLI", "vendor": "OpenAI",
        "commands": ["codex"], "kind": "npm", "package": "@openai/codex",
        "summary": "Terminalden çalışan, yerel dosyalarla çalışan kodlama ajanı.",
        "tip": "ChatGPT hesabınız varsa ek ücret olmadan denemek için.",
        "account": "ChatGPT hesabı veya OpenAI API anahtarı",
        "docs": "https://github.com/openai/codex",
    },
    {
        "id": "gemini", "name": "Gemini CLI", "vendor": "Google",
        "commands": ["gemini"], "kind": "npm", "package": "@google/gemini-cli",
        "summary": "Google hesabıyla kullanılan açık kaynak terminal ajanı.",
        "tip": "Öğrenciler için: kişisel Google hesabıyla ücretsiz kullanım kotası var.",
        "account": "Google hesabı veya Gemini API anahtarı",
        "docs": "https://github.com/google-gemini/gemini-cli",
    },
    {
        "id": "opencode", "name": "OpenCode", "vendor": "Açık kaynak",
        "commands": ["opencode"], "kind": "npm", "package": "opencode-ai",
        "summary": "Birçok model sağlayıcısıyla (Anthropic, OpenAI, Google, yerel Ollama) çalışan açık kaynak terminal ajanı.",
        "tip": "Tek araçta farklı modelleri denemek veya Ollama ile internetsiz çalışmak için.",
        "account": "Seçtiğiniz sağlayıcının hesabı/API anahtarı veya yerel Ollama",
        "docs": "https://opencode.ai/docs",
    },
    {
        "id": "antigravity", "name": "Antigravity CLI", "vendor": "Google",
        "commands": ["agy", "antigravity"], "kind": "user",
        "install": "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        "remove_paths": [".local/bin/agy", ".local/bin/antigravity"],
        "summary": "Antigravity platformunun terminal ajanı.",
        "tip": "Antigravity IDE kullanıyorsanız aynı hesapla terminalden devam etmek için.",
        "account": "Google hesabı",
        "docs": "https://antigravity.google/download",
    },
    {
        "id": "ollama", "name": "Ollama", "vendor": "Yerel modeller",
        "commands": ["ollama"], "kind": "root",
        "install": "curl -fsSL https://ollama.com/install.sh | sh",
        "remove": "systemctl disable --now ollama 2>/dev/null; rm -f /usr/local/bin/ollama "
                  "/etc/systemd/system/ollama.service; rm -rf /usr/local/lib/ollama",
        "run": "ollama run llama3.2:1b",
        "summary": "Küçük dil modellerini internetsiz, Pi'nin kendisinde çalıştırır.",
        "tip": "4 GB RAM için 1-3B parametreli modeller (llama3.2:1b, qwen2.5:1.5b, gemma3:1b).",
        "account": "Hesap gerekmez",
        "docs": "https://ollama.com/library",
    },
]


def _find(agent):
    home = user_info()[2]
    path = NPM_ENV["PATH"].split(":") + [os.path.join(home, ".local/bin")]
    for cmd in agent["commands"]:
        found = shutil.which(cmd, path=":".join(path))
        if found:
            return found
    return None


def _version(exe):
    # Kullanici olarak: bazi CLI'lar --version'da bile ~/.<ad> dizini olusturur;
    # root olarak calisirsa dizin root'a ait kalir ve ajan acilamaz.
    rc, out, err = run(as_user([exe, "--version"]), timeout=15,
                       env={"PATH": NPM_ENV["PATH"], "HOME": user_info()[2]})
    text = (out or err).splitlines()
    return text[0][:60] if rc == 0 and text else ""


def node_version():
    rc, out, _ = run(["/opt/pios/node/bin/node", "--version"])
    return out if rc == 0 else ""


def list_agents():
    rows = []
    for a in AGENTS:
        exe = _find(a)
        row = {k: a[k] for k in ("id", "name", "vendor", "summary", "tip", "account", "docs")}
        row["installed"] = bool(exe)
        row["version"] = _version(exe) if exe else ""
        rows.append(row)
    return rows


def _get(aid):
    return next((a for a in AGENTS if a["id"] == aid), None)


def action(aid, what):
    a = _get(aid)
    if not a:
        return None, "bilinmeyen agent"
    title = "%s: %s" % (a["name"], {"install": "kuruluyor", "update": "guncelleniyor",
                                    "remove": "kaldiriliyor"}.get(what, what))
    if a["kind"] == "npm":
        npm = {"install": ["npm", "install", "-g", a["package"] + "@latest"],
               "update": ["npm", "install", "-g", a["package"] + "@latest"],
               "remove": ["npm", "uninstall", "-g", a["package"]]}.get(what)
        if not npm:
            return None, "gecersiz islem"
        return jobs.start(title, npm, env=NPM_ENV), ""
    if what == "remove":
        if a["kind"] == "user":
            home = user_info()[2]
            for rel in a.get("remove_paths", []):
                p = os.path.join(home, rel)
                if os.path.isdir(p) and not os.path.islink(p):
                    shutil.rmtree(p, ignore_errors=True)
                elif os.path.lexists(p):
                    os.remove(p)
            return jobs.start(title, ["true"]), ""
        return jobs.start(title, ["sh", "-c", a["remove"]]), ""
    script = a.get("update") if what == "update" and a.get("update") else a.get("install")
    if what not in ("install", "update") or not script:
        return None, "gecersiz islem"
    if a["kind"] == "user":
        return jobs.start(title, ["bash", "-lc", script], user=True), ""
    return jobs.start(title, ["sh", "-c", script]), ""


def launch(aid, project_path=None):
    a = _get(aid)
    exe = _find(a) if a else None
    if not exe:
        return False
    command = a.get("run") or os.path.basename(exe)
    open_terminal(command, title=a["name"], cwd=project_path)
    return True
