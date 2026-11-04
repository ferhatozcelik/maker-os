// Package plugins, kullanıcı eklentileridir: zip olarak paylaşılan küçük Python
// uygulamaları (v1 biçimiyle uyumlu). Dosyalar kullanıcının evinde
// (~/.local/share/makeros/plugins/<id>) durur; süreç kullanıcının hesabıyla
// (systemd --user varsa apps.slice içinde, yoksa arka plan süreci) çalışır.
// Web arayüzü olan eklentiler 127.0.0.1:<web_port> dinler; makerd oturum
// doğrulamasından sonra /plugin/<id>/ adresinden yönlendirir.
package plugins

import (
	"archive/zip"
	"bytes"
	"context"
	"embed"
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"path"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

//go:embed template
var tmplFS embed.FS

// Dir eklentilerin kökü (ev dizinine göre).
const Dir = ".local/share/makeros/plugins"

// Sınırlar (v1 ile aynı).
const (
	MaxZip      = 15 << 20
	MaxUnpacked = 64 << 20
	MaxFiles    = 2000
)

var reID = regexp.MustCompile(`^[a-z][a-z0-9-]{0,39}$`)

// Manifest plugin.json'dur.
type Manifest struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`
	Version     string   `json:"version"`
	Description string   `json:"description"`
	Author      string   `json:"author,omitempty"`
	Entrypoint  string   `json:"entrypoint"`
	WebPort     int      `json:"web_port,omitempty"`
	Icon        string   `json:"icon,omitempty"`
	Args        []string `json:"args,omitempty"`
	// Theme isteğe bağlı tema paketi (duvar kâğıtları, logo, işaret, vurgu, uygulamalar); kabuk tema seçicide gösterir.
	Theme json.RawMessage `json:"theme,omitempty"`
	// Status isteğe bağlı durum göstergesi ({"api":"/api/...","label":"..."}); kabuk menü çubuğunda gösterir.
	Status json.RawMessage `json:"status,omitempty"`
	// Widget isteğe bağlı masaüstü kartı ({"path":"/widget","w":340,"h":220}); kabuk masaüstünde gösterir.
	Widget json.RawMessage `json:"widget,omitempty"`
	// App false ise eklentinin kendi web arayüzü ayrı uygulama olarak listelenmez (ör. durumu widget gösterir).
	App *bool `json:"app,omitempty"`
	// Autostart: eklenti gerektiğinde (tema/durum) kabuk tarafından otomatik başlatılır.
	Autostart bool `json:"autostart,omitempty"`
}

// Plugin listede gösterilendir.
type Plugin struct {
	Manifest
	Running bool `json:"running"`
	Pid     int  `json:"pid,omitempty"`
}

// Validate manifest alanlarını denetler.
func (m *Manifest) Validate() error {
	if !reID.MatchString(m.ID) {
		return apierr.Invalid("plugin.json: id küçük harfle başlamalı; en fazla 40 küçük harf, rakam veya tire.")
	}
	for k, v := range map[string]string{"name": m.Name, "version": m.Version} {
		if v == "" || len(v) > 100 {
			return apierr.Invalid("plugin.json: " + k + " eksik veya çok uzun.")
		}
	}
	if len(m.Description) > 500 {
		return apierr.Invalid("plugin.json: description en fazla 500 karakter.")
	}
	if m.Entrypoint == "" {
		m.Entrypoint = "main.py"
	}
	if !strings.HasSuffix(m.Entrypoint, ".py") || strings.Contains(m.Entrypoint, "..") || path.IsAbs(m.Entrypoint) {
		return apierr.Invalid("plugin.json: entrypoint eklenti içindeki bir .py dosyası olmalı.")
	}
	if m.WebPort != 0 && (m.WebPort < 1024 || m.WebPort > 65535) {
		return apierr.Invalid("plugin.json: web_port 1024–65535 arasında olmalı.")
	}
	if m.Icon != "" && (strings.Contains(m.Icon, "..") || path.IsAbs(m.Icon)) {
		return apierr.Invalid("plugin.json: icon eklenti içinde olmalı.")
	}
	if len(m.Theme) > 8<<10 || len(m.Status) > 2<<10 || len(m.Widget) > 1<<10 {
		return apierr.Invalid("plugin.json: theme/status alanı çok büyük.")
	}
	if len(m.Args) > 64 {
		return apierr.Invalid("plugin.json: en fazla 64 argüman.")
	}
	for _, a := range m.Args {
		if len(a) > 1024 || strings.ContainsRune(a, 0) {
			return apierr.Invalid("plugin.json: argüman geçersiz.")
		}
	}
	return nil
}

func root(id string) string { return Dir + "/" + id }

// TemplateZip şablon eklentiyi zip olarak döner.
func TemplateZip() ([]byte, error) {
	var b bytes.Buffer
	zw := zip.NewWriter(&b)
	err := fs.WalkDir(tmplFS, "template", func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		data, err := tmplFS.ReadFile(p)
		if err != nil {
			return err
		}
		w, err := zw.Create(strings.TrimPrefix(p, "template/"))
		if err != nil {
			return err
		}
		_, err = w.Write(data)
		return err
	})
	if err != nil {
		return nil, err
	}
	if err := zw.Close(); err != nil {
		return nil, err
	}
	return b.Bytes(), nil
}

// Unpack zip'i doğrular; manifest ve (göreli yol → içerik) döner.
// Zip kökünde plugin.json olmalı veya tek üst klasör içinde bulunmalı.
func Unpack(data []byte) (Manifest, map[string][]byte, error) {
	var m Manifest
	if len(data) > MaxZip {
		return m, nil, apierr.Invalid("Zip en fazla 15 MB olabilir.")
	}
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return m, nil, apierr.Invalid("Geçerli bir zip dosyası değil.")
	}
	prefix := ""
	for _, f := range zr.File {
		if path.Base(f.Name) == "plugin.json" && strings.Count(strings.Trim(f.Name, "/"), "/") <= 1 {
			if p := path.Dir(f.Name); p != "." && (prefix == "" || len(p) < len(prefix)) {
				prefix = p + "/"
			} else if p == "." {
				prefix = ""
				break
			}
		}
	}
	files := map[string][]byte{}
	var total int64
	for _, f := range zr.File {
		if f.FileInfo().IsDir() {
			continue
		}
		if f.Mode()&fs.ModeSymlink != 0 {
			return m, nil, apierr.Invalid("Zip sembolik bağ içeremez: " + f.Name)
		}
		name := strings.TrimPrefix(f.Name, prefix)
		clean := path.Clean(name)
		if !strings.HasPrefix(f.Name, prefix) || clean == "." || strings.HasPrefix(clean, "../") || clean == ".." || path.IsAbs(clean) || strings.Contains(clean, "\\") {
			return m, nil, apierr.Invalid("Zip eklenti klasörü dışına yazamaz: " + f.Name)
		}
		if strings.HasPrefix(clean, ".venv/") || strings.HasPrefix(clean, "__pycache__/") || strings.Contains(clean, "/__pycache__/") {
			continue
		}
		if len(files) >= MaxFiles {
			return m, nil, apierr.Invalid("Zip en fazla 2000 dosya içerebilir.")
		}
		total += int64(f.UncompressedSize64)
		if total > MaxUnpacked {
			return m, nil, apierr.Invalid("Açılmış eklenti en fazla 64 MB olabilir.")
		}
		rc, err := f.Open()
		if err != nil {
			return m, nil, apierr.Invalid("Zip okunamadı.")
		}
		b, err := io.ReadAll(io.LimitReader(rc, MaxUnpacked+1))
		rc.Close()
		if err != nil || int64(len(b)) > MaxUnpacked {
			return m, nil, apierr.Invalid("Zip okunamadı.")
		}
		files[clean] = b
	}
	mj, ok := files["plugin.json"]
	if !ok {
		return m, nil, apierr.Invalid("Zip kökünde plugin.json bulunmalı.")
	}
	if len(mj) > 16<<10 || json.Unmarshal(mj, &m) != nil {
		return m, nil, apierr.Invalid("plugin.json okunamadı (en fazla 16 KB geçerli JSON).")
	}
	if err := m.Validate(); err != nil {
		return m, nil, err
	}
	if _, ok := files[m.Entrypoint]; !ok {
		return m, nil, apierr.Invalid("Başlangıç dosyası bulunamadı: " + m.Entrypoint)
	}
	if _, ok := files["pyproject.toml"]; !ok {
		return m, nil, apierr.Invalid("pyproject.toml gerekli (bağımlılıklar uv ile kurulur).")
	}
	return m, files, nil
}

// Import zip'i kullanıcının eklenti dizinine yazar (aynı id varsa değiştirir).
func Import(ctx context.Context, a uagent.Agent, user string, data []byte) (Manifest, error) {
	m, files, err := Unpack(data)
	if err != nil {
		return m, err
	}
	_, _ = Stop(ctx, a, user, m.ID)
	if _, err := a.FS(ctx, user, termproc.Open{Mode: "remove", Path: root(m.ID)}); err != nil {
		// yoksa sorun değil
		_ = err
	}
	names := make([]string, 0, len(files))
	for n := range files {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, n := range names {
		if err := a.Write(ctx, user, root(m.ID)+"/"+n, bytes.NewReader(files[n])); err != nil {
			return m, err
		}
	}
	return m, nil
}

// List kurulu eklentileri ve çalışma durumlarını döner.
func List(ctx context.Context, a uagent.Agent, user string) ([]Plugin, error) {
	Seed(ctx, a, user)
	raw, err := a.FS(ctx, user, termproc.Open{Mode: "list", Path: Dir})
	if err != nil {
		return []Plugin{}, nil
	}
	var l termproc.Listing
	_ = json.Unmarshal(raw, &l)
	out := []Plugin{}
	for _, e := range l.Entries {
		if !e.Dir || !reID.MatchString(e.Name) {
			continue
		}
		p, err := Get(ctx, a, user, e.Name)
		if err == nil {
			out = append(out, p)
		}
	}
	return out, nil
}

// Get tek eklentiyi okur.
func Get(ctx context.Context, a uagent.Agent, user, id string) (Plugin, error) {
	if !reID.MatchString(id) {
		return Plugin{}, apierr.Invalid("Geçersiz eklenti kimliği.")
	}
	var b bytes.Buffer
	if err := a.Read(ctx, user, root(id)+"/plugin.json", &b); err != nil {
		return Plugin{}, apierr.NotFound("Eklenti bulunamadı.")
	}
	var p Plugin
	if json.Unmarshal(b.Bytes(), &p.Manifest) != nil || p.Validate() != nil || p.ID != id {
		return Plugin{}, apierr.Invalid("plugin.json bozuk.")
	}
	out, _, _ := uagent.Output(ctx, a, user, "p=$(cat ~/"+root(id)+"/.pid 2>/dev/null) && kill -0 \"$p\" 2>/dev/null && echo \"$p\"")
	if pid, err := strconv.Atoi(strings.TrimSpace(string(out))); err == nil && pid > 0 {
		p.Running, p.Pid = true, pid
	}
	return p, nil
}

func shq(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }

// Start eklentiyi arka planda başlatır (uv run; ilk seferde bağımlılıklar kurulur).
func Start(ctx context.Context, a uagent.Agent, user, id string) (Plugin, error) {
	p, err := Get(ctx, a, user, id)
	if err != nil || p.Running {
		return p, err
	}
	args := ""
	for _, x := range p.Args {
		args += " " + shq(x)
	}
	dir := "$HOME/" + root(id)
	run := fmt.Sprintf("cd %q && exec uv run --quiet %s%s >> .log 2>&1", dir, shq(p.Entrypoint), args)
	env := fmt.Sprintf("MAKEROS_PLUGIN_ID=%s MAKEROS_PLUGIN_PORT=%d", id, p.WebPort)
	// systemd --user varsa apps.slice içinde (bellek sınırı, kabuk korunur); yoksa ayrık süreç.
	cmd := fmt.Sprintf(`cd %q && : > .log && if command -v systemd-run >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
  systemd-run --user --quiet --collect --unit makeros-plugin-%s --slice apps --setenv=MAKEROS_PLUGIN_ID=%s --setenv=MAKEROS_PLUGIN_PORT=%d sh -c %s && sleep 0.3 && systemctl --user show -p MainPID --value makeros-plugin-%s > .pid
else
  env %s setsid sh -c %s </dev/null >/dev/null 2>&1 & echo $! > .pid
fi`, dir, id, id, p.WebPort, shq(run), id, env, shq(run))
	if out, code, err := uagent.Output(ctx, a, user, cmd); err != nil || code != 0 {
		if err == nil {
			err = apierr.Unavailable("Eklenti başlatılamadı: " + strings.TrimSpace(string(out)))
		}
		return p, err
	}
	return Get(ctx, a, user, id)
}

// Stop eklentiyi durdurur.
func Stop(ctx context.Context, a uagent.Agent, user, id string) (Plugin, error) {
	if !reID.MatchString(id) {
		return Plugin{}, apierr.Invalid("Geçersiz eklenti kimliği.")
	}
	cmd := fmt.Sprintf(`systemctl --user stop makeros-plugin-%s 2>/dev/null; p=$(cat ~/%s/.pid 2>/dev/null) && kill -TERM -- -"$p" 2>/dev/null || kill -TERM "$p" 2>/dev/null; rm -f ~/%s/.pid; true`, id, root(id), root(id))
	_, _, _ = uagent.Output(ctx, a, user, cmd)
	return Get(ctx, a, user, id)
}

// Remove eklentiyi durdurup siler.
func Remove(ctx context.Context, a uagent.Agent, user, id string) error {
	if _, err := Stop(ctx, a, user, id); err != nil {
		return err
	}
	if IsDefault(id) {
		_ = a.Write(ctx, user, removedMark(id), strings.NewReader("kullanıcı kaldırdı\n"))
	}
	_, err := a.FS(ctx, user, termproc.Open{Mode: "remove", Path: root(id)})
	return err
}

// Logs son günlük satırlarıdır (en fazla 64 KB).
func Logs(ctx context.Context, a uagent.Agent, user, id string) (string, error) {
	if !reID.MatchString(id) {
		return "", apierr.Invalid("Geçersiz eklenti kimliği.")
	}
	out, _, err := uagent.Output(ctx, a, user, "tail -c 65536 ~/"+root(id)+"/.log 2>/dev/null; true")
	return string(out), err
}

// Export eklentiyi paylaşılabilir zip olarak w'ye yazar (.venv, günlük, pid hariç).
func Export(ctx context.Context, a uagent.Agent, user, id string, w io.Writer) error {
	if _, err := Get(ctx, a, user, id); err != nil {
		return err
	}
	zw := zip.NewWriter(w)
	var walk func(rel string) error
	walk = func(rel string) error {
		raw, err := a.FS(ctx, user, termproc.Open{Mode: "list", Path: root(id) + rel})
		if err != nil {
			return err
		}
		var l termproc.Listing
		_ = json.Unmarshal(raw, &l)
		for _, e := range l.Entries {
			name := strings.TrimPrefix(rel+"/"+e.Name, "/")
			if e.Link || name == ".venv" || name == ".log" || name == ".pid" || e.Name == "__pycache__" {
				continue
			}
			if e.Dir {
				if err := walk(rel + "/" + e.Name); err != nil {
					return err
				}
				continue
			}
			fw, err := zw.Create(name)
			if err != nil {
				return err
			}
			if err := a.Read(ctx, user, root(id)+"/"+name, fw); err != nil {
				return err
			}
		}
		return nil
	}
	if err := walk(""); err != nil {
		return err
	}
	return zw.Close()
}

// --- varsayılan eklentiler -----------------------------------------------------

//go:embed all:defaults
var defaultsFS embed.FS

// removedMark kullanıcı varsayılan bir eklentiyi kaldırdığında yazılır; bir daha kurulmaz.
func removedMark(id string) string { return Dir + "/.kaldirildi-" + id }

// IsDefault eklentinin MakerOS ile birlikte geldiğini söyler.
func IsDefault(id string) bool {
	_, err := fs.Stat(defaultsFS, "defaults/"+id+"/plugin.json")
	return err == nil
}

// Seed MakerOS ile gelen eklentileri (ör. Konya HackerSpace) kullanıcıda yoksa kurar.
// Kullanıcı kaldırdıysa (işaret dosyası) dokunmaz.
func Seed(ctx context.Context, a uagent.Agent, user string) {
	des, _ := defaultsFS.ReadDir("defaults")
	for _, de := range des {
		id := de.Name()
		if !de.IsDir() || !reID.MatchString(id) {
			continue
		}
		if cur, err := Get(ctx, a, user, id); err == nil {
			// Kurulu: MakerOS ile gelen sürüm daha yeniyse dosyalar yenilenir (.venv ve günlük korunur).
			var emb Manifest
			if b, err := defaultsFS.ReadFile("defaults/" + id + "/plugin.json"); err != nil || json.Unmarshal(b, &emb) != nil || !newerVersion(emb.Version, cur.Version) {
				continue
			}
		}
		if _, err := a.FS(ctx, user, termproc.Open{Mode: "stat", Path: removedMark(id)}); err == nil {
			continue
		}
		base := "defaults/" + id
		_ = fs.WalkDir(defaultsFS, base, func(p string, d fs.DirEntry, err error) error {
			if err != nil || d.IsDir() {
				return err
			}
			b, err := defaultsFS.ReadFile(p)
			if err != nil {
				return err
			}
			return a.Write(ctx, user, root(id)+"/"+strings.TrimPrefix(p, base+"/"), bytes.NewReader(b))
		})
	}
}

// Asset eklentinin içindeki bir dosyayı (duvar kâğıdı, logo) w'ye yazar; süreç çalışmak zorunda değil.
func Asset(ctx context.Context, a uagent.Agent, user, id, rel string, w io.Writer) error {
	if !reID.MatchString(id) {
		return apierr.Invalid("Geçersiz eklenti kimliği.")
	}
	clean := path.Clean("/" + rel)[1:]
	if clean == "" || strings.HasPrefix(clean, ".") || strings.Contains(clean, "/.") {
		return apierr.NotFound("Dosya bulunamadı.")
	}
	return a.Read(ctx, user, root(id)+"/"+clean, w)
}

// newerVersion a > b mi (noktalı sayısal sürüm; "1.0.10" > "1.0.9").
func newerVersion(a, b string) bool {
	pa, pb := strings.Split(a, "."), strings.Split(b, ".")
	for i := 0; i < len(pa) || i < len(pb); i++ {
		var x, y int
		if i < len(pa) {
			x, _ = strconv.Atoi(pa[i])
		}
		if i < len(pb) {
			y, _ = strconv.Atoi(pb[i])
		}
		if x != y {
			return x > y
		}
	}
	return false
}
