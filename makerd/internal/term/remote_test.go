package term

import (
	"bytes"
	"context"
	"errors"
	"io"
	"log/slog"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"testing"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

func startUserServer(t *testing.T, dir string, allowed map[uint32]bool) {
	t.Helper()
	ln, err := net.Listen("unix", filepath.Join(dir, strconv.Itoa(os.Getuid())+".sock"))
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	srv := &termproc.Server{Log: slog.New(slog.NewTextHandler(io.Discard, nil)), Allowed: allowed,
		Command: func() (*exec.Cmd, error) { return exec.Command("/bin/sh"), nil }}
	done := make(chan struct{})
	go func() { _ = srv.Serve(ctx, ln); close(done) }()
	t.Cleanup(func() { cancel(); <-done })
}

func readUntil(t *testing.T, s *Session, want string) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		s.mu.Lock()
		got := s.ring.Bytes()
		s.mu.Unlock()
		if bytes.Contains(got, []byte(want)) {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	t.Fatalf("%q not seen; ring %q", want, s.ring.Bytes())
}

func TestRemoteSpawner(t *testing.T) {
	dir := t.TempDir()
	startUserServer(t, dir, map[uint32]bool{uint32(os.Getuid()): true})
	self := func(string) (int, error) { return os.Getuid(), nil }
	m := NewManager(slog.New(slog.NewTextHandler(io.Discard, nil)), Remote(dir, self, "", nil))
	defer m.CloseAll()
	s, err := m.Create("ali", 90, 20)
	if err != nil {
		t.Fatal(err)
	}
	if s.proc.Pid() <= 0 {
		t.Fatal("no pid")
	}
	_ = s.proc.Resize(100, 33)
	_, _ = s.proc.Write([]byte("stty size; echo UZAK_$((40+2))\n"))
	readUntil(t, s, "UZAK_42")
	readUntil(t, s, "33 100")
	// Kabuk çıkınca oturum kapanır.
	_, _ = s.proc.Write([]byte("exit\n"))
	select {
	case <-s.done:
	case <-time.After(5 * time.Second):
		t.Fatal("remote session did not end")
	}
	// Hangup çalışan uzun süreci kapatır.
	s2, err := m.Create("ali", 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	_, _ = s2.proc.Write([]byte("sleep 60\n"))
	time.Sleep(100 * time.Millisecond)
	m.Close(s2.ID)
	select {
	case <-s2.done:
	case <-time.After(8 * time.Second):
		t.Fatal("hangup did not end remote session")
	}
}

func TestRemoteSpawnerFallbackAndPeer(t *testing.T) {
	dir := t.TempDir()
	uid := func(string) (int, error) { return os.Getuid(), nil }
	used := false
	fb := func(user string, c, r uint16, cmd string) (termproc.Proc, error) {
		used = true
		return Local(func() (*exec.Cmd, error) { return exec.Command("/bin/cat"), nil })(user, c, r, cmd)
	}
	sp := Remote(dir, uid, "maker", fb)
	// Soket yok: yalnız makerd'nin kendi kullanıcısı yerel kabuğa düşer.
	if _, err := sp("ali", 80, 24, ""); err == nil {
		t.Fatal("foreign user fell back to local shell")
	} else if e, ok := apierr.As(err); !ok || e.Status != 503 {
		t.Fatalf("want 503, got %v", err)
	}
	p, err := sp("maker", 80, 24, "")
	if err != nil || !used {
		t.Fatalf("fallback: %v", err)
	}
	p.Hangup()
	_ = p.Wait()
	p.Close()

	// İzinsiz çağıran 'e' çerçevesi alır.
	startUserServer(t, dir, map[uint32]bool{})
	if _, err := sp("ali", 80, 24, ""); err == nil {
		t.Fatal("disallowed caller accepted")
	}
	// Karşı uç beklenen UID değilse bağlanılmaz.
	ctx := context.Background()
	if _, err := termproc.Dial(ctx, filepath.Join(dir, strconv.Itoa(os.Getuid())+".sock"), os.Getuid()+1, 80, 24); !errors.Is(err, termproc.ErrPeer) {
		t.Fatalf("peer check: %v", err)
	}
}
