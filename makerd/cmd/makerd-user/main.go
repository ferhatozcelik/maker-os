// makerd-user: kullanıcı başına hizmet (makerd-user@<kullanıcı>.service,
// User=<kullanıcı>). /run/makeros/user/<uid>.sock üzerinde dinler ve makerd'nin
// açtığı her bağlantı için kullanıcının giriş kabuğunu bir PTY'de başlatır.
// makerd'nin kum havuzunun dışında çalıştığı için terminalde sudo çalışır.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io/fs"
	"log/slog"
	"net"
	"os"
	"os/signal"
	"os/user"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"

	"github.com/ferhatozcelik/maker-os/makerd/internal/sd"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
)

// Version derlemede -ldflags "-X main.Version=..." ile ayarlanır.
var Version = "2.0.0-dev"

func main() {
	dir := flag.String("socket-dir", "/run/makeros/user", "soket dizini (<uid>.sock)")
	allow := flag.String("allow-user", "maker", "bağlanabilecek kullanıcılar (virgülle); root her zaman izinli")
	shell := flag.String("shell", "/bin/sh", "giriş kabuğu bulunamazsa kullanılacak kabuk")
	max := flag.Int("max", 16, "aynı anda en fazla terminal")
	showVersion := flag.Bool("version", false, "sürümü yaz ve çık")
	flag.Parse()
	if *showVersion {
		fmt.Println("makerd-user", Version)
		return
	}
	log := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelInfo}))
	if err := run(log, *dir, *allow, *shell, *max); err != nil {
		log.Error("makerd-user durdu", "err", err)
		os.Exit(1)
	}
}

func run(log *slog.Logger, dir, allow, shell string, max int) error {
	uid := os.Getuid()
	if uid == 0 {
		return errors.New("root olarak çalıştırılamaz; birim User=%i ile başlatmalı")
	}
	allowed := map[uint32]bool{0: true, uint32(uid): true}
	for _, name := range strings.Split(allow, ",") {
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
	if os.Getenv("XDG_RUNTIME_DIR") == "" {
		if rd := "/run/user/" + strconv.Itoa(uid); isDir(rd) {
			_ = os.Setenv("XDG_RUNTIME_DIR", rd)
		}
	}

	ln, err := listen(filepath.Join(dir, strconv.Itoa(uid)+".sock"), uid)
	if err != nil {
		return err
	}
	srv := &termproc.Server{Log: log, Allowed: allowed, Max: max, Command: termproc.LoginCommand("/etc/passwd", shell), Home: termproc.HomeOf("/etc/passwd")}
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()
	_ = sd.Notify("READY=1")
	log.Info("makerd-user hazır", "addr", ln.Addr().String(), "uid", uid, "version", Version)
	err = srv.Serve(ctx, ln)
	_ = sd.Notify("STOPPING=1")
	return err
}

// listen soketi açar. Aynı yolda kendimize ait eski bir soket varsa silinir;
// başkasına aitse çalışmayı reddeder (dizin yapışkan bitli ve herkese yazılabilir).
func listen(path string, uid int) (net.Listener, error) {
	if st, err := os.Lstat(path); err == nil {
		sys, ok := st.Sys().(*syscall.Stat_t)
		if !ok || int(sys.Uid) != uid || st.Mode()&fs.ModeSocket == 0 {
			return nil, fmt.Errorf("%s başka bir kullanıcıya ait veya soket değil", path)
		}
		_ = os.Remove(path)
	}
	old := syscall.Umask(0o111)
	ln, err := net.Listen("unix", path)
	syscall.Umask(old)
	if err != nil {
		return nil, err
	}
	// Kimlik denetimi SO_PEERCRED ile yapılır; makerd (başka kullanıcı) bağlanabilmeli.
	if err := os.Chmod(path, 0o666); err != nil {
		ln.Close()
		return nil, err
	}
	ln.(*net.UnixListener).SetUnlinkOnClose(true)
	return ln, nil
}

func isDir(p string) bool {
	st, err := os.Stat(p)
	return err == nil && st.IsDir()
}
