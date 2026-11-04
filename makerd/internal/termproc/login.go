package termproc

import (
	"bufio"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

// UserShell, /etc/passwd'den çalışan kullanıcının giriş kabuğunu ve ev dizinini bulur.
func UserShell(passwd string, uid int) (shell, home, name string) {
	f, err := os.Open(passwd)
	if err == nil {
		defer f.Close()
		sc := bufio.NewScanner(f)
		for sc.Scan() {
			p := strings.Split(sc.Text(), ":")
			if len(p) >= 7 && p[2] == strconv.Itoa(uid) {
				return p[6], p[5], p[0]
			}
		}
	}
	return "", "", ""
}

// LoginCommand, çalışan sürecin kullanıcısının
// giriş kabuğunu ev dizininde hazırlar. fallbackShell kabuk bulunamazsa kullanılır.
func LoginCommand(passwd, fallbackShell string) func() (*exec.Cmd, error) {
	return func() (*exec.Cmd, error) {
		shell, home, name := UserShell(passwd, os.Getuid())
		if shell == "" || strings.HasSuffix(shell, "nologin") || strings.HasSuffix(shell, "false") {
			shell = os.Getenv("SHELL")
		}
		if shell == "" {
			shell = fallbackShell
		}
		if _, err := os.Stat(shell); err != nil {
			if p, err := exec.LookPath("sh"); err == nil {
				shell = p
			} else {
				return nil, ErrNoShell
			}
		}
		if home == "" {
			home, _ = os.UserHomeDir()
		}
		if st, err := os.Stat(home); err != nil || !st.IsDir() {
			home = "/"
		}
		if name == "" {
			name = os.Getenv("USER")
		}
		cmd := &exec.Cmd{Path: shell, Args: []string{"-" + filepath.Base(shell)}, Dir: home}
		env := []string{
			"TERM=xterm-256color", "COLORTERM=truecolor", "HOME=" + home, "SHELL=" + shell,
			"USER=" + name, "LOGNAME=" + name,
			"PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin",
		}
		for _, k := range []string{"LANG", "LC_ALL", "TZ", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS"} {
			if v, ok := os.LookupEnv(k); ok {
				env = append(env, k+"="+v)
			}
		}
		if os.Getenv("LANG") == "" {
			env = append(env, "LANG=C.UTF-8")
		}
		cmd.Env = env
		return cmd, nil
	}
}

// ErrNoShell, kabuk bulunamadı.
var ErrNoShell = errors.New("kabuk bulunamadı")
