package priv

import (
	"bufio"
	"bytes"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

// ErrNoUser, kullanıcı shadow dosyasında yok.
var ErrNoUser = errors.New("kullanıcı bulunamadı")

// ErrLocked, hesap kilitli veya parolasız.
var ErrLocked = errors.New("hesap kilitli")

// ShadowHash, kullanıcının özetini okur.
func ShadowHash(path, user string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		fields := strings.Split(sc.Text(), ":")
		if len(fields) >= 2 && fields[0] == user {
			h := fields[1]
			if h == "" || h[0] == '!' || h[0] == '*' {
				return "", ErrLocked
			}
			return h, nil
		}
	}
	if err := sc.Err(); err != nil {
		return "", err
	}
	return "", ErrNoUser
}

// CheckShadow, kullanıcının parolasını shadow dosyasına karşı doğrular.
func CheckShadow(path, user, password string) (bool, error) {
	h, err := ShadowHash(path, user)
	if errors.Is(err, ErrNoUser) || errors.Is(err, ErrLocked) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	return Verify(password, h)
}

// SetShadowHash, kullanıcının özetini ve son değişiklik gününü atomik olarak günceller.
// Dosya izinleri ve sahipliği korunur.
func SetShadowHash(path, user, hash string, now time.Time) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	lines := bytes.Split(data, []byte("\n"))
	found := false
	for i, line := range lines {
		fields := strings.Split(string(line), ":")
		if len(fields) >= 3 && fields[0] == user {
			fields[1] = hash
			fields[2] = itoa(int(now.Unix() / 86400))
			lines[i] = []byte(strings.Join(fields, ":"))
			found = true
		}
	}
	if !found {
		return ErrNoUser
	}
	return ReplaceFile(path, bytes.Join(lines, []byte("\n")), 0o600)
}

// ReplaceFile dosyayı atomik olarak (geçici dosya + fsync + rename) değiştirir.
// Var olan dosyanın izinleri ve sahipliği korunur; yoksa perm kullanılır.
func ReplaceFile(path string, data []byte, perm os.FileMode) error {
	mode := perm
	uid, gid := -1, -1
	if st, err := os.Stat(path); err == nil {
		mode = st.Mode().Perm()
		if sys, ok := st.Sys().(*syscall.Stat_t); ok {
			uid, gid = int(sys.Uid), int(sys.Gid)
		}
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), "."+filepath.Base(path)+"-")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Chmod(mode); err != nil {
		tmp.Close()
		return err
	}
	if uid >= 0 {
		_ = tmp.Chown(uid, gid)
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), path)
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var b [20]byte
	i := len(b)
	for n > 0 {
		i--
		b[i] = byte('0' + n%10)
		n /= 10
	}
	return string(b[i:])
}
