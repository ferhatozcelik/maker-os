// makerd: MakerOS 2 arka uç hizmeti (yetkisiz kullanıcı olarak çalışır).
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"os/user"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/activity"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apps"
	"github.com/ferhatozcelik/maker-os/makerd/internal/auth"
	"github.com/ferhatozcelik/maker-os/makerd/internal/browser"
	"github.com/ferhatozcelik/maker-os/makerd/internal/camera"
	"github.com/ferhatozcelik/maker-os/makerd/internal/events"
	"github.com/ferhatozcelik/maker-os/makerd/internal/gpio"
	"github.com/ferhatozcelik/maker-os/makerd/internal/httpd"
	"github.com/ferhatozcelik/maker-os/makerd/internal/jobs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/mock"
	"github.com/ferhatozcelik/maker-os/makerd/internal/network"
	"github.com/ferhatozcelik/maker-os/makerd/internal/notify"
	"github.com/ferhatozcelik/maker-os/makerd/internal/ota"
	"github.com/ferhatozcelik/maker-os/makerd/internal/prefs"
	"github.com/ferhatozcelik/maker-os/makerd/internal/priv"
	"github.com/ferhatozcelik/maker-os/makerd/internal/screen"
	"github.com/ferhatozcelik/maker-os/makerd/internal/sd"
	"github.com/ferhatozcelik/maker-os/makerd/internal/setup"
	"github.com/ferhatozcelik/maker-os/makerd/internal/system"
	"github.com/ferhatozcelik/maker-os/makerd/internal/term"
	"github.com/ferhatozcelik/maker-os/makerd/internal/termproc"
	"github.com/ferhatozcelik/maker-os/makerd/internal/uagent"
)

// Version derlemede -ldflags "-X main.Version=..." ile ayarlanır.
var Version = "2.0.0-dev"

func main() {
	listen := flag.String("listen", ":80", "dinleme adresi(leri), virgülle ayrılmış (systemd soket devri varsa yok sayılır)")
	web := flag.String("web", "/usr/share/makeros/shell", "web kabuk dizini")
	mockMode := flag.Bool("mock", false, "sahte veri arka uçları (kök/NM/D-Bus gerekmez)")
	dataDir := flag.String("data-dir", "", "kalıcı veri dizini (varsayılan /var/lib/makerd; mock: geçici dizin)")
	tokenFile := flag.String("token-file", "", "yerel kabuk belirteci (varsayılan /run/makeros/token; mock: <data-dir>/token)")
	privSock := flag.String("priv-socket", priv.DefaultSocket, "makerd-priv soketi")
	userDir := flag.String("user-socket-dir", term.DefaultUserDir, "makerd-user@ soketleri (<uid>.sock)")
	logLevel := flag.String("log-level", "info", "debug|info|warn|error")
	logFormat := flag.String("log-format", "text", "text|json")
	flag.StringVar(&vncAddr, "vnc", "127.0.0.1:5900", "yerel VNC sunucusu (wayvnc); boş: uzak masaüstü kapalı")
	flag.StringVar(&updateRepo, "update-repo", "ferhatozcelik/maker-os", "yazılım güncellemeleri için GitHub deposu (sahip/ad); boş: kapalı")
	showVersion := flag.Bool("version", false, "sürümü yaz ve çık")
	flag.Parse()
	if *showVersion {
		fmt.Println("makerd", Version)
		return
	}

	var lvl slog.Level
	_ = lvl.UnmarshalText([]byte(*logLevel))
	hopts := &slog.HandlerOptions{Level: lvl}
	var log *slog.Logger
	if *logFormat == "json" {
		log = slog.New(slog.NewJSONHandler(os.Stderr, hopts))
	} else {
		log = slog.New(slog.NewTextHandler(os.Stderr, hopts))
	}
	slog.SetDefault(log)

	if *dataDir == "" {
		if *mockMode {
			*dataDir = filepath.Join(os.TempDir(), "makerd-mock")
		} else {
			*dataDir = "/var/lib/makerd"
		}
	}
	if *tokenFile == "" {
		if *mockMode {
			*tokenFile = filepath.Join(*dataDir, "token")
		} else {
			*tokenFile = "/run/makeros/token"
		}
	}
	_ = os.MkdirAll(*dataDir, 0o750)

	if err := run(log, *listen, *web, *mockMode, *dataDir, *tokenFile, *privSock, *userDir); err != nil {
		log.Error("makerd durdu", "err", err)
		os.Exit(1)
	}
}

// vncAddr --vnc bayrağı; updateRepo --update-repo bayrağı.
var vncAddr, updateRepo string

