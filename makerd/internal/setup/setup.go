// Package setup, ilk açılış sihirbazının durumudur: <data-dir>/setup-done
// dosyası (JSON {"user","time"}) ve tek seferlik tamamlama kilidi
// (<data-dir>/setup.lock üzerinde flock + süreç içi kilit).
package setup

import (
	"encoding/json"
	"errors"
	"net/netip"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
)

// ErrDone kurulum zaten tamamlandı.
var ErrDone = errors.New("kurulum tamamlandı")

// ErrBusy başka bir tamamlama isteği sürüyor.
var ErrBusy = errors.New("kurulum sürüyor")

// Store kurulum durumudur.
type Store struct {
	Dir string

	mu   sync.Mutex
	busy bool
}

type doneFile struct {
	User string    `json:"user"`
	Time time.Time `json:"time"`
}

// Path setup-done dosyasının yoludur (makerd-priv --setup-flag ile aynı olmalı).
func (s *Store) Path() string { return filepath.Join(s.Dir, "setup-done") }

// Done kurulum tamamlandı mı.
func (s *Store) Done() bool {
	_, err := os.Stat(s.Path())
	return err == nil
}

// PrimaryUser kurulumda oluşturulan kullanıcıdır (yoksa "").
func (s *Store) PrimaryUser() string {
	b, err := os.ReadFile(s.Path())
	if err != nil {
		return ""
	}
	var d doneFile
	if json.Unmarshal(b, &d) != nil {
		return ""
	}
	return d.User
}

// Begin tamamlama kilidini alır. Kurulum bitmişse ErrDone, başka istek
// sürüyorsa ErrBusy döner. Dönen işlev kilidi bırakır.
func (s *Store) Begin() (func(), error) {
	s.mu.Lock()
	if s.busy {
		s.mu.Unlock()
		return nil, ErrBusy
	}
	s.busy = true
	s.mu.Unlock()
	release := func() {
		s.mu.Lock()
		s.busy = false
		s.mu.Unlock()
	}
	if err := os.MkdirAll(s.Dir, 0o750); err != nil {
		release()
		return nil, err
	}
	f, err := os.OpenFile(filepath.Join(s.Dir, "setup.lock"), os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		release()
		return nil, err
	}
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		release()
		return nil, ErrBusy
	}
	unlock := func() {
		_ = syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
		f.Close()
		release()
	}
	if s.Done() {
		unlock()
		return nil, ErrDone
	}
	return unlock, nil
}

// MarkDone setup-done dosyasını atomik olarak yazar.
func (s *Store) MarkDone(user string) error {
	b, _ := json.Marshal(doneFile{User: user, Time: time.Now().UTC()})
	tmp := s.Path() + ".tmp"
	f, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o644)
	if err != nil {
		return err
	}
	if _, err := f.Write(append(b, '\n')); err != nil {
		f.Close()
		return err
	}
	if err := f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, s.Path())
}

// Reset kurulum durumunu siler (yalnız mock/test).
func (s *Store) Reset() error {
	err := os.Remove(s.Path())
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	return err
}

// LocalAddr, kurulum isteğinin yerel ağdan geldiğini denetler: loopback,
// özel (RFC 1918 / ULA) veya bağlantı-yerel adresler kabul edilir.
func LocalAddr(ip string) bool {
	a, err := netip.ParseAddr(ip)
	if err != nil {
		return false
	}
	a = a.Unmap()
	return a.IsLoopback() || a.IsPrivate() || a.IsLinkLocalUnicast()
}

// SuggestHostname, MAC adresinin son 4 onaltılık hanesinden "makeros-xxxx" önerir.
func SuggestHostname(mac string) string {
	h := strings.ToLower(strings.ReplaceAll(mac, ":", ""))
	if len(h) < 4 {
		return "makeros"
	}
	for _, c := range h {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return "makeros"
		}
	}
	return "makeros-" + h[len(h)-4:]
}
