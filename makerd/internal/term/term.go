// Package term: PTY terminal oturumları. POST /term oturum ve 30 sn'lik tek
// kullanımlık bilet üretir; WS /term/{id}?ticket= bağlanır. Bir oturuma aynı
// anda birden çok istemci bağlanabilir (ör. telefondaki web arayüzü ve Pi'nin
// kendi ekranı): çıktı hepsine yayılır, her biri yazabilir; boyutu son
// yeniden boyutlandıran belirler. Bağlı istemci kalmazsa oturum Linger süresi
// yaşar; çıktı 256 KB'lık halka tamponda tutulur ve bağlanınca gönderilir.
package term

import (
	"context"
	"encoding/json"
	"log/slog"
	"sort"
	"sync"
	"time"

	"github.com/coder/websocket"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/auth"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

// Varsayılanlar (api.md).
const (
	DefaultMax    = 6
	DefaultLinger = 30 * time.Minute
	TicketTTL     = 30 * time.Second
	RingSize      = 256 << 10
)

// Spawner, kullanıcı için verilen boyutta bir terminal süreci başlatır
// (yerel PTY veya makerd-user üzerinden; bkz. Local ve Remote).
type Spawner func(user string, cols, rows uint16, cmd string) (termproc.Proc, error)

type ticket struct {
	id, user string
	exp      time.Time
}

// Manager oturumları yönetir.
type Manager struct {
	Log    *slog.Logger
	Max    int
	Linger time.Duration
	Spawn  Spawner

	mu       sync.Mutex
	sessions map[string]*Session
	tickets  map[string]ticket
	now      func() time.Time
}

// NewManager varsayılan sınırlarla bir yönetici oluşturur.
func NewManager(log *slog.Logger, spawn Spawner) *Manager {
	return &Manager{Log: log, Max: DefaultMax, Linger: DefaultLinger, Spawn: spawn,
		sessions: map[string]*Session{}, tickets: map[string]ticket{}, now: time.Now}
}

// Opts yeni oturum seçenekleridir.
type Opts struct {
	Cmd   string // boşsa giriş kabuğu; doluysa "kabuk -lc Cmd"
	Title string
}

// Info oturum listesindeki bir satırdır.
type Info struct {
	ID      string    `json:"id"`
	Title   string    `json:"title"`
	Cmd     string    `json:"cmd,omitempty"`
	Created time.Time `json:"created"`
	Clients int       `json:"clients"`
	Pid     int       `json:"pid"`
}

type client struct {
	out  chan []byte
	kick chan struct{}
}

// Session tek bir PTY oturumudur.
type Session struct {
	ID      string
	User    string
	Title   string
	Cmd     string
	Created time.Time
	proc    termproc.Proc
	done    chan struct{}

	mu      sync.Mutex
	ring    *Ring
	clients map[*client]struct{}
	timer   *time.Timer
	exited  bool
}

func clampSize(v int, def uint16) uint16 {
	if v <= 0 || v > 1000 {
		return def
	}
	return uint16(v)
}

// Create yeni oturum başlatır.
// Create giriş kabuğuyla yeni oturum açar.
func (m *Manager) Create(user string, cols, rows int) (*Session, error) {
	return m.CreateWith(user, cols, rows, Opts{})
}

// CreateWith seçeneklerle yeni oturum açar.
func (m *Manager) CreateWith(user string, cols, rows int, o Opts) (*Session, error) {
	m.mu.Lock()
	if len(m.sessions) >= m.Max {
		m.mu.Unlock()
		return nil, apierr.Conflict("En fazla 6 terminal oturumu açılabilir.")
	}
	m.mu.Unlock()
	cmd := o.Cmd
	if cmd != "" {
		// Komut bitince çıktı okunabilsin: sonuç açıkça yazılır, oturum etkileşimli kabukla sürer.
		cmd = cmd + "\n" + `c=$?; if [ "$c" = 0 ]; then printf '\n\033[32m✔ Tamamlandı\033[0m\n'; else printf '\n\033[31m✘ Hata (çıkış kodu %s)\033[0m — yukarıdaki çıktıya bakın.\n' "$c"; fi; exec "${SHELL:-/bin/sh}" -l`
	}
	proc, err := m.Spawn(user, clampSize(cols, 80), clampSize(rows, 24), cmd)
	if err != nil {
		if _, ok := apierr.As(err); ok {
			return nil, err
		}
		return nil, apierr.Internal("Kabuk başlatılamadı: " + err.Error())
	}
	if o.Title == "" {
		o.Title = "kabuk"
	}
	s := &Session{ID: auth.RandomToken(12), User: user, Title: o.Title, Cmd: o.Cmd, Created: m.now(), proc: proc,
		done: make(chan struct{}), ring: NewRing(RingSize), clients: map[*client]struct{}{}}
	m.mu.Lock()
	if len(m.sessions) >= m.Max { // yarış: aynı anda iki istek
		m.mu.Unlock()
		s.kill()
		return nil, apierr.Conflict("En fazla 6 terminal oturumu açılabilir.")
	}
	m.sessions[s.ID] = s
	m.mu.Unlock()
	s.mu.Lock()
	s.timer = time.AfterFunc(m.Linger, func() { m.Close(s.ID) })
	s.mu.Unlock()
	go s.readLoop()
	go func() {
		_ = proc.Wait()
		<-s.done
		proc.Close()
		m.mu.Lock()
		delete(m.sessions, s.ID)
		m.mu.Unlock()
		s.mu.Lock()
		if s.timer != nil {
			s.timer.Stop()
		}
		s.mu.Unlock()
		m.Log.Info("terminal oturumu kapandı", "id", s.ID, "user", user)
	}()
	m.Log.Info("terminal oturumu açıldı", "id", s.ID, "user", user, "pid", proc.Pid())
	return s, nil
}

// Get kullanıcının oturumunu döndürür.
func (m *Manager) Get(id, user string) (*Session, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	s, ok := m.sessions[id]
	if !ok || s.User != user {
		return nil, false
	}
	return s, true
}

// List kullanıcının açık oturumlarıdır (en yeni sonda).
func (m *Manager) List(user string) []Info {
	m.mu.Lock()
	ss := make([]*Session, 0, len(m.sessions))
	for _, s := range m.sessions {
		if s.User == user {
			ss = append(ss, s)
		}
	}
	m.mu.Unlock()
	sort.Slice(ss, func(i, j int) bool { return ss[i].Created.Before(ss[j].Created) })
	out := make([]Info, 0, len(ss))
	for _, s := range ss {
		out = append(out, Info{ID: s.ID, Title: s.Title, Cmd: s.Cmd, Created: s.Created, Clients: s.Clients(), Pid: s.proc.Pid()})
	}
	return out
}

// Count açık oturum sayısıdır.
func (m *Manager) Count() int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return len(m.sessions)
}

