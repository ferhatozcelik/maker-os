package httpd

import (
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/auth"
	"github.com/ferhatozcelik/maker-os/makerd/internal/jobs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/network"
)

const setupBody = `{"user":"ayse","password":"gizli-parola","fullname":"Ayşe Yılmaz","hostname":"makeros-0001",
	"timezone":"Europe/Berlin","lang":"de_DE.UTF-8","keymap":"de"}`

func cookieOf(r *http.Response) string {
	for _, c := range r.Cookies() {
		if c.Name == auth.CookieName && c.Value != "" {
			return c.Name + "=" + c.Value
		}
	}
	return ""
}

func TestSetupFlow(t *testing.T) {
	e := newEnvOpts(t, true)
	x := map[string]string{"X-Maker": "1"}
	// Kurulumdan önce giriş yapılamaz, /setup açıktır.
	if r := e.do("POST", "/api/v1/auth/login", `{"user":"maker","password":"maker"}`, x); r.StatusCode != 401 {
		t.Fatalf("login before setup %d", r.StatusCode)
	}
	r := e.do("GET", "/api/v1/setup", "", nil)
	var st setupStateResp
	_ = json.NewDecoder(r.Body).Decode(&st)
	if r.StatusCode != 200 || st.SetupDone || st.Hostname != "makeros-0001" || len(st.Languages) == 0 || len(st.Keymaps) == 0 ||
		len(st.Timezones) == 0 || st.Defaults.Lang != "tr_TR.UTF-8" || st.MinPassword != 6 {
		t.Fatalf("setup state %d %+v", r.StatusCode, st)
	}
	if r := e.do("POST", "/api/v1/setup/wifi/scan", "", x); r.StatusCode != 200 {
		t.Fatalf("setup scan %d", r.StatusCode)
	}
	// /auth/local kurulumdan önce oturum açmaz.
	if r := e.do("GET", "/api/v1/auth/local?token=yerel-belirtec", "", nil); r.StatusCode != 302 || cookieOf(r) != "" {
		t.Fatalf("local before setup %d %q", r.StatusCode, cookieOf(r))
	}
	// Doğrulama ve CSRF.
	if r := e.do("POST", "/api/v1/setup/complete", setupBody, nil); r.StatusCode != 403 {
		t.Fatalf("no csrf %d", r.StatusCode)
	}
	for _, bad := range []string{
		strings.Replace(setupBody, `"ayse"`, `"root"`, 1),
		strings.Replace(setupBody, `gizli-parola`, `abc`, 1),
		strings.Replace(setupBody, `makeros-0001`, `Bad_Name`, 1),
		strings.Replace(setupBody, `Europe/Berlin`, `../../etc`, 1),
		`{"user":"ayse"`,
	} {
		if r := e.do("POST", "/api/v1/setup/complete", bad, x); r.StatusCode != 400 {
			t.Fatalf("bad body accepted %d: %s", r.StatusCode, bad)
		}
	}
	// Eşzamanlı tamamlama: tam olarak biri başarılı olur.
	var wg sync.WaitGroup
	var mu sync.Mutex
	codes := map[int]int{}
	var winner *http.Response
	for i := 0; i < 6; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			req, _ := http.NewRequest("POST", e.srv.URL+"/api/v1/setup/complete", strings.NewReader(setupBody))
			req.Header.Set("X-Maker", "1")
			resp, err := http.DefaultTransport.RoundTrip(req)
			if err != nil {
				t.Error(err)
				return
			}
			mu.Lock()
			codes[resp.StatusCode]++
			if resp.StatusCode == 200 {
				winner = resp
			} else {
				resp.Body.Close()
			}
			mu.Unlock()
		}()
	}
	wg.Wait()
	if codes[200] != 1 || codes[409] != 5 {
		t.Fatalf("codes %v", codes)
	}
	defer winner.Body.Close()
	var sr setupResp
	_ = json.NewDecoder(winner.Body).Decode(&sr)
	cookie := cookieOf(winner)
	if sr.User != "ayse" || !sr.Admin || cookie == "" || len(sr.Warnings) != 0 {
		t.Fatalf("setup resp %+v cookie %q", sr, cookie)
	}
	// Oturum açık, ayarlar uygulandı, setup uçları artık 409.
	r = e.do("GET", "/api/v1/session", "", map[string]string{"Cookie": cookie})
	var sess userResp
	_ = json.NewDecoder(r.Body).Decode(&sess)
	if r.StatusCode != 200 || sess.User != "ayse" || !sess.Admin || sess.SetupDone == nil || !*sess.SetupDone {
		t.Fatalf("session %d %+v", r.StatusCode, sess)
	}
	if v, _ := e.d.System.Time(t.Context()); v.Timezone != "Europe/Berlin" {
		t.Errorf("tz %s", v.Timezone)
	}
	if v, _ := e.d.System.Locale(t.Context()); v.Lang != "de_DE.UTF-8" || v.Keymap != "de" {
		t.Errorf("locale %+v", v)
	}
	if in, _ := e.d.System.Info(t.Context()); in.Hostname != "makeros-0001" {
		t.Errorf("hostname %s", in.Hostname)
	}
	for _, p := range []string{"/api/v1/setup"} {
		if r := e.do("GET", p, "", nil); r.StatusCode != 409 {
			t.Fatalf("%s after setup %d", p, r.StatusCode)
		}
	}
	if r := e.do("POST", "/api/v1/setup/complete", setupBody, x); r.StatusCode != 409 {
		t.Fatalf("complete after setup %d", r.StatusCode)
	}
	if r := e.do("POST", "/api/v1/auth/login", `{"user":"ayse","password":"gizli-parola"}`, x); r.StatusCode != 200 {
		t.Fatalf("login new user %d", r.StatusCode)
	}
	// Yerel kabuk artık kurulum kullanıcısıyla açılır.
	r = e.do("GET", "/api/v1/auth/local?token=yerel-belirtec", "", nil)
	r = e.do("GET", "/api/v1/session", "", map[string]string{"Cookie": cookieOf(r)})
	_ = json.NewDecoder(r.Body).Decode(&sess)
	if sess.User != "ayse" {
		t.Fatalf("local user %q", sess.User)
	}
}

