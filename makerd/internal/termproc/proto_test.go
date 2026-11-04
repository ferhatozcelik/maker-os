package termproc

import (
	"bytes"
	"errors"
	"testing"
)

func TestFrames(t *testing.T) {
	var b bytes.Buffer
	_ = WriteFrame(&b, FrameData, []byte("merhaba"))
	_ = WriteJSON(&b, FrameResize, Size{Cols: 80, Rows: 24})
	_ = WriteFrame(&b, FrameHangup, nil)
	typ, p, err := ReadFrame(&b)
	if err != nil || typ != FrameData || string(p) != "merhaba" {
		t.Fatalf("%c %q %v", typ, p, err)
	}
	typ, p, _ = ReadFrame(&b)
	if typ != FrameResize || string(p) != `{"cols":80,"rows":24}` {
		t.Fatalf("%c %q", typ, p)
	}
	typ, p, _ = ReadFrame(&b)
	if typ != FrameHangup || len(p) != 0 {
		t.Fatal("hangup")
	}
	if _, _, err := ReadFrame(bytes.NewReader([]byte{'d', 0xff, 0, 0, 0})); !errors.Is(err, ErrFrameTooBig) {
		t.Fatalf("size limit: %v", err)
	}
	if _, _, err := ReadFrame(bytes.NewReader([]byte{'d', 0, 0, 0, 9, 'a'})); err == nil {
		t.Fatal("short frame accepted")
	}
}
