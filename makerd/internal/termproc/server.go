package termproc

import (
	"bufio"
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"net"
	"os/exec"
	"sync"
	"sync/atomic"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/ucred"
)

// Server, makerd-user'ın PTY sunucusudur: her bağlantı bir terminal sürecidir.
type Server struct {
	Log *slog.Logger
	// Allowed izinli çağıran UID'leridir (makerd ve root).
	Allowed map[uint32]bool
	// Command yeni kabuk komutunu hazırlar.
	Command func() (*exec.Cmd, error)
	// Max aynı anda açık süreç sınırı (0: 16).
	Max int
	// Home dosya işlemlerinin göreli yol kökü (kullanıcının ev dizini).
	Home string

	active atomic.Int32
}

// Serve bağlantıları ctx bitene kadar kabul eder.
func (s *Server) Serve(ctx context.Context, ln net.Listener) error {
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	var wg sync.WaitGroup
	defer wg.Wait()
	for {
		conn, err := ln.Accept()
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			if ne, ok := err.(net.Error); ok && ne.Timeout() {
				continue
			}
			return err
		}
		wg.Add(1)
		go func() {
			defer wg.Done()
			s.handle(ctx, conn)
		}()
	}
}

func (s *Server) handle(ctx context.Context, conn net.Conn) {
	defer conn.Close()
	c, err := ucred.Of(conn)
	if err != nil || !s.Allowed[c.UID] {
		s.Log.Warn("izinsiz bağlantı reddedildi", "uid", c.UID, "pid", c.PID)
		_ = WriteFrame(conn, FrameError, []byte("Bu işlem için yetkiniz yok."))
		return
	}
	max := s.Max
	if max <= 0 {
		max = 16
	}
	if int(s.active.Add(1)) > max {
		s.active.Add(-1)
		_ = WriteFrame(conn, FrameError, []byte("Terminal oturumu sınırına ulaşıldı."))
		return
	}
	defer s.active.Add(-1)

	_ = conn.SetDeadline(time.Now().Add(10 * time.Second))
	br := bufio.NewReaderSize(conn, 64<<10)
	typ, p, err := ReadFrame(br)
	if err != nil || typ != FrameOpen {
		return
	}
	var op Open
	_ = json.Unmarshal(p, &op)
	if op.Mode != "" {
		_ = conn.SetDeadline(time.Time{})
		s.serveOp(ctx, conn, br, op)
		return
	}
	sz := Size{Cols: op.Cols, Rows: op.Rows}
	if sz.Cols == 0 || sz.Cols > 1000 {
		sz.Cols = 80
	}
	if sz.Rows == 0 || sz.Rows > 1000 {
		sz.Rows = 24
	}
	cmd, err := WithCmd(s.Command, op.Cmd)()
	if err != nil {
		_ = WriteFrame(conn, FrameError, []byte("Kabuk hazırlanamadı: "+err.Error()))
		return
	}
	proc, err := StartLocal(cmd, sz.Cols, sz.Rows)
	if err != nil {
		_ = WriteFrame(conn, FrameError, []byte("Kabuk başlatılamadı: "+err.Error()))
		return
	}
	defer proc.Close()
	_ = conn.SetDeadline(time.Time{})
	var wmu sync.Mutex
	send := func(typ byte, b []byte) error {
		wmu.Lock()
		defer wmu.Unlock()
		return WriteFrame(conn, typ, b)
	}
	b, _ := json.Marshal(Opened{Pid: proc.Pid()})
	if send(FramePid, b) != nil {
		proc.Hangup()
		_ = proc.Wait()
		return
	}
	s.Log.Info("terminal açıldı", "pid", proc.Pid(), "caller_uid", c.UID)

	// Girdi: makerd koparsa ya da kapat isterse süreç kapatılır.
	go func() {
		for {
			typ, p, err := ReadFrame(br)
			if err != nil {
				proc.Hangup()
				return
			}
			switch typ {
			case FrameData:
				_, _ = proc.Write(p)
			case FrameResize:
				var sz Size
				if json.Unmarshal(p, &sz) == nil && sz.Cols > 0 && sz.Rows > 0 && sz.Cols <= 1000 && sz.Rows <= 1000 {
					_ = proc.Resize(sz.Cols, sz.Rows)
				}
			case FrameHangup:
				proc.Hangup()
			}
		}
	}()
	go func() {
		<-ctx.Done()
		proc.Hangup()
	}()
	buf := make([]byte, 32<<10)
	for {
		n, err := proc.Read(buf)
		if n > 0 {
			if send(FrameData, buf[:n]) != nil {
				proc.Hangup()
			}
		}
		if err != nil {
			break
		}
	}
	werr := proc.Wait()
	b, _ = json.Marshal(Exit{Code: ExitCode(werr)})
	_ = send(FrameExit, b)
	s.Log.Info("terminal kapandı", "pid", proc.Pid())
}

// serveOp PTY dışı kipleri (exec, read, write, list, ...) yürütür.
func (s *Server) serveOp(ctx context.Context, conn net.Conn, br *bufio.Reader, op Open) {
	fail := func(err error) { _ = WriteFrame(conn, FrameError, []byte(err.Error())) }
	exit := func(code int) { _ = WriteJSON(conn, FrameExit, Exit{Code: code}) }
	home := s.Home
	if home == "" {
		home = HomeOf("/etc/passwd")
	}
	switch op.Mode {
	case "exec":
		cmd, err := WithCmd(s.Command, op.Cmd)()
		if err != nil {
			fail(err)
			return
		}
		code, err := RunPiped(ctx, cmd, &frameReader{r: br}, frameWriter{conn})
		if err != nil {
			fail(err)
			return
		}
		exit(code)
	case "read":
		f, err := OpenRead(home, op.Path)
		if err != nil {
			fail(err)
			return
		}
		defer f.Close()
		if _, err := io.Copy(frameWriter{conn}, f); err != nil {
			return
		}
		exit(0)
	case "write":
		if err := WriteFileAtomic(home, op.Path, &frameReader{r: br}); err != nil {
			fail(err)
			return
		}
		exit(0)
	default:
		b, err := FSOp(home, op)
		if err != nil {
			fail(err)
			return
		}
		if WriteFrame(conn, FrameData, b) == nil {
			exit(0)
		}
	}
}

// frameWriter yazılanları 'd' çerçevelerine böler.
type frameWriter struct{ w io.Writer }

func (f frameWriter) Write(p []byte) (int, error) {
	n := 0
	for len(p) > 0 {
		c := p
		if len(c) > 256<<10 {
			c = c[:256<<10]
		}
		if err := WriteFrame(f.w, FrameData, c); err != nil {
			return n, err
		}
		n += len(c)
		p = p[len(c):]
	}
	return n, nil
}

// frameReader 'd' çerçevelerini akış olarak okur; 'k' (veya bağlantı sonu) akış sonudur.
type frameReader struct {
	r    *bufio.Reader
	pend []byte
}

func (f *frameReader) Read(p []byte) (int, error) {
	if len(f.pend) > 0 {
		n := copy(p, f.pend)
		f.pend = f.pend[n:]
		return n, nil
	}
	for {
		typ, b, err := ReadFrame(f.r)
		if err != nil {
			return 0, io.EOF
		}
		switch typ {
		case FrameData:
			if len(b) == 0 {
				continue
			}
			n := copy(p, b)
			f.pend = b[n:]
			return n, nil
		case FrameHangup:
			return 0, io.EOF
		}
	}
}
