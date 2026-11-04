package priv

import (
	"crypto/rand"
	"crypto/sha512"
	"crypto/subtle"
	"errors"
	"strconv"
	"strings"
)

// SHA-512 crypt ($6$), Ulrich Drepper'ın "Unix crypt using SHA-256 and SHA-512"
// belirtimine göre saf Go uygulaması. yescrypt ($y$) desteklenmez: MakerOS
// imajı /etc/login.defs içinde ENCRYPT_METHOD SHA512 ile yapılandırılır.

const (
	itoa64        = "./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
	roundsDefault = 5000
	roundsMin     = 1000
	roundsMax     = 999999999
	saltMax       = 16
	newHashRounds = 65536
	sha512Prefix  = "$6$"
	roundsPrefix  = "rounds="
)

// ErrUnsupportedHash, tanınmayan bir parola özeti biçimidir (ör. yescrypt).
var ErrUnsupportedHash = errors.New("desteklenmeyen parola özeti biçimi")

// Verify, parolayı crypt(3) biçimli özetle karşılaştırır.
func Verify(password, hash string) (bool, error) {
	if !strings.HasPrefix(hash, sha512Prefix) {
		return false, ErrUnsupportedHash
	}
	got, err := SHA512Crypt(password, hash)
	if err != nil {
		return false, err
	}
	return subtle.ConstantTimeCompare([]byte(got), []byte(hash)) == 1, nil
}

// HashPassword, yeni rastgele tuzla $6$rounds=65536$ özeti üretir.
func HashPassword(password string) (string, error) {
	raw := make([]byte, saltMax)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	salt := make([]byte, saltMax)
	for i, b := range raw {
		salt[i] = itoa64[int(b)&0x3f]
	}
	return SHA512Crypt(password, sha512Prefix+roundsPrefix+strconv.Itoa(newHashRounds)+"$"+string(salt))
}

// SHA512Crypt, "$6$[rounds=N$]salt[$...]" ayarıyla özeti hesaplar.
func SHA512Crypt(password, setting string) (string, error) {
	if !strings.HasPrefix(setting, sha512Prefix) {
		return "", ErrUnsupportedHash
	}
	rest := setting[len(sha512Prefix):]
	rounds, custom := roundsDefault, false
	if strings.HasPrefix(rest, roundsPrefix) {
		num, after, ok := strings.Cut(rest[len(roundsPrefix):], "$")
		if !ok {
			return "", errors.New("geçersiz rounds alanı")
		}
		n, err := strconv.Atoi(num)
		if err != nil {
			return "", errors.New("geçersiz rounds alanı")
		}
		rounds, custom, rest = min(max(n, roundsMin), roundsMax), true, after
	}
	salt, _, _ := strings.Cut(rest, "$")
	if len(salt) > saltMax {
		salt = salt[:saltMax]
	}
	sum := sha512Sum([]byte(password), []byte(salt), rounds)
	var b strings.Builder
	b.WriteString(sha512Prefix)
	if custom {
		b.WriteString(roundsPrefix + strconv.Itoa(rounds) + "$")
	}
	b.WriteString(salt)
	b.WriteByte('$')
	for i := 0; i < 21; i++ {
		x, y, z := sum[i], sum[i+21], sum[i+42]
		switch i % 3 {
		case 1:
			x, y, z = y, z, x
		case 2:
			x, y, z = z, x, y
		}
		b64From24(&b, x, y, z, 4)
	}
	b64From24(&b, 0, 0, sum[63], 2)
	return b.String(), nil
}

func b64From24(b *strings.Builder, b2, b1, b0 byte, n int) {
	w := uint(b2)<<16 | uint(b1)<<8 | uint(b0)
	for ; n > 0; n-- {
		b.WriteByte(itoa64[w&0x3f])
		w >>= 6
	}
}

func repeatTo(src []byte, n int) []byte {
	out := make([]byte, 0, n)
	for len(out) < n {
		out = append(out, src[:min(len(src), n-len(out))]...)
	}
	return out
}

func sha512Sum(p, s []byte, rounds int) []byte {
	hb := sha512.New()
	hb.Write(p)
	hb.Write(s)
	hb.Write(p)
	B := hb.Sum(nil)

	ha := sha512.New()
	ha.Write(p)
	ha.Write(s)
	ha.Write(repeatTo(B, len(p)))
	for i := len(p); i > 0; i >>= 1 {
		if i&1 != 0 {
			ha.Write(B)
		} else {
			ha.Write(p)
		}
	}
	A := ha.Sum(nil)

	hp := sha512.New()
	for range p {
		hp.Write(p)
	}
	P := repeatTo(hp.Sum(nil), len(p))

	hs := sha512.New()
	for i := 0; i < 16+int(A[0]); i++ {
		hs.Write(s)
	}
	S := repeatTo(hs.Sum(nil), len(s))

	C := A
	for r := 0; r < rounds; r++ {
		h := sha512.New()
		if r&1 != 0 {
			h.Write(P)
		} else {
			h.Write(C)
		}
		if r%3 != 0 {
			h.Write(S)
		}
		if r%7 != 0 {
			h.Write(P)
		}
		if r&1 != 0 {
			h.Write(C)
		} else {
			h.Write(P)
		}
		C = h.Sum(nil)
	}
	return C
}
