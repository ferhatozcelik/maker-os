// Package agents, terminal tabanlı yapay zeka kodlama ajanları ve yerel model
// çalıştırıcı kataloğudur. Kurulum/güncelleme/kaldırma kullanıcının hesabıyla
// bir terminal oturumunda görünür çalışır (npm önekleri ~/.local).
package agents

import (
	"context"
	"strings"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

// Agent katalog girdisidir.
type Agent struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	Vendor    string `json:"vendor"`
	Command   string `json:"command"`
	Summary   string `json:"summary"`
	Tip       string `json:"tip"`
	Account   string `json:"account"`
	Docs      string `json:"docs"`
	Installed bool   `json:"installed"`
	Path      string `json:"path,omitempty"`

	install, update, remove, run string
}

// npm komutları: bildirimsiz (fund/audit/güncelleme uyarısı yok) ve paketin kendi kurulum betiklerine
// açık izinle (npm 11 betikleri varsayılan engeller; ör. OpenCode platform ikilisini, Gemini keytar'ı kurar).
func npm(pkg string, allow ...string) (string, string, string) {
	a := strings.Join(append([]string{pkg}, allow...), ",")
	flags := "--no-fund --no-audit --loglevel=error --allow-scripts=" + a + " "
	return "npm install -g " + flags + pkg, "npm install -g " + flags + pkg + "@latest", "npm uninstall -g --loglevel=error " + pkg
}

// Catalog sabit listedir (kurulum yöntemleri sağlayıcıların resmi belgelerinden).
func Catalog() []Agent {
	ci, cu, cr := npm("@openai/codex")
	gi, gu, gr := npm("@google/gemini-cli", "@github/keytar")
	oi, ou, or := npm("opencode-ai")
	return []Agent{
		{ID: "claude", Name: "Claude Code", Vendor: "Anthropic", Command: "claude",
			Summary: "Proje klasöründe kod yazan, düzenleyen ve komut çalıştıran ajan.",
			Tip:     "Çok dosyalı Python/GPIO projelerini adım adım kurmak ve hata ayıklamak için.",
			Account: "Claude hesabı (Pro/Max) veya Anthropic API anahtarı", Docs: "https://docs.claude.com/claude-code",
			install: "curl -fsSL https://claude.ai/install.sh | bash", update: "claude update",
			remove: "rm -rf ~/.local/bin/claude ~/.local/share/claude && echo kaldırıldı"},
		{ID: "codex", Name: "Codex CLI", Vendor: "OpenAI", Command: "codex",
			Summary: "Terminalden çalışan, yerel dosyalarla çalışan kodlama ajanı.",
			Tip:     "ChatGPT hesabınız varsa ek ücret olmadan denemek için.",
			Account: "ChatGPT hesabı veya OpenAI API anahtarı", Docs: "https://github.com/openai/codex",
			install: ci, update: cu, remove: cr},
		{ID: "gemini", Name: "Gemini CLI", Vendor: "Google", Command: "gemini",
			Summary: "Google hesabıyla kullanılan açık kaynak terminal ajanı.",
			Tip:     "Kişisel Google hesabıyla ücretsiz kullanım kotası vardır.",
			Account: "Google hesabı veya Gemini API anahtarı", Docs: "https://github.com/google-gemini/gemini-cli",
			install: gi, update: gu, remove: gr},
		{ID: "opencode", Name: "OpenCode", Vendor: "Açık kaynak", Command: "opencode",
			Summary: "Birçok model sağlayıcısıyla (Anthropic, OpenAI, Google, yerel Ollama) çalışan açık kaynak ajan.",
			Tip:     "Tek araçta farklı modelleri denemek veya Ollama ile internetsiz çalışmak için.",
			Account: "Seçtiğiniz sağlayıcının hesabı/API anahtarı veya yerel Ollama", Docs: "https://opencode.ai/docs",
			install: oi, update: ou, remove: or},
		{ID: "ollama", Name: "Ollama", Vendor: "Yerel modeller", Command: "ollama",
			Summary: "Küçük dil modellerini internetsiz, Pi'nin kendisinde çalıştırır.",
			Tip:     "4 GB RAM için 1-3B parametreli modeller (llama3.2:1b, qwen2.5:1.5b, gemma3:1b).",
			Account: "Hesap gerekmez", Docs: "https://ollama.com/library",
			install: "curl -fsSL https://ollama.com/install.sh | sudo sh", update: "curl -fsSL https://ollama.com/install.sh | sudo sh",
			remove: "sudo systemctl disable --now ollama 2>/dev/null; sudo rm -rf /usr/local/bin/ollama /usr/local/lib/ollama /etc/systemd/system/ollama.service && echo kaldırıldı",
			run:    "ollama run llama3.2:1b"},
	}
}

// List katalogu kurulu durumla döner.
func List(ctx context.Context, a uagent.Agent, user string) ([]Agent, error) {
	cat := Catalog()
	var sb strings.Builder
	for _, x := range cat {
		sb.WriteString("printf '%s=%s\\n' " + x.ID + " \"$(command -v " + x.Command + " 2>/dev/null)\"; ")
	}
	out, _, err := uagent.Output(ctx, a, user, sb.String())
	if err != nil {
		return nil, err
	}
	paths := map[string]string{}
	for _, l := range strings.Split(string(out), "\n") {
		if k, v, ok := strings.Cut(strings.TrimSpace(l), "="); ok {
			paths[k] = v
		}
	}
	for i := range cat {
		cat[i].Path = paths[cat[i].ID]
		cat[i].Installed = cat[i].Path != ""
	}
	return cat, nil
}

// Cmd işlem için terminal komutunu ve başlığını üretir.
func Cmd(id, op string) (cmd, title string, err error) {
	for _, x := range Catalog() {
		if x.ID != id {
			continue
		}
		switch op {
		case "install":
			return x.install, x.Name + " · kurulum", nil
		case "update":
			return x.update, x.Name + " · güncelleme", nil
		case "remove":
			return x.remove, x.Name + " · kaldırma", nil
		case "run":
			r := x.run
			if r == "" {
				r = x.Command
			}
			return "mkdir -p ~/Projeler && cd ~/Projeler && " + r, x.Name, nil
		}
		return "", "", apierr.Invalid("Geçersiz işlem.")
	}
	return "", "", apierr.NotFound("Ajan bulunamadı.")
}