// Ticket oturum için tek kullanımlık bilet üretir.
func (m *Manager) Ticket(id, user string) (string, error) {
	if _, ok := m.Get(id, user); !ok {
		return "", apierr.NotFound("Terminal oturumu bulunamadı.")
	}
	t := auth.RandomToken(24)
	now := m.now()
	m.mu.Lock()
	for k, v := range m.tickets {
		if now.After(v.exp) {
			delete(m.tickets, k)
		}
	}
	m.tickets[t] = ticket{id: id, user: user, exp: now.Add(TicketTTL)}
	m.mu.Unlock()
	return t, nil
}

// Redeem bileti tüketir; geçerliyse oturumu döndürür.
func (m *Manager) Redeem(t, id string) (*Session, bool) {
	m.mu.Lock()
	tk, ok := m.tickets[t]
	if ok {
		delete(m.tickets, t)
	}
	m.mu.Unlock()
	if !ok || tk.id != id || m.now().After(tk.exp) {
		return nil, false
	}
	return m.Get(id, tk.user)
}

// Close oturumu sonlandırır.
func (m *Manager) Close(id string) {
	m.mu.Lock()
	s, ok := m.sessions[id]
	m.mu.Unlock()
	if ok {
		s.kill()
	}
}

// CloseAll tüm oturumları sonlandırır (kapanışta).
func (m *Manager) CloseAll() {
	m.mu.Lock()
	list := make([]*Session, 0, len(m.sessions))
	for _, s := range m.sessions {
		list = append(list, s)
	}
	m.mu.Unlock()
	for _, s := range list {
		s.kill()
	}
}

