// Package jobs, uzun süren işleri (ör. Wi-Fi bağlantısı) bellekte izler.
// Her güncelleme "jobs" konusuna {"id","state","progress","log"?} olarak yayımlanır.
package jobs

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"fmt"
	"log/slog"
	"sync"
	"time"
)

const (
	StateRunning = "running"
	StateDone    = "done"
	StateFailed  = "failed"

	maxLog    = 200
	keepFor   = 15 * time.Minute
	jobMaxDur = 3 * time.Minute
)

// Event, events WS'e giden iş güncellemesidir.
type Event struct {
	ID       string `json:"id"`
	State    string `json:"state"`
	Progress int    `json:"progress"`
	Log      string `json:"log,omitempty"`
}

// Job, bir işin anlık görüntüsüdür (GET /jobs/{id}).
type Job struct {
	ID       string    `json:"id"`
	Kind     string    `json:"kind"`
	User     string    `json:"-"`
	State    string    `json:"state"`
	Progress int       `json:"progress"`
	Log      []string  `json:"log"`
	Error    string    `json:"error,omitempty"`
	Created  time.Time `json:"created"`
	Updated  time.Time `json:"updated"`
}

// Manager işleri tutar.
type Manager struct {
	ctx     context.Context
	log     *slog.Logger
	publish func(topic string, data any)
	mu      sync.Mutex
	jobs    map[string]*Job
}

// New bir iş yöneticisi oluşturur; publish nil olabilir.
func New(ctx context.Context, log *slog.Logger, publish func(string, any)) *Manager {
	if publish == nil {
		publish = func(string, any) {}
	}
	return &Manager{ctx: ctx, log: log, publish: publish, jobs: map[string]*Job{}}
}

// Handle, çalışan işin ilerleme bildirme arayüzüdür.
type Handle struct {
	m  *Manager
	id string
}

// Progress ilerlemeyi (0-100) ayarlar.
func (h *Handle) Progress(p int) { h.m.update(h.id, p, "") }

// Log bir günlük satırı ekler.
func (h *Handle) Log(line string) { h.m.update(h.id, -1, line) }

// Logf biçimli günlük satırı ekler.
func (h *Handle) Logf(format string, args ...any) { h.Log(fmt.Sprintf(format, args...)) }

// Step hem ilerleme hem satır bildirir.
func (h *Handle) Step(p int, line string) { h.m.update(h.id, p, line) }

// Start, fn'yi arka planda çalıştırır ve iş kimliğini döndürür (en çok jobMaxDur).
func (m *Manager) Start(kind, user string, fn func(ctx context.Context, h *Handle) error) string {
	return m.StartFor(kind, user, jobMaxDur, fn)
}

// StartFor Start gibidir; uzun işler (ör. yazılım güncellemesi) için süre sınırı verilir.
func (m *Manager) StartFor(kind, user string, max time.Duration, fn func(ctx context.Context, h *Handle) error) string {
	id := newID()
	now := time.Now()
	m.mu.Lock()
	for k, j := range m.jobs {
		if j.State != StateRunning && now.Sub(j.Updated) > keepFor {
			delete(m.jobs, k)
		}
	}
	m.jobs[id] = &Job{ID: id, Kind: kind, User: user, State: StateRunning, Log: []string{}, Created: now, Updated: now}
	m.mu.Unlock()
	m.publish("jobs", Event{ID: id, State: StateRunning})
	go func() {
		ctx, cancel := context.WithTimeout(m.ctx, max)
		defer cancel()
		h := &Handle{m: m, id: id}
		var err error
		func() {
			defer func() {
				if r := recover(); r != nil {
					err = fmt.Errorf("iş çöktü: %v", r)
				}
			}()
			err = fn(ctx, h)
		}()
		m.finish(id, err)
	}()
	return id
}

// Get bir işin kopyasını döndürür.
func (m *Manager) Get(id string) (Job, bool) {
	m.mu.Lock()
	defer m.mu.Unlock()
	j, ok := m.jobs[id]
	if !ok {
		return Job{}, false
	}
	c := *j
	c.Log = append([]string(nil), j.Log...)
	return c, true
}

func (m *Manager) update(id string, progress int, line string) {
	m.mu.Lock()
	j, ok := m.jobs[id]
	if !ok || j.State != StateRunning {
		m.mu.Unlock()
		return
	}
	if progress >= 0 {
		j.Progress = clamp(progress)
	}
	if line != "" {
		j.Log = append(j.Log, line)
		if len(j.Log) > maxLog {
			j.Log = j.Log[len(j.Log)-maxLog:]
		}
	}
	j.Updated = time.Now()
	ev := Event{ID: id, State: j.State, Progress: j.Progress, Log: line}
	m.mu.Unlock()
	m.publish("jobs", ev)
}

func (m *Manager) finish(id string, err error) {
	m.mu.Lock()
	j, ok := m.jobs[id]
	if !ok {
		m.mu.Unlock()
		return
	}
	ev := Event{ID: id}
	if err != nil {
		j.State = StateFailed
		j.Error = err.Error()
		j.Log = append(j.Log, err.Error())
		ev.Log = err.Error()
	} else {
		j.State = StateDone
		j.Progress = 100
	}
	j.Updated = time.Now()
	ev.State, ev.Progress = j.State, j.Progress
	kind := j.Kind
	m.mu.Unlock()
	if err != nil {
		m.log.Warn("iş başarısız", "id", id, "kind", kind, "err", err)
	} else {
		m.log.Info("iş tamamlandı", "id", id, "kind", kind)
	}
	m.publish("jobs", ev)
}

func clamp(p int) int {
	if p < 0 {
		return 0
	}
	if p > 100 {
		return 100
	}
	return p
}

func newID() string {
	b := make([]byte, 8)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}
