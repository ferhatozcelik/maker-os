// Package sd, systemd ile cgo'suz etkileşim sağlar: sd_notify ve soket etkinleştirme.
package sd

import (
	"net"
	"os"
	"strconv"
	"syscall"
)

// Notify, NOTIFY_SOCKET ayarlıysa systemd'ye durum iletir (ör. "READY=1").
// Soket yoksa sessizce hiçbir şey yapmaz.
func Notify(state string) error {
	addr := os.Getenv("NOTIFY_SOCKET")
	if addr == "" {
		return nil
	}
	if addr[0] == '@' {
		addr = "\x00" + addr[1:]
	}
	c, err := net.DialUnix("unixgram", nil, &net.UnixAddr{Name: addr, Net: "unixgram"})
	if err != nil {
		return err
	}
	defer c.Close()
	_, err = c.Write([]byte(state))
	return err
}

// Listeners, systemd soket etkinleştirmesiyle devredilen dinleyicileri döndürür
// (LISTEN_PID/LISTEN_FDS). Devir yoksa nil döner. Ortam değişkenleri temizlenir.
func Listeners() ([]net.Listener, error) {
	files := Files()
	var out []net.Listener
	for _, f := range files {
		l, err := net.FileListener(f)
		f.Close()
		if err != nil {
			return nil, err
		}
		out = append(out, l)
	}
	return out, nil
}

// Files, devredilen dosya tanıtıcılarını döndürür (fd 3'ten başlar).
func Files() []*os.File {
	pid, err := strconv.Atoi(os.Getenv("LISTEN_PID"))
	if err != nil || pid != os.Getpid() {
		return nil
	}
	n, err := strconv.Atoi(os.Getenv("LISTEN_FDS"))
	if err != nil || n <= 0 {
		return nil
	}
	os.Unsetenv("LISTEN_PID")
	os.Unsetenv("LISTEN_FDS")
	os.Unsetenv("LISTEN_FDNAMES")
	files := make([]*os.File, 0, n)
	for fd := 3; fd < 3+n; fd++ {
		syscall.CloseOnExec(fd)
		files = append(files, os.NewFile(uintptr(fd), "LISTEN_FD_"+strconv.Itoa(fd)))
	}
	return files
}
