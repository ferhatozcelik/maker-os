// Package httpd, makerd'nin HTTP katmanıdır: /api/v1 REST uçları, events ve
// term WebSocket'leri, CSRF/kimlik ara katmanları ve kabuk statik sunumu.
package httpd

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net"
	"net/http"
	"net/url"
	"runtime/debug"
	"strings"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/activity"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apps"
	"github.com/ferhatozcelik/maker-os/makerd/internal/auth"
	"github.com/ferhatozcelik/maker-os/makerd/internal/browser"
	"github.com/ferhatozcelik/maker-os/makerd/internal/camera"
	"github.com/ferhatozcelik/maker-os/makerd/internal/events"
	"github.com/ferhatozcelik/maker-os/makerd/internal/gpio"
	"github.com/ferhatozcelik/maker-os/makerd/internal/jobs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/network"
	"github.com/ferhatozcelik/maker-os/makerd/internal/notify"
	"github.com/ferhatozcelik/maker-os/makerd/internal/ota"
	"github.com/ferhatozcelik/maker-os/makerd/internal/prefs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
	"github.com/ferhatozcelik/maker-os/makerd/internal/screen"
	"github.com/ferhatozcelik/maker-os/makerd/internal/setup"
	"github.com/ferhatozcelik/maker-os/makerd/internal/system"
	"github.com/ferhatozcelik/maker-os/makerd/internal/term"
	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

// Topics events WS konularıdır.
var Topics = []string{"system", "net", "jobs", "notifications", "processes"}

// Deps sunucunun bağımlılıklarıdır.
type Deps struct {
	Log       *slog.Logger
	WebDir    string
	Sessions  *auth.Manager
	Checker   auth.Checker
	Limiter   *auth.Limiter
	IsAdmin   func(user string) bool // nil: Checker sonucu kullanılır
	LocalUser string
	// LocalToken /auth/local belirteci; boşsa uç kapalıdır.
	LocalToken string
	// LocalLoopbackOnly, /auth/local yalnız 127.0.0.1/::1'den kabul edilir.
	LocalLoopbackOnly bool
	// SetupDone nil ise Setup'tan (o da nil ise hep true) okunur.
	SetupDone func() bool
	// Setup ilk açılış sihirbazı durumu; nil ise /setup uçları 409 döner.
	Setup *setup.Store
	// Accounts kurulumda yönetici hesabını oluşturur (gerçek: makerd-priv user.create).
	Accounts Accounts
	// Prefs kullanıcı tercihleri; nil ise /prefs 503 döner.
	Prefs *prefs.Store

	Hub      *events.Hub
	Jobs     *jobs.Manager
	Notify   *notify.Store
	System   system.Provider
	Network  network.Provider
	Apps     *apps.Registry
	Activity activity.Source
	Term     *term.Manager
	// Agent kullanıcı adına komut ve dosya işlemleri (makerd-user; mock: süreç içi).
	Agent uagent.Agent
	// Camera canlı kamera yayını (rpicam; mock: sentetik).
	Camera *camera.Hub
	// GPIO 40 pin başlık (gpiochip; mock: benzetim).
	GPIO gpio.Provider
	// VNCAddr yerel VNC sunucusu (wayvnc, 127.0.0.1:5900); boşsa /vnc 503.
	VNCAddr string
	// Screen Ekran Modu (varsayılan kapalı; açıkken /vnc çalışır). nil: her zaman açık sayılır (eski davranış, testler).
	Screen *screen.Manager
	// Browser MakerOS'taki "Chromium" penceresinin uzak tarayıcı oturumu; nil: yok.
	Browser *browser.Manager
	// Update yazılım güncellemesi (GitHub Releases → RAUC); nil: yok.
	Update *ota.Updater
}

// Accounts kullanıcı hesabı oluşturucudur.
type Accounts interface {
	CreateUser(ctx context.Context, a priv.UserCreateArgs) error
}

// Server HTTP işleyicisidir.
type Server struct {
	Deps
	ctx     context.Context
	static  *Static
	mux     *http.ServeMux
	netBusy chan struct{}
}

