package priv

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"syscall"
	"time"
	"unicode/utf8"
)

var userRe = regexp.MustCompile(`^[a-z_][a-z0-9_-]{0,31}$`)

var reservedUsers = map[string]bool{
	"root": true, "daemon": true, "bin": true, "sys": true, "sync": true, "nobody": true, "adm": true,
	"lp": true, "mail": true, "news": true, "uucp": true, "operator": true, "man": true, "sshd": true,
	"messagebus": true, "polkitd": true, "avahi": true, "dbus": true, "admin": true, "wheel": true,
	"users": true, "makerd": true,
}

// ValidUsername, kullanıcı adı biçimini (küçük harfle başlar, en fazla 32) ve ayrılmış adları denetler.
func ValidUsername(s string) bool {
	return userRe.MatchString(s) && !reservedUsers[s] && !strings.HasPrefix(s, "systemd-")
}

// ValidFullname, GECOS alanına yazılabilir mi (':' ve satır sonu yok, en fazla 64 karakter).
func ValidFullname(s string) bool {
	return utf8.ValidString(s) && utf8.RuneCountInString(s) <= 64 && !strings.ContainsAny(s, ":\n\r\x00,=")
}

// ValidNewPassword yeni parola kuralıdır (en az 6 karakter, NUL/satır sonu yok).
func ValidNewPassword(s string) bool {
	return len(s) >= MinPasswordLen && len(s) <= 512 && !strings.ContainsAny(s, "\x00\n\r")
}

// DefaultUserGroups ilk kullanıcının ek gruplarıdır (sistemde olmayanlar atlanır).
var DefaultUserGroups = []string{"wheel", "video", "gpio", "i2c", "spi", "dialout", "input", "render", "audio", "plugdev"}

// UserFiles hesap dosyalarının yollarıdır (testte geçici dizin).
type UserFiles struct {
	Passwd, Group, Gshadow string
	Lock                   string // lckpwdf uyumlu kilit (/etc/.pwd.lock)
	HomeBase, Skel         string
	// SetupFlag varsa user.create reddedilir (makerd'nin <data-dir>/setup-done dosyası).
	SetupFlag string
	// Systemctl boş değilse makerd-user@<user>.service etkinleştirilip başlatılır.
	Systemctl string
	Shells    []string // ilk var olan giriş kabuğu
}

// DefaultUserFiles sistem yollarıdır.
func DefaultUserFiles() UserFiles {
	return UserFiles{Passwd: "/etc/passwd", Group: "/etc/group", Gshadow: "/etc/gshadow", Lock: "/etc/.pwd.lock",
		HomeBase: "/home", Skel: "/etc/skel", SetupFlag: "/var/lib/makerd/setup-done",
		Systemctl: "/usr/bin/systemctl", Shells: []string{"/bin/bash", "/bin/sh"}}
}

// lockPasswd, shadow araçlarıyla uyumlu fcntl kilidi alır (en fazla 15 sn bekler).
func lockPasswd(path string) (func(), error) {
	if path == "" {
		return func() {}, nil
	}
	f, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE, 0o600)
	if err != nil {
		return nil, err
	}
	lk := syscall.Flock_t{Type: syscall.F_WRLCK, Whence: 0}
	deadline := time.Now().Add(15 * time.Second)
	for {
		err = syscall.FcntlFlock(f.Fd(), syscall.F_SETLK, &lk)
		if err == nil {
			break
		}
		if time.Now().After(deadline) {
			f.Close()
			return nil, perr("internal", "Hesap dosyaları kilitli.")
		}
		time.Sleep(100 * time.Millisecond)
	}
	return func() {
		lk.Type = syscall.F_UNLCK
		_ = syscall.FcntlFlock(f.Fd(), syscall.F_SETLK, &lk)
		f.Close()
	}, nil
}

func readLines(path string) ([]string, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	s := strings.TrimRight(string(b), "\n")
	if s == "" {
		return nil, nil
	}
	return strings.Split(s, "\n"), nil
}