func run(log *slog.Logger, listen, web string, mockMode bool, dataDir, tokenFile, privSock, userDir string) error {
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()
	appCtx, cancelApp := context.WithCancel(context.Background())
	defer cancelApp()

	hub := events.NewHub(appCtx, log, httpd.Topics...)
	jobMgr := jobs.New(appCtx, log, hub.Publish)
	notes := notify.New(hub.Publish)

	localUser := "maker"
	if u, err := user.Current(); err == nil && !mockMode {
		localUser = u.Username
	}
	token, err := auth.WriteToken(tokenFile)
	if err != nil {
		log.Warn("yerel belirteç yazılamadı; /auth/local kapalı", "path", tokenFile, "err", err)
	}

	setupStore := &setup.Store{Dir: dataDir}
	d := httpd.Deps{
		Log: log, WebDir: web, Sessions: auth.NewManager(), Limiter: auth.NewLimiter(5, time.Minute),
		LocalUser: localUser, LocalToken: token, Hub: hub, Jobs: jobMgr, Notify: notes,
		Setup: setupStore, Prefs: &prefs.Store{Dir: filepath.Join(dataDir, "prefs")},
	}
	reg := &apps.Registry{Log: log, WebDir: web}
	shell := termproc.LoginCommand("/etc/passwd", "/bin/sh")
	if mockMode {
		fresh := os.Getenv("MAKERD_MOCK_FRESH") == "1"
		st := mock.NewState()
		if fresh {
			st = mock.NewFreshState()
			if err := setupStore.Reset(); err != nil {
				return err
			}
		} else if !setupStore.Done() {
			if err := setupStore.MarkDone(mock.User); err != nil {
				return err
			}
		}
		d.Checker = mock.Checker{S: st}
		d.Accounts = st
		d.System = mock.NewSystem(st, Version, log)
		d.Network = mock.NewNetwork()
		d.Activity = mock.NewActivity()
		reg.Extra = mock.NativeApps()
		reg.Launch = mock.Launcher(log, apps.SystemLauncher(log))
		d.Term = term.NewManager(log, term.Local(shell))
		d.Agent = uagent.Local{Command: shell}
		notes.Add("makeros.settings", "MakerOS'a hoş geldiniz", "Bu bir önizlemedir (mock mod).")
		// Docker önizlemesi: güncelleme GitHub'dan gerçekten denetlenir, kurulum benzetilir.
		d.Update = &ota.Updater{Repo: updateRepo, Current: Version, Dir: filepath.Join(dataDir, "ota"), Installer: &ota.SimInstaller{Log: log}, Log: log}
		if fresh {
			log.Warn("MOCK MODU (ilk açılış): kurulum sihirbazı bekleniyor (MAKERD_MOCK_FRESH=1)")
		} else {
			log.Warn("MOCK MODU: sahte veriler kullanılıyor", "user", mock.User, "password", mock.Password)
		}
	} else {
		pc := priv.NewClient(privSock)
		d.Update = &ota.Updater{Repo: updateRepo, Current: Version, Dir: priv.OTADir, Installer: ota.PrivInstaller{C: pc}, Log: log}
		d.Checker = privChecker{c: pc}
		d.Accounts = privAccounts{c: pc}
		d.IsAdmin = func(u string) bool { return auth.IsAdmin("/etc/group", u) || u == setupStore.PrimaryUser() }
		d.LocalLoopbackOnly = true
		d.System = &system.Real{Root: "/", Version: Version, Priv: pc}
		d.Network = &network.NM{Log: log, Root: "/"}
		d.Activity = activity.NewProc()
		reg.DesktopDirs = []string{"/usr/share/applications", "/usr/local/share/applications"}
		reg.Launch = apps.SystemLauncher(log)
		// Terminal kullanıcının makerd-user@ hizmetinde açılır; o yoksa yalnız
		// makerd'nin kendi kullanıcısı için süreç içi (kum havuzlu) kabuğa düşülür.
		d.Term = term.NewManager(log, term.Remote(userDir, uidOf, localUser, term.Local(shell)))
		d.Agent = uagent.Remote{Dir: userDir, UIDOf: uidOf, Self: localUser, Fallback: uagent.Local{Command: shell}}
	}
	d.Apps = reg
	// Donanım: gerçek kamera/GPIO varsa onlar, yoksa (Docker önizleme) benzetim.
	model := "Raspberry Pi"
	if b, err := os.ReadFile("/proc/device-tree/model"); err == nil {
		model = strings.TrimRight(string(b), "\x00\n")
	}
	if _, err := exec.LookPath("rpicam-vid"); err == nil && !mockMode {
		d.Camera = camera.NewHub(camera.Rpicam{})
	} else {
		d.Camera = camera.NewHub(camera.Synthetic{})
	}
	if g, ok := gpio.Detect(model); ok && !mockMode {
		d.GPIO = g
	} else {
		d.GPIO = gpio.NewSim("Raspberry Pi 4 Model B (benzetim)")
	}
	d.VNCAddr = vncAddr
	if vncAddr != "" {
		// Ekran Modu varsayılan kapalı; açıksa (Ayarlar) wayvnc başlatılır. Bkz. internal/screen.
		d.Screen = &screen.Manager{Path: filepath.Join(dataDir, "screen-mode"), Addr: vncAddr, Log: log}
		d.Screen.Apply()
		defer d.Screen.Close()
	}
	if updateRepo == "" {
		d.Update = nil
	} else if d.Update != nil {
		go watchUpdates(appCtx, d.Update, notes, log)
	}
	// Uzak tarayıcı oturumu (MakerOS'taki "Chromium" penceresi); betik imajda yoksa (yalın mock) kullanılamaz.
	d.Browser = &browser.Manager{Addr: "127.0.0.1:5901", Script: "/usr/lib/makeros/browser-session", Log: log}
	defer d.Browser.Close()

	srv := httpd.New(appCtx, d)
	hs := &http.Server{
		Handler: srv, ReadHeaderTimeout: 10 * time.Second, IdleTimeout: 120 * time.Second,
		MaxHeaderBytes: 64 << 10, ErrorLog: slog.NewLogLogger(log.Handler(), slog.LevelDebug),
	}
	hs.RegisterOnShutdown(cancelApp)

	listeners, err := sd.Listeners()
	if err != nil {
		return fmt.Errorf("soket devri: %w", err)
	}
	if len(listeners) == 0 {
		for _, addr := range strings.Split(listen, ",") {
			l, err := net.Listen("tcp", strings.TrimSpace(addr))
			if err != nil {
				return err
			}
			listeners = append(listeners, l)
		}
	}
	errc := make(chan error, len(listeners))
	for _, l := range listeners {
		log.Info("dinleniyor", "addr", l.Addr().String(), "web", web, "mock", mockMode, "version", Version)
		go func(l net.Listener) { errc <- hs.Serve(l) }(l)
	}
	_ = sd.Notify("READY=1\nSTATUS=makerd hazır")

	select {
	case <-ctx.Done():
	case err := <-errc:
		if !errors.Is(err, http.ErrServerClosed) {
			return err
		}
	}
	log.Info("kapanıyor")
	_ = sd.Notify("STOPPING=1")
	cancelApp()
	d.Term.CloseAll()
	sctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	return hs.Shutdown(sctx)
}

