package termproc

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
)

// Çerçeve türleri. Çerçeve: 1 bayt tür + 4 bayt (big-endian) uzunluk + yük.
//
//	makerd → makerd-user: 'o' aç {cols,rows} (ilk çerçeve), 'd' girdi, 'r' boyut {cols,rows}, 'k' kapat (SIGHUP)
//	makerd-user → makerd: 'p' {pid} (açıldı), 'e' hata metni, 'd' çıktı, 'x' {code} (süreç bitti)
const (
	FrameOpen   = 'o'
	FramePid    = 'p'
	FrameError  = 'e'
	FrameData   = 'd'
	FrameResize = 'r'
	FrameHangup = 'k'
	FrameExit   = 'x'
)

// MaxFrame en büyük çerçeve yüküdür.
const MaxFrame = 1 << 20

// Size boyut çerçevesinin gövdesidir.
type Size struct {
	Cols uint16 `json:"cols"`
	Rows uint16 `json:"rows"`
}

// Open ilk çerçevenin gövdesidir. Mode boşsa PTY açılır (Cmd boşsa giriş
// kabuğu, doluysa "kabuk -lc Cmd"). Diğer kipler kullanıcı adına iş yapar:
//
//	exec    Cmd borularla (PTY yok) çalışır; makerd'nin 'd' çerçeveleri stdin,
//	        'k' stdin sonu; çıktı (stdout+stderr) 'd', sonunda 'x' {code}.
//	read    Path dosyası 'd' parçaları olarak gelir, sonra 'x'.
//	write   makerd 'd' parçaları gönderir, 'k' ile bitirir; yanıt 'x'.
//	list, mkdir, rename (Path→To), remove, stat: tek 'd' JSON yanıtı + 'x'.
type Open struct {
	Cols uint16 `json:"cols"`
	Rows uint16 `json:"rows"`
	Mode string `json:"mode,omitempty"`
	Cmd  string `json:"cmd,omitempty"`
	Path string `json:"path,omitempty"`
	To   string `json:"to,omitempty"`
}

// Opened 'p' çerçevesinin gövdesidir.
type Opened struct {
	Pid int `json:"pid"`
}

// Exit 'x' çerçevesinin gövdesidir.
type Exit struct {
	Code int `json:"code"`
}

// ErrFrameTooBig, sınırı aşan çerçeve.
var ErrFrameTooBig = errors.New("çerçeve çok büyük")

// WriteFrame tek çerçeveyi tek Write ile yazar.
func WriteFrame(w io.Writer, typ byte, payload []byte) error {
	if len(payload) > MaxFrame {
		return ErrFrameTooBig
	}
	buf := make([]byte, 5+len(payload))
	buf[0] = typ
	binary.BigEndian.PutUint32(buf[1:5], uint32(len(payload)))
	copy(buf[5:], payload)
	_, err := w.Write(buf)
	return err
}

// WriteJSON v'yi JSON çerçevesi olarak yazar.
func WriteJSON(w io.Writer, typ byte, v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	return WriteFrame(w, typ, b)
}

// ReadFrame bir çerçeve okur.
func ReadFrame(r io.Reader) (byte, []byte, error) {
	var hdr [5]byte
	if _, err := io.ReadFull(r, hdr[:]); err != nil {
		return 0, nil, err
	}
	n := binary.BigEndian.Uint32(hdr[1:])
	if n > MaxFrame {
		return 0, nil, ErrFrameTooBig
	}
	p := make([]byte, n)
	if _, err := io.ReadFull(r, p); err != nil {
		return 0, nil, fmt.Errorf("eksik çerçeve: %w", err)
	}
	return hdr[0], p, nil
}
