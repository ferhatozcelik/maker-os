// Package prefs, kullanıcı başına kabuk tercihlerini (serbest JSON nesnesi,
// en fazla 16 KB) <data-dir>/prefs/<kullanıcı>.json dosyasında tutar.
package prefs

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"regexp"
	"sync"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
)

// MaxSize tercih belgesinin en büyük boyutudur.
const MaxSize = 16 << 10

var nameRe = regexp.MustCompile(`^[a-z_][a-z0-9_-]{0,31}$`)

// Store tercih deposudur.
type Store struct {
	Dir string // <data-dir>/prefs
	mu  sync.Mutex
}

func (s *Store) path(user string) (string, error) {
	if !nameRe.MatchString(user) {
		return "", apierr.Invalid("Geçersiz kullanıcı adı.")
	}
	return filepath.Join(s.Dir, user+".json"), nil
}

// Get kullanıcının tercihlerini döndürür; yoksa {}.
func (s *Store) Get(user string) (json.RawMessage, error) {
	p, err := s.path(user)
	if err != nil {
		return nil, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	b, err := os.ReadFile(p)
	if errors.Is(err, os.ErrNotExist) || (err == nil && !isObject(b)) {
		return json.RawMessage("{}"), nil
	}
	if err != nil {
		return nil, err
	}
	return b, nil
}

// Put tercihleri atomik olarak (geçici dosya + rename) yazar. Gövde JSON nesnesi olmalı.
func (s *Store) Put(user string, raw []byte) (json.RawMessage, error) {
	p, err := s.path(user)
	if err != nil {
		return nil, err
	}
	if len(raw) > MaxSize {
		return nil, apierr.New(413, "invalid", "Tercihler en fazla 16 KB olabilir.")
	}
	if !isObject(raw) {
		return nil, apierr.Invalid("Tercihler bir JSON nesnesi olmalıdır.")
	}
	var buf bytes.Buffer
	if err := json.Compact(&buf, raw); err != nil {
		return nil, apierr.Invalid("Geçersiz JSON.")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := os.MkdirAll(s.Dir, 0o750); err != nil {
		return nil, err
	}
	tmp, err := os.CreateTemp(s.Dir, "."+user+"-*.tmp")
	if err != nil {
		return nil, err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(buf.Bytes()); err != nil {
		tmp.Close()
		return nil, err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return nil, err
	}
	if err := tmp.Close(); err != nil {
		return nil, err
	}
	if err := os.Rename(tmp.Name(), p); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func isObject(b []byte) bool {
	b = bytes.TrimSpace(b)
	if len(b) == 0 || b[0] != '{' {
		return false
	}
	var m map[string]json.RawMessage
	return json.Unmarshal(b, &m) == nil
}
