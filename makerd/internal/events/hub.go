// Package events, konu (topic) abonelikli olay dağıtıcısıdır. Üreticiler
// (ör. 2 sn'de bir sistem özeti) yalnızca o konuya en az bir abone varken çalışır.
package events

import (
	"context"
	"encoding/json"
	"log/slog"
	"sync"
	"time"
)

// Producer, bir konunun verisini üretir; ctx iptal edilince dönmelidir.
type Producer func(ctx context.Context, publish func(data any))

// Message, istemciye giden çerçevedir.
type Message struct {
	Topic string `json:"topic"`
	Data  any    `json:"data"`
}

// Client, tek bir WebSocket bağlantısının abonelik durumudur.
type Client struct {
	Send   chan []byte
	topics map[string]bool
}

// Hub, istemcileri ve üreticileri yönetir.
type Hub struct {
	ctx       context.Context
	log       *slog.Logger
	mu        sync.Mutex
	allowed   map[string]bool
	clients   map[*Client]struct{}
	counts    map[string]int
	producers map[string]Producer
	running   map[string]context.CancelFunc
	last      map[string][]byte // üretici konularının son çerçevesi (yeni abone hemen alır)
}

// NewHub, izin verilen konularla bir dağıtıcı oluşturur.
func NewHub(ctx context.Context, log *slog.Logger, topics ...string) *Hub {
	h := &Hub{
		ctx: ctx, log: log,
		allowed:   map[string]bool{},
		clients:   map[*Client]struct{}{},
		counts:    map[string]int{},
		producers: map[string]Producer{},
		running:   map[string]context.CancelFunc{},
		last:      map[string][]byte{},
	}
	for _, t := range topics {
		h.allowed[t] = true
	}
	return h
}

// Register, bir konu için üretici kaydeder.
func (h *Hub) Register(topic string, p Producer) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.allowed[topic] = true
	h.producers[topic] = p
	if h.counts[topic] > 0 {
		h.startLocked(topic)
	}
}

// NewClient bir istemci kaydı oluşturur.
func (h *Hub) NewClient() *Client {
	c := &Client{Send: make(chan []byte, 32), topics: map[string]bool{}}
	h.mu.Lock()
	h.clients[c] = struct{}{}
	h.mu.Unlock()
	return c
}

// Subscribe, istemciyi konulara abone eder; bilinmeyen konular yok sayılır.
func (h *Hub) Subscribe(c *Client, topics []string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if _, ok := h.clients[c]; !ok {
		return
	}
	for _, t := range topics {
		if !h.allowed[t] || c.topics[t] {
			continue
		}
		c.topics[t] = true
		h.counts[t]++
		if h.counts[t] == 1 {
			h.startLocked(t)
		} else if b := h.last[t]; b != nil {
			trySend(c, b)
		}
	}
}

// Unsubscribe aboneliği kaldırır.
func (h *Hub) Unsubscribe(c *Client, topics []string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	for _, t := range topics {
		if c.topics[t] {
			delete(c.topics, t)
			h.decLocked(t)
		}
	}
}

// Remove, istemciyi tüm aboneliklerinden çıkarır.
func (h *Hub) Remove(c *Client) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if _, ok := h.clients[c]; !ok {
		return
	}
	for t := range c.topics {
		h.decLocked(t)
	}
	c.topics = map[string]bool{}
	delete(h.clients, c)
}

// Subscribers, konunun abone sayısıdır.
func (h *Hub) Subscribers(topic string) int {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.counts[topic]
}

// Running, konunun üreticisi çalışıyor mu.
func (h *Hub) Running(topic string) bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.running[topic] != nil
}

// Publish, konuya abone tüm istemcilere veriyi gönderir. Abone yoksa
// serileştirme bile yapılmaz. Yavaş istemcinin kuyruğu doluysa çerçeve atlanır.
func (h *Hub) Publish(topic string, data any) {
	h.mu.Lock()
	n := h.counts[topic]
	h.mu.Unlock()
	if n == 0 {
		return
	}
	b, err := json.Marshal(Message{Topic: topic, Data: data})
	if err != nil {
		h.log.Error("olay serileştirilemedi", "topic", topic, "err", err)
		return
	}
	h.mu.Lock()
	defer h.mu.Unlock()
	if _, ok := h.producers[topic]; ok {
		h.last[topic] = b
	}
	for c := range h.clients {
		if c.topics[topic] {
			trySend(c, b)
		}
	}
}

func trySend(c *Client, b []byte) {
	select {
	case c.Send <- b:
	default:
	}
}

func (h *Hub) startLocked(topic string) {
	p := h.producers[topic]
	if p == nil || h.running[topic] != nil {
		return
	}
	ctx, cancel := context.WithCancel(h.ctx)
	h.running[topic] = cancel
	h.log.Debug("üretici başladı", "topic", topic)
	go p(ctx, func(data any) {
		if ctx.Err() == nil {
			h.Publish(topic, data)
		}
	})
}

func (h *Hub) decLocked(topic string) {
	h.counts[topic]--
	if h.counts[topic] > 0 {
		return
	}
	delete(h.counts, topic)
	if cancel := h.running[topic]; cancel != nil {
		cancel()
		delete(h.running, topic)
		delete(h.last, topic)
		h.log.Debug("üretici durdu", "topic", topic)
	}
}

// Every, fn'yi hemen ve sonra her aralıkta çağırıp sonucunu yayımlayan üreticidir.
func Every(interval time.Duration, fn func(ctx context.Context) (any, error)) Producer {
	return func(ctx context.Context, publish func(any)) {
		t := time.NewTicker(interval)
		defer t.Stop()
		for {
			if v, err := fn(ctx); err == nil {
				publish(v)
			}
			select {
			case <-ctx.Done():
				return
			case <-t.C:
			}
		}
	}
}

// OnChange, fn'yi aralıkla yoklar ve yalnız JSON gösterimi değişince yayımlar.
func OnChange(interval time.Duration, fn func(ctx context.Context) (any, error)) Producer {
	return func(ctx context.Context, publish func(any)) {
		t := time.NewTicker(interval)
		defer t.Stop()
		var prev []byte
		for {
			if v, err := fn(ctx); err == nil {
				if b, err := json.Marshal(v); err == nil && string(b) != string(prev) {
					prev = b
					publish(v)
				}
			}
			select {
			case <-ctx.Done():
				return
			case <-t.C:
			}
		}
	}
}
