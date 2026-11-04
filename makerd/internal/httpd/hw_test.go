package httpd

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"

	"github.com/ferhatozcelik/maker-os/makerd/internal/camera"
	"github.com/ferhatozcelik/maker-os/makerd/internal/gpio"
)

func hwEnv(t *testing.T) *env {
	return newEnvWith(t, false, func(d *Deps) {
		d.Camera = camera.NewHub(camera.Synthetic{})
		d.GPIO = gpio.NewSim("test")
	})
}

func TestGPIOSim(t *testing.T) {
	e := hwEnv(t)
	h := map[string]string{"Cookie": e.login(), "X-Maker": "1"}
	var b gpio.Board
	_ = json.NewDecoder(e.do("GET", "/api/v1/gpio", "", h).Body).Decode(&b)
	if len(b.Pins) != 40 || b.Pins[10].Name != "GPIO17" || !b.Sim {
		t.Fatalf("pano: %d pin %+v", len(b.Pins), b.Pins[10])
	}
	r := e.do("POST", "/api/v1/gpio/17", `{"value":1}`, h)
	_ = json.NewDecoder(r.Body).Decode(&b)
	if r.StatusCode != 200 || b.Pins[10].Value != 1 || b.Pins[10].Dir != "out" || !b.Pins[10].Held {
		t.Fatalf("set: %d %+v", r.StatusCode, b.Pins[10])
	}
	if r := e.do("POST", "/api/v1/gpio/0", `{"value":1}`, h); r.StatusCode != 400 {
		t.Fatalf("EEPROM pini: %d", r.StatusCode)
	}
	if r := e.do("POST", "/api/v1/gpio/17", `{"release":true}`, h); r.StatusCode != 200 {
		t.Fatalf("release %d", r.StatusCode)
	}
}

func TestCameraSim(t *testing.T) {
	e := hwEnv(t)
	h := map[string]string{"Cookie": e.login(), "X-Maker": "1"}
	var st camera.Status
	_ = json.NewDecoder(e.do("GET", "/api/v1/camera", "", h).Body).Decode(&st)
	if !st.Sim || len(st.Cams) != 1 {
		t.Fatalf("durum %+v", st)
	}
	req, _ := http.NewRequest("GET", e.srv.URL+"/api/v1/camera/stream", nil)
	req.Header.Set("Cookie", h["Cookie"])
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	buf := make([]byte, 64<<10)
	n, _ := io.ReadAtLeast(res.Body, buf, 2000)
	res.Body.Close()
	if !strings.HasPrefix(res.Header.Get("Content-Type"), "multipart/x-mixed-replace") || !bytes.Contains(buf[:n], []byte{0xFF, 0xD8}) {
		t.Fatalf("akış: %s %d", res.Header.Get("Content-Type"), n)
	}
	r := e.do("POST", "/api/v1/camera/photo", "", h)
	var ph struct{ Path string }
	_ = json.NewDecoder(r.Body).Decode(&ph)
	if r.StatusCode != 200 || !strings.HasPrefix(ph.Path, "~/Resimler/Kamera/") {
		t.Fatalf("foto %d %+v", r.StatusCode, ph)
	}
}

func TestPluginsImportExport(t *testing.T) {
	e := hwEnv(t)
	h := map[string]string{"Cookie": e.login(), "X-Maker": "1"}
	tpl, _ := io.ReadAll(e.do("GET", "/api/v1/plugins/template.zip", "", h).Body)
	if r := e.do("POST", "/api/v1/plugins/import", string(tpl), h); r.StatusCode != 200 {
		b, _ := io.ReadAll(r.Body)
		t.Fatalf("içe aktar %d %s", r.StatusCode, b)
	}
	var list []map[string]any
	_ = json.NewDecoder(e.do("GET", "/api/v1/plugins", "", h).Body).Decode(&list)
	byID := map[string]map[string]any{}
	for _, p := range list {
		byID[p["id"].(string)] = p
	}
	// MakerOS ile gelen Konya HackerSpace eklentisi kendiliğinden kurulur.
	if len(list) != 2 || byID["benim-eklentim"]["web_port"].(float64) != 8765 || byID["khs"]["theme"] == nil {
		t.Fatalf("liste %+v", list)
	}
	if r := e.do("GET", "/api/v1/plugins/khs/asset/wallpapers/khs-dark.svg", "", h); r.StatusCode != 200 || r.Header.Get("Content-Type") != "image/svg+xml" {
		t.Fatalf("tema varlığı %d %s", r.StatusCode, r.Header.Get("Content-Type"))
	}
	if r := e.do("GET", "/api/v1/plugins/khs/asset/main.py", "", h); r.StatusCode != 404 {
		t.Fatalf("görsel olmayan varlık sunuldu: %d", r.StatusCode)
	}
	if r := e.do("DELETE", "/api/v1/plugins/khs", "", h); r.StatusCode != 204 {
		t.Fatalf("khs sil %d", r.StatusCode)
	}
	var after []map[string]any
	_ = json.NewDecoder(e.do("GET", "/api/v1/plugins", "", h).Body).Decode(&after)
	if len(after) != 1 {
		t.Fatalf("kaldırılan varsayılan eklenti geri geldi: %+v", after)
	}
	ex, _ := io.ReadAll(e.do("GET", "/api/v1/plugins/benim-eklentim/export.zip", "", h).Body)
	zr, err := zip.NewReader(bytes.NewReader(ex), int64(len(ex)))
	if err != nil {
		t.Fatal(err)
	}
	names := map[string]bool{}
	for _, f := range zr.File {
		names[f.Name] = true
	}
	for _, want := range []string{"plugin.json", "main.py", "pyproject.toml", "README.md", "icon.svg"} {
		if !names[want] {
			t.Fatalf("dışa aktarımda %s yok: %v", want, names)
		}
	}
	// Kötü zip'ler
	bad := func(files map[string]string) int {
		var b bytes.Buffer
		zw := zip.NewWriter(&b)
		for n, c := range files {
			w, _ := zw.Create(n)
			_, _ = w.Write([]byte(c))
		}
		_ = zw.Close()
		return e.do("POST", "/api/v1/plugins/import", b.String(), h).StatusCode
	}
	good := `{"id":"x","name":"X","version":"1","entrypoint":"main.py"}`
	if c := bad(map[string]string{"plugin.json": good, "main.py": "", "pyproject.toml": "", "../kacis.py": ""}); c != 400 {
		t.Fatalf("yol aşımı kabul edildi: %d", c)
	}
	if c := bad(map[string]string{"plugin.json": `{"id":"Kotu Id","name":"X","version":"1"}`, "main.py": "", "pyproject.toml": ""}); c != 400 {
		t.Fatalf("geçersiz id kabul edildi: %d", c)
	}
	if c := bad(map[string]string{"plugin.json": good, "main.py": ""}); c != 400 {
		t.Fatalf("pyproject'siz kabul edildi: %d", c)
	}
	// Tek üst klasörlü zip (GitHub "Download ZIP" biçimi) kabul edilir.
	if c := bad(map[string]string{"proje-main/plugin.json": `{"id":"y","name":"Y","version":"1"}`, "proje-main/main.py": "", "proje-main/pyproject.toml": ""}); c != 200 {
		t.Fatalf("üst klasörlü zip reddedildi: %d", c)
	}
	if r := e.do("DELETE", "/api/v1/plugins/benim-eklentim", "", h); r.StatusCode != 204 {
		t.Fatalf("sil %d", r.StatusCode)
	}
}
