package apps

import (
	"context"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestSplitExec(t *testing.T) {
	cases := map[string][]string{
		"foot":                     {"foot"},
		"thonny %F":                {"thonny"},
		`sh -c "echo a b" %u`:      {"sh", "-c", "echo a b"},
		`app --pct 100%% \"q\"`:    {"app", "--pct", "100%", `"q"`},
		`"/opt/My App/bin" --x ""`: {"/opt/My App/bin", "--x", ""},
	}
	for in, want := range cases {
		if got := SplitExec(in); !reflect.DeepEqual(got, want) {
			t.Errorf("SplitExec(%q) = %q want %q", in, got, want)
		}
	}
}

func TestRegistry(t *testing.T) {
	dir := t.TempDir()
	write := func(name, body string) { _ = os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644) }
	write("foot.desktop", "[Desktop Entry]\nType=Application\nName=Foot\nName[tr]=Foot Uçbirim\nExec=foot\nIcon=foot\nCategories=System;TerminalEmulator;\n")
	write("hidden.desktop", "[Desktop Entry]\nType=Application\nName=H\nExec=h\nNoDisplay=true\n")
	write("link.desktop", "[Desktop Entry]\nType=Link\nName=L\nURL=x\n")
	var launched []string
	r := &Registry{Log: slog.New(slog.NewTextHandler(io.Discard, nil)), DesktopDirs: []string{dir},
		Launch: func(_ context.Context, a App, argv []string) error { launched = argv; return nil }}
	list := r.List()
	if len(list) != len(Builtin())+1 {
		t.Fatalf("got %d apps", len(list))
	}
	foot := list[len(list)-1]
	if foot.ID != "foot" || foot.Name != "Foot Uçbirim" || foot.Kind != "native" || foot.Icon != "/shell/icons/app.svg" || len(foot.Categories) != 2 {
		t.Fatalf("foot = %+v", foot)
	}
	for _, a := range Builtin() {
		if !strings.HasPrefix(a.ID, "makeros.") || a.URL != "/shell/apps/"+strings.TrimPrefix(a.ID, "makeros.")+"/" {
			t.Errorf("bad builtin %+v", a)
		}
	}
	if err := r.Start(context.Background(), "foot"); err != nil || !reflect.DeepEqual(launched, []string{"foot"}) {
		t.Fatal(err, launched)
	}
	if err := r.Start(context.Background(), "makeros.settings"); err == nil || !strings.HasPrefix(err.Error(), "invalid") {
		t.Fatal("web launch should be invalid", err)
	}
	if err := r.Start(context.Background(), "nope"); err == nil || !strings.HasPrefix(err.Error(), "not_found") {
		t.Fatal(err)
	}
}
