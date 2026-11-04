// Package pyman, Python sürümleri (uv) ve şablonlu Python projeleridir.
// Web arayüzü (makerd → makerd-user) ve `mos` CLI (süreç içi) aynı kodu
// kullanır; tüm durum kullanıcının ev dizinindedir (~/Projeler, uv önbelleği).
// Uzun işler (kurulum, çalıştırma) terminal oturumunda görünür çalışır: bu
// paket yalnız komutu üretir, çalıştıran taraf onu bir oturuma bağlar.
package pyman

import (
	"context"
	"embed"
	"encoding/json"
	"fmt"
	"io/fs"
	"path"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

//go:embed templates
var tmplFS embed.FS

// ProjectsDir kullanıcının proje kökü (ev dizinine göre).
const ProjectsDir = "Projeler"

// SystemPython imajdaki Python'dur (libcamera, picamera2, gpiozero, lgpio burada).
const SystemPython = "/usr/bin/python3"

// Text iki dilli metindir.
type Text struct {
	TR string `json:"tr"`
	EN string `json:"en"`
}

// Template bir proje şablonudur.
type Template struct {
	ID       string   `json:"id"`
	Order    int      `json:"order"`
	Category string   `json:"category"`
	Icon     string   `json:"icon"`
	Title    Text     `json:"title"`
	Desc     Text     `json:"desc"`
	Deps     []string `json:"deps"`
	Needs    []string `json:"needs,omitempty"`
	Run      string   `json:"run,omitempty"`
}

// Templates gömülü şablonlardır (sıralı).
func Templates() []Template {
	var out []Template
	des, _ := tmplFS.ReadDir("templates")
	for _, de := range des {
		b, err := tmplFS.ReadFile("templates/" + de.Name() + "/template.json")
		if err != nil {
			continue
		}
		var t Template
		if json.Unmarshal(b, &t) == nil && t.ID == de.Name() {
			out = append(out, t)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Order < out[j].Order })
	return out
}

func templateByID(id string) (Template, bool) {
	for _, t := range Templates() {
		if t.ID == id {
			return t, true
		}
	}
	return Template{}, false
}

var (
	reName    = regexp.MustCompile(`^[a-z0-9][a-z0-9_-]{0,39}$`)
	reVersion = regexp.MustCompile(`^3\.[0-9]{1,2}(\.[0-9]{1,2})?$`)
)

// ValidName proje adını doğrular.
func ValidName(n string) error {
	if !reName.MatchString(n) {
		return apierr.Invalid("Proje adı küçük harf veya rakamla başlamalı; en fazla 40 karakter (a-z, 0-9, - ve _).")
	}
	return nil
}

// ValidVersion Python sürümünü doğrular ("3.12" veya "3.12.7").
func ValidVersion(v string) error {
	if !reVersion.MatchString(v) {
		return apierr.Invalid("Geçersiz Python sürümü (örnek: 3.12).")
	}
	return nil
}

// --- Python sürümleri ------------------------------------------------------

// Version kurulu veya indirilebilir bir Python'dur.
type Version struct {
	Version   string `json:"version"`
	Minor     string `json:"minor"`
	Installed bool   `json:"installed"`
	System    bool   `json:"system,omitempty"`
	Default   bool   `json:"default,omitempty"`
	Path      string `json:"path,omitempty"`
}

// Versions uv'nin kurulu ve indirilebilir CPython sürümleridir (her küçük sürümün en yenisi).
func Versions(ctx context.Context, a uagent.Agent, user string) ([]Version, error) {
	var raw []struct {
		Version        string  `json:"version"`
		Path           *string `json:"path"`
		Implementation string  `json:"implementation"`
		Variant        string  `json:"variant"`
		Parts          struct {
			Major, Minor int
		} `json:"version_parts"`
	}
	if err := uagent.JSON(ctx, a, user, "uv python list --all-versions --output-format json 2>/dev/null || uv python list --output-format json", &raw); err != nil {
		return nil, apierr.Unavailable("uv ile Python sürümleri okunamadı: " + err.Error())
	}
	def, _, _ := uagent.Output(ctx, a, user, `readlink -f "$HOME/.local/bin/python3" 2>/dev/null || true`)
	defPath := strings.TrimSpace(string(def))
	best := map[string]Version{}
	var installed []Version
	for _, r := range raw {
		if r.Implementation != "cpython" || (r.Variant != "" && r.Variant != "default") || strings.ContainsAny(r.Version, "abrc") {
			continue
		}
		minor := fmt.Sprintf("%d.%d", r.Parts.Major, r.Parts.Minor)
		v := Version{Version: r.Version, Minor: minor}
		if r.Path != nil {
			v.Installed, v.Path = true, *r.Path
			v.System = !strings.Contains(v.Path, "/uv/python/")
			v.Default = defPath != "" && (defPath == v.Path || strings.HasPrefix(defPath, path.Dir(path.Dir(v.Path))+"/"))
			installed = append(installed, v)
			continue
		}
		if b, ok := best[minor]; !ok || newer(r.Version, b.Version) {
			best[minor] = v
		}
	}
	out := installed
	have := map[string]bool{}
	for _, v := range installed {
		have[v.Minor] = true
	}
	for m, v := range best {
		if !have[m] {
			out = append(out, v)
		}
		_ = m
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Installed != out[j].Installed {
			return out[i].Installed
		}
		return newer(out[i].Version, out[j].Version)
	})
	return out, nil
}