// New sunucuyu kurar ve üreticileri events hub'ına kaydeder. ctx kapanışta
// iptal edilir; açık WebSocket'ler o zaman kapanır.
func New(ctx context.Context, d Deps) *Server {
	s := &Server{Deps: d, ctx: ctx, static: NewStatic(d.WebDir), mux: http.NewServeMux(), netBusy: make(chan struct{}, 1)}
	if s.SetupDone == nil {
		if s.Setup != nil {
			s.SetupDone = s.Setup.Done
		} else {
			s.SetupDone = func() bool { return true }
		}
	}
	s.routes()
	d.Hub.Register("system", events.Every(2*time.Second, func(ctx context.Context) (any, error) { return d.System.Info(ctx) }))
	d.Hub.Register("processes", events.Every(2*time.Second, func(ctx context.Context) (any, error) { return d.Activity.Processes(ctx) }))
	d.Hub.Register("net", events.OnChange(3*time.Second, func(ctx context.Context) (any, error) { return d.Network.Status(ctx) }))
	return s
}

func (s *Server) routes() {
	m := s.mux
	a := s.authed
	m.HandleFunc("POST /api/v1/auth/login", s.login)
	m.HandleFunc("POST /api/v1/auth/logout", s.logout)
	m.HandleFunc("GET /api/v1/auth/local", s.local)
	m.HandleFunc("GET /api/v1/session", a(s.session))

	m.HandleFunc("GET /api/v1/system", a(s.systemInfo))
	m.HandleFunc("PUT /api/v1/system/hostname", s.admin(s.setHostname))
	m.HandleFunc("GET /api/v1/system/time", a(s.getTime))
	m.HandleFunc("PUT /api/v1/system/time", s.admin(s.setTime))
	m.HandleFunc("GET /api/v1/system/timezones", a(s.timezones))
	m.HandleFunc("GET /api/v1/system/locale", a(s.getLocale))
	m.HandleFunc("PUT /api/v1/system/locale", s.admin(s.setLocale))
	m.HandleFunc("PUT /api/v1/system/password", a(s.setPassword))
	m.HandleFunc("POST /api/v1/system/power", s.admin(s.power))

	m.HandleFunc("GET /api/v1/net", a(s.netStatus))
	m.HandleFunc("POST /api/v1/net/wifi/scan", s.admin(s.wifiScan))
	m.HandleFunc("POST /api/v1/net/wifi/connect", s.admin(s.wifiConnect))
	m.HandleFunc("POST /api/v1/net/wifi/forget", s.admin(s.wifiForget))
	m.HandleFunc("PUT /api/v1/net/wifi", s.admin(s.wifiEnable))

	m.HandleFunc("GET /api/v1/prefs", a(s.getPrefs))
	m.HandleFunc("PUT /api/v1/prefs", a(s.putPrefs))

	// İlk açılış: kimlik gerekmez, yalnız kurulum bitmemişken ve yerel ağdan.
	m.HandleFunc("GET /api/v1/setup", s.setupOnly(s.setupState))
	m.HandleFunc("POST /api/v1/setup/wifi/scan", s.setupOnly(s.wifiScan))
	m.HandleFunc("POST /api/v1/setup/complete", s.setupOnly(s.setupComplete))

	m.HandleFunc("GET /api/v1/apps", a(s.appsList))
	m.HandleFunc("POST /api/v1/apps/{id}/launch", a(s.appLaunch))

	m.HandleFunc("GET /api/v1/processes", a(s.processes))
	m.HandleFunc("POST /api/v1/processes/{pid}/signal", a(s.signal))

	m.HandleFunc("GET /api/v1/notifications", a(s.notifications))
	m.HandleFunc("POST /api/v1/notifications/{id}/read", a(s.notificationRead))
	m.HandleFunc("DELETE /api/v1/notifications", a(s.notificationsClear))

	m.HandleFunc("GET /api/v1/jobs/{id}", a(s.job))

	m.HandleFunc("GET /api/v1/term", a(s.termList))
	m.HandleFunc("POST /api/v1/term", a(s.termCreate))
	m.HandleFunc("DELETE /api/v1/term/{id}", a(s.termClose))
	m.HandleFunc("POST /api/v1/term/{id}/ticket", a(s.termTicket))
	m.HandleFunc("GET /api/v1/term/{id}", s.termWS)
	m.HandleFunc("GET /api/v1/events", a(s.eventsWS))
	m.HandleFunc("GET /api/v1/vnc", s.admin(s.vncWS))
	m.HandleFunc("GET /api/v1/browser/vnc", s.admin(s.browserWS))
	m.HandleFunc("GET /api/v1/system/update", a(s.updateStatus))
	m.HandleFunc("POST /api/v1/system/update/check", a(s.updateCheck))
	m.HandleFunc("POST /api/v1/system/update/apply", s.admin(s.updateApply))
	m.HandleFunc("GET /api/v1/screen", a(s.screenState))
	m.HandleFunc("PUT /api/v1/screen", s.admin(s.screenSet))

	m.HandleFunc("GET /api/v1/files", a(s.filesList))
	m.HandleFunc("GET /api/v1/files/raw", a(s.filesRaw))
	m.HandleFunc("PUT /api/v1/files/raw", a(s.filesUpload))
	m.HandleFunc("POST /api/v1/files/op", a(s.filesOp))
	m.HandleFunc("POST /api/v1/files/run", a(s.filesRun))

	m.HandleFunc("GET /api/v1/python", a(s.pythonList))
	m.HandleFunc("POST /api/v1/python/{op}", a(s.pythonOp))
	m.HandleFunc("GET /api/v1/projects", a(s.projectList))
	m.HandleFunc("GET /api/v1/projects/templates", a(s.projectTemplates))
	m.HandleFunc("POST /api/v1/projects", a(s.projectCreate))
	m.HandleFunc("POST /api/v1/projects/{name}/run", a(s.projectRun))
	m.HandleFunc("DELETE /api/v1/projects/{name}", a(s.projectDelete))
	m.HandleFunc("GET /api/v1/agents", a(s.agentList))
	m.HandleFunc("POST /api/v1/agents/{id}/{op}", a(s.agentOp))

	m.HandleFunc("GET /api/v1/camera", a(s.cameraStatus))
	m.HandleFunc("GET /api/v1/camera/stream", a(s.cameraStream))
	m.HandleFunc("POST /api/v1/camera/photo", a(s.cameraPhoto))
	m.HandleFunc("GET /api/v1/gpio", a(s.gpioBoard))
	m.HandleFunc("POST /api/v1/gpio/{n}", a(s.gpioSet))
	m.HandleFunc("GET /api/v1/plugins", a(s.pluginList))
	m.HandleFunc("GET /api/v1/plugins/template.zip", a(s.pluginTemplate))
	m.HandleFunc("POST /api/v1/plugins/import", a(s.pluginImport))
	m.HandleFunc("GET /api/v1/plugins/{id}/export.zip", a(s.pluginExport))
	m.HandleFunc("GET /api/v1/plugins/{id}/asset/{path...}", a(s.pluginAsset))
	m.HandleFunc("POST /api/v1/plugins/{id}/{op}", a(s.pluginOp))
	m.HandleFunc("DELETE /api/v1/plugins/{id}", a(s.pluginDelete))
	m.HandleFunc("/plugin/{id}/", a(s.pluginProxy))

	m.HandleFunc("/api/", func(w http.ResponseWriter, r *http.Request) {
		writeErr(w, apierr.NotFound("Uç nokta bulunamadı."))
	})
	m.HandleFunc("GET /{$}", func(w http.ResponseWriter, r *http.Request) { s.static.Serve(w, r, "index.html") })
	// Tarayıcıların kendiliğinden istediği simge: MakerOS logosu.
	m.HandleFunc("GET /favicon.ico", func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "/shell/icons/logo.svg", http.StatusMovedPermanently)
	})
	// Web arayüzü (uzak tarayıcı varsayılanı): kısa adres /web.
	// PWA: manifest ve service worker kökten sunulur (SW kapsamı "/" olabilsin).
	m.HandleFunc("GET /manifest.webmanifest", func(w http.ResponseWriter, r *http.Request) { s.static.Serve(w, r, "manifest.webmanifest") })
	m.HandleFunc("GET /sw.js", func(w http.ResponseWriter, r *http.Request) { s.static.Serve(w, r, "sw.js") })
	m.HandleFunc("GET /web", func(w http.ResponseWriter, r *http.Request) { s.static.Serve(w, r, "web/index.html") })
	m.HandleFunc("GET /web/{$}", func(w http.ResponseWriter, r *http.Request) { http.Redirect(w, r, "/web", http.StatusMovedPermanently) })
	m.HandleFunc("GET /shell/", func(w http.ResponseWriter, r *http.Request) {
		s.static.Serve(w, r, strings.TrimPrefix(r.URL.Path, "/shell/"))
	})
}

