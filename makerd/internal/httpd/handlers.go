package httpd

import (
	"context"
	"crypto/subtle"
	"io"
	"net"
	"net/http"
	"strconv"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/jobs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/network"
	"github.com/ferhatozcelik/maker-os/makerd/internal/prefs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/system"
)

// --- auth ------------------------------------------------------------------

type userResp struct {
	User      string `json:"user"`
	Admin     bool   `json:"admin"`
	SetupDone *bool  `json:"setup_done,omitempty"`
}

func (s *Server) rateLimited(w http.ResponseWriter, ip string) bool {
	if ok, wait := s.Limiter.Allow(ip); !ok {
		w.Header().Set("Retry-After", strconv.Itoa(int(wait.Seconds())+1))
		writeErr(w, apierr.New(http.StatusTooManyRequests, "rate_limited", "Çok fazla başarısız deneme. Bir dakika sonra tekrar deneyin."))
		return true
	}
	return false
}

func (s *Server) login(w http.ResponseWriter, r *http.Request) {
	ip := clientIP(r)
	if s.rateLimited(w, ip) {
		return
	}
	var req struct {
		User     string `json:"user"`
		Password string `json:"password"`
	}
	if !decode(w, r, &req) {
		return
	}
	res, err := s.Checker.Check(r.Context(), req.User, req.Password)
	if err != nil {
		s.Log.Error("parola denetlenemedi", "err", err)
		writeErr(w, apierr.Unavailable("Parola şu anda denetlenemiyor."))
		return
	}
	if !res.OK {
		s.Limiter.Fail(ip)
		s.Log.Warn("başarısız giriş", "user", req.User, "ip", ip)
		writeErr(w, apierr.Unauthorized("Kullanıcı adı veya parola hatalı."))
		return
	}
	s.Limiter.Reset(ip)
	admin := res.Admin
	if s.IsAdmin != nil {
		admin = s.IsAdmin(req.User)
	}
	_, cookie := s.Sessions.Create(req.User, admin)
	s.Sessions.SetCookie(w, r, cookie)
	s.Log.Info("giriş", "user", req.User, "ip", ip)
	writeJSON(w, 200, userResp{User: req.User, Admin: admin})
}

func (s *Server) logout(w http.ResponseWriter, r *http.Request) {
	if c, err := r.Cookie("mk_session"); err == nil {
		s.Sessions.Delete(c.Value)
	}
	s.Sessions.ClearCookie(w, r)
	noContent(w)
}

func (s *Server) local(w http.ResponseWriter, r *http.Request) {
	ip := clientIP(r)
	if s.LocalLoopbackOnly {
		if p := net.ParseIP(ip); p == nil || !p.IsLoopback() {
			writeErr(w, apierr.Forbidden("Yerel giriş yalnızca cihazın kendisinden yapılabilir."))
			return
		}
	}
	if s.rateLimited(w, ip) {
		return
	}
	tok := r.URL.Query().Get("token")
	if s.LocalToken == "" || subtle.ConstantTimeCompare([]byte(tok), []byte(s.LocalToken)) != 1 {
		s.Limiter.Fail(ip)
		writeErr(w, apierr.Forbidden("Geçersiz yerel belirteç."))
		return
	}
	if !s.SetupDone() {
		// İlk açılış: oturum açılmaz, kabuk kurulum sihirbazını gösterir.
		http.Redirect(w, r, "/?ui=desktop&local=1", http.StatusFound)
		return
	}
	user := s.localUser()
	admin := true
	if s.IsAdmin != nil {
		admin = s.IsAdmin(user)
	}
	_, cookie := s.Sessions.Create(user, admin)
	s.Sessions.SetCookie(w, r, cookie)
	// Pi'nin kendi ekranı (kiosk): masaüstü arayüzü; uzak tarayıcılar Web arayüzünü alır.
	http.Redirect(w, r, "/?ui=desktop&local=1", http.StatusFound)
}

// localUser yerel kabuğun oturum kullanıcısıdır: kurulumda oluşturulan kullanıcı, yoksa LocalUser.
func (s *Server) localUser() string {
	if s.Setup != nil {
		if u := s.Setup.PrimaryUser(); u != "" {
			return u
		}
	}
	return s.LocalUser
}

func (s *Server) session(w http.ResponseWriter, r *http.Request) {
	sess := sessionOf(r)
	done := s.SetupDone()
	writeJSON(w, 200, userResp{User: sess.User, Admin: sess.Admin, SetupDone: &done})
}

// --- system ----------------------------------------------------------------

func reply[T any](w http.ResponseWriter, v T, err error) {
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, v)
}

func (s *Server) systemInfo(w http.ResponseWriter, r *http.Request) {
	v, err := s.System.Info(r.Context())
	reply(w, v, err)
}

