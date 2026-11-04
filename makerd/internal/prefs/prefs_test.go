package prefs

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestStore(t *testing.T) {
	s := &Store{Dir: filepath.Join(t.TempDir(), "prefs")}
	if v, err := s.Get("ayse"); err != nil || string(v) != "{}" {
		t.Fatalf("empty: %s %v", v, err)
	}
	if _, err := s.Get("../etc"); err == nil {
		t.Fatal("path traversal")
	}
	v, err := s.Put("ayse", []byte(` {"theme": "dark", "dock": {"apps": ["a"]}} `))
	if err != nil || string(v) != `{"theme":"dark","dock":{"apps":["a"]}}` {
		t.Fatalf("put: %s %v", v, err)
	}
	if v, _ := s.Get("ayse"); string(v) != `{"theme":"dark","dock":{"apps":["a"]}}` {
		t.Fatalf("get: %s", v)
	}
	for _, bad := range []string{`[1]`, `"x"`, `{"a":`, `null`} {
		if _, err := s.Put("ayse", []byte(bad)); err == nil {
			t.Errorf("accepted %s", bad)
		}
	}
	if _, err := s.Put("ayse", []byte(`{"x":"`+strings.Repeat("a", MaxSize)+`"}`)); err == nil {
		t.Error("oversized accepted")
	}
	ents, _ := os.ReadDir(s.Dir)
	if len(ents) != 1 {
		t.Errorf("temp files left: %v", ents)
	}
}