// privChecker parolayı makerd-priv ile doğrular. makerd-priv yalnız insan
// hesaplarını (UID 1000–59999) kabul eder.
type privChecker struct {
	c *priv.Client
}

func (p privChecker) Check(ctx context.Context, u, password string) (auth.Result, error) {
	if !priv.ValidUsername(u) || password == "" {
		return auth.Result{}, nil
	}
	var res priv.AuthCheckResult
	if err := p.c.Call(ctx, priv.CmdAuthCheck, priv.AuthCheckArgs{User: u, Password: password}, &res); err != nil {
		return auth.Result{}, err
	}
	return auth.Result{OK: res.OK, Admin: res.OK}, nil
}

// privAccounts kurulum hesabını makerd-priv user.create ile oluşturur.
type privAccounts struct{ c *priv.Client }

func (p privAccounts) CreateUser(ctx context.Context, a priv.UserCreateArgs) error {
	var res priv.UserCreateResult
	if err := p.c.Call(ctx, priv.CmdUserCreate, a, &res); err != nil {
		return system.PrivError(err)
	}
	slog.Info("kurulum hesabı", "user", a.User, "uid", res.UID, "created", res.Created, "makerd_user_service", res.Service)
	return nil
}

func uidOf(name string) (int, error) {
	u, err := user.Lookup(name)
	if err != nil {
		return 0, err
	}
	return strconv.Atoi(u.Uid)
}

// watchUpdates açılıştan biraz sonra ve 12 saatte bir yeni sürüm denetler; yeni sürüm bir kez bildirilir.
func watchUpdates(ctx context.Context, u *ota.Updater, notes *notify.Store, log *slog.Logger) {
	told := ""
	t := time.NewTimer(2 * time.Minute)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
		if st, err := u.Check(ctx); err == nil && st.Available != nil && st.Available.Version != told {
			told = st.Available.Version
			notes.Add("makeros.settings", "Yazılım güncellemesi", "MakerOS "+told+" kullanılabilir. Ayarlar › Genel › Yazılım güncelleme.")
			log.Info("yeni sürüm var", "version", told)
		}
		t.Reset(12 * time.Hour)
	}
}
