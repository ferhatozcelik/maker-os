package screen

import (
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"testing"
)

func TestDefaultOffAndPersist(t *testing.T) {
	dir := t.TempDir()
	// Yönetilmeyen adres: wayvnc başlatılmaz, yalnız işaret dosyası değişir.
	m := &Manager{Path: filepath.Join(dir, "screen-mode"), Addr: "desktop:5900", Log: slog.New(slog.NewTextHandler(io.Discard, nil)), DRMRoot: dir}
	if m.Enabled() {
		t.Fatal("varsayılan kapalı olmalı")
	}
	if err := m.Set(true); err != nil || !m.Enabled() {
		t.Fatalf("açılamadı: %v", err)
	}
	if err := m.Set(false); err != nil || m.Enabled() {
		t.Fatalf("kapatılamadı: %v", err)
	}
	if err := m.Set(false); err != nil {
		t.Fatalf("ikinci kapatma hata vermemeli: %v", err)
	}
	if m.State().Managed {
		t.Fatal("uzak adres yönetilmemeli")
	}
}

func TestHeadless(t *testing.T) {
	dir := t.TempDir()
	m := &Manager{DRMRoot: dir}
	if !m.headless() {
		t.Fatal("DRM yoksa monitörsüz")
	}
	p := filepath.Join(dir, "card1-HDMI-A-1")
	_ = os.MkdirAll(p, 0o755)
	_ = os.WriteFile(filepath.Join(p, "status"), []byte("disconnected\n"), 0o644)
	if !m.headless() {
		t.Fatal("bağlı olmayan HDMI → monitörsüz")
	}
	_ = os.WriteFile(filepath.Join(p, "status"), []byte("connected\n"), 0o644)
	if m.headless() {
		t.Fatal("HDMI bağlı → monitör var")
	}
}
