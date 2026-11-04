package httpd

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apps"
	"github.com/ferhatozcelik/maker-os/makerd/internal/auth"
	"github.com/ferhatozcelik/maker-os/makerd/internal/events"
	"github.com/ferhatozcelik/maker-os/makerd/internal/jobs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/mock"
	"github.com/ferhatozcelik/maker-os/makerd/internal/notify"
	"github.com/ferhatozcelik/maker-os/makerd/internal/prefs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/setup"
	"github.com/ferhatozcelik/maker-os/makerd/internal/term"
	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

type env struct {
	t   *testing.T
	srv *httptest.Server
	web string
	d   Deps
}

func newEnv(t *testing.T) *env { t.Helper(); return newEnvOpts(t, false) }

// newEnvOpts: fresh=true ilk açılış durumunda (kullanıcı yok, setup-done yok) başlatır.
func newEnvOpts(t *testing.T, fresh bool) *env { t.Helper(); return newEnvWith(t, fresh, nil) }

// newEnvWith: mod, Deps sunucu kurulmadan önce değiştirmek için.
func newEnvWith(t *testing.T, fresh bool, mod func(*Deps)) *env {
	t.Helper()
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	web := t.TempDir()
	hub := events.NewHub(ctx, log, Topics...)
	st := mock.NewState()
	store := &setup.Store{Dir: t.TempDir()}
	if fresh {
		st = mock.NewFreshState()
	} else {
		_ = store.MarkDone("maker")
	}
	n := mock.NewNetwork()
	n.Step = 5 * time.Millisecond
	d := Deps{
		Log: log, WebDir: web, Sessions: auth.NewManager(), Checker: mock.Checker{S: st},
		Limiter: auth.NewLimiter(5, time.Minute), LocalUser: "maker", LocalToken: "yerel-belirtec",
		Hub: hub, Jobs: jobs.New(ctx, log, hub.Publish), Notify: notify.New(hub.Publish),
		System: mock.NewSystem(st, "test", log), Network: n, Activity: mock.NewActivity(),
		Apps:  &apps.Registry{Log: log, Extra: mock.NativeApps(), Launch: mock.Launcher(log, nil)},
		Term:  term.NewManager(log, term.Local(func() (*exec.Cmd, error) { return exec.Command("/bin/sh"), nil })),
		Setup: store, Accounts: st, Prefs: &prefs.Store{Dir: filepath.Join(t.TempDir(), "prefs")},
	}
	d.Agent = uagent.Local{Command: func() (*exec.Cmd, error) { return exec.Command("/bin/sh"), nil }, Home: t.TempDir()}
	t.Cleanup(d.Term.CloseAll)
	if mod != nil {
		mod(&d)
	}
	s := New(ctx, d)
	hs := httptest.NewServer(s)
	t.Cleanup(hs.Close)
	return &env{t: t, srv: hs, web: web, d: d}
}

func (e *env) do(method, path, body string, hdr map[string]string) *http.Response {
	e.t.Helper()
	req, _ := http.NewRequest(method, e.srv.URL+path, strings.NewReader(body))
	req.Header.Set("Accept-Encoding", "identity") // Transport'un otomatik gzip'i karışmasın
	for k, v := range hdr {
		req.Header.Set(k, v)
	}
	resp, err := http.DefaultTransport.RoundTrip(req)
	if err != nil {
		e.t.Fatal(err)
	}
	e.t.Cleanup(func() { resp.Body.Close() })
	return resp
}

func (e *env) login() string {
	e.t.Helper()
	r := e.do("POST", "/api/v1/auth/login", `{"user":"maker","password":"maker"}`, map[string]string{"X-Maker": "1"})
	if r.StatusCode != 200 {
		e.t.Fatalf("login %d", r.StatusCode)
	}
	for _, c := range r.Cookies() {
		if c.Name == auth.CookieName {
			if !c.HttpOnly || c.SameSite != http.SameSiteStrictMode || c.Path != "/" || c.MaxAge != 7*24*3600 {
				e.t.Fatalf("cookie flags: %+v", c)
			}
			return c.Name + "=" + c.Value
		}
	}
	e.t.Fatal("no cookie")
	return ""
}

