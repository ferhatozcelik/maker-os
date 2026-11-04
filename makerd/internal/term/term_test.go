package term

import (
	"bytes"
	"io"
	"log/slog"
	"os/exec"
	"testing"
	"time"
)

func TestRing(t *testing.T) {
	r := NewRing(8)
	cases := []struct{ write, want string }{
		{"abc", "abc"}, {"defgh", "abcdefgh"}, {"ij", "cdefghij"}, {"0123456789", "23456789"}, {"x", "3456789x"},
	}
	for _, c := range cases {
		r.Write([]byte(c.write))
		if got := string(r.Bytes()); got != c.want {
			t.Fatalf("after %q: %q want %q", c.write, got, c.want)
		}
	}
}

var catSpawner = Local(func() (*exec.Cmd, error) { return exec.Command("/bin/cat"), nil })

func TestTickets(t *testing.T) {
	m := NewManager(slog.New(slog.NewTextHandler(io.Discard, nil)), catSpawner)
	defer m.CloseAll()
	s, err := m.Create("maker", 80, 24)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := m.Ticket(s.ID, "someone"); err == nil {
		t.Fatal("ticket for foreign user")
	}
	tk, _ := m.Ticket(s.ID, "maker")
	if _, ok := m.Redeem(tk, "wrong-id"); ok {
		t.Fatal("redeemed with wrong id")
	}
	if _, ok := m.Redeem(tk, s.ID); ok {
		t.Fatal("ticket reusable after failed attempt")
	}
	tk, _ = m.Ticket(s.ID, "maker")
	if got, ok := m.Redeem(tk, s.ID); !ok || got != s {
		t.Fatal("valid ticket rejected")
	}
	if _, ok := m.Redeem(tk, s.ID); ok {
		t.Fatal("ticket used twice")
	}
	now := time.Now()
	m.now = func() time.Time { return now }
	tk, _ = m.Ticket(s.ID, "maker")
	now = now.Add(TicketTTL + time.Second)
	if _, ok := m.Redeem(tk, s.ID); ok {
		t.Fatal("expired ticket accepted")
	}
}

func TestMaxSessionsAndExit(t *testing.T) {
	m := NewManager(slog.New(slog.NewTextHandler(io.Discard, nil)), catSpawner)
	m.Max = 2
	defer m.CloseAll()
	a, _ := m.Create("maker", 0, 0)
	if _, err := m.Create("maker", 0, 0); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Create("maker", 0, 0); err == nil {
		t.Fatal("limit not enforced")
	}
	// PTY yankısı halka tampona düşer.
	_, _ = a.proc.Write([]byte("merhaba\n"))
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		a.mu.Lock()
		got := a.ring.Bytes()
		a.mu.Unlock()
		if bytes.Contains(got, []byte("merhaba")) {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	m.Close(a.ID)
	select {
	case <-a.done:
	case <-time.After(5 * time.Second):
		t.Fatal("session did not exit")
	}
	for i := 0; i < 50 && m.Count() != 1; i++ {
		time.Sleep(20 * time.Millisecond)
	}
	if m.Count() != 1 {
		t.Fatalf("count %d", m.Count())
	}
}
