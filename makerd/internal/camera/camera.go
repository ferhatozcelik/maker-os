// Package camera, Raspberry Pi kamera modülleri (libcamera: Pi 4'te unicam+ISP,
// Pi 5'te CFE+PiSP) ve libcamera'nın tanıdığı USB kameralar içindir.
// Canlı görüntü rpicam-vid'in MJPEG çıktısıdır: tek üretici, çok izleyici; son
// izleyici ayrılınca kamera birkaç saniye içinde kapanır (CPU/güç boşa gitmez).
// Donanım yoksa (Docker önizleme) sentetik görüntü üretilir.
package camera

import (
	"bufio"
	"bytes"
	"context"
	"image"
	"image/color"
	"image/jpeg"
	"io"
	"math"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
)

// Cam bir kameradır.
type Cam struct {
	Index  int    `json:"index"`
	Sensor string `json:"sensor"` // imx708, imx500 (AI Camera), ov5647 ...
	Modes  string `json:"modes"`
	Path   string `json:"path"`
	AI     bool   `json:"ai,omitempty"` // IMX500: sensör üstü sinir ağı
}

// Status kamera durumudur.
type Status struct {
	Cams      []Cam `json:"cams"`
	Streaming bool  `json:"streaming"`
	Viewers   int   `json:"viewers"`
	Sim       bool  `json:"sim"`
	Width     int   `json:"width"`
	Height    int   `json:"height"`
	FPS       int   `json:"fps"`
}

// Source kare üreticisidir.
type Source interface {
	List(ctx context.Context) ([]Cam, error)
	// Run kareleri emit'e verir; ctx bitince döner.
	Run(ctx context.Context, w, h, fps int, emit func([]byte)) error
	// Still tam çözünürlüklü fotoğraf (akış yokken).
	Still(ctx context.Context) ([]byte, error)
	Sim() bool
}

// Hub tek üretici çok izleyici yayın merkezidir.
type Hub struct {
	Src           Source
	Width, Height int
	FPS           int
	Linger        time.Duration

	mu      sync.Mutex
	subs    map[chan []byte]struct{}
	last    []byte
	cancel  context.CancelFunc
	running bool
	idle    *time.Timer
}

// NewHub varsayılan 1280x720@30 ile kurar.
func NewHub(src Source) *Hub {
	return &Hub{Src: src, Width: 1280, Height: 720, FPS: 30, Linger: 3 * time.Second, subs: map[chan []byte]struct{}{}}
}

// Status durum.
func (h *Hub) Status(ctx context.Context) Status {
	cams, _ := h.Src.List(ctx)
	if cams == nil {
		cams = []Cam{}
	}
	h.mu.Lock()
	defer h.mu.Unlock()
	return Status{Cams: cams, Streaming: h.running, Viewers: len(h.subs), Sim: h.Src.Sim(), Width: h.Width, Height: h.Height, FPS: h.FPS}
}

// Subscribe izleyici ekler; dönen işlev ayrılır.
func (h *Hub) Subscribe() (<-chan []byte, func()) {
	ch := make(chan []byte, 2)
	h.mu.Lock()
	h.subs[ch] = struct{}{}
	if h.idle != nil {
		h.idle.Stop()
		h.idle = nil
	}
	if !h.running {
		ctx, cancel := context.WithCancel(context.Background())
		h.cancel, h.running = cancel, true
		go func() {
			_ = h.Src.Run(ctx, h.Width, h.Height, h.FPS, h.publish)
			h.mu.Lock()
			h.running = false
			for c := range h.subs {
				close(c)
				delete(h.subs, c)
			}
			h.mu.Unlock()
		}()
	}
	if h.last != nil {
		ch <- h.last
	}
	h.mu.Unlock()
	return ch, func() {
		h.mu.Lock()
		defer h.mu.Unlock()
		if _, ok := h.subs[ch]; ok {
			delete(h.subs, ch)
			close(ch)
		}
		if len(h.subs) == 0 && h.running && h.idle == nil {
			h.idle = time.AfterFunc(h.Linger, h.stopIfIdle)
		}
	}
}

func (h *Hub) stopIfIdle() {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.idle = nil
	if len(h.subs) == 0 && h.cancel != nil {
		h.cancel()
	}
}

func (h *Hub) publish(frame []byte) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.last = frame
	for c := range h.subs {
		select {
		case c <- frame:
		default: // yavaş izleyici kare atlar
		}
	}
}

// Photo akış açıksa son kareyi, değilse tam çözünürlüklü fotoğrafı döner.
func (h *Hub) Photo(ctx context.Context) ([]byte, error) {
	h.mu.Lock()
	running, last := h.running, h.last
	h.mu.Unlock()
	if running && last != nil {
		return last, nil
	}
	return h.Src.Still(ctx)
}

// --- rpicam ----------------------------------------------------------------

// Rpicam rpicam-apps arka ucudur.
type Rpicam struct{}

var reCam = regexp.MustCompile(`^\s*(\d+)\s*:\s*(\S+)\s*\[([^\]]*)\]\s*\(([^)]*)\)`)

