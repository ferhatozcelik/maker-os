package auth

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestSessions(t *testing.T) {
	m := NewManager()
	now := time.Now()
	m.now = func() time.Time { return now }
	s, cookie := m.Create("maker", true)
	other, _ := NewManager().Create("maker", true)
	cases := []struct {
		name, value string
		ok          bool
	}{
		{"valid", cookie, true},
		{"empty", "", false},
		{"no-sig", s.ID, false},
		{"bad-sig", s.ID + ".AAAA", false},
		{"foreign-key", other.ID + "." + m.sign(other.ID)[:10], false},
		{"unknown-id-valid-sig", "abc." + m.sign("abc"), false},
	}
	for _, c := range cases {
		if _, ok := m.Lookup(c.value); ok != c.ok {
			t.Errorf("%s: Lookup = %v, want %v", c.name, ok, c.ok)
		}
	}
	now = now.Add(SessionTTL + time.Second)
	if _, ok := m.Lookup(cookie); ok {
		t.Error("expired session accepted")
	}
	now = time.Now()
	_, c2 := m.Create("maker", false)
	m.Delete(c2)
	if _, ok := m.Lookup(c2); ok {
		t.Error("deleted session accepted")
	}
}

func TestLimiter(t *testing.T) {
	l := NewLimiter(5, time.Minute)
	now := time.Now()
	l.now = func() time.Time { return now }
	for i := 0; i < 5; i++ {
		if ok, _ := l.Allow("1.2.3.4"); !ok {
			t.Fatalf("attempt %d blocked", i)
		}
		l.Fail("1.2.3.4")
	}
	if ok, wait := l.Allow("1.2.3.4"); ok || wait <= 0 {
		t.Fatal("6th attempt allowed")
	}
	if ok, _ := l.Allow("5.6.7.8"); !ok {
		t.Fatal("other IP blocked")
	}
	now = now.Add(time.Minute)
	if ok, _ := l.Allow("1.2.3.4"); !ok {
		t.Fatal("not unblocked after window")
	}
	l.Fail("9.9.9.9")
	l.Reset("9.9.9.9")
	if len(l.fails["9.9.9.9"]) != 0 {
		t.Fatal("reset failed")
	}
}

func TestTokenAndAdmin(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "run", "token")
	tok, err := WriteToken(p)
	if err != nil || len(tok) < 30 {
		t.Fatal(tok, err)
	}
	b, _ := os.ReadFile(p)
	if string(b) != tok {
		t.Fatal("token mismatch")
	}
	if st, _ := os.Stat(p); st.Mode().Perm() != 0o640 {
		t.Fatalf("mode %v", st.Mode())
	}
	g := filepath.Join(dir, "group")
	_ = os.WriteFile(g, []byte("root:x:0:\nwheel:x:10:maker,ali\nvideo:x:44:veli\n"), 0o644)
	for u, want := range map[string]bool{"maker": true, "ali": true, "veli": false} {
		if IsAdmin(g, u) != want {
			t.Errorf("IsAdmin(%s) != %v", u, want)
		}
	}
}