// ServeHTTP: kurtarma → güvenlik başlıkları → CSRF → yönlendirici.
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	start := time.Now()
	sw := &statusWriter{ResponseWriter: w, status: 200}
	defer func() {
		if rec := recover(); rec != nil {
			if rec == http.ErrAbortHandler {
				panic(rec)
			}
			s.Log.Error("işleyici çöktü", "path", r.URL.Path, "panic", rec, "stack", string(debug.Stack()))
			if !sw.wrote {
				writeErr(sw, apierr.Internal("İstek tamamlanamadı."))
			}
		}
		s.Log.Debug("istek", "method", r.Method, "path", r.URL.Path, "status", sw.status, "dur", time.Since(start).Round(time.Microsecond))
	}()
	h := sw.Header()
	h.Set("X-Content-Type-Options", "nosniff")
	h.Set("Referrer-Policy", "same-origin")
	h.Set("X-Frame-Options", "SAMEORIGIN")
	if strings.HasPrefix(r.URL.Path, "/api/") {
		h.Set("Cache-Control", "no-store")
		switch r.Method {
		case http.MethodPost, http.MethodPut, http.MethodDelete, http.MethodPatch:
			if r.Header.Get("X-Maker") != "1" {
				writeErr(sw, apierr.Forbidden("Geçersiz istek (X-Maker başlığı eksik)."))
				return
			}
		}
	}
	s.mux.ServeHTTP(sw, r)
}

