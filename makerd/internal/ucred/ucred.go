// Package ucred, unix soketinin karşı ucunun kimliğini (SO_PEERCRED) okur.
package ucred

import (
	"errors"
	"net"
	"syscall"
)

// Cred karşı ucun süreç kimliğidir.
type Cred struct {
	UID, GID uint32
	PID      int32
}

// Of bağlantının karşı ucunun kimliğini döndürür.
func Of(conn net.Conn) (Cred, error) {
	uc, ok := conn.(*net.UnixConn)
	if !ok {
		return Cred{}, errors.New("unix soketi değil")
	}
	raw, err := uc.SyscallConn()
	if err != nil {
		return Cred{}, err
	}
	var cred *syscall.Ucred
	var cerr error
	if err := raw.Control(func(fd uintptr) {
		cred, cerr = syscall.GetsockoptUcred(int(fd), syscall.SOL_SOCKET, syscall.SO_PEERCRED)
	}); err != nil {
		return Cred{}, err
	}
	if cerr != nil {
		return Cred{}, cerr
	}
	return Cred{UID: cred.Uid, GID: cred.Gid, PID: cred.Pid}, nil
}
