package uagent

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

func shellCmd() (*exec.Cmd, error) {
	return &exec.Cmd{Path: "/bin/sh", Args: []string{"sh"}, Env: []string{"PATH=/usr/bin:/bin", "LANG=C.UTF-8"}}, nil
}

// Aynı süreçte gerçek makerd-user sunucusu: Remote ve Local aynı sonucu vermeli.
func agents(t *testing.T) map[string]Agent {
	t.Helper()
	home := t.TempDir()
	dir := t.TempDir()
	uid := os.Getuid()
	ln, err := net.Listen("unix", filepath.Join(dir, strconv.Itoa(uid)+".sock"))
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	srv := &termproc.Server{Log: slog.New(slog.NewTextHandler(io.Discard, nil)), Allowed: map[uint32]bool{uint32(uid): true}, Command: shellCmd, Home: home}
	go srv.Serve(ctx, ln)
	return map[string]Agent{
		"local":  Local{Command: shellCmd, Home: home},
		"remote": Remote{Dir: dir, UIDOf: func(string) (int, error) { return uid, nil }},
	}
}

func TestAgentOps(t *testing.T) {
	for name, a := range agents(t) {
		t.Run(name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
			defer cancel()
			out, code, err := Output(ctx, a, "u", "echo merhaba; exit 3")
			if err != nil || code != 3 || strings.TrimSpace(string(out)) != "merhaba" {
				t.Fatalf("exec: %q %d %v", out, code, err)
			}
			var b bytes.Buffer
			if code, err := a.Exec(ctx, "u", "tr a-z A-Z", strings.NewReader("stdin akisi"), &b); err != nil || code != 0 || b.String() != "STDIN AKISI" {
				t.Fatalf("stdin: %q %d %v", b.String(), code, err)
			}
			big := bytes.Repeat([]byte("0123456789"), 70000) // 700 KB: çerçevelere bölünür
			if err := a.Write(ctx, "u", name+"/dosya.bin", bytes.NewReader(big)); err != nil {
				t.Fatal(err)
			}
			var got bytes.Buffer
			if err := a.Read(ctx, "u", "~/"+name+"/dosya.bin", &got); err != nil || !bytes.Equal(got.Bytes(), big) {
				t.Fatalf("read: %d bayt %v", got.Len(), err)
			}
			raw, err := a.FS(ctx, "u", termproc.Open{Mode: "list", Path: name})
			if err != nil {
				t.Fatal(err)
			}
			var l termproc.Listing
			_ = json.Unmarshal(raw, &l)
			if len(l.Entries) != 1 || l.Entries[0].Name != "dosya.bin" || l.Entries[0].Size != int64(len(big)) {
				t.Fatalf("list: %+v", l)
			}
			if _, err := a.FS(ctx, "u", termproc.Open{Mode: "rename", Path: name + "/dosya.bin", To: name + "/yeni.bin"}); err != nil {
				t.Fatal(err)
			}
			if _, err := a.FS(ctx, "u", termproc.Open{Mode: "stat", Path: name + "/yok"}); err == nil {
				t.Fatal("olmayan dosya için hata beklenirdi")
			}
		})
	}
}
