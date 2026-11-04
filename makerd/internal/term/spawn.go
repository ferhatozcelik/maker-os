package term

import (
	"context"
	"errors"
	"os/exec"
	"path/filepath"
	"strconv"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

// DefaultUserDir makerd-user soketlerinin dizinidir (<uid>.sock).
const DefaultUserDir = "/run/makeros/user"

// Local, komutu makerd sürecinin içinde (aynı kullanıcı ve kum havuzuyla)
// yeni bir PTY'de başlatır. Mock modda ve makerd-user yokken kullanılır.
func Local(command func() (*exec.Cmd, error)) Spawner {
	return func(_ string, cols, rows uint16, c string) (termproc.Proc, error) {
		cmd, err := termproc.WithCmd(command, c)()
		if err != nil {
			return nil, apierr.Internal("Kabuk hazırlanamadı: " + err.Error())
		}
		return termproc.StartLocal(cmd, cols, rows)
	}
}

// Remote, oturumu kullanıcının makerd-user@<user>.service hizmetinde açar
// (dir/<uid>.sock). Soket yoksa ve kullanıcı selfUser ise fallback kullanılır
// (makerd'nin kendi kullanıcısı; kum havuzu içinde, sudo çalışmaz).
func Remote(dir string, uidOf func(user string) (int, error), selfUser string, fallback Spawner) Spawner {
	return func(user string, cols, rows uint16, c string) (termproc.Proc, error) {
		uid, err := uidOf(user)
		if err != nil {
			return nil, apierr.Forbidden("Kullanıcı bulunamadı.")
		}
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		p, err := termproc.DialOpen(ctx, filepath.Join(dir, strconv.Itoa(uid)+".sock"), uid, termproc.Open{Cols: cols, Rows: rows, Cmd: c})
		if err == nil {
			return p, nil
		}
		if errors.Is(err, syscall.ENOENT) || errors.Is(err, syscall.ECONNREFUSED) {
			if fallback != nil && user == selfUser {
				return fallback(user, cols, rows, c)
			}
			return nil, apierr.Unavailable("Kullanıcı terminal hizmeti (makerd-user@" + user + ") çalışmıyor.")
		}
		if errors.Is(err, termproc.ErrPeer) {
			return nil, apierr.Forbidden("Terminal hizmeti soketi güvenilir değil.")
		}
		var re termproc.RemoteError
		if errors.As(err, &re) {
			return nil, apierr.Unavailable(string(re))
		}
		return nil, apierr.Unavailable("Terminal hizmetine bağlanılamadı: " + err.Error())
	}
}