func TestSetupWifiJobAndRemoteAddr(t *testing.T) {
	e := newEnvOpts(t, true)
	x := map[string]string{"X-Maker": "1"}
	body := strings.Replace(setupBody, `"keymap":"de"}`, `"keymap":"de","wifi":{"ssid":"Kafe_WiFi"}}`, 1)
	r := e.do("POST", "/api/v1/setup/complete", body, x)
	var sr setupResp
	_ = json.NewDecoder(r.Body).Decode(&sr)
	if r.StatusCode != 200 || sr.Job == "" {
		t.Fatalf("%d %+v", r.StatusCode, sr)
	}
	deadline := time.Now().Add(5 * time.Second)
	for {
		j, _ := e.d.Jobs.Get(sr.Job)
		if j.State == jobs.StateDone {
			break
		}
		if j.State == jobs.StateFailed || time.Now().After(deadline) {
			t.Fatalf("job %+v", j)
		}
		time.Sleep(20 * time.Millisecond)
	}

	// Yerel ağ dışı adres reddedilir.
	e2 := newEnvOpts(t, true)
	h := e2.srv.Config.Handler
	for ip, want := range map[string]int{"203.0.113.9:5000": 403, "10.42.0.20:5000": 200} {
		req, _ := http.NewRequest("GET", "/api/v1/setup", nil)
		req.RemoteAddr = ip
		rec := &recorder{h: http.Header{}}
		h.ServeHTTP(rec, req)
		if rec.code != want {
			t.Errorf("%s: %d want %d", ip, rec.code, want)
		}
	}
}

