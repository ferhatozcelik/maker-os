package httpd

import (
	"context"
	"errors"
	"net/http"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/network"
	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
	"github.com/ferhatozcelik/maker-os/makerd/internal/setup"
	"github.com/ferhatozcelik/maker-os/makerd/internal/system"
)

// setupOnly: kurulum bitmişse 409; istek yerel ağdan değilse 403. Kimlik gerekmez.
func (s *Server) setupOnly(h http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if s.Setup == nil || s.SetupDone() {
			writeErr(w, apierr.Conflict("Kurulum zaten tamamlandı."))
			return
		}
		if !setup.LocalAddr(clientIP(r)) {
			s.Log.Warn("kurulum isteği yerel ağ dışından reddedildi", "ip", clientIP(r))
			writeErr(w, apierr.Forbidden("Kurulum yalnızca yerel ağdan veya cihazın erişim noktasından yapılabilir."))
			return
		}
		h(w, r)
	}
}

type setupDefaults struct {
	Lang     string `json:"lang"`
	Keymap   string `json:"keymap"`
	Timezone string `json:"timezone"`
}

type setupStateResp struct {
	SetupDone   bool            `json:"setup_done"`
	Hostname    string          `json:"hostname"`
	Defaults    setupDefaults   `json:"defaults"`
	Languages   []system.Option `json:"languages"`
	Keymaps     []system.Option `json:"keymaps"`
	Timezones   []string        `json:"timezones"`
	Online      bool            `json:"online"`
	WifiNeeded  bool            `json:"wifi_needed"`
	Hotspot     bool            `json:"hotspot"`
	MinPassword int             `json:"min_password"`
}

func (s *Server) setupState(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	resp := setupStateResp{Hostname: "makeros", Languages: system.Languages, Keymaps: system.Keymaps,
		MinPassword: priv.MinPasswordLen, Defaults: setupDefaults{Lang: "tr_TR.UTF-8", Keymap: "trq", Timezone: "Europe/Istanbul"}}
	if l, err := s.System.Locale(ctx); err == nil {
		if l.Lang != "" && l.Lang != "C.UTF-8" {
			resp.Defaults.Lang = l.Lang
		}
		if l.Keymap != "" && l.Keymap != "us" {
			resp.Defaults.Keymap = l.Keymap
		}
	}
	if t, err := s.System.Time(ctx); err == nil && t.Timezone != "" && t.Timezone != "UTC" {
		resp.Defaults.Timezone = t.Timezone
	}
	resp.Timezones, _ = s.System.Timezones(ctx)
	if resp.Timezones == nil {
		resp.Timezones = system.FallbackTimezones
	}
	if st, err := s.Network.Status(ctx); err == nil {
		resp.Online, resp.Hotspot = st.Online, st.Hotspot.Active
		resp.WifiNeeded = !st.Online || st.Hotspot.Active
		mac := ""
		for _, i := range st.Ifaces {
			if mac == "" || i.Type == "ethernet" {
				mac = i.MAC
			}
		}
		resp.Hostname = setup.SuggestHostname(mac)
	}
	writeJSON(w, 200, resp)
}

type setupRequest struct {
	User     string                  `json:"user"`
	Password string                  `json:"password"`
	Fullname string                  `json:"fullname"`
	Hostname string                  `json:"hostname"`
	Timezone string                  `json:"timezone"`
	Lang     string                  `json:"lang"`
	Keymap   string                  `json:"keymap"`
	Wifi     *network.ConnectRequest `json:"wifi"`
}

type setupResp struct {
	User     string   `json:"user"`
	Admin    bool     `json:"admin"`
	Job      string   `json:"job,omitempty"`
	Warnings []string `json:"warnings,omitempty"`
}

