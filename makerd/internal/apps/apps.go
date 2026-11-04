// Package apps, kabuğun uygulama kaydıdır: yerleşik web uygulamaları ve
// /usr/share/applications altındaki .desktop dosyalarından yerel uygulamalar.
package apps

import (
	"bufio"
	"context"
	"errors"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
)

// App, GET /apps öğesi.
type App struct {
	ID         string   `json:"id"`
	Name       string   `json:"name"`
	Icon       string   `json:"icon"`
	Kind       string   `json:"kind"` // web | native
	URL        string   `json:"url,omitempty"`
	Exec       string   `json:"exec,omitempty"`
	Categories []string `json:"categories"`

	argv     []string
	terminal bool
}

func web(name, title string, cats ...string) App {
	return App{ID: "makeros." + name, Name: title, Icon: "/shell/icons/" + name + ".svg", Kind: "web",
		URL: "/shell/apps/" + name + "/", Categories: cats}
}

// Builtin yerleşik web uygulamalarıdır.
func Builtin() []App {
	return []App{
		web("settings", "Ayarlar", "System", "Settings"),
		web("terminal", "Terminal", "System", "Utility"),
		web("activity", "Etkinlik Monitörü", "System", "Monitor"),
		web("files", "Dosyalar", "System", "FileManager"),
		web("python", "Python", "Development"),
		web("agents", "AI Ajanları", "Development", "AI"),
		web("camera", "Kamera", "AudioVideo", "Development"),
		web("gpio", "GPIO", "Development", "Electronics"),
		web("plugins", "Eklentiler", "System", "Plugin"),
		web("screen", "Ekran Modu", "System", "RemoteAccess"),
	}
}

// Launcher yerel uygulamayı başlatır (mock'ta yalnız günlük).
type Launcher func(ctx context.Context, app App, argv []string) error

// Registry uygulama kaydı.
type Registry struct {
	Log         *slog.Logger
	DesktopDirs []string // .desktop dizinleri
	WebDir      string   // ikon varlığı denetimi için
	Extra       []App    // mock yerel uygulamalar
	Launch      Launcher

	mu     sync.Mutex
	cached []App
	at     time.Time
}

// List uygulamaları döndürür (.desktop taraması 10 sn önbelleklidir).
func (r *Registry) List() []App {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.cached != nil && time.Since(r.at) < 10*time.Second {
		return r.cached
	}
	out := Builtin()
	out = append(out, r.Extra...)
	var native []App
	seen := map[string]bool{}
	for _, d := range r.DesktopDirs {
		files, _ := filepath.Glob(filepath.Join(d, "*.desktop"))
		for _, f := range files {
			a, ok := ParseDesktop(f)
			if !ok || seen[a.ID] {
				continue
			}
			seen[a.ID] = true
			a.Icon = r.icon(a.Icon)
			native = append(native, a)
		}
	}
	sort.Slice(native, func(i, j int) bool { return strings.ToLower(native[i].Name) < strings.ToLower(native[j].Name) })
	r.cached, r.at = append(out, native...), time.Now()
	return r.cached
}

func (r *Registry) icon(name string) string {
	if name != "" && !strings.ContainsAny(name, "/.") && r.WebDir != "" {
		if _, err := os.Stat(filepath.Join(r.WebDir, "icons", name+".svg")); err == nil {
			return "/shell/icons/" + name + ".svg"
		}
	}
	return "/shell/icons/app.svg"
}

// Start uygulamayı başlatır: web → 400 (kabuk açar), yok → 404.
func (r *Registry) Start(ctx context.Context, id string) error {
	for _, a := range r.List() {
		if a.ID != id {
			continue
		}
		if a.Kind == "web" {
			return apierr.Invalid("Web uygulamalarını kabuk kendisi açar.")
		}
		argv := a.argv
		if len(argv) == 0 {
			argv = SplitExec(a.Exec)
		}
		if len(argv) == 0 {
			return apierr.Invalid("Uygulamanın Exec satırı boş.")
		}
		if a.terminal {
			argv = append([]string{"foot"}, argv...)
		}
		if err := r.Launch(ctx, a, argv); err != nil {
			return apierr.Internal("Uygulama başlatılamadı: " + err.Error())
		}
		return nil
	}
	return apierr.NotFound("Uygulama bulunamadı.")
}