type recorder struct {
	h    http.Header
	code int
}

func (r *recorder) Header() http.Header         { return r.h }
func (r *recorder) Write(b []byte) (int, error) { return len(b), nil }
func (r *recorder) WriteHeader(c int)           { r.code = c }

func TestPrefs(t *testing.T) {
	e := newEnv(t)
	h := map[string]string{"Cookie": e.login(), "X-Maker": "1"}
	r := e.do("GET", "/api/v1/prefs", "", h)
	if b, _ := io.ReadAll(r.Body); r.StatusCode != 200 || string(b) != "{}" {
		t.Fatalf("empty prefs %d %s", r.StatusCode, b)
	}
	r = e.do("PUT", "/api/v1/prefs", `{"theme":"dark","accent":"#ff6600","dock":["makeros.terminal"]}`, h)
	if r.StatusCode != 200 {
		t.Fatalf("put %d", r.StatusCode)
	}
	r = e.do("GET", "/api/v1/prefs", "", h)
	var m map[string]any
	_ = json.NewDecoder(r.Body).Decode(&m)
	if m["theme"] != "dark" || m["accent"] != "#ff6600" {
		t.Fatalf("prefs %v", m)
	}
	if r := e.do("PUT", "/api/v1/prefs", `[1,2]`, h); r.StatusCode != 400 {
		t.Fatalf("array accepted %d", r.StatusCode)
	}
	if r := e.do("PUT", "/api/v1/prefs", `{"x":"`+strings.Repeat("a", 17<<10)+`"}`, h); r.StatusCode != 413 {
		t.Fatalf("oversized %d", r.StatusCode)
	}
	if r := e.do("GET", "/api/v1/prefs", "", nil); r.StatusCode != 401 {
		t.Fatalf("unauth %d", r.StatusCode)
	}
}

func TestWifiToggleAndSystemUsage(t *testing.T) {
	e := newEnv(t)
	h := map[string]string{"Cookie": e.login(), "X-Maker": "1"}
	if r := e.do("PUT", "/api/v1/net/wifi", `{}`, h); r.StatusCode != 400 {
		t.Fatalf("missing enabled %d", r.StatusCode)
	}
	r := e.do("PUT", "/api/v1/net/wifi", `{"enabled":false}`, h)
	var st network.Status
	_ = json.NewDecoder(r.Body).Decode(&st)
	if r.StatusCode != 200 || st.WifiEnabled {
		t.Fatalf("disable %d %+v", r.StatusCode, st)
	}
	for _, i := range st.Ifaces {
		if i.Type == "wifi" && (i.State != "unavailable" || i.SSID != "") {
			t.Fatalf("wifi iface %+v", i)
		}
	}
	r = e.do("PUT", "/api/v1/net/wifi", `{"enabled":true}`, h)
	_ = json.NewDecoder(r.Body).Decode(&st)
	if !st.WifiEnabled {
		t.Fatal("enable")
	}
	for _, i := range st.Ifaces {
		if i.Type == "wifi" && i.SSID != "MakerLab" {
			t.Fatalf("not reconnected %+v", i)
		}
	}
	r = e.do("GET", "/api/v1/system", "", h)
	var raw map[string]map[string]any
	b, _ := io.ReadAll(r.Body)
	_ = json.Unmarshal(b, &raw)
	if _, ok := raw["cpu"]["usage_pct"]; !ok {
		t.Fatalf("no usage_pct: %s", b)
	}
	r = e.do("GET", "/api/v1/apps", "", h)
	var list []map[string]any
	_ = json.NewDecoder(r.Body).Decode(&list)
	for _, a := range list {
		if ic, _ := a["icon"].(string); a["kind"] == "native" && !strings.HasPrefix(ic, "/shell/icons/") {
			t.Fatalf("native icon %v", a)
		}
	}
}