func joinLines(l []string) []byte { return []byte(strings.Join(l, "\n") + "\n") }

// addMember, "ad:x:gid:üyeler" satırının üye listesine kullanıcıyı ekler (memberField 3: group, 3: gshadow).
func addMember(line, user string) string {
	f := strings.Split(line, ":")
	if len(f) < 4 {
		return line
	}
	for _, m := range strings.Split(f[3], ",") {
		if m == user {
			return line
		}
	}
	if f[3] == "" {
		f[3] = user
	} else {
		f[3] += "," + user
	}
	return strings.Join(f, ":")
}

func (c *Commands) userCreate(ctx context.Context, _ Peer, raw json.RawMessage) (any, error) {
	var a UserCreateArgs
	if err := decode(raw, &a); err != nil {
		return nil, err
	}
	if !ValidUsername(a.User) {
		return nil, perr("invalid", "Kullanıcı adı küçük harfle başlamalı; yalnızca a-z, 0-9, '-' ve '_' içerebilir (en fazla 32).")
	}
	if !ValidNewPassword(a.Password) {
		return nil, perr("invalid", "Parola en az 6 karakter olmalıdır.")
	}
	if !ValidFullname(a.Fullname) {
		return nil, perr("invalid", "Ad soyad geçersiz karakter içeriyor.")
	}
	uf := c.Users
	if uf.SetupFlag != "" {
		if _, err := os.Stat(uf.SetupFlag); err == nil {
			return nil, perr("forbidden", "Kurulum tamamlandı; bu işlem yalnızca ilk açılışta yapılabilir.")
		}
	}
	hash, err := HashPassword(a.Password)
	if err != nil {
		return nil, err
	}
	unlock, err := lockPasswd(uf.Lock)
	if err != nil {
		return nil, err
	}
	defer unlock()

	passwd, err := readLines(uf.Passwd)
	if err != nil {
		return nil, err
	}
	groups, err := readLines(uf.Group)
	if err != nil {
		return nil, err
	}
	shadow, err := readLines(c.Shadow)
	if err != nil {
		return nil, err
	}
	gshadow, gsErr := readLines(uf.Gshadow)
	hasGshadow := gsErr == nil

	res := UserCreateResult{}
	uid, gid := -1, -1
	home := filepath.Join(uf.HomeBase, a.User)
	usedUID, usedGID := map[int]bool{}, map[int]bool{}
	for i, l := range passwd {
		f := strings.Split(l, ":")
		if len(f) < 7 {
			continue
		}
		n, _ := strconv.Atoi(f[2])
		usedUID[n] = true
		if f[0] == a.User {
			uid = n
			gid, _ = strconv.Atoi(f[3])
			home = f[5]
			if a.Fullname != "" {
				f[4] = a.Fullname
				passwd[i] = strings.Join(f, ":")
			}
		}
	}
	for _, l := range groups {
		f := strings.Split(l, ":")
		if len(f) >= 3 {
			n, _ := strconv.Atoi(f[2])
			usedGID[n] = true
			if f[0] == a.User && uid < 0 {
				return nil, perr("conflict", "Bu ad bir grup tarafından kullanılıyor.")
			}
		}
	}
	if uid >= 0 {
		// Kurulumdan önce var olan hesap (ör. imajdaki kilitli kullanıcı) devralınır.
		if uid < c.MinUID || uid >= 60000 {
			return nil, perr("forbidden", "Bu ad bir sistem hesabına ait.")
		}
	} else {
		res.Created = true
		uid = c.MinUID
		for usedUID[uid] {
			uid++
		}
		gid = uid
		for usedGID[gid] {
			gid++
		}
		if uid >= 60000 || gid >= 60000 {
			return nil, perr("internal", "Boş kullanıcı numarası kalmadı.")
		}
		shell := "/bin/sh"
		for _, s := range uf.Shells {
			if _, err := os.Stat(s); err == nil {
				shell = s
				break
			}
		}
		passwd = append(passwd, strings.Join([]string{a.User, "x", strconv.Itoa(uid), strconv.Itoa(gid), a.Fullname, home, shell}, ":"))
		groups = append(groups, a.User+":x:"+strconv.Itoa(gid)+":")
		shadow = append(shadow, a.User+":!:"+strconv.Itoa(int(time.Now().Unix()/86400))+":0:99999:7:::")
		if hasGshadow {
			gshadow = append(gshadow, a.User+":!::")
		}
	}
	want := map[string]bool{}
	for _, g := range DefaultUserGroups {
		want[g] = true
	}
	for i, l := range groups {
		if name, _, _ := strings.Cut(l, ":"); want[name] {
			groups[i] = addMember(l, a.User)
		}
	}
	for i, l := range gshadow {
		if name, _, _ := strings.Cut(l, ":"); want[name] {
			gshadow[i] = addMember(l, a.User)
		}
	}
	// Sıra: shadow → group/gshadow → passwd (passwd'de görünen hesabın shadow kaydı hep vardır).
	if err := ReplaceFile(c.Shadow, joinLines(shadow), 0o600); err != nil {
		return nil, err
	}
	if hasGshadow {
		if err := ReplaceFile(uf.Gshadow, joinLines(gshadow), 0o600); err != nil {
			return nil, err
		}
	}
	if err := ReplaceFile(uf.Group, joinLines(groups), 0o644); err != nil {
		return nil, err
	}
	if err := ReplaceFile(uf.Passwd, joinLines(passwd), 0o644); err != nil {
		return nil, err
	}
	if err := SetShadowHash(c.Shadow, a.User, hash, time.Now()); err != nil {
		return nil, err
	}
	if err := makeHome(home, uf.Skel, uid, gid); err != nil {
		return nil, err
	}
	res.UID = uid
	if uf.Systemctl != "" {
		if _, err := os.Stat(uf.Systemctl); err == nil {
			cctx, cancel := context.WithTimeout(ctx, 20*time.Second)
			err := exec.CommandContext(cctx, uf.Systemctl, "enable", "--now", "makerd-user@"+a.User+".service").Run()
			cancel()
			res.Service = err == nil
		}
	}
	return res, nil
}

