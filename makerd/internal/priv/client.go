package priv

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net"
	"time"
)

// Client makerd tarafındaki istemcidir.
type Client struct {
	Socket  string
	Timeout time.Duration
}

// NewClient bir istemci oluşturur.
func NewClient(socket string) *Client { return &Client{Socket: socket, Timeout: 15 * time.Second} }

// Call komutu çalıştırır; out nil değilse yanıt verisi ona çözülür.
// makerd-priv'in döndürdüğü hatalar *Error türündedir.
func (c *Client) Call(ctx context.Context, cmd string, args, out any) error {
	ctx, cancel := context.WithTimeout(ctx, c.Timeout)
	defer cancel()
	var d net.Dialer
	conn, err := d.DialContext(ctx, "unix", c.Socket)
	if err != nil {
		return fmt.Errorf("makerd-priv'e bağlanılamadı: %w", err)
	}
	defer conn.Close()
	if dl, ok := ctx.Deadline(); ok {
		_ = conn.SetDeadline(dl)
	}
	raw, err := json.Marshal(args)
	if err != nil {
		return err
	}
	req, _ := json.Marshal(Request{Cmd: cmd, Args: raw})
	// Sunucu isteği okumadan yanıtlayıp kapatabilir (ör. izinsiz çağıran → forbidden): yazma
	// "broken pipe" ile başarısız olsa da gönderilmiş yanıt okunur; asıl hata odur.
	_, werr := conn.Write(append(req, '\n'))
	line, err := bufio.NewReader(conn).ReadBytes('\n')
	if err != nil {
		if werr != nil {
			return werr
		}
		return fmt.Errorf("makerd-priv yanıt vermedi: %w", err)
	}
	var resp Response
	if err := json.Unmarshal(line, &resp); err != nil {
		return err
	}
	if !resp.OK {
		if resp.Error == nil {
			resp.Error = &Error{Code: "internal", Message: "bilinmeyen hata"}
		}
		return resp.Error
	}
	if out != nil && len(resp.Data) > 0 {
		return json.Unmarshal(resp.Data, out)
	}
	return nil
}
