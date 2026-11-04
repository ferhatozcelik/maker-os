package pyman

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

func TestTemplatesAndCreate(t *testing.T) {
	ts := Templates()
	if len(ts) < 8 || ts[0].ID != "bos" {
		t.Fatalf("şablonlar: %d", len(ts))
	}
	for _, tp := range ts {
		if tp.Title.TR == "" || tp.Title.EN == "" || tp.Desc.TR == "" {
			t.Errorf("%s: başlık/açıklama eksik", tp.ID)
		}
		if _, err := tmplFS.ReadFile("templates/" + tp.ID + "/main.py"); err != nil {
			t.Errorf("%s: main.py yok", tp.ID)
		}
	}
	home := t.TempDir()
	a := uagent.Local{Command: func() (*exec.Cmd, error) { return exec.Command("/bin/sh"), nil }, Home: home}
	ctx := context.Background()
	cmd, err := Create(ctx, a, "u", "ilk-proje", "nesne-tespiti", "system")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(cmd, "uv venv --quiet --system-site-packages --python /usr/bin/python3") || !strings.Contains(cmd, "uv sync") {
		t.Fatalf("komut: %s", cmd)
	}
	main, _ := os.ReadFile(filepath.Join(home, "Projeler/ilk-proje/main.py"))
	if !strings.Contains(string(main), "ilk-proje") || strings.Contains(string(main), "{{name}}") {
		t.Fatal("şablon adı işlenmedi")
	}
	pp, _ := os.ReadFile(filepath.Join(home, "Projeler/ilk-proje/pyproject.toml"))
	if !strings.Contains(string(pp), `dependencies = ["ncnn", "opencv-python-headless"]`) {
		t.Fatalf("pyproject: %s", pp)
	}
	if _, err := Create(ctx, a, "u", "ilk-proje", "bos", ""); err == nil {
		t.Fatal("aynı ad ikinci kez oluşturuldu")
	}
	if _, err := Create(ctx, a, "u", "Kötü Ad", "bos", ""); err == nil {
		t.Fatal("geçersiz ad kabul edildi")
	}
	ps, err := Projects(ctx, a, "u")
	if err != nil || len(ps) != 1 || ps[0].Template != "nesne-tespiti" || ps[0].Python != "system" {
		t.Fatalf("projeler: %+v %v", ps, err)
	}
	run, _ := RunCmd(ctx, a, "u", "ilk-proje")
	if run != "cd ~/Projeler/ilk-proje && uv run main.py" {
		t.Fatalf("çalıştır: %s", run)
	}
	if err := Delete(ctx, a, "u", "ilk-proje"); err != nil {
		t.Fatal(err)
	}
	if _, err := PythonCmd("install", "3.12; rm -rf /"); err == nil {
		t.Fatal("komut enjeksiyonu kabul edildi")
	}
}
