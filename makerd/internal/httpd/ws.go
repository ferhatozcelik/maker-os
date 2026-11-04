package httpd

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/coder/websocket"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/term"
)

func (s *Server) accept(w http.ResponseWriter, r *http.Request) (*websocket.Conn, bool) {
	if !originOK(r) {
		writeErr(w, apierr.Forbidden("Geçersiz Origin."))
		return nil, false
	}
	// Origin yukarıda Host ile karşılaştırıldı.
	c, err := websocket.Accept(w, r, &websocket.AcceptOptions{InsecureSkipVerify: true, CompressionMode: websocket.CompressionDisabled})
	if err != nil {
		return nil, false
	}
	return c, true
}

// --- terminal --------------------------------------------------------------

type termResp struct {
	ID     string `json:"id"`
	Ticket string `json:"ticket"`
}

func (s *Server) termCreate(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Cols  int    `json:"cols"`
		Rows  int    `json:"rows"`
		ID    string `json:"id"`    // var olan (başka cihazda açılmış olabilir) oturuma bağlanmak için bilet
		Cmd   string `json:"cmd"`   // isteğe bağlı: kabuk yerine bu komut (ör. "uv run main.py")
		Title string `json:"title"` // sekme başlığı
	}
	if !decode(w, r, &req) {
		return
	}
	user := sessionOf(r).User
	id := req.ID
	if id == "" {
		if len(req.Cmd) > 4096 || len(req.Title) > 80 {
			writeErr(w, apierr.Invalid("Komut veya başlık çok uzun."))
			return
		}
		sess, err := s.Term.CreateWith(user, req.Cols, req.Rows, term.Opts{Cmd: req.Cmd, Title: req.Title})
		if err != nil {
			writeErr(w, err)
			return
		}
		id = sess.ID
	}
	t, err := s.Term.Ticket(id, user)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, termResp{ID: id, Ticket: t})
}

// termList kullanıcının açık terminal oturumları (web ve Pi ekranı ortak).
func (s *Server) termList(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, 200, s.Term.List(sessionOf(r).User))
}

// termClose oturumu sonlandırır.
func (s *Server) termClose(w http.ResponseWriter, r *http.Request) {
	if _, ok := s.Term.Get(r.PathValue("id"), sessionOf(r).User); !ok {
		writeErr(w, apierr.NotFound("Terminal oturumu bulunamadı."))
		return
	}
	s.Term.Close(r.PathValue("id"))
	noContent(w)
}

func (s *Server) termTicket(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	t, err := s.Term.Ticket(id, sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, termResp{ID: id, Ticket: t})
}

func (s *Server) termWS(w http.ResponseWriter, r *http.Request) {
	if !originOK(r) {
		writeErr(w, apierr.Forbidden("Geçersiz Origin."))
		return
	}
	sess, ok := s.Term.Redeem(r.URL.Query().Get("ticket"), r.PathValue("id"))
	if !ok {
		writeErr(w, apierr.Forbidden("Geçersiz veya süresi dolmuş bilet."))
		return
	}
	c, ok := s.accept(w, r)
	if !ok {
		return
	}
	defer c.CloseNow()
	ctx, cancel := mergeCtx(r.Context(), s.ctx)
	defer cancel()
	s.Term.Attach(ctx, sess, c)
}

// --- events ----------------------------------------------------------------

type eventOp struct {
	Op     string   `json:"op"`
	Topics []string `json:"topics"`
}

func (s *Server) eventsWS(w http.ResponseWriter, r *http.Request) {
	c, ok := s.accept(w, r)
	if !ok {
		return
	}
	defer c.CloseNow()
	c.SetReadLimit(64 << 10)
	ctx, cancel := mergeCtx(r.Context(), s.ctx)
	defer cancel()
	cl := s.Hub.NewClient()
	defer s.Hub.Remove(cl)
	go func() {
		defer cancel()
		for {
			_, data, err := c.Read(ctx)
			if err != nil {
				return
			}
			var op eventOp
			if json.Unmarshal(data, &op) != nil {
				continue
			}
			switch op.Op {
			case "sub":
				s.Hub.Subscribe(cl, op.Topics)
			case "unsub":
				s.Hub.Unsubscribe(cl, op.Topics)
			}
		}
	}()
	ping := time.NewTicker(30 * time.Second)
	defer ping.Stop()
	for {
		select {
		case b := <-cl.Send:
			wctx, wc := context.WithTimeout(ctx, 10*time.Second)
			err := c.Write(wctx, websocket.MessageText, b)
			wc()
			if err != nil {
				return
			}
		case <-ping.C:
			pctx, pc := context.WithTimeout(ctx, 10*time.Second)
			err := c.Ping(pctx)
			pc()
			if err != nil {
				return
			}
		case <-ctx.Done():
			c.Close(websocket.StatusGoingAway, "")
			return
		}
	}
}

// mergeCtx, a veya b iptal olunca iptal olan bağlam döndürür.
func mergeCtx(a, b context.Context) (context.Context, context.CancelFunc) {
	ctx, cancel := context.WithCancel(a)
	stop := context.AfterFunc(b, cancel)
	return ctx, func() { stop(); cancel() }
}