func (s *Session) kill() { s.proc.Hangup() }

// dropLocked yavaş veya kopan istemciyi çıkarır.
func (s *Session) dropLocked(c *client) {
	if _, ok := s.clients[c]; ok {
		delete(s.clients, c)
		close(c.kick)
	}
}

// Clients bağlı istemci sayısıdır.
func (s *Session) Clients() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.clients)
}

func (s *Session) readLoop() {
	buf := make([]byte, 32<<10)
	for {
		n, err := s.proc.Read(buf)
		if n > 0 {
			data := append([]byte(nil), buf[:n]...)
			s.mu.Lock()
			s.ring.Write(data)
			for c := range s.clients {
				select {
				case c.out <- data:
				default: // yavaş istemci: düşür; yeniden bağlanınca halka tampon gönderilir
					s.dropLocked(c)
				}
			}
			s.mu.Unlock()
		}
		if err != nil {
			s.mu.Lock()
			s.exited = true
			s.mu.Unlock()
			close(s.done)
			return
		}
	}
}

type control struct {
	Type string `json:"type"`
	Cols int    `json:"cols"`
	Rows int    `json:"rows"`
}

// Attach WebSocket'i oturuma ekler ve bağlantı bitene kadar bloklar. Diğer
// istemciler bağlı kalır (ortak terminal).
func (m *Manager) Attach(ctx context.Context, s *Session, c *websocket.Conn) {
	c.SetReadLimit(1 << 20)
	cl := &client{out: make(chan []byte, 256), kick: make(chan struct{})}
	s.mu.Lock()
	if s.timer != nil {
		s.timer.Stop()
		s.timer = nil
	}
	s.clients[cl] = struct{}{}
	replay := s.ring.Bytes()
	s.mu.Unlock()
	out, kick := cl.out, cl.kick

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	defer func() {
		s.mu.Lock()
		s.dropLocked(cl)
		if len(s.clients) == 0 && !s.exited && s.timer == nil {
			s.timer = time.AfterFunc(m.Linger, func() { m.Close(s.ID) })
		}
		s.mu.Unlock()
	}()

	if len(replay) > 0 {
		if err := c.Write(ctx, websocket.MessageBinary, replay); err != nil {
			return
		}
	}
	go func() {
		defer cancel()
		for {
			typ, data, err := c.Read(ctx)
			if err != nil {
				return
			}
			if typ == websocket.MessageBinary {
				if _, err := s.proc.Write(data); err != nil {
					return
				}
				continue
			}
			var ctl control
			if json.Unmarshal(data, &ctl) == nil && ctl.Type == "resize" {
				_ = s.proc.Resize(clampSize(ctl.Cols, 80), clampSize(ctl.Rows, 24))
			}
		}
	}()
	ping := time.NewTicker(30 * time.Second)
	defer ping.Stop()
	for {
		select {
		case data := <-out:
			if err := c.Write(ctx, websocket.MessageBinary, data); err != nil {
				return
			}
		case <-kick:
			c.Close(4000, "slow")
			return
		case <-s.done:
			// Kalan çıktıyı boşalt.
			for {
				select {
				case data := <-out:
					_ = c.Write(ctx, websocket.MessageBinary, data)
					continue
				default:
				}
				break
			}
			c.Close(websocket.StatusNormalClosure, "exit")
			return
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