// List `rpicam-hello --list-cameras` çıktısını çözer.
func (Rpicam) List(ctx context.Context) ([]Cam, error) {
	ctx, cancel := context.WithTimeout(ctx, 8*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, "rpicam-hello", "--list-cameras").CombinedOutput()
	if err != nil && len(out) == 0 {
		return nil, apierr.Unavailable("rpicam-hello çalışmadı: " + err.Error())
	}
	var cams []Cam
	for _, l := range strings.Split(string(out), "\n") {
		if m := reCam.FindStringSubmatch(l); m != nil {
			c := Cam{Sensor: m[2], Modes: m[3], Path: m[4], AI: strings.Contains(m[2], "imx500")}
			c.Index, _ = strconv.Atoi(m[1])
			cams = append(cams, c)
		}
	}
	return cams, nil
}

func (Rpicam) Sim() bool { return false }

// Run rpicam-vid MJPEG akışını JPEG karelerine böler (SOI..EOI).
func (Rpicam) Run(ctx context.Context, w, h, fps int, emit func([]byte)) error {
	cmd := exec.CommandContext(ctx, "rpicam-vid", "-t", "0", "-n", "--codec", "mjpeg", "--quality", "80",
		"--width", itoa(w), "--height", itoa(h), "--framerate", itoa(fps), "-o", "-")
	out, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	if err := cmd.Start(); err != nil {
		return apierr.Unavailable("rpicam-vid başlatılamadı: " + err.Error())
	}
	defer func() { _ = cmd.Wait() }()
	return SplitJPEG(bufio.NewReaderSize(out, 1<<20), emit)
}

// Still rpicam-still ile JPEG çeker.
func (Rpicam) Still(ctx context.Context) ([]byte, error) {
	ctx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, "rpicam-still", "-n", "-t", "800", "--quality", "92", "-o", "-").Output()
	if err != nil || len(out) < 4 {
		return nil, apierr.Unavailable("Fotoğraf çekilemedi (kamera takılı mı, başka program kullanıyor mu?).")
	}
	return out, nil
}

// SplitJPEG ardışık JPEG akışını karelere ayırır.
func SplitJPEG(r io.Reader, emit func([]byte)) error {
	br, ok := r.(*bufio.Reader)
	if !ok {
		br = bufio.NewReaderSize(r, 1<<20)
	}
	var buf bytes.Buffer
	var prev byte
	in := false
	for {
		b, err := br.ReadByte()
		if err != nil {
			return err
		}
		if !in {
			if prev == 0xFF && b == 0xD8 {
				in = true
				buf.Reset()
				buf.Write([]byte{0xFF, 0xD8})
			}
			prev = b
			continue
		}
		buf.WriteByte(b)
		if prev == 0xFF && b == 0xD9 {
			emit(append([]byte(nil), buf.Bytes()...))
			in = false
		}
		prev = b
	}
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var d [12]byte
	i := len(d)
	for n > 0 {
		i--
		d[i] = byte('0' + n%10)
		n /= 10
	}
	return string(d[i:])
}

// --- benzetim ----------------------------------------------------------------

// Synthetic donanımsız önizleme kaynağıdır (hareketli test görüntüsü).
type Synthetic struct{}

func (Synthetic) Sim() bool { return true }

func (Synthetic) List(context.Context) ([]Cam, error) {
	return []Cam{{Index: 0, Sensor: "benzetim", Modes: "640x360 15 FPS", Path: "test"}}, nil
}

func frame(t float64, w, h int) []byte {
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			v := uint8(30 + 25*float64(y)/float64(h))
			img.Pix[(y*w+x)*4], img.Pix[(y*w+x)*4+1], img.Pix[(y*w+x)*4+2], img.Pix[(y*w+x)*4+3] = v, v+2, v+6, 255
		}
	}
	cx, cy := int(float64(w)/2+math.Cos(t)*float64(w)/3), int(float64(h)/2+math.Sin(t*1.3)*float64(h)/4)
	or := color.RGBA{255, 122, 26, 255}
	for y := cy - 40; y < cy+40; y++ {
		for x := cx - 40; x < cx+40; x++ {
			if x >= 0 && y >= 0 && x < w && y < h && (x-cx)*(x-cx)+(y-cy)*(y-cy) < 1600 {
				img.Set(x, y, or)
			}
		}
	}
	var b bytes.Buffer
	_ = jpeg.Encode(&b, img, &jpeg.Options{Quality: 70})
	return b.Bytes()
}

func (Synthetic) Run(ctx context.Context, _, _, _ int, emit func([]byte)) error {
	tk := time.NewTicker(time.Second / 15)
	defer tk.Stop()
	start := time.Now()
	for {
		select {
		case <-ctx.Done():
			return nil
		case <-tk.C:
			emit(frame(time.Since(start).Seconds(), 640, 360))
		}
	}
}

func (Synthetic) Still(context.Context) ([]byte, error) { return frame(1, 1280, 720), nil }
