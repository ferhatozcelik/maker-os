package termproc

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"sync"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/ucred"
)

// ErrPeer, soketin karşı ucu beklenen kullanıcı değil (soket ele geçirilmiş olabilir).
var ErrPeer = errors.New("terminal hizmetinin kullanıcısı beklenen kullanıcı değil")

// RemoteError, makerd-user'ın 'e' çerçevesiyle bildirdiği hatadır.
type RemoteError string

func (e RemoteError) Error() string { return string(e) }

type remote struct {
	conn   net.Conn
	pid    int
	pr     *io.PipeReader
	wmu    sync.Mutex
	exited chan struct{}
	err    error
	once   sync.Once
}

// Dial makerd-user soketine bağlanır, karşı ucun UID'sini denetler
// (wantUID < 0 ise denetlemez) ve yeni bir PTY açar.
func Dial(ctx context.Context, path string, wantUID int, cols, rows uint16) (Proc, error) {
	return DialOpen(ctx, path, wantUID, Open{Cols: cols, Rows: rows})
}

// DialOpen Dial gibidir; Open.Cmd doluysa PTY içinde o komut çalışır.
func DialOpen(ctx context.Context, path string, wantUID int, o Open) (Proc, error) {
	conn, br, err := dialRaw(ctx, path, wantUID, o)
	if err != nil {
		return nil, err
	}
	typ, p, err := ReadFrame(br)
	if err != nil {
		conn.Close()
		return nil, fmt.Errorf("terminal hizmeti yanıt vermedi: %w", err)
	}
	switch typ {
	case FramePid:
	case FrameError:
		conn.Close()
		return nil, RemoteError(p)
	default:
		conn.Close()
		return nil, fmt.Errorf("beklenmeyen çerçeve %q", typ)
	}
	var op Opened
	_ = json.Unmarshal(p, &op)
	_ = conn.SetDeadline(time.Time{})
	pr, pw := io.Pipe()
	r := &remote{conn: conn, pid: op.Pid, pr: pr, exited: make(chan struct{})}
	go r.readLoop(br, pw)
	return r, nil
}

// dialRaw bağlanır, karşı ucu doğrular ve açma çerçevesini gönderir.
func dialRaw(ctx context.Context, path string, wantUID int, o Open) (net.Conn, *bufio.Reader, error) {
	var d net.Dialer
	conn, err := d.DialContext(ctx, "unix", path)
	if err != nil {
		return nil, nil, err
	}
	if wantUID >= 0 {
		c, err := ucred.Of(conn)
		if err != nil || c.UID != uint32(wantUID) {
			conn.Close()
			return nil, nil, ErrPeer
		}
	}
	dl := time.Now().Add(10 * time.Second)
	if t, ok := ctx.Deadline(); ok && t.Before(dl) {
		dl = t
	}
	_ = conn.SetDeadline(dl)
	if err := WriteJSON(conn, FrameOpen, o); err != nil {
		conn.Close()
		return nil, nil, err
	}
	return conn, bufio.NewReaderSize(conn, 64<<10), nil
}

func (r *remote) readLoop(br *bufio.Reader, pw *io.PipeWriter) {
	defer close(r.exited)
	for {
		typ, p, err := ReadFrame(br)
		if err != nil {
			r.err = fmt.Errorf("terminal hizmeti bağlantısı koptu: %w", err)
			pw.CloseWithError(io.EOF)
			return
		}
		switch typ {
		case FrameData:
			if _, err := pw.Write(p); err != nil {
				r.conn.Close()
			}
		case FrameExit:
			var ex Exit
			_ = json.Unmarshal(p, &ex)
			if ex.Code != 0 {
				r.err = fmt.Errorf("kabuk %d koduyla çıktı", ex.Code)
			}
			pw.CloseWithError(io.EOF)
			return
		}
	}
}

func (r *remote) Read(p []byte) (int, error) { return r.pr.Read(p) }

func (r *remote) Write(p []byte) (int, error) {
	r.wmu.Lock()
	defer r.wmu.Unlock()
	n := 0
	for len(p) > 0 {
		k := min(len(p), 32<<10)
		if err := WriteFrame(r.conn, FrameData, p[:k]); err != nil {
			return n, err
		}
		n += k
		p = p[k:]
	}
	return n, nil
}

func (r *remote) Resize(cols, rows uint16) error {
	r.wmu.Lock()
	defer r.wmu.Unlock()
	return WriteJSON(r.conn, FrameResize, Size{Cols: cols, Rows: rows})
}

func (r *remote) Hangup() {
	r.wmu.Lock()
	err := WriteFrame(r.conn, FrameHangup, nil)
	r.wmu.Unlock()
	if err != nil {
		r.conn.Close()
		return
	}
	go func() {
		select {
		case <-r.exited:
		case <-time.After(HangupGrace + 3*time.Second):
			r.conn.Close() // sunucu bağlantı kopunca süreci kapatır
		}
	}()
}

func (r *remote) Wait() error { <-r.exited; return r.err }
func (r *remote) Pid() int    { return r.pid }

func (r *remote) Close() error {
	var err error
	r.once.Do(func() {
		_ = r.pr.Close()
		err = r.conn.Close()
	})
	return err
}
