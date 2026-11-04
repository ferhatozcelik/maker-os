// Package auth: bellek içi oturumlar (HMAC imzalı çerez), giriş hız sınırı,
// yerel kabuk belirteci ve parola doğrulayıcı arayüzü.
package auth

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"net/http"
	"strings"
	"sync"
	"time"
)

const (
	// CookieName oturum çerezinin adıdır.
	CookieName = "mk_session"
	// SessionTTL oturum ömrüdür.
	SessionTTL  = 7 * 24 * time.Hour
	maxSessions = 256
)

// Result, parola denetiminin sonucudur.
type Result struct {
	OK    bool
	Admin bool
}

// Checker kullanıcı/parola doğrular. Gerçek modda makerd-priv, mock'ta sabit.
type Checker interface {
	Check(ctx context.Context, user, password string) (Result, error)
}

// Session bir oturumdur.
type Session struct {
	ID      string
	User    string
	Admin   bool
	Expires time.Time
}

// Manager oturumları tutar.
type Manager struct {
	key      []byte
	ttl      time.Duration
	now      func() time.Time
	mu       sync.Mutex
	sessions map[string]*Session
}

// NewManager rastgele imza anahtarıyla bir oturum yöneticisi oluşturur.
func NewManager() *Manager {
	key := make([]byte, 32)
	_, _ = rand.Read(key)
	return &Manager{key: key, ttl: SessionTTL, now: time.Now, sessions: map[string]*Session{}}
}

func (m *Manager) sign(id string) string {
	h := hmac.New(sha256.New, m.key)
	h.Write([]byte(id))
	return base64.RawURLEncoding.EncodeToString(h.Sum(nil))
}

// Create yeni oturum açar ve çerez değerini döndürür.
func (m *Manager) Create(user string, admin bool) (*Session, string) {
	id := RandomToken(18)
	now := m.now()
	s := &Session{ID: id, User: user, Admin: admin, Expires: now.Add(m.ttl)}
	m.mu.Lock()
	for k, v := range m.sessions {
		if now.After(v.Expires) {
			delete(m.sessions, k)
		}
	}
	if len(m.sessions) >= maxSessions {
		var oldest *Session
		for _, v := range m.sessions {
			if oldest == nil || v.Expires.Before(oldest.Expires) {
				oldest = v
			}
		}
		delete(m.sessions, oldest.ID)
	}
	m.sessions[id] = s
	m.mu.Unlock()
	return s, id + "." + m.sign(id)
}

// Lookup çerez değerini doğrular. İmza bozuksa harita bile taranmaz.
func (m *Manager) Lookup(value string) (*Session, bool) {
	id, sig, ok := strings.Cut(value, ".")
	if !ok || id == "" || !hmac.Equal([]byte(sig), []byte(m.sign(id))) {
		return nil, false
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	s, ok := m.sessions[id]
	if !ok {
		return nil, false
	}
	if m.now().After(s.Expires) {
		delete(m.sessions, id)
		return nil, false
	}
	c := *s
	return &c, true
}

// Delete oturumu kapatır.
func (m *Manager) Delete(value string) {
	if s, ok := m.Lookup(value); ok {
		m.mu.Lock()
		delete(m.sessions, s.ID)
		m.mu.Unlock()
	}
}

// FromRequest istekteki çerezden oturumu çözer.
func (m *Manager) FromRequest(r *http.Request) (*Session, bool) {
	c, err := r.Cookie(CookieName)
	if err != nil {
		return nil, false
	}
	return m.Lookup(c.Value)
}

// SetCookie oturum çerezini yazar (HTTPS'de Secure).
func (m *Manager) SetCookie(w http.ResponseWriter, r *http.Request, value string) {
	http.SetCookie(w, &http.Cookie{
		Name: CookieName, Value: value, Path: "/",
		MaxAge: int(m.ttl / time.Second), HttpOnly: true,
		SameSite: http.SameSiteStrictMode, Secure: r.TLS != nil,
	})
}

// ClearCookie oturum çerezini siler.
func (m *Manager) ClearCookie(w http.ResponseWriter, r *http.Request) {
	http.SetCookie(w, &http.Cookie{
		Name: CookieName, Value: "", Path: "/", MaxAge: -1, HttpOnly: true,
		SameSite: http.SameSiteStrictMode, Secure: r.TLS != nil,
	})
}

// RandomToken n rastgele baytın base64url gösterimidir.
func RandomToken(n int) string {
	b := make([]byte, n)
	_, _ = rand.Read(b)
	return base64.RawURLEncoding.EncodeToString(b)
}
