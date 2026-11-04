package httpd

import (
	"fmt"
	"mime"
	"net/http"
	"os"
	"path"
	"strconv"
	"strings"
	"sync"
)

// Static kabuk dizinini sunar: os.Root ile dizin dışına (sembolik bağ dahil)
// çıkılamaz. Yalnız *.woff2 bir yıl "immutable", diğerleri (vendor/ dahil)
// no-cache + ETag. .br/.gz ön-sıkıştırılmış eşi varsa ve istemci kabul ediyorsa o sunulur.
type Static struct {
	dir  string
	mu   sync.Mutex
	root *os.Root
}

// NewStatic dizin için sunucu oluşturur (dizin sonradan oluşabilir).
func NewStatic(dir string) *Static { return &Static{dir: dir} }

func (s *Static) getRoot() *os.Root {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.root == nil {
		if r, err := os.OpenRoot(s.dir); err == nil {
			s.root = r
		}
	}
	return s.root
}

var extraTypes = map[string]string{
	".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8",
	".css": "text/css; charset=utf-8", ".html": "text/html; charset=utf-8",
	".json": "application/json", ".svg": "image/svg+xml", ".woff2": "font/woff2",
	".wasm": "application/wasm", ".webmanifest": "application/manifest+json",
	".png": "image/png", ".webp": "image/webp", ".ico": "image/x-icon", ".txt": "text/plain; charset=utf-8",
}

func contentType(name string) string {
	ext := strings.ToLower(path.Ext(name))
	if t, ok := extraTypes[ext]; ok {
		return t
	}
	if t := mime.TypeByExtension(ext); t != "" {
		return t
	}
	return "application/octet-stream"
}

// accepts, Accept-Encoding başlığının kodlamayı q>0 ile kabul edip etmediği.
func accepts(header, enc string) bool {
	for _, part := range strings.Split(header, ",") {
		name, params, _ := strings.Cut(strings.TrimSpace(part), ";")
		if !strings.EqualFold(strings.TrimSpace(name), enc) {
			continue
		}
		if q, ok := strings.CutPrefix(strings.TrimSpace(params), "q="); ok {
			if v, err := strconv.ParseFloat(q, 64); err == nil && v == 0 {
				return false
			}
		}
		return true
	}
	return false
}

func notFound(w http.ResponseWriter) {
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(http.StatusNotFound)
	_, _ = w.Write([]byte("Bulunamadı\n"))
}

// Serve, kabuk köküne göre rel yolundaki dosyayı sunar.
func (s *Static) Serve(w http.ResponseWriter, r *http.Request, rel string) {
	root := s.getRoot()
	if root == nil {
		notFound(w)
		return
	}
	rel = strings.TrimPrefix(path.Clean("/"+rel), "/")
	if rel == "" {
		rel = "index.html"
	}
	for _, seg := range strings.Split(rel, "/") {
		if strings.HasPrefix(seg, ".") {
			notFound(w)
			return
		}
	}
	st, err := root.Stat(rel)
	if err == nil && st.IsDir() {
		if !strings.HasSuffix(r.URL.Path, "/") {
			http.Redirect(w, r, r.URL.Path+"/", http.StatusMovedPermanently)
			return
		}
		rel = path.Join(rel, "index.html")
		st, err = root.Stat(rel)
	}
	if err != nil || !st.Mode().IsRegular() {
		notFound(w)
		return
	}
	h := w.Header()
	h.Set("Content-Type", contentType(rel))
	// Yalnız yazı tipleri değişmez kabul edilir. Diğer her şey (vendor/ dahil: üretilen ya da
	// yamalanan dosyalar olabilir) ETag ile yeniden doğrulanır (değişmediyse 304; Pi'de ucuz).
	// Önceden vendor/ bir yıl "immutable" idi: sonradan eklenen simgeler eski tarayıcılarda boş kalıyordu.
	if strings.HasSuffix(rel, ".woff2") {
		h.Set("Cache-Control", "public, max-age=31536000, immutable")
	} else {
		h.Set("Cache-Control", "no-cache")
	}
	name, suffix := rel, ""
	ae := r.Header.Get("Accept-Encoding")
	for _, v := range []struct{ ext, enc string }{{".br", "br"}, {".gz", "gzip"}} {
		if vst, err := root.Stat(rel + v.ext); err == nil && vst.Mode().IsRegular() {
			h.Set("Vary", "Accept-Encoding")
			if suffix == "" && accepts(ae, v.enc) {
				name, suffix, st = rel+v.ext, "-"+v.enc, vst
				h.Set("Content-Encoding", v.enc)
			}
		}
	}
	f, err := root.Open(name)
	if err != nil {
		notFound(w)
		return
	}
	defer f.Close()
	h.Set("ETag", fmt.Sprintf(`"%x-%x%s"`, st.ModTime().UnixNano(), st.Size(), suffix))
	http.ServeContent(w, r, "", st.ModTime(), f)
}
