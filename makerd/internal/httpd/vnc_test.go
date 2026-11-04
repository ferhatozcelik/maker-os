package httpd

import (
	"bytes"
	"context"
	"io"
	"log/slog"
	"net"
	"net/http"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/ferhatozcelik/maker-os/makerd/internal/browser"
	"github.com/ferhatozcelik/maker-os/makerd/internal/screen"
)

// Sahte RFB sunucusu: sürüm el sıkışmasını gönderir, sonra gelen baytları yankılar.
func fakeRFB(t *testing.T) string {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { l.Close() })
	go func() {
		for {
			c, err := l.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				_, _ = c.Write([]byte("RFB 003.008\n"))
				_, _ = io.Copy(c, c)
			}(c)
		}
	}()
	return l.Addr().String()
}

func vncEnv(t *testing.T, addr string) *env {
	return newEnvWith(t, false, func(d *Deps) { d.VNCAddr = addr })
}

func TestVNCBridge(t *testing.T) {
	e := vncEnv(t, fakeRFB(t))
	cookie := e.login()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	c, resp, err := websocket.Dial(ctx, wsURL(e, "/api/v1/vnc"), &websocket.DialOptions{
		HTTPHeader:   http.Header{"Cookie": {cookie}, "Origin": {e.srv.URL}},
		Subprotocols: []string{"binary"},
	})
	if err != nil {
		t.Fatalf("dial: %v (%v)", err, resp)
	}
	defer c.CloseNow()
	if c.Subprotocol() != "binary" {
		t.Fatalf("alt protokol = %q", c.Subprotocol())
	}
	_, b, err := c.Read(ctx)
	if err != nil || string(b) != "RFB 003.008\n" {
		t.Fatalf("el sıkışma: %q %v", b, err)
	}
	msg := []byte{0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x03, 0x00}
	if err := c.Write(ctx, websocket.MessageBinary, msg); err != nil {
		t.Fatal(err)
	}
	_, b, err = c.Read(ctx)
	if err != nil || !bytes.Equal(b, msg) {
		t.Fatalf("yankı: %v %v", b, err)
	}
}

func TestVNCGuards(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	e := vncEnv(t, fakeRFB(t))
	// Girişsiz: 401.
	if _, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Origin": {e.srv.URL}}}); err == nil || r.StatusCode != 401 {
		t.Fatal("girişsiz VNC kabul edildi")
	}
	cookie := e.login()
	// Origin yok: 403.
	if _, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {cookie}}}); err == nil || r.StatusCode != 403 {
		t.Fatal("Origin'siz VNC kabul edildi")
	}
	// Sunucu kapalı: 503.
	e2 := vncEnv(t, "127.0.0.1:1")
	ck2 := e2.login()
	if _, r, err := websocket.Dial(ctx, wsURL(e2, "/api/v1/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {ck2}, "Origin": {e2.srv.URL}}}); err == nil || r.StatusCode != 503 {
		t.Fatal("VNC sunucusu yokken 503 beklenirdi")
	}
	// Özellik kapalı: 503.
	e3 := vncEnv(t, "")
	ck3 := e3.login()
	if _, r, err := websocket.Dial(ctx, wsURL(e3, "/api/v1/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {ck3}, "Origin": {e3.srv.URL}}}); err == nil || r.StatusCode != 503 {
		t.Fatal("kapalıyken 503 beklenirdi")
	}
}

// Ekran Modu varsayılan kapalı: /vnc 403; yönetici açınca bağlanır; GET /screen durumu verir.
func TestScreenModeToggle(t *testing.T) {
	addr := fakeRFB(t)
	// Yönetilmeyen adres ("desktop:"): test wayvnc başlatmaya çalışmaz; yalnız işaret ve köprü kapısı denenir.
	mgr := &screen.Manager{Path: filepath.Join(t.TempDir(), "screen-mode"), Addr: "desktop:5900", Log: slog.New(slog.NewTextHandler(io.Discard, nil)), DRMRoot: t.TempDir()}
	e := newEnvWith(t, false, func(d *Deps) { d.VNCAddr = addr; d.Screen = mgr })
	cookie := e.login()
	hdr := map[string]string{"Cookie": cookie, "X-Maker": "1"}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	dial := func() int {
		c, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {cookie}, "Origin": {e.srv.URL}}, Subprotocols: []string{"binary"}})
		if err == nil {
			c.CloseNow()
			return 101
		}
		return r.StatusCode
	}
	if r := e.do("GET", "/api/v1/screen", "", hdr); r.StatusCode != 200 || !strings.Contains(readAll(r), `"enabled":false`) {
		t.Fatal("varsayılan kapalı olmalı")
	}
	if code := dial(); code != 403 {
		t.Fatalf("kapalıyken /vnc 403 beklenirdi, %d", code)
	}
	if r := e.do("PUT", "/api/v1/screen", `{"enabled":true}`, hdr); r.StatusCode != 200 || !strings.Contains(readAll(r), `"enabled":true`) {
		t.Fatal("açılamadı")
	}
	if code := dial(); code != 101 {
		t.Fatalf("açıkken bağlanmalıydı, %d", code)
	}
	if r := e.do("PUT", "/api/v1/screen", `{"enabled":false}`, hdr); r.StatusCode != 200 {
		t.Fatal("kapatılamadı")
	}
	if code := dial(); code != 403 {
		t.Fatalf("yeniden kapalıyken 403 beklenirdi, %d", code)
	}
}

func readAll(r *http.Response) string { b, _ := io.ReadAll(r.Body); return string(b) }

// Uzak tarayıcı: oturum (sahte RFB) dinliyorsa yeniden başlatılmadan bağlanır; Ekran Modu kapalıyken de çalışır.
func TestBrowserBridge(t *testing.T) {
	addr := fakeRFB(t)
	mgr := &screen.Manager{Path: filepath.Join(t.TempDir(), "screen-mode"), Addr: "desktop:5900", Log: slog.New(slog.NewTextHandler(io.Discard, nil))}
	br := &browser.Manager{Addr: addr, Script: "/yok", Grace: time.Millisecond, Log: slog.New(slog.NewTextHandler(io.Discard, nil))}
	e := newEnvWith(t, false, func(d *Deps) { d.VNCAddr = addr; d.Screen = mgr; d.Browser = br })
	cookie := e.login()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	c, _, err := websocket.Dial(ctx, wsURL(e, "/api/v1/browser/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {cookie}, "Origin": {e.srv.URL}}, Subprotocols: []string{"binary"}})
	if err != nil {
		t.Fatalf("tarayıcı köprüsü: %v", err)
	}
	_, msg, err := c.Read(ctx)
	if err != nil || !bytes.HasPrefix(msg, []byte("RFB ")) {
		t.Fatalf("RFB el sıkışması gelmedi: %q %v", msg, err)
	}
	c.CloseNow()
	// Oturum yok ve betik yoksa: 503 (çökme yok).
	e2 := newEnvWith(t, false, func(d *Deps) {
		d.Browser = &browser.Manager{Addr: "127.0.0.1:1", Script: "/yok", Log: slog.New(slog.NewTextHandler(io.Discard, nil))}
	})
	ck2 := e2.login()
	if _, r, err := websocket.Dial(ctx, wsURL(e2, "/api/v1/browser/vnc"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {ck2}, "Origin": {e2.srv.URL}}}); err == nil || r.StatusCode != 503 {
		t.Fatal("oturum başlatılamıyorken 503 beklenirdi")
	}
}
