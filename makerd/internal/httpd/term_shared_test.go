package httpd

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"testing"
	"time"

	"github.com/coder/websocket"
)

// Web arayüzü ile Pi ekranındaki terminal aynı oturuma bağlanır; çıktı ikisine de
// gider ve ikisi de yazabilir.
func TestSharedTerminal(t *testing.T) {
	e := newEnv(t)
	cookie := e.login()
	h := map[string]string{"Cookie": cookie, "X-Maker": "1"}
	r := e.do("POST", "/api/v1/term", `{"cols":80,"rows":24,"title":"ortak"}`, h)
	var tr termResp
	_ = json.NewDecoder(r.Body).Decode(&tr)
	if r.StatusCode != 200 || tr.ID == "" {
		t.Fatalf("create %d", r.StatusCode)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	dial := func(ticket string) *websocket.Conn {
		c, _, err := websocket.Dial(ctx, wsURL(e, "/api/v1/term/"+tr.ID+"?ticket="+ticket), &websocket.DialOptions{HTTPHeader: http.Header{"Origin": {e.srv.URL}}})
		if err != nil {
			t.Fatal(err)
		}
		return c
	}
	web := dial(tr.Ticket)
	defer web.CloseNow()
	// İkinci cihaz: oturum listesinden bulur, yeni bilet alır.
	var list []struct {
		ID      string `json:"id"`
		Title   string `json:"title"`
		Clients int    `json:"clients"`
	}
	_ = json.NewDecoder(e.do("GET", "/api/v1/term", "", h).Body).Decode(&list)
	if len(list) != 1 || list[0].ID != tr.ID || list[0].Title != "ortak" || list[0].Clients != 1 {
		t.Fatalf("list %+v", list)
	}
	var t2 termResp
	_ = json.NewDecoder(e.do("POST", "/api/v1/term", `{"id":"`+tr.ID+`"}`, h).Body).Decode(&t2)
	screen := dial(t2.Ticket)
	defer screen.CloseNow()
	waitFor := func(c *websocket.Conn, want string) {
		var got []byte
		for !bytes.Contains(got, []byte(want)) {
			_, b, err := c.Read(ctx)
			if err != nil {
				t.Fatalf("%q beklenirken: %v (gelen %q)", want, err, got)
			}
			got = append(got, b...)
		}
	}
	_ = web.Write(ctx, websocket.MessageBinary, []byte("echo WEB$((1+1))\n"))
	waitFor(web, "WEB2")
	waitFor(screen, "WEB2")
	_ = screen.Write(ctx, websocket.MessageBinary, []byte("echo EKRAN$((2+3))\n"))
	waitFor(web, "EKRAN5")
	waitFor(screen, "EKRAN5")
	// Komutla açılan oturum
	r = e.do("POST", "/api/v1/term", `{"cmd":"echo KOMUT_TAMAM","title":"is"}`, h)
	var t3 termResp
	_ = json.NewDecoder(r.Body).Decode(&t3)
	c3, _, err := websocket.Dial(ctx, wsURL(e, "/api/v1/term/"+t3.ID+"?ticket="+t3.Ticket), &websocket.DialOptions{HTTPHeader: http.Header{"Origin": {e.srv.URL}}})
	if err != nil {
		t.Fatal(err)
	}
	defer c3.CloseNow()
	waitFor(c3, "KOMUT_TAMAM")
	if r := e.do("DELETE", "/api/v1/term/"+tr.ID, "", h); r.StatusCode != 204 {
		t.Fatalf("close %d", r.StatusCode)
	}
}