func (req *setupRequest) validate() error {
	switch {
	case !priv.ValidUsername(req.User):
		return apierr.Invalid("Kullanıcı adı küçük harfle başlamalı; yalnızca a-z, 0-9, '-' ve '_' içerebilir (en fazla 32).")
	case !priv.ValidNewPassword(req.Password):
		return apierr.Invalid("Parola en az 6 karakter olmalıdır.")
	case !priv.ValidFullname(req.Fullname):
		return apierr.Invalid("Ad soyad geçersiz karakter içeriyor.")
	case !priv.ValidHostname(req.Hostname):
		return apierr.Invalid("Cihaz adı yalnızca küçük harf, rakam ve '-' içerebilir (en fazla 63).")
	case !system.ValidTimezone(req.Timezone):
		return apierr.Invalid("Geçersiz saat dilimi.")
	case !system.ValidLang(req.Lang):
		return apierr.Invalid("Geçersiz dil.")
	case !system.ValidKeymap(req.Keymap):
		return apierr.Invalid("Geçersiz klavye düzeni.")
	case req.Wifi != nil && !network.ValidSSID(req.Wifi.SSID):
		return apierr.Invalid("Ağ adı 1–32 bayt olmalıdır.")
	}
	return nil
}

// setupComplete tek seferliktir: hesap → cihaz adı/saat/dil → setup-done →
// oturum çerezi; Wi-Fi isteniyorsa yanıttan sonra iş olarak bağlanır.
func (s *Server) setupComplete(w http.ResponseWriter, r *http.Request) {
	ip := clientIP(r)
	if s.rateLimited(w, ip) {
		return
	}
	var req setupRequest
	if !decode(w, r, &req) {
		return
	}
	if err := req.validate(); err != nil {
		writeErr(w, err)
		return
	}
	if s.Accounts == nil {
		writeErr(w, apierr.Unavailable("Hesap oluşturucu yok."))
		return
	}
	unlock, err := s.Setup.Begin()
	switch {
	case errors.Is(err, setup.ErrDone):
		writeErr(w, apierr.Conflict("Kurulum zaten tamamlandı."))
		return
	case errors.Is(err, setup.ErrBusy):
		writeErr(w, apierr.Conflict("Kurulum şu anda sürüyor."))
		return
	case err != nil:
		writeErr(w, apierr.Internal("Kurulum kilidi alınamadı: "+err.Error()))
		return
	}
	defer unlock()
	ctx, cancel := context.WithTimeout(context.WithoutCancel(r.Context()), 60*time.Second)
	defer cancel()

	if err := s.Accounts.CreateUser(ctx, priv.UserCreateArgs{User: req.User, Password: req.Password, Fullname: req.Fullname}); err != nil {
		s.Log.Error("kurulum: hesap oluşturulamadı", "user", req.User, "err", err)
		s.Limiter.Fail(ip)
		writeErr(w, err)
		return
	}
	var warn []string
	if err := s.System.SetHostname(ctx, req.Hostname); err != nil {
		warn = append(warn, "Cihaz adı ayarlanamadı: "+errMsg(err))
	}
	tz := req.Timezone
	if err := s.System.SetTime(ctx, system.TimeUpdate{Timezone: &tz}); err != nil {
		warn = append(warn, "Saat dilimi ayarlanamadı: "+errMsg(err))
	}
	if err := s.System.SetLocale(ctx, system.Locale{Lang: req.Lang, Keymap: req.Keymap}); err != nil {
		warn = append(warn, "Dil/klavye ayarlanamadı: "+errMsg(err))
	}
	if err := s.Setup.MarkDone(req.User); err != nil {
		s.Log.Error("kurulum: setup-done yazılamadı", "err", err)
		writeErr(w, apierr.Internal("Kurulum durumu kaydedilemedi."))
		return
	}
	s.Limiter.Reset(ip)
	admin := true
	if s.IsAdmin != nil {
		admin = s.IsAdmin(req.User)
	}
	_, cookie := s.Sessions.Create(req.User, admin)
	s.Sessions.SetCookie(w, r, cookie)
	resp := setupResp{User: req.User, Admin: admin, Warnings: warn}
	if req.Wifi != nil {
		if id, err := s.startConnect(req.User, *req.Wifi); err == nil {
			resp.Job = id
		} else {
			resp.Warnings = append(resp.Warnings, "Wi-Fi bağlantısı başlatılamadı: "+errMsg(err))
		}
	}
	for _, m := range warn {
		s.Log.Warn("kurulum uyarısı", "msg", m)
	}
	s.Log.Info("kurulum tamamlandı", "user", req.User, "hostname", req.Hostname, "ip", ip)
	writeJSON(w, 200, resp)
}

func errMsg(err error) string {
	if e, ok := apierr.As(err); ok {
		return e.Message
	}
	return err.Error()
}