type statusWriter struct {
	http.ResponseWriter
	status int
	wrote  bool
}

func (w *statusWriter) WriteHeader(code int) {
	if !w.wrote {
		w.status, w.wrote = code, true
	}
	w.ResponseWriter.WriteHeader(code)
}

func (w *statusWriter) Write(b []byte) (int, error) {
	w.wrote = true
	return w.ResponseWriter.Write(b)
}

// Unwrap, http.ResponseController ve WebSocket hijack için.
func (w *statusWriter) Unwrap() http.ResponseWriter { return w.ResponseWriter }

// --- ara katmanlar ---------------------------------------------------------

type ctxKey struct{}

func sessionOf(r *http.Request) *auth.Session {
	s, _ := r.Context().Value(ctxKey{}).(*auth.Session)
	return s
}

func (s *Server) authed(h http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		sess, ok := s.Sessions.FromRequest(r)
		if !ok {
			writeErr(w, apierr.Unauthorized("Giriş gerekli."))
			return
		}
		h(w, r.WithContext(context.WithValue(r.Context(), ctxKey{}, sess)))
	}
}

func (s *Server) admin(h http.HandlerFunc) http.HandlerFunc {
	return s.authed(func(w http.ResponseWriter, r *http.Request) {
		if !sessionOf(r).Admin {
			writeErr(w, apierr.Forbidden("Bu işlem yönetici yetkisi gerektirir."))
			return
		}
		h(w, r)
	})
}

// --- yardımcılar -----------------------------------------------------------

type errBody struct {
	Error struct {
		Code    string `json:"code"`
		Message string `json:"message"`
	} `json:"error"`
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	b, err := json.Marshal(v)
	if err != nil {
		status, b = 500, []byte(`{"error":{"code":"internal","message":"Yanıt üretilemedi."}}`)
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_, _ = w.Write(b)
}

func writeErr(w http.ResponseWriter, err error) {
	e, ok := apierr.As(err)
	if !ok {
		switch {
		case errors.Is(err, context.DeadlineExceeded):
			e = apierr.Unavailable("İşlem zaman aşımına uğradı.")
		default:
			e = apierr.Internal(err.Error())
		}
	}
	var body errBody
	body.Error.Code, body.Error.Message = e.Code, e.Message
	writeJSON(w, e.Status, body)
}

func noContent(w http.ResponseWriter) { w.WriteHeader(http.StatusNoContent) }

// decode JSON gövdesini (en fazla 1 MB) çözer.
func decode(w http.ResponseWriter, r *http.Request, v any) bool {
	r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
	dec := json.NewDecoder(r.Body)
	if err := dec.Decode(v); err != nil && !errors.Is(err, io.EOF) {
		writeErr(w, apierr.Invalid("Geçersiz JSON gövdesi."))
		return false
	}
	return true
}

func clientIP(r *http.Request) string {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return host
}

// originOK, Origin başlığının Host ile aynı olmasını şart koşar (WS için zorunlu).
func originOK(r *http.Request) bool {
	o := r.Header.Get("Origin")
	if o == "" {
		return false
	}
	u, err := url.Parse(o)
	if err != nil || u.Host == "" {
		return false
	}
	return strings.EqualFold(u.Host, r.Host)
}
