package termproc

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
	"time"
)

// Bu dosya kullanıcı adına yapılan işlerdir. makerd-user (kullanıcının kendi
// hesabı) bunları soket üzerinden yapar; mock modda ve `mos` CLI'da aynı
// işlevler doğrudan süreç içinde çağrılır. Böylece web arayüzü ve komut
// satırı aynı dosyaları, projeleri ve ortamları görür.

// WithCmd, giriş kabuğu komutunu "kabuk -l -c cmd" biçimine çevirir.
// cmd boşsa etkileşimli giriş kabuğu aynen döner.
func WithCmd(base func() (*exec.Cmd, error), cmd string) func() (*exec.Cmd, error) {
	if cmd == "" {
		return base
	}
	return func() (*exec.Cmd, error) {
		c, err := base()
		if err != nil {
			return nil, err
		}
		c.Args = []string{c.Path, "-l", "-c", cmd}
		return c, nil
	}
}

// Entry bir dizin girdisidir (list yanıtı).
type Entry struct {
	Name  string    `json:"name"`
	Dir   bool      `json:"dir"`
	Size  int64     `json:"size"`
	Mtime time.Time `json:"mtime"`
	Link  bool      `json:"link,omitempty"`
	Mode  string    `json:"mode"`
}

// Listing list yanıtıdır.
type Listing struct {
	Path    string  `json:"path"`
	Home    string  `json:"home"`
	Entries []Entry `json:"entries"`
}

// ErrFS kullanıcıya gösterilecek dosya sistemi hatasıdır.
type ErrFS string

func (e ErrFS) Error() string { return string(e) }

// Resolve yolu kullanıcının ev dizinine göre çözer ("~", göreli yollar).
func Resolve(home, p string) string {
	switch {
	case p == "" || p == "~":
		return home
	case strings.HasPrefix(p, "~/"):
		return filepath.Join(home, p[2:])
	case !filepath.IsAbs(p):
		return filepath.Join(home, p)
	}
	return filepath.Clean(p)
}

func fsErr(err error) error {
	switch {
	case err == nil:
		return nil
	case errors.Is(err, fs.ErrNotExist):
		return ErrFS("Dosya veya klasör bulunamadı.")
	case errors.Is(err, fs.ErrPermission):
		return ErrFS("Bu konuma erişim izniniz yok.")
	case errors.Is(err, fs.ErrExist):
		return ErrFS("Bu adla bir dosya veya klasör zaten var.")
	}
	return ErrFS(err.Error())
}

// List dizini listeler (klasörler önce, sonra ada göre).
func List(home, p string) (Listing, error) {
	dir := Resolve(home, p)
	des, err := os.ReadDir(dir)
	if err != nil {
		return Listing{}, fsErr(err)
	}
	out := Listing{Path: dir, Home: home, Entries: make([]Entry, 0, len(des))}
	for _, de := range des {
		info, err := de.Info()
		if err != nil {
			continue
		}
		e := Entry{Name: de.Name(), Dir: info.IsDir(), Size: info.Size(), Mtime: info.ModTime(), Mode: info.Mode().Perm().String()}
		if info.Mode()&fs.ModeSymlink != 0 {
			e.Link = true
			if st, err := os.Stat(filepath.Join(dir, de.Name())); err == nil {
				e.Dir, e.Size = st.IsDir(), st.Size()
			}
		}
		out.Entries = append(out.Entries, e)
	}
	sort.Slice(out.Entries, func(i, j int) bool {
		a, b := out.Entries[i], out.Entries[j]
		if a.Dir != b.Dir {
			return a.Dir
		}
		return strings.ToLower(a.Name) < strings.ToLower(b.Name)
	})
	return out, nil
}

// FSOp mkdir, rename, remove ve stat işlemlerini yapar; JSON yanıt döner.
func FSOp(home string, o Open) ([]byte, error) {
	p := Resolve(home, o.Path)
	switch o.Mode {
	case "list":
		l, err := List(home, o.Path)
		if err != nil {
			return nil, err
		}
		return json.Marshal(l)
	case "mkdir":
		if err := os.MkdirAll(p, 0o755); err != nil {
			return nil, fsErr(err)
		}
	case "rename":
		to := Resolve(home, o.To)
		if _, err := os.Lstat(to); err == nil {
			return nil, ErrFS("Hedefte aynı adla bir dosya var.")
		}
		if err := os.Rename(p, to); err != nil {
			return nil, fsErr(err)
		}
	case "remove":
		if p == home || p == "/" {
			return nil, ErrFS("Ev dizini silinemez.")
		}
		if err := os.RemoveAll(p); err != nil {
			return nil, fsErr(err)
		}
	case "stat":
		st, err := os.Stat(p)
		if err != nil {
			return nil, fsErr(err)
		}
		return json.Marshal(Entry{Name: filepath.Base(p), Dir: st.IsDir(), Size: st.Size(), Mtime: st.ModTime(), Mode: st.Mode().Perm().String()})
	default:
		return nil, ErrFS("Bilinmeyen işlem: " + o.Mode)
	}
	return []byte(`{"ok":true}`), nil
}

// OpenRead dosyayı okumak için açar.
func OpenRead(home, p string) (*os.File, error) {
	f, err := os.Open(Resolve(home, p))
	if err != nil {
		return nil, fsErr(err)
	}
	if st, _ := f.Stat(); st != nil && st.IsDir() {
		f.Close()
		return nil, ErrFS("Klasör indirilemez.")
	}
	return f, nil
}

// WriteFileAtomic r'yi geçici dosyaya yazar ve yerine taşır (yarım dosya kalmaz).
func WriteFileAtomic(home, p string, r io.Reader) error {
	dst := Resolve(home, p)
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return fsErr(err)
	}
	tmp, err := os.CreateTemp(filepath.Dir(dst), ".mk-yukleme-*")
	if err != nil {
		return fsErr(err)
	}
	if _, err := io.Copy(tmp, r); err != nil {
		tmp.Close()
		os.Remove(tmp.Name())
		return fsErr(err)
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmp.Name())
		return fsErr(err)
	}
	if err := os.Chmod(tmp.Name(), 0o644); err != nil {
		os.Remove(tmp.Name())
		return fsErr(err)
	}
	return fsErr(os.Rename(tmp.Name(), dst))
}

// HomeOf çalışan kullanıcının ev dizinidir.
func HomeOf(passwd string) string {
	_, home, _ := UserShell(passwd, os.Getuid())
	if home == "" {
		home, _ = os.UserHomeDir()
	}
	if home == "" {
		home = "/"
	}
	return home
}

// RunPiped komutu PTY'siz çalıştırır: stdin'i r'den okur, stdout+stderr'i w'ye
// yazar. ctx iptal olursa süreç grubu sonlandırılır. Dönen değer çıkış kodudur.
func RunPiped(ctx context.Context, cmd *exec.Cmd, r io.Reader, w io.Writer) (int, error) {
	cmd.Stdout, cmd.Stderr = w, w
	if r != nil {
		cmd.Stdin = r
	}
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	if err := cmd.Start(); err != nil {
		return -1, err
	}
	done := make(chan struct{})
	go func() {
		select {
		case <-ctx.Done():
			_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGTERM)
			select {
			case <-done:
			case <-time.After(HangupGrace):
				_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
			}
		case <-done:
		}
	}()
	err := cmd.Wait()
	close(done)
	return ExitCode(err), nil
}
