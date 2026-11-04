package ota

import (
	"context"
	"log/slog"
	"sync"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
)

// PrivInstaller gerçek cihaz: makerd-priv üzerinden RAUC (kök) ve tryboot yeniden başlatması.
type PrivInstaller struct{ C *priv.Client }

func (p PrivInstaller) Install(ctx context.Context, path string) error {
	return p.C.Call(ctx, priv.CmdOTAInstall, priv.OTAInstallArgs{Path: path}, nil)
}

func (p PrivInstaller) State(ctx context.Context) (InstallState, error) {
	var s priv.OTAStatus
	if err := p.C.Call(ctx, priv.CmdOTAStatus, nil, &s); err != nil {
		return InstallState{}, err
	}
	return InstallState{Operation: s.Operation, Progress: s.Progress, Message: s.Message, LastError: s.LastError}, nil
}

func (p PrivInstaller) Reboot(ctx context.Context) error {
	return p.C.Call(ctx, priv.CmdPower, priv.PowerArgs{Action: "tryboot"}, nil)
}

// SimInstaller Docker önizlemesi (mock): cihaza dokunmaz; kurulum adımlarını ilerlemeyle benzetir.
type SimInstaller struct {
	Log *slog.Logger

	mu    sync.Mutex
	start time.Time
}

func (s *SimInstaller) Install(context.Context, string) error {
	s.mu.Lock()
	s.start = time.Now()
	s.mu.Unlock()
	return nil
}

func (s *SimInstaller) State(context.Context) (InstallState, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	el := time.Since(s.start)
	if el < 6*time.Second {
		return InstallState{Operation: "installing", Progress: int(el * 100 / (6 * time.Second)), Message: "Yazılıyor (benzetim)"}, nil
	}
	return InstallState{Operation: "idle", Progress: 100}, nil
}

func (s *SimInstaller) Reboot(context.Context) error {
	s.Log.Warn("MOCK: güncelleme benzetimi tamamlandı; yeniden başlatma yok sayıldı")
	return nil
}
