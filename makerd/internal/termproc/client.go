package termproc

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"time"
)

// RemoteOp makerd-user'da PTY dışı bir işlem açar. Dönen bağlantı üzerinden
// çağıran veri gönderir (SendData/CloseInput) ve Drain ile sonucu okur.
type RemoteOp struct {
	conn net.Conn
	br   *bufio.Reader
}

// DialOp işlemi başlatır.
func DialOp(ctx context.Context, path string, wantUID int, o Open) (*RemoteOp, error) {
	conn, br, err := dialRaw(ctx, path, wantUID, o)
	if err != nil {
		return nil, err
	}
	if dl, ok := ctx.Deadline(); ok {
		_ = conn.SetDeadline(dl)
	} else {
		_ = conn.SetDeadline(time.Time{})
	}
	go func() { <-ctx.Done(); conn.Close() }()
	return &RemoteOp{conn: conn, br: br}, nil
}

// Write girdiyi 'd' çerçeveleri olarak gönderir.
func (r *RemoteOp) Write(p []byte) (int, error) { return frameWriter{r.conn}.Write(p) }

// CloseInput girdinin bittiğini bildirir ('k').
func (r *RemoteOp) CloseInput() error { return WriteFrame(r.conn, FrameHangup, nil) }

// Drain çıktıyı w'ye kopyalar ve çıkış kodunu döner. Karşı taraf hata
// çerçevesi gönderirse RemoteError döner.
func (r *RemoteOp) Drain(w io.Writer) (int, error) {
	defer r.conn.Close()
	for {
		typ, p, err := ReadFrame(r.br)
		if err != nil {
			return -1, fmt.Errorf("kullanıcı hizmeti bağlantısı koptu: %w", err)
		}
		switch typ {
		case FrameData:
			if w != nil {
				if _, err := w.Write(p); err != nil {
					return -1, err
				}
			}
		case FrameError:
			return -1, RemoteError(p)
		case FrameExit:
			var e Exit
			_ = json.Unmarshal(p, &e)
			return e.Code, nil
		}
	}
}
