package auth

import (
	"bufio"
	"os"
	"strings"
)

// AdminGroups, üyeleri yönetici sayılan gruplardır.
var AdminGroups = []string{"wheel", "sudo", "admin"}

// IsAdmin, kullanıcının /etc/group'ta yönetici gruplarından birinde olup olmadığını döndürür.
func IsAdmin(groupFile, user string) bool {
	f, err := os.Open(groupFile)
	if err != nil {
		return false
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		parts := strings.Split(sc.Text(), ":")
		if len(parts) < 4 {
			continue
		}
		admin := false
		for _, g := range AdminGroups {
			if parts[0] == g {
				admin = true
			}
		}
		if !admin {
			continue
		}
		for _, m := range strings.Split(parts[3], ",") {
			if strings.TrimSpace(m) == user {
				return true
			}
		}
	}
	return false
}
