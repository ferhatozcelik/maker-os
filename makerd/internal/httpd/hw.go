package httpd

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httputil"
	"net/url"
	"path"
	"strconv"
	"strings"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/plugins"
)

// --- kamera ------------------------------------------------------------------

func (s *Server) cameraStatus(w http.ResponseWriter, r *http.Request) {
	if s.Camera == nil {
		writeErr(w, apierr.Unavailable("Kamera hizmeti yok."))
		return
	}
	writeJSON(w, 200, s.Camera.Status(r.Context()))
}

// cameraStream multipart MJPEG (img etiketinde doğrudan gösterilir).
func (s *Server) cameraStream(w http.ResponseWriter, r *http.Request) {
	if s.Camera == nil {
		writeErr(w, apierr.Unavailable("Kamera hizmeti yok."))
		return
	}
	ch, leave := s.Camera.Subscribe()
	defer leave()
	w.Header().Set("Content-Type", "multipart/x-mixed-replace; boundary=mkframe")
	w.Header().Set("Cache-Control", "no-store")
	rc := http.NewResponseController(w)
	ctx, cancel := mergeCtx(r.Context(), s.ctx)
	defer cancel()
	for {
		select {
		case <-ctx.Done():
			return
		case f, ok := <-ch:
			if !ok {
				return
			}
			_ = rc.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if _, err := fmt.Fprintf(w, "--mkframe\r\nContent-Type: image/jpeg\r\nContent-Length: %d\r\n\r\n", len(f)); err != nil {
				return
			}
			if _, err := w.Write(f); err != nil {
				return
			}
			_, _ = io.WriteString(w, "\r\n")
			_ = rc.Flush()
		}
	}
}

// cameraPhoto fotoğraf çeker; ?save=1 ise ~/Resimler/Kamera altına kaydeder.
func (s *Server) cameraPhoto(w http.ResponseWriter, r *http.Request) {
	if s.Camera == nil {
		writeErr(w, apierr.Unavailable("Kamera hizmeti yok."))
		return
	}
	img, err := s.Camera.Photo(r.Context())
	if err != nil {
		writeErr(w, err)
		return
	}
	name := "Resimler/Kamera/" + time.Now().Format("2006-01-02_15-04-05") + ".jpg"
	if s.Agent != nil {
		if err := s.Agent.Write(r.Context(), sessionOf(r).User, name, bytes.NewReader(img)); err != nil {
			writeErr(w, err)
			return
		}
	}
	writeJSON(w, 200, map[string]any{"path": "~/" + name, "size": len(img)})
}

// --- GPIO --------------------------------------------------------------------

func (s *Server) gpioBoard(w http.ResponseWriter, r *http.Request) {
	if s.GPIO == nil {
		writeErr(w, apierr.Unavailable("GPIO hizmeti yok."))
		return
	}
	b, err := s.GPIO.Board()
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, b)
}

func (s *Server) gpioSet(w http.ResponseWriter, r *http.Request) {
	if s.GPIO == nil {
		writeErr(w, apierr.Unavailable("GPIO hizmeti yok."))
		return
	}
	n, err := strconv.Atoi(r.PathValue("n"))
	if err != nil {
		writeErr(w, apierr.Invalid("Geçersiz GPIO numarası."))
		return
	}
	var req struct {
		Value   *int `json:"value"`
		Release bool `json:"release"`
	}
	if !decode(w, r, &req) {
		return
	}
	switch {
	case req.Release:
		err = s.GPIO.Release(n)
	case req.Value != nil:
		err = s.GPIO.Set(n, *req.Value)
	default:
		err = apierr.Invalid("value veya release gerekli.")
	}
	if err != nil {
		writeErr(w, err)
		return
	}
	s.gpioBoard(w, r)
}

// --- eklentiler --------------------------------------------------------------

func (s *Server) pluginList(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	l, err := plugins.List(r.Context(), s.Agent, sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, l)
}

