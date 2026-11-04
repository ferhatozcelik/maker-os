package auth

import (
	"sync"
	"time"
)

// Limiter, IP başına kayan pencerede başarısız giriş denemelerini sınırlar
// (varsayılan 5 deneme / dakika).
type Limiter struct {
	Max    int
	Window time.Duration
	now    func() time.Time
	mu     sync.Mutex
	fails  map[string][]time.Time
}

// NewLimiter yeni bir sınırlayıcı oluşturur.
func NewLimiter(max int, window time.Duration) *Limiter {
	return &Limiter{Max: max, Window: window, now: time.Now, fails: map[string][]time.Time{}}
}

func (l *Limiter) prune(ip string, now time.Time) []time.Time {
	list := l.fails[ip]
	i := 0
	for i < len(list) && now.Sub(list[i]) >= l.Window {
		i++
	}
	list = list[i:]
	if len(list) == 0 {
		delete(l.fails, ip)
	} else {
		l.fails[ip] = list
	}
	return list
}

// Allow, ip yeni deneme yapabiliyorsa true; değilse kaç saniye beklemesi gerektiğini döndürür.
func (l *Limiter) Allow(ip string) (bool, time.Duration) {
	l.mu.Lock()
	defer l.mu.Unlock()
	now := l.now()
	list := l.prune(ip, now)
	if len(list) < l.Max {
		return true, 0
	}
	return false, l.Window - now.Sub(list[0])
}

// Fail başarısız denemeyi kaydeder.
func (l *Limiter) Fail(ip string) {
	l.mu.Lock()
	defer l.mu.Unlock()
	now := l.now()
	l.prune(ip, now)
	if len(l.fails) > 4096 { // bellek koruması
		for k := range l.fails {
			l.prune(k, now)
		}
	}
	l.fails[ip] = append(l.fails[ip], now)
}

// Reset başarılı girişte sayacı sıfırlar.
func (l *Limiter) Reset(ip string) {
	l.mu.Lock()
	delete(l.fails, ip)
	l.mu.Unlock()
}
