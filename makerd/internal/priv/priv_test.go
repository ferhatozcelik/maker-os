package priv

import (
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestSHA512Crypt(t *testing.T) {
	cases := []struct{ pw, setting, want string }{
		{"Hello world!", "$6$saltstring", "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"},
		{"Hello world!", "$6$rounds=10000$saltstringsaltstring", "$6$rounds=10000$saltstringsaltst$OW1/O6BYHV6BcXZu8QVeXbDWra3Oeqh0sbHbbMCVNSnCM/UrjmM0Dp8vOuZeHBy/YTBmSK6H9qs/y3RnOaw5v."},
		{"This is just a test", "$6$rounds=5000$toolongsaltstring", "$6$rounds=5000$toolongsaltstrin$lQ8jolhgVRVhY4b5pZKaysCLi0QBxGoNeKQzQ3glMhwllF7oGDZxUhx1yxdYcz/e1JSbq3y6JMxxl8audkUEm0"},
	}
	for _, c := range cases {
		got, err := SHA512Crypt(c.pw, c.setting)
		if err != nil || got != c.want {
			t.Errorf("SHA512Crypt(%q,%q) = %q, %v; want %q", c.pw, c.setting, got, err, c.want)
		}
		if ok, _ := Verify(c.pw, c.want); !ok {
			t.Errorf("Verify failed for %q", c.want)
		}
		if ok, _ := Verify("wrong", c.want); ok {
			t.Errorf("Verify accepted wrong password")
		}
	}
	if _, err := Verify("x", "$y$j9T$abc$def"); err != ErrUnsupportedHash {
		t.Errorf("yescrypt: want ErrUnsupportedHash, got %v", err)
	}
	h, err := HashPassword("yeni-parola")
	if err != nil || !strings.HasPrefix(h, "$6$rounds=65536$") {
		t.Fatalf("HashPassword: %q %v", h, err)
	}
	if ok, _ := Verify("yeni-parola", h); !ok {
		t.Error("new hash does not verify")
	}
}

func writeShadow(t *testing.T) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "shadow")
	h, _ := SHA512Crypt("maker123", "$6$abcdefgh")
	data := "root:*:19000:0:99999:7:::\nmaker:" + h + ":19000:0:99999:7:::\nlocked:!" + h + ":19000::::::\n"
	if err := os.WriteFile(p, []byte(data), 0o600); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestShadow(t *testing.T) {
	p := writeShadow(t)
	cases := []struct {
		user, pw string
		ok       bool
	}{
		{"maker", "maker123", true}, {"maker", "nope", false}, {"root", "", false},
		{"locked", "maker123", false}, {"ghost", "x", false},
	}
	for _, c := range cases {
		if ok, err := CheckShadow(p, c.user, c.pw); ok != c.ok || err != nil {
			t.Errorf("CheckShadow(%s,%s) = %v,%v want %v", c.user, c.pw, ok, err, c.ok)
		}
	}
	h, _ := HashPassword("degisti")
	if err := SetShadowHash(p, "maker", h, time.Unix(86400*20000, 0)); err != nil {
		t.Fatal(err)
	}
	if ok, _ := CheckShadow(p, "maker", "degisti"); !ok {
		t.Error("password not updated")
	}
	data, _ := os.ReadFile(p)
	if !strings.Contains(string(data), ":20000:0:99999") || !strings.HasPrefix(string(data), "root:*:") {
		t.Errorf("shadow fields broken:\n%s", data)
	}
	if st, _ := os.Stat(p); st.Mode().Perm() != 0o600 {
		t.Errorf("mode changed: %v", st.Mode())
	}
}

func TestServerRoundTrip(t *testing.T) {
	dir := t.TempDir()
	sock := filepath.Join(dir, "priv.sock")
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatal(err)
	}
	called := ""
	srv := &Server{
		Log:     slog.New(slog.NewTextHandler(io.Discard, nil)),
		Allowed: map[uint32]bool{uint32(os.Getuid()): true},
		Handlers: map[string]Handler{
			CmdAuthCheck: func(_ context.Context, p Peer, raw json.RawMessage) (any, error) {
				var a AuthCheckArgs
				_ = json.Unmarshal(raw, &a)
				called = a.User
				return AuthCheckResult{OK: a.Password == "ok" && p.UID == uint32(os.Getuid())}, nil
			},
			CmdPower: func(context.Context, Peer, json.RawMessage) (any, error) {
				return nil, &Error{Code: "invalid", Message: "hayır"}
			},
		},
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { _ = srv.Serve(ctx, ln); close(done) }()
	c := NewClient(sock)
	var res AuthCheckResult
	if err := c.Call(ctx, CmdAuthCheck, AuthCheckArgs{User: "maker", Password: "ok"}, &res); err != nil || !res.OK || called != "maker" {
		t.Fatalf("auth.check: %v %+v %q", err, res, called)
	}
	err = c.Call(ctx, CmdPower, PowerArgs{Action: "x"}, nil)
	if pe, ok := err.(*Error); !ok || pe.Code != "invalid" {
		t.Fatalf("power: want invalid error, got %v", err)
	}
	if err := c.Call(ctx, "rm -rf", nil, nil); err == nil || !strings.Contains(err.Error(), "Bilinmeyen") {
		t.Fatalf("unknown cmd: %v", err)
	}
	// İzinsiz UID.
	srv.Allowed = map[uint32]bool{99999: true}
	if err := c.Call(ctx, CmdAuthCheck, AuthCheckArgs{}, nil); err == nil || !strings.Contains(err.Error(), "forbidden") {
		t.Fatalf("forbidden: %v", err)
	}
	cancel()
	<-done
}

func TestValidHostname(t *testing.T) {
	for name, want := range map[string]bool{"makeros": true, "pi-4": true, "a": true, "-x": false, "X": false, "a_b": false, "": false, strings.Repeat("a", 64): false} {
		if ValidHostname(name) != want {
			t.Errorf("ValidHostname(%q) != %v", name, want)
		}
	}
}