func errCode(t *testing.T, r *http.Response) string {
	t.Helper()
	var b errBody
	_ = json.NewDecoder(r.Body).Decode(&b)
	return b.Error.Code
}

func TestAuthAndCSRF(t *testing.T) {
	e := newEnv(t)
	cookie := e.login()
	authd := map[string]string{"Cookie": cookie, "X-Maker": "1"}
	noCSRF := map[string]string{"Cookie": cookie}
	cases := []struct {
		name, method, path, body string
		hdr                      map[string]string
		status                   int
		code                     string
	}{
		{"session no cookie", "GET", "/api/v1/session", "", nil, 401, "unauthorized"},
		{"session bad cookie", "GET", "/api/v1/session", "", map[string]string{"Cookie": "mk_session=x.y"}, 401, "unauthorized"},
		{"session ok", "GET", "/api/v1/session", "", noCSRF, 200, ""},
		{"system ok", "GET", "/api/v1/system", "", noCSRF, 200, ""},
		{"login without X-Maker", "POST", "/api/v1/auth/login", `{"user":"maker","password":"maker"}`, nil, 403, "forbidden"},
		{"login wrong X-Maker", "POST", "/api/v1/auth/login", `{"user":"maker","password":"maker"}`, map[string]string{"X-Maker": "0"}, 403, "forbidden"},
		{"put without X-Maker", "PUT", "/api/v1/system/hostname", `{"hostname":"pi"}`, noCSRF, 403, "forbidden"},
		{"put unauth", "PUT", "/api/v1/system/hostname", `{"hostname":"pi"}`, map[string]string{"X-Maker": "1"}, 401, "unauthorized"},
		{"hostname invalid", "PUT", "/api/v1/system/hostname", `{"hostname":"Bad_Name"}`, authd, 400, "invalid"},
		{"hostname ok", "PUT", "/api/v1/system/hostname", `{"hostname":"atolye-pi"}`, authd, 204, ""},
		{"bad json", "PUT", "/api/v1/system/locale", `{`, authd, 400, "invalid"},
		{"locale ok", "PUT", "/api/v1/system/locale", `{"lang":"en_US.UTF-8"}`, authd, 200, ""},
		{"password wrong old", "PUT", "/api/v1/system/password", `{"old":"x","new":"yenisifre"}`, authd, 403, "forbidden"},
		{"power invalid", "POST", "/api/v1/system/power", `{"action":"explode"}`, authd, 400, "invalid"},
		{"power ok", "POST", "/api/v1/system/power", `{"action":"reboot"}`, authd, 202, ""},
		{"unknown api", "GET", "/api/v1/nope", "", noCSRF, 404, "not_found"},
		{"app launch web", "POST", "/api/v1/apps/makeros.settings/launch", "", authd, 400, "invalid"},
		{"app launch native", "POST", "/api/v1/apps/chromium/launch", "", authd, 202, ""},
		{"signal foreign", "POST", "/api/v1/processes/1/signal", `{"signal":"TERM"}`, authd, 403, "forbidden"},
		{"signal own", "POST", "/api/v1/processes/1240/signal", `{"signal":"KILL"}`, authd, 204, ""},
		{"notification missing", "POST", "/api/v1/notifications/zz/read", "", authd, 404, "not_found"},
		{"notifications clear", "DELETE", "/api/v1/notifications", "", authd, 204, ""},
		{"files run non-py", "POST", "/api/v1/files/run", `{"path":"a.sh"}`, authd, 400, ""},
		{"forget unknown", "POST", "/api/v1/net/wifi/forget", `{"ssid":"Yok"}`, authd, 404, "not_found"},
		{"forget known", "POST", "/api/v1/net/wifi/forget", `{"ssid":"Ev-5G"}`, authd, 204, ""},
		{"logout no csrf", "POST", "/api/v1/auth/logout", "", noCSRF, 403, "forbidden"},
	}
	for _, c := range cases {
		r := e.do(c.method, c.path, c.body, c.hdr)
		if r.StatusCode != c.status {
			t.Errorf("%s: status %d want %d", c.name, r.StatusCode, c.status)
			continue
		}
		if c.code != "" {
			if got := errCode(t, r); got != c.code {
				t.Errorf("%s: code %q want %q", c.name, got, c.code)
			}
		}
	}
	// session gövdesi
	r := e.do("GET", "/api/v1/session", "", noCSRF)
	var sess map[string]any
	_ = json.NewDecoder(r.Body).Decode(&sess)
	if sess["user"] != "maker" || sess["admin"] != true || sess["setup_done"] != true {
		t.Errorf("session body %v", sess)
	}
	// logout geçersiz kılar
	if r := e.do("POST", "/api/v1/auth/logout", "", authd); r.StatusCode != 204 {
		t.Fatal(r.StatusCode)
	}
	if r := e.do("GET", "/api/v1/session", "", noCSRF); r.StatusCode != 401 {
		t.Fatal("session alive after logout")
	}
}

