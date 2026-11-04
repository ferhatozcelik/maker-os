package httpd

import (
	"context"
	"net"
	"net/http"
	"time"

	"github.com/coder/websocket"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/screen"
)

// vncWS, tarayıcıdaki noVNC'yi yerel VNC sunucusuna (wayvnc, yalnız
// 127.0.0.1) bağlar: WebSocket ikili çerçeveler <-> ham RFB TCP akışı
// (websockify'ın yaptığı iş). Uzak masaüstü sistemi tam denetler, bu yüzden
// yönetici oturumu ister. wayvnc kendisi hiçbir zaman dışarı açılmaz.
func (s *Server) vncWS(w http.ResponseWriter, r *http.Request) {
	if s.VNCAddr == "" {
		writeErr(w, apierr.Unavailable("Uzak masaüstü bu sistemde kapalı."))
		return
	}
	if s.Screen != nil && !s.Screen.Enabled() {
		writeErr(w, apierr.Forbidden("Ekran Modu kapalı. Ayarlar › Ekran Modu'ndan açabilirsiniz."))
		return
	}
	if !originOK(r) {
		writeErr(w, apierr.Forbidden("Geçersiz Origin."))
		return
	}
	s.bridgeRFB(w, r, s.VNCAddr, "uzak masaüstü")
}

// browserWS, MakerOS'taki "Chromium" penceresini uzak tarayıcı oturumuna bağlar (internal/browser):
// oturum ilk pencerede başlar, son pencere kapanınca bir süre sonra durur. Ekran Modu'ndan bağımsızdır.
func (s *Server) browserWS(w http.ResponseWriter, r *http.Request) {
	if s.Browser == nil {
		writeErr(w, apierr.Unavailable("Tarayıcı bu sistemde yok."))
		return
	}
	if !originOK(r) {
		writeErr(w, apierr.Forbidden("Geçersiz Origin."))
		return
	}
	if err := s.Browser.Acquire(r.Context()); err != nil {
		writeErr(w, apierr.Unavailable("Tarayıcı başlatılamadı: "+err.Error()))
		return
	}
	defer s.Browser.Release()
	s.bridgeRFB(w, r, s.Browser.Addr, "tarayıcı")
}

// bridgeRFB: WebSocket ikili çerçeveler <-> yerel RFB TCP akışı (websockify'ın yaptığı iş).
func (s *Server) bridgeRFB(w http.ResponseWriter, r *http.Request, addr, what string) {
	dctx, dcancel := context.WithTimeout(r.Context(), 3*time.Second)
	var d net.Dialer
	tc, err := d.DialContext(dctx, "tcp", addr)
	dcancel()
	if err != nil {
		writeErr(w, apierr.Unavailable("Uzak masaüstü sunucusu çalışmıyor (ekran oturumu açık mı?)."))
		return
	}
	defer tc.Close()
	if t, ok := tc.(*net.TCPConn); ok {
		_ = t.SetNoDelay(true)
	}
	c, err := websocket.Accept(w, r, &websocket.AcceptOptions{
		// noVNC "binary" alt protokolünü ister; Origin yukarıda doğrulandı.
		Subprotocols:       []string{"binary"},
		InsecureSkipVerify: true,
		CompressionMode:    websocket.CompressionDisabled,
	})
	if err != nil {
		return
	}
	defer c.CloseNow()
	c.SetReadLimit(4 << 20)
	ctx, cancel := mergeCtx(r.Context(), s.ctx)
	defer cancel()
	s.Log.Info(what+" bağlandı", "user", sessionOf(r).User, "ip", clientIP(r))
	defer s.Log.Info(what+" kapandı", "user", sessionOf(r).User)

	// TCP -> WS
	go func() {
		defer cancel()
		buf := make([]byte, 64<<10)
		for {
			n, err := tc.Read(buf)
			if n > 0 {
				wctx, wc := context.WithTimeout(ctx, 15*time.Second)
				werr := c.Write(wctx, websocket.MessageBinary, buf[:n])
				wc()
				if werr != nil {
					return
				}
			}
			if err != nil {
				return
			}
		}
	}()
	// WS -> TCP
	go func() {
		defer cancel()
		for {
			_, data, err := c.Read(ctx)
			if err != nil {
				return
			}
			if _, err := tc.Write(data); err != nil {
				return
			}
		}
	}()
	<-ctx.Done()
	c.Close(websocket.StatusNormalClosure, "")
}

// screenState: Ekran Modu açık mı, Pi'ye monitör bağlı mı (kabuk arayüzü buna göre gösterir/gizler).
func (s *Server) screenState(w http.ResponseWriter, r *http.Request) {
	if s.Screen == nil {
		writeJSON(w, 200, screen.State{Enabled: s.VNCAddr != "", Headless: true})
		return
	}
	writeJSON(w, 200, s.Screen.State())
}

func (s *Server) screenSet(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Enabled bool `json:"enabled"`
	}
	if !decode(w, r, &req) {
		return
	}
	if s.Screen == nil {
		writeErr(w, apierr.Unavailable("Ekran Modu bu sistemde yönetilemiyor."))
		return
	}
	if err := s.Screen.Set(req.Enabled); err != nil {
		writeErr(w, apierr.Internal("Ekran Modu ayarı kaydedilemedi: "+err.Error()))
		return
	}
	s.Log.Info("Ekran Modu", "enabled", req.Enabled, "user", sessionOf(r).User)
	writeJSON(w, 200, s.Screen.State())
}
