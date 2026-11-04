// Package notify, bildirimleri bellekte tutar ve "notifications" konusuna yayımlar.
package notify

import (
	"strconv"
	"sync"
	"time"
)

const maxItems = 100

// Notification, GET /notifications öğesidir.
type Notification struct {
	ID    string    `json:"id"`
	App   string    `json:"app"`
	Title string    `json:"title"`
	Body  string    `json:"body"`
	Time  time.Time `json:"time"`
	Read  bool      `json:"read"`
}

// Store bildirim deposudur.
type Store struct {
	mu      sync.Mutex
	items   []Notification // eskiden yeniye
	next    int
	publish func(topic string, data any)
}

// New bir depo oluşturur; publish nil olabilir.
func New(publish func(string, any)) *Store {
	if publish == nil {
		publish = func(string, any) {}
	}
	return &Store{publish: publish}
}

// Add yeni bildirim ekler ve yayımlar.
func (s *Store) Add(app, title, body string) Notification {
	s.mu.Lock()
	s.next++
	n := Notification{ID: "n" + strconv.Itoa(s.next), App: app, Title: title, Body: body, Time: time.Now().UTC().Truncate(time.Second)}
	s.items = append(s.items, n)
	if len(s.items) > maxItems {
		s.items = s.items[len(s.items)-maxItems:]
	}
	s.mu.Unlock()
	s.publish("notifications", n)
	return n
}

// List bildirimleri yeniden eskiye döndürür.
func (s *Store) List() []Notification {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]Notification, 0, len(s.items))
	for i := len(s.items) - 1; i >= 0; i-- {
		out = append(out, s.items[i])
	}
	return out
}

// MarkRead bildirimi okundu yapar; bulunamazsa false.
func (s *Store) MarkRead(id string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i := range s.items {
		if s.items[i].ID == id {
			s.items[i].Read = true
			return true
		}
	}
	return false
}

// Clear tüm bildirimleri siler ve istemcilere {"cleared":true} yayımlar.
func (s *Store) Clear() {
	s.mu.Lock()
	s.items = nil
	s.mu.Unlock()
	s.publish("notifications", map[string]bool{"cleared": true})
}