func (s *Server) pluginTemplate(w http.ResponseWriter, r *http.Request) {
	b, err := plugins.TemplateZip()
	if err != nil {
		writeErr(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/zip")
	w.Header().Set("Content-Disposition", `attachment; filename="makeros-eklenti-sablonu.zip"`)
	_, _ = w.Write(b)
}

func (s *Server) pluginImport(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	data, err := io.ReadAll(http.MaxBytesReader(w, r.Body, plugins.MaxZip+1))
	if err != nil {
		writeErr(w, apierr.Invalid("Zip en fazla 15 MB olabilir."))
		return
	}
	m, err := plugins.Import(r.Context(), s.Agent, sessionOf(r).User, data)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, m)
}

func (s *Server) pluginExport(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	var b bytes.Buffer
	id := r.PathValue("id")
	if err := plugins.Export(r.Context(), s.Agent, sessionOf(r).User, id, &b); err != nil {
		writeErr(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/zip")
	w.Header().Set("Content-Disposition", `attachment; filename="`+id+`.zip"`)
	_, _ = w.Write(b.Bytes())
}

func (s *Server) pluginOp(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 30*time.Second)
	defer cancel()
	user, id := sessionOf(r).User, r.PathValue("id")
	var (
		p   plugins.Plugin
		err error
	)
	switch r.PathValue("op") {
	case "start":
		p, err = plugins.Start(ctx, s.Agent, user, id)
	case "stop":
		p, err = plugins.Stop(ctx, s.Agent, user, id)
	case "logs":
		var l string
		l, err = plugins.Logs(ctx, s.Agent, user, id)
		if err == nil {
			writeJSON(w, 200, map[string]string{"log": l})
			return
		}
	default:
		err = apierr.Invalid("Geçersiz işlem.")
	}
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, p)
}

func (s *Server) pluginDelete(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	if err := plugins.Remove(r.Context(), s.Agent, sessionOf(r).User, r.PathValue("id")); err != nil {
		writeErr(w, err)
		return
	}
	noContent(w)
}

// pluginProxy /plugin/<id>/... isteklerini eklentinin 127.0.0.1:<web_port> sunucusuna iletir.
func (s *Server) pluginProxy(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	id := r.PathValue("id")
	p, err := plugins.Get(r.Context(), s.Agent, sessionOf(r).User, id)
	if err != nil {
		writeErr(w, err)
		return
	}
	if p.WebPort == 0 {
		writeErr(w, apierr.NotFound("Bu eklentinin web arayüzü yok."))
		return
	}
	target, _ := url.Parse("http://127.0.0.1:" + strconv.Itoa(p.WebPort))
	rp := httputil.NewSingleHostReverseProxy(target)
	prefix := "/plugin/" + id
	rp.Director = func(q *http.Request) {
		q.URL.Scheme, q.URL.Host = target.Scheme, target.Host
		q.URL.Path = "/" + strings.TrimPrefix(strings.TrimPrefix(q.URL.Path, prefix), "/")
		q.Host = target.Host
		q.Header.Del("Cookie") // kabuk oturum çerezi eklentiye gitmez
		q.Header.Set("X-Forwarded-Prefix", prefix)
	}
	rp.ErrorHandler = func(w http.ResponseWriter, _ *http.Request, _ error) {
		writeErr(w, apierr.Unavailable("Eklenti çalışmıyor veya henüz hazır değil (Eklentiler → Başlat)."))
	}
	rp.ServeHTTP(w, r)
}

// pluginAsset eklenti içindeki statik dosya (tema duvar kâğıdı, logo, işaret).
func (s *Server) pluginAsset(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	rel := r.PathValue("path")
	switch strings.ToLower(path.Ext(rel)) {
	case ".svg":
		w.Header().Set("Content-Type", "image/svg+xml")
	case ".png":
		w.Header().Set("Content-Type", "image/png")
	case ".jpg", ".jpeg":
		w.Header().Set("Content-Type", "image/jpeg")
	case ".webp":
		w.Header().Set("Content-Type", "image/webp")
	default:
		writeErr(w, apierr.NotFound("Yalnız görsel dosyalar sunulur."))
		return
	}
	w.Header().Set("Content-Security-Policy", "sandbox")
	w.Header().Set("Cache-Control", "private, max-age=300")
	var b bytes.Buffer
	if err := plugins.Asset(r.Context(), s.Agent, sessionOf(r).User, r.PathValue("id"), rel, &b); err != nil {
		writeErr(w, err)
		return
	}
	_, _ = w.Write(b.Bytes())
}