// makeHome ev dizinini yoksa oluşturur, /etc/skel'i kopyalar ve sahipliği verir.
func makeHome(home, skel string, uid, gid int) error {
	if _, err := os.Lstat(home); err == nil {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(home), 0o755); err != nil {
		return err
	}
	if err := os.Mkdir(home, 0o700); err != nil {
		return err
	}
	chown := func(p string) error {
		if err := os.Lchown(p, uid, gid); err != nil && !(errors.Is(err, fs.ErrPermission) && os.Geteuid() != 0) {
			return err
		}
		return nil
	}
	if err := chown(home); err != nil {
		return err
	}
	if skel == "" {
		return nil
	}
	return filepath.WalkDir(skel, func(p string, d fs.DirEntry, err error) error {
		if err != nil || p == skel {
			return nil
		}
		rel, _ := filepath.Rel(skel, p)
		dst := filepath.Join(home, rel)
		info, err := d.Info()
		if err != nil {
			return nil
		}
		switch {
		case d.IsDir():
			if err := os.Mkdir(dst, info.Mode().Perm()); err != nil {
				return err
			}
		case info.Mode().IsRegular():
			if err := copyFile(p, dst, info.Mode().Perm()); err != nil {
				return err
			}
		case info.Mode()&fs.ModeSymlink != 0:
			t, err := os.Readlink(p)
			if err != nil {
				return nil
			}
			if err := os.Symlink(t, dst); err != nil {
				return err
			}
		default:
			return nil
		}
		return chown(dst)
	})
}

func copyFile(src, dst string, perm os.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_EXCL, perm)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}