// ParseDesktop bir .desktop dosyasını ayrıştırır. Gizli/uygulama dışı girdiler false döner.
func ParseDesktop(path string) (App, bool) {
	f, err := os.Open(path)
	if err != nil {
		return App{}, false
	}
	defer f.Close()
	kv := map[string]string{}
	in := false
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		l := strings.TrimSpace(sc.Text())
		if strings.HasPrefix(l, "[") {
			in = l == "[Desktop Entry]"
			continue
		}
		if !in || l == "" || l[0] == '#' {
			continue
		}
		if k, v, ok := strings.Cut(l, "="); ok {
			kv[strings.TrimSpace(k)] = strings.TrimSpace(v)
		}
	}
	if kv["Type"] != "Application" || kv["NoDisplay"] == "true" || kv["Hidden"] == "true" || kv["Exec"] == "" {
		return App{}, false
	}
	name := kv["Name[tr]"]
	if name == "" {
		name = kv["Name"]
	}
	if name == "" {
		return App{}, false
	}
	cats := []string{}
	for _, c := range strings.Split(kv["Categories"], ";") {
		if c = strings.TrimSpace(c); c != "" {
			cats = append(cats, c)
		}
	}
	exec := kv["Exec"]
	return App{ID: strings.TrimSuffix(filepath.Base(path), ".desktop"), Name: name, Icon: kv["Icon"], Kind: "native",
		Exec: exec, Categories: cats, argv: SplitExec(exec), terminal: kv["Terminal"] == "true"}, true
}

// SplitExec, Exec satırını (çift tırnak ve ters bölü kaçışlarıyla) argümanlara
// böler ve alan kodlarını (%f %U ...) kaldırır. Kabuk kullanılmaz.
func SplitExec(s string) []string {
	var out []string
	var cur strings.Builder
	inQ, has, esc := false, false, false
	for _, r := range s {
		switch {
		case esc:
			cur.WriteRune(r)
			esc = false
		case r == '\\':
			esc = true
		case r == '"':
			inQ, has = !inQ, true
		case (r == ' ' || r == '\t') && !inQ:
			if has || cur.Len() > 0 {
				out = append(out, cur.String())
			}
			cur.Reset()
			has = false
		default:
			cur.WriteRune(r)
		}
	}
	if has || cur.Len() > 0 {
		out = append(out, cur.String())
	}
	res := out[:0]
	for _, a := range out {
		if len(a) == 2 && a[0] == '%' {
			if a[1] == '%' {
				res = append(res, "%")
			}
			continue // %f %F %u %U %i %c %k ...
		}
		res = append(res, strings.ReplaceAll(a, "%%", "%"))
	}
	return res
}

// SystemLauncher, uygulamayı varsa systemd-run --user ile apps.slice'ta,
// yoksa ayrı süreç grubunda doğrudan başlatır. Wayland ortamı eksikse tamamlanır.
func SystemLauncher(log *slog.Logger) Launcher {
	return func(_ context.Context, a App, argv []string) error {
		env := os.Environ()
		uid := strconv.Itoa(os.Getuid())
		if os.Getenv("XDG_RUNTIME_DIR") == "" {
			env = append(env, "XDG_RUNTIME_DIR=/run/user/"+uid)
		}
		if os.Getenv("WAYLAND_DISPLAY") == "" {
			env = append(env, "WAYLAND_DISPLAY=wayland-0")
		}
		if run, err := exec.LookPath("systemd-run"); err == nil {
			args := append([]string{"--user", "--collect", "--quiet", "--slice=apps.slice",
				"--unit=app-" + sanitize(a.ID) + "-" + strconv.FormatInt(time.Now().UnixNano()%1e6, 10), "--"}, argv...)
			cmd := exec.Command(run, args...)
			cmd.Env = env
			if out, err := cmd.CombinedOutput(); err == nil {
				log.Info("uygulama başlatıldı", "id", a.ID, "via", "systemd-run")
				return nil
			} else {
				log.Warn("systemd-run başarısız, doğrudan başlatılıyor", "err", err, "out", strings.TrimSpace(string(out)))
			}
		}
		path, err := exec.LookPath(argv[0])
		if err != nil {
			return errors.New(argv[0] + " bulunamadı")
		}
		cmd := exec.Command(path, argv[1:]...)
		cmd.Env = env
		cmd.Dir, _ = os.UserHomeDir()
		cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
		if err := cmd.Start(); err != nil {
			return err
		}
		go func() { _ = cmd.Wait() }()
		log.Info("uygulama başlatıldı", "id", a.ID, "pid", cmd.Process.Pid)
		return nil
	}
}

func sanitize(s string) string {
	b := []byte(s)
	for i, c := range b {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_') {
			b[i] = '_'
		}
	}
	return string(b)
}
