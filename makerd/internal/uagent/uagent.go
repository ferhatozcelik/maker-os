// Package uagent, kullanıcı adına iş yapan temsilcidir: komut çalıştırma ve
// ev dizinindeki dosya işlemleri. Gerçek sistemde iş makerd-user@<kullanıcı>
// hizmetinde (kullanıcının kendi hesabı ve izinleriyle) yapılır; mock modda ve
// `mos` CLI'da aynı işlevler süreç içinde çalışır.
package uagent

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os/exec"
	"path/filepath"
	"strconv"
	"syscall"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

// Agent kullanıcı bağlamında çalışan işlemlerdir.
type Agent interface {
	// Exec cmd'yi kullanıcının giriş kabuğuyla (-l -c) çalıştırır; çıktı out'a yazılır.
	Exec(ctx context.Context, user, cmd string, stdin io.Reader, out io.Writer) (int, error)
	// FS list/mkdir/rename/remove/stat işlemi; JSON yanıt.
	FS(ctx context.Context, user string, op termproc.Open) ([]byte, error)
	// Read dosyayı w'ye yazar.
	Read(ctx context.Context, user, path string, w io.Writer) error
	// Write r'yi dosyaya (atomik) yazar.
	Write(ctx context.Context, user, path string, r io.Reader) error
}

// Output Exec'i çalıştırıp çıktıyı döndürür.
func Output(ctx context.Context, a Agent, user, cmd string) ([]byte, int, error) {
	var b bytes.Buffer
	code, err := a.Exec(ctx, user, cmd, nil, &b)
	return b.Bytes(), code, err
}

// JSON Exec çıktısını v'ye çözer (çıkış kodu 0 olmalı).
func JSON(ctx context.Context, a Agent, user, cmd string, v any) error {
	out, code, err := Output(ctx, a, user, cmd)
	if err != nil {
		return err
	}
	if code != 0 {
		return apierr.Unavailable("Komut başarısız (" + strconv.Itoa(code) + "): " + tail(out, 300))
	}
	return json.Unmarshal(out, v)
}

func tail(b []byte, n int) string {
	if len(b) > n {
		b = b[len(b)-n:]
	}
	return string(bytes.TrimSpace(b))
}

// --- süreç içi -----------------------------------------------------------

// Local işleri bu süreçte yapar (mock, CLI ve makerd-user yokken kendi kullanıcısı).
type Local struct {
	Command func() (*exec.Cmd, error) // giriş kabuğu (termproc.LoginCommand)
	Home    string
}

func (l Local) home() string {
	if l.Home != "" {
		return l.Home
	}
	return termproc.HomeOf("/etc/passwd")
}

func (l Local) Exec(ctx context.Context, _ string, cmd string, stdin io.Reader, out io.Writer) (int, error) {
	c, err := termproc.WithCmd(l.Command, cmd)()
	if err != nil {
		return -1, err
	}
	return termproc.RunPiped(ctx, c, stdin, out)
}

func (l Local) FS(_ context.Context, _ string, op termproc.Open) ([]byte, error) {
	b, err := termproc.FSOp(l.home(), op)
	return b, wrap(err)
}

func (l Local) Read(_ context.Context, _ string, path string, w io.Writer) error {
	f, err := termproc.OpenRead(l.home(), path)
	if err != nil {
		return wrap(err)
	}
	defer f.Close()
	_, err = io.Copy(w, f)
	return err
}

func (l Local) Write(_ context.Context, _ string, path string, r io.Reader) error {
	return wrap(termproc.WriteFileAtomic(l.home(), path, r))
}

// --- makerd-user ---------------------------------------------------------

// Remote işleri kullanıcının makerd-user hizmetinde yapar. Soket yoksa ve
// kullanıcı Self ise Fallback kullanılır.
type Remote struct {
	Dir      string
	UIDOf    func(user string) (int, error)
	Self     string
	Fallback Agent
}

func (r Remote) dial(ctx context.Context, user string, o termproc.Open) (*termproc.RemoteOp, Agent, error) {
	uid, err := r.UIDOf(user)
	if err != nil {
		return nil, nil, apierr.Forbidden("Kullanıcı bulunamadı.")
	}
	op, err := termproc.DialOp(ctx, filepath.Join(r.Dir, strconv.Itoa(uid)+".sock"), uid, o)
	if err == nil {
		return op, nil, nil
	}
	if (errors.Is(err, syscall.ENOENT) || errors.Is(err, syscall.ECONNREFUSED)) && r.Fallback != nil && user == r.Self {
		return nil, r.Fallback, nil
	}
	if errors.Is(err, termproc.ErrPeer) {
		return nil, nil, apierr.Forbidden("Kullanıcı hizmeti soketi güvenilir değil.")
	}
	return nil, nil, apierr.Unavailable("Kullanıcı hizmeti (makerd-user@" + user + ") çalışmıyor.")
}

func (r Remote) Exec(ctx context.Context, user, cmd string, stdin io.Reader, out io.Writer) (int, error) {
	op, fb, err := r.dial(ctx, user, termproc.Open{Mode: "exec", Cmd: cmd})
	if err != nil {
		return -1, err
	}
	if fb != nil {
		return fb.Exec(ctx, user, cmd, stdin, out)
	}
	go func() {
		if stdin != nil {
			_, _ = io.Copy(op, stdin)
		}
		_ = op.CloseInput()
	}()
	code, err := op.Drain(out)
	return code, wrap(err)
}

func (r Remote) FS(ctx context.Context, user string, o termproc.Open) ([]byte, error) {
	op, fb, err := r.dial(ctx, user, o)
	if err != nil {
		return nil, err
	}
	if fb != nil {
		return fb.FS(ctx, user, o)
	}
	var b bytes.Buffer
	_, err = op.Drain(&b)
	return b.Bytes(), wrap(err)
}

func (r Remote) Read(ctx context.Context, user, path string, w io.Writer) error {
	op, fb, err := r.dial(ctx, user, termproc.Open{Mode: "read", Path: path})
	if err != nil {
		return err
	}
	if fb != nil {
		return fb.Read(ctx, user, path, w)
	}
	_, err = op.Drain(w)
	return wrap(err)
}

func (r Remote) Write(ctx context.Context, user, path string, rd io.Reader) error {
	op, fb, err := r.dial(ctx, user, termproc.Open{Mode: "write", Path: path})
	if err != nil {
		return err
	}
	if fb != nil {
		return fb.Write(ctx, user, path, rd)
	}
	if _, err := io.Copy(op, rd); err != nil {
		return err
	}
	if err := op.CloseInput(); err != nil {
		return err
	}
	_, err = op.Drain(nil)
	return wrap(err)
}

// wrap kullanıcı hatalarını API hatasına çevirir.
func wrap(err error) error {
	if err == nil {
		return nil
	}
	var fe termproc.ErrFS
	if errors.As(err, &fe) {
		return apierr.Invalid(string(fe))
	}
	var re termproc.RemoteError
	if errors.As(err, &re) {
		return apierr.Invalid(string(re))
	}
	return err
}
