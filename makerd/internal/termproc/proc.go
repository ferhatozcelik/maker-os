// Package termproc, terminal süreçlerinin ortak arayüzüdür: yerel PTY
// (creack/pty) ve makerd-user'a unix soketi üzerinden çerçeveli protokolle
// bağlanan uzak PTY. makerd-user da sunucu tarafını buradan kullanır.
package termproc

import (
	"errors"
	"os"
	"os/exec"
	"sync"
	"syscall"
	"time"

	"github.com/creack/pty"
)

// Proc çalışan bir terminal sürecidir.
type Proc interface {
	// Read PTY çıktısını okur; süreç bitince hata (io.EOF/EIO) döner.
	Read(p []byte) (int, error)
	// Write PTY girdisine yazar.
	Write(p []byte) (int, error)
	Resize(cols, rows uint16) error
	// Hangup süreç grubuna SIGHUP gönderir; kısa süre içinde çıkmazsa öldürür.
	Hangup()
	// Wait süreç bitene kadar bekler.
	Wait() error
	// Close kaynakları bırakır (Wait'ten sonra çağrılır).
	Close() error
	Pid() int
}

// HangupGrace, SIGHUP'tan SIGKILL'e kadar beklenen süredir.
var HangupGrace = 2 * time.Second

type local struct {
	cmd    *exec.Cmd
	f      *os.File
	exited chan struct{}
	err    error
	once   sync.Once
}

// StartLocal komutu yeni bir PTY'de (ayrı oturum ve süreç grubu) başlatır.
func StartLocal(cmd *exec.Cmd, cols, rows uint16) (Proc, error) {
	f, err := pty.StartWithSize(cmd, &pty.Winsize{Cols: cols, Rows: rows})
	if err != nil {
		return nil, err
	}
	l := &local{cmd: cmd, f: f, exited: make(chan struct{})}
	go func() {
		l.err = cmd.Wait()
		close(l.exited)
	}()
	return l, nil
}

func (l *local) Read(p []byte) (int, error)  { return l.f.Read(p) }
func (l *local) Write(p []byte) (int, error) { return l.f.Write(p) }
func (l *local) Pid() int                    { return l.cmd.Process.Pid }
func (l *local) Wait() error                 { <-l.exited; return l.err }

func (l *local) Resize(cols, rows uint16) error {
	return pty.Setsize(l.f, &pty.Winsize{Cols: cols, Rows: rows})
}

func (l *local) Hangup() {
	pid := l.cmd.Process.Pid
	_ = syscall.Kill(-pid, syscall.SIGHUP)
	go func() {
		select {
		case <-l.exited:
		case <-time.After(HangupGrace):
			_ = syscall.Kill(-pid, syscall.SIGKILL)
			_ = l.cmd.Process.Kill()
		}
	}()
}

func (l *local) Close() error {
	var err error
	l.once.Do(func() { err = l.f.Close() })
	return err
}

// ExitCode Wait hatasından çıkış kodunu çıkarır (sinyalle bitişte 128+n).
func ExitCode(err error) int {
	if err == nil {
		return 0
	}
	var ee *exec.ExitError
	if errors.As(err, &ee) {
		if ws, ok := ee.Sys().(syscall.WaitStatus); ok && ws.Signaled() {
			return 128 + int(ws.Signal())
		}
		return ee.ExitCode()
	}
	return -1
}