func TestLoginRateLimit(t *testing.T) {
	e := newEnv(t)
	h := map[string]string{"X-Maker": "1"}
	for i := 0; i < 5; i++ {
		if r := e.do("POST", "/api/v1/auth/login", `{"user":"maker","password":"bad"}`, h); r.StatusCode != 401 {
			t.Fatalf("attempt %d: %d", i, r.StatusCode)
		}
	}
	r := e.do("POST", "/api/v1/auth/login", `{"user":"maker","password":"maker"}`, h)
	if r.StatusCode != 429 || r.Header.Get("Retry-After") == "" || errCode(t, r) != "rate_limited" {
		t.Fatalf("want 429, got %d", r.StatusCode)
	}
}

func TestLocalHandshake(t *testing.T) {
	e := newEnv(t)
	if r := e.do("GET", "/api/v1/auth/local?token=yanlis", "", nil); r.StatusCode != 403 {
		t.Fatal(r.StatusCode)
	}
	r := e.do("GET", "/api/v1/auth/local?token=yerel-belirtec", "", nil)
	if r.StatusCode != 302 || r.Header.Get("Location") != "/?ui=desktop&local=1" || len(r.Cookies()) == 0 {
		t.Fatalf("local: %d %v", r.StatusCode, r.Header)
	}
	c := r.Cookies()[0]
	if r := e.do("GET", "/api/v1/session", "", map[string]string{"Cookie": c.Name + "=" + c.Value}); r.StatusCode != 200 {
		t.Fatal("local session invalid")
	}
}

