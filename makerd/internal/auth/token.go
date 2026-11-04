package auth

import (
	"os"
	"path/filepath"
)

// WriteToken, yerel kabuk el sıkışması için yeni bir belirteç üretir ve
// dosyaya atomik olarak (0640) yazar. Kiosk tarayıcısını başlatan betik
// bu dosyayı okuyup /api/v1/auth/local?token=... adresini açar.
func WriteToken(path string) (string, error) {
	token := RandomToken(24)
	if err := os.MkdirAll(filepath.Dir(path), 0o750); err != nil {
		return "", err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, []byte(token), 0o640); err != nil {
		return "", err
	}
	if err := os.Chmod(tmp, 0o640); err != nil {
		return "", err
	}
	if err := os.Rename(tmp, path); err != nil {
		return "", err
	}
	return token, nil
}
