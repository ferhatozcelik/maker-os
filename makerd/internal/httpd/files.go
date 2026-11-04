package httpd

import (
	"mime"
	"net/http"
	"path"
	"strconv"
	"strings"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

// Dosyalar: tüm işlemler kullanıcının kendi hesabıyla (makerd-user) yapılır;
// makerd kullanıcının ev dizinini doğrudan okumaz. Göreli yollar ev dizinine göredir.
const maxUpload = 512 << 20

func (s *Server) filesAvailable(w http.ResponseWriter) bool {
	if s.Agent == nil {
		writeErr(w, apierr.Unavailable("Dosya hizmeti bu sistemde yok."))
		return false
	}
	return true
}

func (s *Server) filesList(w http.ResponseWriter, r *http.Request) {
	if !s.filesAvailable(w) {
		return
	}
	b, err := s.Agent.FS(r.Context(), sessionOf(r).User, termproc.Open{Mode: "list", Path: r.URL.Query().Get("path")})
	if err != nil {
		writeErr(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	_, _ = w.Write(b)
}

func (s *Server) filesRaw(w http.ResponseWriter, r *http.Request) {
	if !s.filesAvailable(w) {
		return
	}
	p := r.URL.Query().Get("path")
	ct := mime.TypeByExtension(path.Ext(p))
	if ct == "" {
		ct = "application/octet-stream"
	}
	w.Header().Set("Content-Type", ct)
	// Etkin içerik (HTML/SVG) kabuk kökenine gömülmesin.
	w.Header().Set("Content-Security-Policy", "sandbox")
	if r.URL.Query().Get("download") == "1" {
		w.Header().Set("Content-Disposition", mime.FormatMediaType("attachment", map[string]string{"filename": path.Base(p)}))
	}
	if err := s.Agent.Read(r.Context(), sessionOf(r).User, p, w); err != nil {
		writeErr(w, err)
	}
}

func (s *Server) filesUpload(w http.ResponseWriter, r *http.Request) {
	if !s.filesAvailable(w) {
		return
	}
	p := r.URL.Query().Get("path")
	if p == "" {
		writeErr(w, apierr.Invalid("Hedef yol gerekli."))
		return
	}
	if n, _ := strconv.ParseInt(r.Header.Get("Content-Length"), 10, 64); n > maxUpload {
		writeErr(w, apierr.Invalid("Dosya en fazla 512 MB olabilir."))
		return
	}
	if err := s.Agent.Write(r.Context(), sessionOf(r).User, p, http.MaxBytesReader(w, r.Body, maxUpload)); err != nil {
		writeErr(w, err)
		return
	}
	noContent(w)
}

func (s *Server) filesOp(w http.ResponseWriter, r *http.Request) {
	if !s.filesAvailable(w) {
		return
	}
	var req struct {
		Op   string `json:"op"`
		Path string `json:"path"`
		To   string `json:"to"`
	}
	if !decode(w, r, &req) {
		return
	}
	switch req.Op {
	case "mkdir", "rename", "remove", "stat":
	default:
		writeErr(w, apierr.Invalid("Geçersiz dosya işlemi."))
		return
	}
	b, err := s.Agent.FS(r.Context(), sessionOf(r).User, termproc.Open{Mode: req.Op, Path: req.Path, To: req.To})
	if err != nil {
		writeErr(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	_, _ = w.Write(b)
}

// filesRun bir Python betiğini terminal oturumunda çalıştırır (Dosyalar'da .py'ye tıklama).
// Betiğin klasöründe pyproject.toml varsa "uv run" (projenin sanal ortamı), yoksa python3.
func (s *Server) filesRun(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	var req struct {
		Path string `json:"path"`
	}
	if !decode(w, r, &req) {
		return
	}
	cmd, err := runFileCmd(req.Path)
	if err != nil {
		writeErr(w, err)
		return
	}
	s.termRun(w, r, cmd, path.Base(req.Path))
}

func runFileCmd(p string) (string, error) {
	if !strings.HasSuffix(strings.ToLower(p), ".py") || strings.ContainsAny(p, "\x00\n\r") {
		return "", apierr.Invalid("Yalnız Python (.py) dosyaları çalıştırılabilir.")
	}
	base := "~"
	switch {
	case strings.HasPrefix(p, "~/"):
		p = p[2:]
	case strings.HasPrefix(p, "/"):
		base = ""
	}
	p = path.Clean(p)
	dir, file := path.Split(p)
	if dir == "" {
		dir = "."
	}
	cd := "cd " + shq(dir)
	if base == "~" {
		cd = "cd ~ && " + cd
	}
	f := shq(file)
	return cd + " && if [ -f pyproject.toml ]; then uv run " + f + "; else python3 " + f + "; fi", nil
}

func shq(s string) string { return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'" }