func TestStaticCaching(t *testing.T) {
	e := newEnv(t)
	write := func(rel, body string) {
		p := filepath.Join(e.web, rel)
		_ = os.MkdirAll(filepath.Dir(p), 0o755)
		_ = os.WriteFile(p, []byte(body), 0o644)
	}
	write("index.html", "<!doctype html>kabuk")
	write("app.js", "console.log(1)")
	write("app.js.br", "BR")
	write("app.js.gz", "GZ")
	write("vendor/lib.js", "lib")
	write("fonts/inter.woff2", "font")
	write("apps/settings/index.html", "ayarlar")
	write(".secret", "x")
	cases := []struct {
		name, path string
		hdr        map[string]string
		status     int
		body, cc   string
		enc, ctype string
	}{
		{"root", "/", nil, 200, "<!doctype html>kabuk", "no-cache", "", "text/html; charset=utf-8"},
		{"js plain", "/shell/app.js", nil, 200, "console.log(1)", "no-cache", "", "text/javascript; charset=utf-8"},
		{"js br", "/shell/app.js", map[string]string{"Accept-Encoding": "gzip, br"}, 200, "BR", "no-cache", "br", "text/javascript; charset=utf-8"},
		{"js gz", "/shell/app.js", map[string]string{"Accept-Encoding": "gzip, br;q=0"}, 200, "GZ", "no-cache", "gzip", ""},
		{"vendor", "/shell/vendor/lib.js", nil, 200, "lib", "no-cache", "", ""},
		{"font", "/shell/fonts/inter.woff2", nil, 200, "font", "public, max-age=31536000, immutable", "", "font/woff2"},
		{"dir index", "/shell/apps/settings/", nil, 200, "ayarlar", "no-cache", "", ""},
		{"dir redirect", "/shell/apps/settings", nil, 301, "", "", "", ""},
		{"dotfile", "/shell/.secret", nil, 404, "", "", "", ""},
		{"traversal", "/shell/../../etc/passwd", nil, 307, "", "", "", ""},
		{"missing", "/shell/nope.js", nil, 404, "", "", "", ""},
		{"other root path", "/etc/passwd", nil, 404, "", "", "", ""},
	}
	for _, c := range cases {
		r := e.do("GET", c.path, "", c.hdr)
		if r.StatusCode != c.status {
			t.Errorf("%s: status %d want %d", c.name, r.StatusCode, c.status)
			continue
		}
		if c.status != 200 {
			continue
		}
		b, _ := io.ReadAll(r.Body)
		if string(b) != c.body || r.Header.Get("Cache-Control") != c.cc || r.Header.Get("Content-Encoding") != c.enc {
			t.Errorf("%s: body=%q cc=%q enc=%q", c.name, b, r.Header.Get("Cache-Control"), r.Header.Get("Content-Encoding"))
		}
		if c.ctype != "" && r.Header.Get("Content-Type") != c.ctype {
			t.Errorf("%s: ctype %q", c.name, r.Header.Get("Content-Type"))
		}
		etag := r.Header.Get("ETag")
		if etag == "" {
			t.Errorf("%s: no ETag", c.name)
			continue
		}
		h := map[string]string{"If-None-Match": etag}
		for k, v := range c.hdr {
			h[k] = v
		}
		if r2 := e.do("GET", c.path, "", h); r2.StatusCode != 304 {
			t.Errorf("%s: revalidate %d", c.name, r2.StatusCode)
		}
	}
	plain := e.do("GET", "/shell/app.js", "", nil).Header.Get("ETag")
	br := e.do("GET", "/shell/app.js", "", map[string]string{"Accept-Encoding": "br"}).Header.Get("ETag")
	if plain == br {
		t.Error("ETag must differ per encoding")
	}
}

func wsURL(e *env, path string) string { return "ws" + strings.TrimPrefix(e.srv.URL, "http") + path }

func TestEventsWS(t *testing.T) {
	e := newEnv(t)
	cookie := e.login()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	// Origin yoksa reddedilir.
	if _, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/events"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {cookie}}}); err == nil || r.StatusCode != 403 {
		t.Fatal("events without origin accepted")
	}
	// Çerez yoksa 401.
	if _, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/events"), &websocket.DialOptions{HTTPHeader: http.Header{"Origin": {e.srv.URL}}}); err == nil || r.StatusCode != 401 {
		t.Fatal("events without cookie accepted")
	}
	c, _, err := websocket.Dial(ctx, wsURL(e, "/api/v1/events"), &websocket.DialOptions{HTTPHeader: http.Header{"Cookie": {cookie}, "Origin": {e.srv.URL}}})
	if err != nil {
		t.Fatal(err)
	}
	defer c.CloseNow()
	_ = c.Write(ctx, websocket.MessageText, []byte(`{"op":"sub","topics":["system","jobs","net"]}`))
	seen := map[string]bool{}
	// Bir iş başlat: jobs olayları gelmeli.
	go func() {
		time.Sleep(100 * time.Millisecond)
		e.do("POST", "/api/v1/net/wifi/connect", `{"ssid":"Kafe_WiFi"}`, map[string]string{"Cookie": cookie, "X-Maker": "1"})
	}()
	var lastJob map[string]any
	for !(seen["system"] && seen["net"] && lastJob != nil && lastJob["state"] == "done") {
		_, b, err := c.Read(ctx)
		if err != nil {
			t.Fatalf("read: %v (seen %v, job %v)", err, seen, lastJob)
		}
		var m struct {
			Topic string          `json:"topic"`
			Data  json.RawMessage `json:"data"`
		}
		_ = json.Unmarshal(b, &m)
		seen[m.Topic] = true
		if m.Topic == "system" && !bytes.Contains(m.Data, []byte(`"MakerOS"`)) {
			t.Fatalf("system data %s", m.Data)
		}
		if m.Topic == "jobs" {
			_ = json.Unmarshal(m.Data, &lastJob)
		}
	}
	if !e.d.Hub.Running("system") {
		t.Fatal("system producer not running")
	}
	_ = c.Write(ctx, websocket.MessageText, []byte(`{"op":"unsub","topics":["system"]}`))
	for i := 0; i < 50 && e.d.Hub.Running("system"); i++ {
		time.Sleep(10 * time.Millisecond)
	}
	if e.d.Hub.Running("system") {
		t.Fatal("system producer still running after unsub")
	}
	c.Close(websocket.StatusNormalClosure, "")
}

