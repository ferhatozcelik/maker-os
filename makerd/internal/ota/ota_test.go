package ota

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestNewer(t *testing.T) {
	for _, c := range []struct {
		a, b string
		want bool
	}{
		{"2.0.1", "2.0.0", true}, {"2.0.0", "2.0.1", false}, {"2.0.0", "2.0.0", false},
		{"2.0.0", "2.0.0-dev", true}, {"2.0.0-dev", "2.0.0", false}, {"2.1.0", "2.0.9", true},
		{"10.0.0", "9.9.9", true}, {"v2.0.1", "2.0.0", true}, {"garip", "2.0.0", false}, {"2.0.0", "garip", true},
		{"1.9.9", "2.0.0-dev", false},
	} {
		if got := Newer(c.a, c.b); got != c.want {
			t.Errorf("Newer(%q, %q) = %v, beklenen %v", c.a, c.b, got, c.want)
		}
	}
}

type fakeInst struct {
	mu        sync.Mutex
	installed string
	rebooted  bool
	polls     int
	fail      string
}

func (f *fakeInst) Install(_ context.Context, p string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.installed = p
	return nil
}
func (f *fakeInst) State(context.Context) (InstallState, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.polls++
	if f.polls < 2 {
		return InstallState{Operation: "installing", Progress: 50}, nil
	}
	return InstallState{Operation: "idle", Progress: 100, LastError: f.fail}, nil
}
func (f *fakeInst) Reboot(context.Context) error {
	f.mu.Lock()
	f.rebooted = true
	f.mu.Unlock()
	return nil
}

func server(t *testing.T, bundle []byte, sum string) *httptest.Server {
	var srv *httptest.Server
	srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/repos/o/r/releases":
			// RAUC paketi olmayan sürüm önerilmemeli; taslak atlanmalı; en yeni paketli sürüm 2.0.1.
			fmt.Fprintf(w, `[
			 {"tag_name":"v2.0.2","draft":true,"assets":[{"name":"makeros-2.0.2-rpi4.raucb","browser_download_url":"%[1]s/b"},{"name":"makeros-2.0.2-rpi4.raucb.sha256","browser_download_url":"%[1]s/s"}]},
			 {"tag_name":"v2.0.1","name":"MakerOS 2.0.1","body":"notlar","assets":[{"name":"makeros-2.0.1-rpi4.raucb","size":%[2]d,"browser_download_url":"%[1]s/b"},{"name":"makeros-2.0.1-rpi4.raucb.sha256","browser_download_url":"%[1]s/s"}]},
			 {"tag_name":"v3.0.0","assets":[{"name":"maker-os-3.0.0.img.xz","browser_download_url":"%[1]s/x"}]},
			 {"tag_name":"v1.9.9","assets":[{"name":"eski-ota.tar.xz","browser_download_url":"%[1]s/x"}]}]`, srv.URL, len(bundle))
		case "/b":
			_, _ = w.Write(bundle)
		case "/s":
			fmt.Fprintf(w, "%s  makeros-2.0.1-rpi4.raucb\n", sum)
		default:
			http.NotFound(w, r)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestCheckAndApply(t *testing.T) {
	bundle := []byte(strings.Repeat("imzalı-paket", 1000))
	h := sha256.Sum256(bundle)
	srv := server(t, bundle, hex.EncodeToString(h[:]))
	inst := &fakeInst{}
	dir := t.TempDir()
	u := &Updater{Repo: "o/r", Current: "2.0.0", Dir: dir, API: srv.URL, Installer: inst, Log: slog.New(slog.NewTextHandler(io.Discard, nil)), RebootDelay: time.Millisecond}
	st, err := u.Check(context.Background())
	if err != nil || st.Available == nil || st.Available.Version != "2.0.1" || st.Available.Notes != "notlar" {
		t.Fatalf("denetim: %+v %v", st.Available, err)
	}
	var last int
	if err := u.Apply(context.Background(), "j1", func(p int, _ string) { last = p }); err != nil {
		t.Fatalf("kurulum: %v", err)
	}
	if !strings.HasSuffix(inst.installed, "makeros-2.0.1-rpi4.raucb") || !inst.rebooted || last != 100 {
		t.Fatalf("kurulum/yeniden başlatma: %+v son=%d", inst, last)
	}
	if left, _ := filepath.Glob(filepath.Join(dir, "*")); len(left) != 0 {
		t.Fatalf("indirilen paket silinmeli: %v", left)
	}
	// Aynı sürümdeyken önerilmez.
	u2 := &Updater{Repo: "o/r", Current: "2.0.1", Dir: dir, API: srv.URL, Log: u.Log}
	if st, _ := u2.Check(context.Background()); st.Available != nil {
		t.Fatal("güncelken sürüm önerilmemeli")
	}
}

func TestApplyRejectsBadSum(t *testing.T) {
	srv := server(t, []byte("paket"), strings.Repeat("0", 64))
	inst := &fakeInst{}
	u := &Updater{Repo: "o/r", Current: "2.0.0", Dir: t.TempDir(), API: srv.URL, Installer: inst, Log: slog.New(slog.NewTextHandler(io.Discard, nil))}
	if _, err := u.Check(context.Background()); err != nil {
		t.Fatal(err)
	}
	err := u.Apply(context.Background(), "j", func(int, string) {})
	if err == nil || !strings.Contains(err.Error(), "özeti") || inst.installed != "" {
		t.Fatalf("bozuk paket kurulmamalı: %v %+v", err, inst)
	}
	if st := u.Status(); st.Phase != "error" {
		t.Fatalf("durum hata olmalı: %+v", st)
	}
}

func TestApplyRAUCError(t *testing.T) {
	bundle := []byte("paket")
	h := sha256.Sum256(bundle)
	srv := server(t, bundle, hex.EncodeToString(h[:]))
	inst := &fakeInst{fail: "signature verification failed"}
	u := &Updater{Repo: "o/r", Current: "2.0.0", Dir: t.TempDir(), API: srv.URL, Installer: inst, Log: slog.New(slog.NewTextHandler(io.Discard, nil))}
	_, _ = u.Check(context.Background())
	if err := u.Apply(context.Background(), "j", func(int, string) {}); err == nil || !strings.Contains(err.Error(), "signature") || inst.rebooted {
		t.Fatalf("RAUC hatasında yeniden başlatılmamalı: %v", err)
	}
	_ = os.RemoveAll(u.Dir)
}
