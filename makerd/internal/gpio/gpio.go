// Package gpio, Raspberry Pi'nin 40 pinlik başlığıdır (Pi 4, Pi 400, Pi 5 aynı
// yerleşim). Gerçek sistemde libgpiod v2 karakter aygıtı (/dev/gpiochipN)
// kullanılır: Pi 4'te "pinctrl-bcm2711", Pi 5'te RP1 ("pinctrl-rp1").
// Donanım yoksa (Docker önizleme) benzetim kullanılır.
package gpio

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/warthog618/go-gpiocdev"
)

// Pin başlıktaki bir fiziksel pindir.
type Pin struct {
	Phys     int    `json:"phys"`
	Name     string `json:"name"`           // "GPIO17", "3V3", "GND", "5V"
	GPIO     int    `json:"gpio"`           // -1: güç/toprak
	Alt      string `json:"alt,omitempty"`  // "I2C1 SDA", "SPI0 MOSI", "UART TX" ...
	Dir      string `json:"dir,omitempty"`  // "in" | "out"
	Value    int    `json:"value"`          // 0/1 (güç pinlerinde -1)
	Used     bool   `json:"used,omitempty"` // başka bir süreç veya çekirdek kullanıyor
	Consumer string `json:"consumer,omitempty"`
	Held     bool   `json:"held,omitempty"` // MakerOS (bu panel) çıkış olarak tutuyor
}

// Board panel yanıtıdır.
type Board struct {
	Model string `json:"model"`
	Chip  string `json:"chip"`
	Label string `json:"label"`
	Sim   bool   `json:"sim"`
	Pins  []Pin  `json:"pins"`
}

// header: fiziksel pin → (ad, GPIO, alternatif işlev)
var header = []Pin{
	{1, "3V3", -1, "", "", -1, false, "", false}, {2, "5V", -1, "", "", -1, false, "", false},
	{3, "GPIO2", 2, "I2C1 SDA", "", 0, false, "", false}, {4, "5V", -1, "", "", -1, false, "", false},
	{5, "GPIO3", 3, "I2C1 SCL", "", 0, false, "", false}, {6, "GND", -1, "", "", -1, false, "", false},
	{7, "GPIO4", 4, "GPCLK0", "", 0, false, "", false}, {8, "GPIO14", 14, "UART TX", "", 0, false, "", false},
	{9, "GND", -1, "", "", -1, false, "", false}, {10, "GPIO15", 15, "UART RX", "", 0, false, "", false},
	{11, "GPIO17", 17, "", "", 0, false, "", false}, {12, "GPIO18", 18, "PCM CLK / PWM0", "", 0, false, "", false},
	{13, "GPIO27", 27, "", "", 0, false, "", false}, {14, "GND", -1, "", "", -1, false, "", false},
	{15, "GPIO22", 22, "", "", 0, false, "", false}, {16, "GPIO23", 23, "", "", 0, false, "", false},
	{17, "3V3", -1, "", "", -1, false, "", false}, {18, "GPIO24", 24, "", "", 0, false, "", false},
	{19, "GPIO10", 10, "SPI0 MOSI", "", 0, false, "", false}, {20, "GND", -1, "", "", -1, false, "", false},
	{21, "GPIO9", 9, "SPI0 MISO", "", 0, false, "", false}, {22, "GPIO25", 25, "", "", 0, false, "", false},
	{23, "GPIO11", 11, "SPI0 SCLK", "", 0, false, "", false}, {24, "GPIO8", 8, "SPI0 CE0", "", 0, false, "", false},
	{25, "GND", -1, "", "", -1, false, "", false}, {26, "GPIO7", 7, "SPI0 CE1", "", 0, false, "", false},
	{27, "GPIO0", 0, "ID EEPROM SDA", "", 0, false, "", false}, {28, "GPIO1", 1, "ID EEPROM SCL", "", 0, false, "", false},
	{29, "GPIO5", 5, "", "", 0, false, "", false}, {30, "GND", -1, "", "", -1, false, "", false},
	{31, "GPIO6", 6, "", "", 0, false, "", false}, {32, "GPIO12", 12, "PWM0", "", 0, false, "", false},
	{33, "GPIO13", 13, "PWM1", "", 0, false, "", false}, {34, "GND", -1, "", "", -1, false, "", false},
	{35, "GPIO19", 19, "PCM FS / SPI1 MISO", "", 0, false, "", false}, {36, "GPIO16", 16, "SPI1 CE2", "", 0, false, "", false},
	{37, "GPIO26", 26, "", "", 0, false, "", false}, {38, "GPIO20", 20, "PCM DIN / SPI1 MOSI", "", 0, false, "", false},
	{39, "GND", -1, "", "", -1, false, "", false}, {40, "GPIO21", 21, "PCM DOUT / SPI1 SCLK", "", 0, false, "", false},
}

// Header başlık yerleşiminin kopyasıdır.
func Header() []Pin { return append([]Pin(nil), header...) }

// Provider panel arka ucudur.
type Provider interface {
	Board() (Board, error)
	// Set pini çıkış yapıp değeri yazar ve tutar (Release edilene kadar).
	Set(gpio, value int) error
	// Release tutulan pini bırakır (giriş olarak serbest).
	Release(gpio int) error
}

