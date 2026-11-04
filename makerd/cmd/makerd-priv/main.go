// makerd-priv: kök yetkili dar yardımcı. Yalnız sabit komutları (auth.check,
// system.password, system.hostname, power, user.create) izinli UID'lerden kabul eder.
// systemd soket etkinleştirmesiyle çalışır ve boşta kalınca çıkar.
package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"net"
	"os"
	"os/signal"
	"os/user"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
	"github.com/ferhatozcelik/maker-os/makerd/internal/sd"
)

// Version derlemede -ldflags "-X main.Version=..." ile ayarlanır.
var Version = "2.0.0-dev"

func main() {
	sock := flag.String("socket", priv.DefaultSocket, "soket yolu (soket devri yoksa)")
	allow := flag.String("allow-user", "maker", "izinli çağıran kullanıcılar (virgülle); root her zaman izinli")
	sockGroup := flag.String("socket-group", "maker", "kendi açtığı soketin grubu")
	shadow := flag.String("shadow", "/etc/shadow", "shadow dosyası")
	setupFlag := flag.String("setup-flag", "/var/lib/makerd/setup-done", "bu dosya varsa user.create reddedilir")
	idle := flag.Duration("idle", 30*time.Second, "soket devrinde boşta çıkış süresi (0: çıkma)")
	showVersion := flag.Bool("version", false, "sürümü yaz ve çık")
	flag.Parse()
	if *showVersion {
		fmt.Println("makerd-priv", Version)
		return
	}
	log := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelInfo}))

	allowed := map[uint32]bool{0: true}
	for _, name := range strings.Split(*allow, ",") {
		if name = strings.TrimSpace(name); name == "" {
			continue
		}
		u, err := user.Lookup(name)
		if err != nil {
			log.Warn("izinli kullanıcı bulunamadı", "user", name)
			continue
		}
		id, _ := strconv.Atoi(u.Uid)
		allowed[uint32(id)] = true
	}

	ls, err := sd.Listeners()
	if err != nil {
		log.Error("soket devri", "err", err)
		os.Exit(1)
	}
	var ln net.Listener
	idleExit := time.Duration(0)
	if len(ls) > 0 {
		ln, idleExit = ls[0], *idle
	} else {
		_ = os.Remove(*sock)
		_ = os.MkdirAll(dirOf(*sock), 0o755)
		l, err := net.Listen("unix", *sock)
		if err != nil {
			log.Error("soket açılamadı", "path", *sock, "err", err)
			os.Exit(1)
		}
		_ = os.Chmod(*sock, 0o660)
		if g, err := user.LookupGroup(*sockGroup); err == nil {
			gid, _ := strconv.Atoi(g.Gid)
			_ = os.Chown(*sock, 0, gid)
		}
		ln = l
	}

	cmds := priv.DefaultCommands()
	cmds.Shadow = *shadow
	cmds.Users.SetupFlag = *setupFlag
	srv := &priv.Server{Log: log, Allowed: allowed, Handlers: cmds.Handlers(), Idle: idleExit}
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()
	_ = sd.Notify("READY=1")
	log.Info("makerd-priv hazır", "addr", ln.Addr().String(), "idle", idleExit)
	if err := srv.Serve(ctx, ln); err != nil {
		log.Error("sunucu", "err", err)
		os.Exit(1)
	}
}

func dirOf(p string) string {
	if i := strings.LastIndexByte(p, '/'); i > 0 {
		return p[:i]
	}
	return "."
}