func newer(a, b string) bool {
	pa, pb := strings.Split(a, "."), strings.Split(b, ".")
	for i := 0; i < 3; i++ {
		var x, y int
		if i < len(pa) {
			fmt.Sscan(pa[i], &x)
		}
		if i < len(pb) {
			fmt.Sscan(pb[i], &y)
		}
		if x != y {
			return x > y
		}
	}
	return false
}

// PythonCmd sürüm işlemi için terminal komutunu üretir.
func PythonCmd(op, version string) (string, error) {
	if err := ValidVersion(version); err != nil {
		return "", err
	}
	switch op {
	case "install":
		return "uv python install " + version, nil
	case "remove":
		return "uv python uninstall " + version, nil
	case "default":
		// python/python3 komutları ~/.local/bin altında bu sürüme bağlanır.
		return "uv python install --default " + version + " && python3 --version", nil
	}
	return "", apierr.Invalid("Geçersiz işlem.")
}

// --- Projeler --------------------------------------------------------------

// Meta proje kökündeki .makeros.json'dur.
type Meta struct {
	Template string    `json:"template"`
	Python   string    `json:"python"`
	Created  time.Time `json:"created"`
	Run      string    `json:"run,omitempty"`
}

// Project listede gösterilen projedir.
type Project struct {
	Name     string    `json:"name"`
	Path     string    `json:"path"`
	Template string    `json:"template,omitempty"`
	Python   string    `json:"python,omitempty"`
	Created  time.Time `json:"created"`
	Modified time.Time `json:"modified"`
	Venv     bool      `json:"venv"`
}

// Projects ~/Projeler altındaki Python projeleridir (pyproject.toml olanlar).
func Projects(ctx context.Context, a uagent.Agent, user string) ([]Project, error) {
	raw, err := a.FS(ctx, user, termproc.Open{Mode: "list", Path: ProjectsDir})
	if err != nil {
		return []Project{}, nil // klasör henüz yok
	}
	var l termproc.Listing
	if err := json.Unmarshal(raw, &l); err != nil {
		return nil, err
	}
	out := []Project{}
	for _, e := range l.Entries {
		if !e.Dir || strings.HasPrefix(e.Name, ".") {
			continue
		}
		dir := ProjectsDir + "/" + e.Name
		inner, err := a.FS(ctx, user, termproc.Open{Mode: "list", Path: dir})
		if err != nil {
			continue
		}
		var il termproc.Listing
		_ = json.Unmarshal(inner, &il)
		p := Project{Name: e.Name, Path: l.Path + "/" + e.Name, Modified: e.Mtime, Created: e.Mtime}
		ok := false
		for _, f := range il.Entries {
			switch f.Name {
			case "pyproject.toml":
				ok = true
			case ".venv":
				p.Venv = true
			case ".makeros.json":
				var b strings.Builder
				if a.Read(ctx, user, dir+"/.makeros.json", &b) == nil {
					var m Meta
					if json.Unmarshal([]byte(b.String()), &m) == nil {
						p.Template, p.Python, p.Created = m.Template, m.Python, m.Created
					}
				}
			}
		}
		if ok {
			out = append(out, p)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Modified.After(out[j].Modified) })
	return out, nil
}