func (s *Server) setHostname(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Hostname string `json:"hostname"`
	}
	if !decode(w, r, &req) {
		return
	}
	if err := s.System.SetHostname(r.Context(), req.Hostname); err != nil {
		writeErr(w, err)
		return
	}
	s.Log.Info("cihaz adı değişti", "hostname", req.Hostname, "by", sessionOf(r).User)
	noContent(w)
}

func (s *Server) getTime(w http.ResponseWriter, r *http.Request) {
	v, err := s.System.Time(r.Context())
	reply(w, v, err)
}

func (s *Server) setTime(w http.ResponseWriter, r *http.Request) {
	var req system.TimeUpdate
	if !decode(w, r, &req) {
		return
	}
	if err := s.System.SetTime(r.Context(), req); err != nil {
		writeErr(w, err)
		return
	}
	v, err := s.System.Time(r.Context())
	reply(w, v, err)
}

func (s *Server) timezones(w http.ResponseWriter, r *http.Request) {
	v, err := s.System.Timezones(r.Context())
	reply(w, v, err)
}

func (s *Server) getLocale(w http.ResponseWriter, r *http.Request) {
	v, err := s.System.Locale(r.Context())
	reply(w, v, err)
}

func (s *Server) setLocale(w http.ResponseWriter, r *http.Request) {
	var req system.Locale
	if !decode(w, r, &req) {
		return
	}
	if req.Lang == "" && req.Keymap == "" {
		writeErr(w, apierr.Invalid("lang veya keymap gerekli."))
		return
	}
	if err := s.System.SetLocale(r.Context(), req); err != nil {
		writeErr(w, err)
		return
	}
	v, err := s.System.Locale(r.Context())
	reply(w, v, err)
}

func (s *Server) setPassword(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Old string `json:"old"`
		New string `json:"new"`
	}
	if !decode(w, r, &req) {
		return
	}
	user := sessionOf(r).User
	if err := s.System.SetPassword(r.Context(), user, req.Old, req.New); err != nil {
		writeErr(w, err)
		return
	}
	s.Log.Info("parola değişti", "user", user)
	noContent(w)
}

func (s *Server) power(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Action string `json:"action"`
	}
	if !decode(w, r, &req) {
		return
	}
	if req.Action != "reboot" && req.Action != "poweroff" {
		writeErr(w, apierr.Invalid("action reboot veya poweroff olmalıdır."))
		return
	}
	s.Log.Info("güç işlemi", "action", req.Action, "by", sessionOf(r).User)
	// Yanıt istemciye ulaşsın diye işlem kısa gecikmeyle yapılır.
	go func() {
		time.Sleep(time.Second)
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		if err := s.System.Power(ctx, req.Action); err != nil {
			s.Log.Error("güç işlemi başarısız", "err", err)
			s.Notify.Add("makeros.settings", "Güç işlemi başarısız", err.Error())
		}
	}()
	w.WriteHeader(http.StatusAccepted)
}

// --- net -------------------------------------------------------------------

func (s *Server) netStatus(w http.ResponseWriter, r *http.Request) {
	v, err := s.Network.Status(r.Context())
	reply(w, v, err)
}

// netLock, aynı anda tek ağ işlemi (tarama/bağlanma) çalışmasını sağlar.
func (s *Server) netLock() bool {
	select {
	case s.netBusy <- struct{}{}:
		return true
	default:
		return false
	}
}

func (s *Server) netUnlock() { <-s.netBusy }

func (s *Server) wifiScan(w http.ResponseWriter, r *http.Request) {
	if !s.netLock() {
		writeErr(w, apierr.Conflict("Bir ağ işlemi devam ediyor. İşlem durumunu bekleyin."))
		return
	}
	defer s.netUnlock()
	ctx, cancel := context.WithTimeout(r.Context(), 30*time.Second)
	defer cancel()
	v, err := s.Network.Scan(ctx)
	if v == nil && err == nil {
		v = []network.AP{}
	}
	reply(w, v, err)
}

func (s *Server) wifiConnect(w http.ResponseWriter, r *http.Request) {
	var req network.ConnectRequest
	if !decode(w, r, &req) {
		return
	}
	if !network.ValidSSID(req.SSID) {
		writeErr(w, apierr.Invalid("Ağ adı 1–32 bayt olmalıdır; Türkçe karakterler birden çok bayt kullanabilir."))
		return
	}
	if req.EAP != nil && req.EAP.Identity == "" {
		writeErr(w, apierr.Invalid("Kurumsal ağ için kimlik (identity) gerekli."))
		return
	}
	id, err := s.startConnect(sessionOf(r).User, req)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, http.StatusAccepted, map[string]string{"job": id})
}