const consumer = "makeros-panel"

// --- gerçek donanım --------------------------------------------------------

// Real karakter aygıtı arka ucudur.
type Real struct {
	Model string
	mu    sync.Mutex
	held  map[int]*gpiocdev.Line
	chip  string
	label string
}

// Detect başlığı süren yonga varsa Real döner (Pi 4: bcm2711, Pi 5: rp1).
func Detect(model string) (*Real, bool) {
	for _, name := range gpiocdev.Chips() {
		c, err := gpiocdev.NewChip(name)
		if err != nil {
			continue
		}
		label := c.Label
		c.Close()
		if strings.Contains(label, "bcm2711") || strings.Contains(label, "bcm2835") || strings.Contains(label, "rp1") {
			return &Real{Model: model, chip: name, label: label, held: map[int]*gpiocdev.Line{}}, true
		}
	}
	return nil, false
}

func (r *Real) Board() (Board, error) {
	c, err := gpiocdev.NewChip(r.chip)
	if err != nil {
		return Board{}, apierr.Unavailable("GPIO yongası açılamadı: " + err.Error())
	}
	defer c.Close()
	r.mu.Lock()
	defer r.mu.Unlock()
	pins := Header()
	for i := range pins {
		p := &pins[i]
		if p.GPIO < 0 {
			continue
		}
		if l, ok := r.held[p.GPIO]; ok {
			p.Held, p.Dir = true, "out"
			p.Value, _ = l.Value()
			continue
		}
		info, err := c.LineInfo(p.GPIO)
		if err != nil {
			continue
		}
		p.Used, p.Consumer = info.Used, info.Consumer
		p.Dir = "in"
		if info.Config.Direction == gpiocdev.LineDirectionOutput {
			p.Dir = "out"
		}
		if !info.Used {
			// Boştaki hat yönü değiştirilmeden kısa süre istenip okunur.
			if l, err := c.RequestLine(p.GPIO, gpiocdev.AsIs, gpiocdev.WithConsumer(consumer)); err == nil {
				p.Value, _ = l.Value()
				l.Close()
			}
		}
	}
	return Board{Model: r.Model, Chip: r.chip, Label: r.label, Pins: pins}, nil
}

func valid(gpio int) error {
	for _, p := range header {
		if p.GPIO == gpio && gpio >= 0 {
			if gpio <= 1 {
				return apierr.Invalid("GPIO0/1 HAT kimlik EEPROM'u içindir; kullanılmaz.")
			}
			return nil
		}
	}
	return apierr.Invalid("Başlıkta böyle bir GPIO yok.")
}

func (r *Real) Set(gpio, value int) error {
	if err := valid(gpio); err != nil {
		return err
	}
	if value != 0 {
		value = 1
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if l, ok := r.held[gpio]; ok {
		return l.SetValue(value)
	}
	l, err := gpiocdev.RequestLine(r.chip, gpio, gpiocdev.AsOutput(value), gpiocdev.WithConsumer(consumer))
	if err != nil {
		if errors.Is(err, os.ErrPermission) {
			return apierr.Forbidden("GPIO izni yok ('gpio' grubu).")
		}
		return apierr.Conflict(fmt.Sprintf("GPIO%d başka bir program tarafından kullanılıyor.", gpio))
	}
	r.held[gpio] = l
	return nil
}

func (r *Real) Release(gpio int) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if l, ok := r.held[gpio]; ok {
		delete(r.held, gpio)
		return l.Close()
	}
	return nil
}

// --- benzetim ----------------------------------------------------------------

// Sim donanımsız önizlemedir: GPIO4 her saniye değişen bir sensör gibi davranır,
// GPIO2/3 (I2C) çekirdek kullanımında görünür.
type Sim struct {
	Model string
	mu    sync.Mutex
	out   map[int]int
	start time.Time
}

// NewSim benzetim panelini kurar.
func NewSim(model string) *Sim { return &Sim{Model: model, out: map[int]int{}, start: time.Now()} }

func (s *Sim) Board() (Board, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	pins := Header()
	tick := int(time.Since(s.start).Seconds())
	for i := range pins {
		p := &pins[i]
		if p.GPIO < 0 {
			continue
		}
		p.Dir, p.Value = "in", 0
		switch p.GPIO {
		case 2, 3:
			p.Used, p.Consumer, p.Value = true, "i2c1", 1
		case 4:
			p.Value = tick % 2
		case 14, 15:
			p.Used, p.Consumer = true, "serial0"
		}
		if v, ok := s.out[p.GPIO]; ok {
			p.Dir, p.Value, p.Held = "out", v, true
		}
	}
	return Board{Model: s.Model, Chip: "sim", Label: "benzetim (donanım yok)", Sim: true, Pins: pins}, nil
}

func (s *Sim) Set(gpio, value int) error {
	if err := valid(gpio); err != nil {
		return err
	}
	if value != 0 {
		value = 1
	}
	s.mu.Lock()
	s.out[gpio] = value
	s.mu.Unlock()
	return nil
}

func (s *Sim) Release(gpio int) error {
	s.mu.Lock()
	delete(s.out, gpio)
	s.mu.Unlock()
	return nil
}
