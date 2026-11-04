package priv

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net"
	"os/user"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/ucred"
)

// Peer, SO_PEERCRED ile öğrenilen çağırandır.
type Peer struct {
	UID, GID uint32
	PID      int32
	User     string
}

// Handler bir komutu işler.
type Handler func(ctx context.Context, peer Peer, args json.RawMessage) (any, error)

// Server makerd-priv sunucusudur.
type Server struct {
	Log      *slog.Logger
	Allowed  map[uint32]bool // izinli çağıran UID'leri
	Handlers map[string]Handler
	// Idle sıfırdan büyükse bu süre istek gelmezse Serve döner (soket etkinleştirmede çıkış).
	Idle time.Duration

	last atomic.Int64
}

// Serve dinleyiciden bağlantı kabul eder.
func (s *Server) Serve(ctx context.Context, ln net.Listener) error {
	s.last.Store(time.Now().UnixNano())
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	var wg sync.WaitGroup
	var active atomic.Int32
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	if s.Idle > 0 {
		go func() {
			t := time.NewTicker(time.Second)
			defer t.Stop()
			for {
				select {
				case <-ctx.Done():
					return
				case <-t.C:
					if active.Load() == 0 && time.Since(time.Unix(0, s.last.Load())) > s.Idle {
						s.Log.Info("boşta kalındı, çıkılıyor")
						cancel()
						return
					}
				}
			}
		}()
	}
	for {
		conn, err := ln.Accept()
		if err != nil {
			wg.Wait()
			if ctx.Err() != nil {
				return nil
			}
			return err
		}
		active.Add(1)
		wg.Add(1)
		go func() {
			defer wg.Done()
			defer active.Add(-1)
			s.handle(ctx, conn)
			s.last.Store(time.Now().UnixNano())
		}()
	}
}

// PeerOf, SO_PEERCRED ile bağlantının karşı ucunu (ve kullanıcı adını) döndürür.
func PeerOf(conn net.Conn) (Peer, error) {
	c, err := ucred.Of(conn)
	if err != nil {
		return Peer{}, err
	}
	p := Peer{UID: c.UID, GID: c.GID, PID: c.PID}
	if u, err := user.LookupId(strconv.Itoa(int(c.UID))); err == nil {
		p.User = u.Username
	}
	return p, nil
}

func (s *Server) handle(ctx context.Context, conn net.Conn) {
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(30 * time.Second))
	reply := func(r Response) {
		b, _ := json.Marshal(r)
		_, _ = conn.Write(append(b, '\n'))
	}
	fail := func(code, msg string) { reply(Response{Error: &Error{Code: code, Message: msg}}) }

	peer, err := PeerOf(conn)
	if err != nil {
		s.Log.Warn("çağıran kimliği alınamadı", "err", err)
		return
	}
	if !s.Allowed[peer.UID] {
		s.Log.Warn("izinsiz çağıran reddedildi", "uid", peer.UID, "pid", peer.PID)
		fail("forbidden", "Bu işlem için yetkiniz yok.")
		return
	}
	line, err := bufio.NewReader(&limitedReader{r: conn, n: 64 << 10}).ReadBytes('\n')
	if err != nil {
		fail("invalid", "İstek okunamadı.")
		return
	}
	var req Request
	if err := json.Unmarshal(line, &req); err != nil {
		fail("invalid", "Geçersiz JSON.")
		return
	}
	h, ok := s.Handlers[req.Cmd]
	if !ok {
		s.Log.Warn("bilinmeyen komut", "cmd", req.Cmd, "uid", peer.UID)
		fail("invalid", "Bilinmeyen komut.")
		return
	}
	// Her çağrı günlüğe yazılır (argümanlar parola içerebildiği için yazılmaz).
	s.Log.Info("komut", "cmd", req.Cmd, "uid", peer.UID, "user", peer.User, "pid", peer.PID)
	out, err := h(ctx, peer, req.Args)
	if err != nil {
		var pe *Error
		if errors.As(err, &pe) {
			reply(Response{Error: pe})
		} else {
			s.Log.Error("komut başarısız", "cmd", req.Cmd, "err", err)
			fail("internal", err.Error())
		}
		return
	}
	data, _ := json.Marshal(out)
	reply(Response{OK: true, Data: data})
}

type limitedReader struct {
	r interface{ Read([]byte) (int, error) }
	n int
}

func (l *limitedReader) Read(p []byte) (int, error) {
	if l.n <= 0 {
		return 0, errors.New("istek çok büyük")
	}
	if len(p) > l.n {
		p = p[:l.n]
	}
	n, err := l.r.Read(p)
	l.n -= n
	return n, err
}