// Create şablondan projeyi yazar ve ortamı kuracak terminal komutunu döner.
// python: "system" (varsayılan; kamera/GPIO kütüphaneleri hazır) veya "3.12" gibi.
func Create(ctx context.Context, a uagent.Agent, user, name, tmplID, python string) (string, error) {
	if err := ValidName(name); err != nil {
		return "", err
	}
	t, ok := templateByID(tmplID)
	if !ok {
		return "", apierr.Invalid("Şablon bulunamadı.")
	}
	if python == "" {
		python = "system"
	}
	if python != "system" {
		if err := ValidVersion(python); err != nil {
			return "", err
		}
	}
	dir := ProjectsDir + "/" + name
	if _, err := a.FS(ctx, user, termproc.Open{Mode: "stat", Path: dir}); err == nil {
		return "", apierr.Conflict("Bu adla bir proje zaten var.")
	}
	root := "templates/" + t.ID
	err := fs.WalkDir(tmplFS, root, func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || path.Base(p) == "template.json" {
			return err
		}
		b, err := tmplFS.ReadFile(p)
		if err != nil {
			return err
		}
		body := strings.ReplaceAll(string(b), "{{name}}", name)
		return a.Write(ctx, user, dir+"/"+strings.TrimPrefix(p, root+"/"), strings.NewReader(body))
	})
	if err != nil {
		return "", err
	}
	deps := make([]string, len(t.Deps))
	for i, d := range t.Deps {
		deps[i] = fmt.Sprintf("%q", d)
	}
	pyproject := fmt.Sprintf("[project]\nname = %q\nversion = \"0.1.0\"\ndescription = %q\nrequires-python = \">=3.11\"\ndependencies = [%s]\n\n[tool.uv]\npackage = false\n",
		name, t.Title.TR, strings.Join(deps, ", "))
	if err := a.Write(ctx, user, dir+"/pyproject.toml", strings.NewReader(pyproject)); err != nil {
		return "", err
	}
	meta, _ := json.MarshalIndent(Meta{Template: t.ID, Python: python, Created: time.Now().UTC(), Run: t.Run}, "", "  ")
	if err := a.Write(ctx, user, dir+"/.makeros.json", strings.NewReader(string(meta)+"\n")); err != nil {
		return "", err
	}
	_ = a.Write(ctx, user, dir+"/.gitignore", strings.NewReader(".venv/\n__pycache__/\n"))
	py := SystemPython
	if python != "system" {
		py = python
	}
	// --system-site-packages: imajdaki libcamera/picamera2/gpiozero/lgpio/numpy projede de görünür.
	return fmt.Sprintf("cd ~/%s && echo '» %s: ortam hazırlanıyor (uv)' && uv venv --quiet --system-site-packages --python %s && uv sync && echo && echo '✔ Hazır. Çalıştırmak için: %s'",
		dir, name, py, runCmd(t.Run)), nil
}

func runCmd(r string) string {
	if r == "" {
		return "uv run main.py"
	}
	return r
}

// RunCmd projeyi çalıştıracak terminal komutudur.
func RunCmd(ctx context.Context, a uagent.Agent, user, name string) (string, error) {
	if err := ValidName(name); err != nil {
		return "", err
	}
	dir := ProjectsDir + "/" + name
	run := ""
	var b strings.Builder
	if a.Read(ctx, user, dir+"/.makeros.json", &b) == nil {
		var m Meta
		if json.Unmarshal([]byte(b.String()), &m) == nil {
			run = m.Run
		}
	}
	return fmt.Sprintf("cd ~/%s && %s", dir, runCmd(run)), nil
}

// Delete projeyi siler.
func Delete(ctx context.Context, a uagent.Agent, user, name string) error {
	if err := ValidName(name); err != nil {
		return err
	}
	_, err := a.FS(ctx, user, termproc.Open{Mode: "remove", Path: ProjectsDir + "/" + name})
	return err
}