// startConnect Wi-Fi bağlanma işini başlatır (aynı anda tek ağ işlemi).
func (s *Server) startConnect(user string, req network.ConnectRequest) (string, error) {
	if !s.netLock() {
		return "", apierr.Conflict("Bir ağ işlemi devam ediyor. İşlem durumunu bekleyin.")
	}
	id := s.Jobs.Start("net.connect", user, func(ctx context.Context, h *jobs.Handle) error {
		defer s.netUnlock()
		// HTTP yanıtı radyo değişmeden istemciye ulaşsın.
		time.Sleep(300 * time.Millisecond)
		err := s.Network.Connect(ctx, req, h.Step)
		if err != nil {
			s.Notify.Add("makeros.settings", "Wi-Fi bağlantısı başarısız", req.SSID)
		} else {
			s.Notify.Add("makeros.settings", "Wi-Fi bağlandı", req.SSID+" ağına bağlanıldı.")
			if st, e := s.Network.Status(ctx); e == nil {
				s.Hub.Publish("net", st)
			}
		}
		return err
	})
	return id, nil
}

func (s *Server) wifiForget(w http.ResponseWriter, r *http.Request) {
	var req struct {
		SSID string `json:"ssid"`
	}
	if !decode(w, r, &req) {
		return
	}
	if !network.ValidSSID(req.SSID) {
		writeErr(w, apierr.Invalid("Geçersiz ağ adı."))
		return
	}
	if err := s.Network.Forget(r.Context(), req.SSID); err != nil {
		writeErr(w, err)
		return
	}
	noContent(w)
}

func (s *Server) wifiEnable(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Enabled *bool `json:"enabled"`
	}
	if !decode(w, r, &req) {
		return
	}
	if req.Enabled == nil {
		writeErr(w, apierr.Invalid("enabled (true/false) gerekli."))
		return
	}
	if err := s.Network.SetWifiEnabled(r.Context(), *req.Enabled); err != nil {
		writeErr(w, err)
		return
	}
	s.Log.Info("Wi-Fi radyosu", "enabled", *req.Enabled, "by", sessionOf(r).User)
	st, err := s.Network.Status(r.Context())
	if err == nil {
		s.Hub.Publish("net", st)
	}
	reply(w, st, err)
}

// --- prefs -----------------------------------------------------------------

func (s *Server) getPrefs(w http.ResponseWriter, r *http.Request) {
	if s.Prefs == nil {
		writeErr(w, apierr.Unavailable("Tercih deposu yok."))
		return
	}
	v, err := s.Prefs.Get(sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	_, _ = w.Write(v)
}

func (s *Server) putPrefs(w http.ResponseWriter, r *http.Request) {
	if s.Prefs == nil {
		writeErr(w, apierr.Unavailable("Tercih deposu yok."))
		return
	}
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, prefs.MaxSize))
	if err != nil {
		writeErr(w, apierr.New(http.StatusRequestEntityTooLarge, "invalid", "Tercihler en fazla 16 KB olabilir."))
		return
	}
	v, err := s.Prefs.Put(sessionOf(r).User, body)
	if err != nil {
		writeErr(w, err)
		return
	}
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	_, _ = w.Write(v)
}

// --- apps, processes, notifications, jobs ------------------------------------

func (s *Server) appsList(w http.ResponseWriter, r *http.Request) { writeJSON(w, 200, s.Apps.List()) }

func (s *Server) appLaunch(w http.ResponseWriter, r *http.Request) {
	if err := s.Apps.Start(r.Context(), r.PathValue("id")); err != nil {
		writeErr(w, err)
		return
	}
	w.WriteHeader(http.StatusAccepted)
}

func (s *Server) processes(w http.ResponseWriter, r *http.Request) {
	v, err := s.Activity.Processes(r.Context())
	reply(w, v, err)
}

func (s *Server) signal(w http.ResponseWriter, r *http.Request) {
	pid, err := strconv.Atoi(r.PathValue("pid"))
	if err != nil || pid <= 0 {
		writeErr(w, apierr.Invalid("Geçersiz süreç numarası."))
		return
	}
	var req struct {
		Signal string `json:"signal"`
	}
	if !decode(w, r, &req) {
		return
	}
	if err := s.Activity.Signal(r.Context(), pid, req.Signal); err != nil {
		writeErr(w, err)
		return
	}
	noContent(w)
}

func (s *Server) notifications(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, 200, s.Notify.List())
}

func (s *Server) notificationRead(w http.ResponseWriter, r *http.Request) {
	if !s.Notify.MarkRead(r.PathValue("id")) {
		writeErr(w, apierr.NotFound("Bildirim bulunamadı."))
		return
	}
	noContent(w)
}

func (s *Server) notificationsClear(w http.ResponseWriter, r *http.Request) {
	s.Notify.Clear()
	noContent(w)
}

func (s *Server) job(w http.ResponseWriter, r *http.Request) {
	j, ok := s.Jobs.Get(r.PathValue("id"))
	if !ok || j.User != sessionOf(r).User {
		writeErr(w, apierr.NotFound("İş bulunamadı."))
		return
	}
	writeJSON(w, 200, j)
}