func TestTermWS(t *testing.T) {
	e := newEnv(t)
	cookie := e.login()
	h := map[string]string{"Cookie": cookie, "X-Maker": "1"}
	r := e.do("POST", "/api/v1/term", `{"cols":100,"rows":30}`, h)
	var tr termResp
	_ = json.NewDecoder(r.Body).Decode(&tr)
	if r.StatusCode != 200 || tr.ID == "" || tr.Ticket == "" {
		t.Fatalf("term create %d %+v", r.StatusCode, tr)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	origin := http.Header{"Origin": {e.srv.URL}}
	// Yanlış Origin.
	if _, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/term/"+tr.ID+"?ticket="+tr.Ticket), &websocket.DialOptions{HTTPHeader: http.Header{"Origin": {"http://evil.example"}}}); err == nil || r.StatusCode != 403 {
		t.Fatal("bad origin accepted")
	}
	// Bilet hatalı Origin denemesinde harcanmaz; şimdi bağlan.
	c, _, err := websocket.Dial(ctx, wsURL(e, "/api/v1/term/"+tr.ID+"?ticket="+tr.Ticket), &websocket.DialOptions{HTTPHeader: origin})
	if err != nil {
		t.Fatal(err)
	}
	_ = c.Write(ctx, websocket.MessageText, []byte(`{"type":"resize","cols":120,"rows":40}`))
	_ = c.Write(ctx, websocket.MessageBinary, []byte("echo MERHABA_$((40+2))\n"))
	readUntil := func(c *websocket.Conn, want string) {
		t.Helper()
		var acc []byte
		for !bytes.Contains(acc, []byte(want)) {
			_, b, err := c.Read(ctx)
			if err != nil {
				t.Fatalf("read: %v; got %q", err, acc)
			}
			acc = append(acc, b...)
		}
	}
	readUntil(c, "MERHABA_42")
	// Bilet tek kullanımlık.
	if _, r, err := websocket.Dial(ctx, wsURL(e, "/api/v1/term/"+tr.ID+"?ticket="+tr.Ticket), &websocket.DialOptions{HTTPHeader: origin}); err == nil || r.StatusCode != 403 {
		t.Fatal("ticket reused")
	}
	c.Close(websocket.StatusNormalClosure, "")
	// Yeniden bağlan: yeni bilet, halka tampon tekrar gönderilir.
	r = e.do("POST", "/api/v1/term/"+tr.ID+"/ticket", "", h)
	var tr2 termResp
	_ = json.NewDecoder(r.Body).Decode(&tr2)
	if r.StatusCode != 200 || tr2.ID != tr.ID {
		t.Fatalf("reticket %d", r.StatusCode)
	}
	c2, _, err := websocket.Dial(ctx, wsURL(e, "/api/v1/term/"+tr.ID+"?ticket="+tr2.Ticket), &websocket.DialOptions{HTTPHeader: origin})
	if err != nil {
		t.Fatal(err)
	}
	readUntil(c2, "MERHABA_42")
	// Kabuk çıkınca WS normal kapanır.
	_ = c2.Write(ctx, websocket.MessageBinary, []byte("exit\n"))
	for {
		_, _, err := c2.Read(ctx)
		if err != nil {
			if websocket.CloseStatus(err) != websocket.StatusNormalClosure {
				t.Fatalf("close status: %v", err)
			}
			break
		}
	}
}
