package term

// Ring sabit boyutlu bayt halka tamponudur; yalnız son Size baytı tutar.
type Ring struct {
	buf   []byte
	start int
	n     int
}

// NewRing boyutu verilen halka tampon oluşturur.
func NewRing(size int) *Ring { return &Ring{buf: make([]byte, size)} }

// Write veriyi ekler; taşan eski baytlar silinir.
func (r *Ring) Write(p []byte) {
	size := len(r.buf)
	if len(p) >= size {
		copy(r.buf, p[len(p)-size:])
		r.start, r.n = 0, size
		return
	}
	end := (r.start + r.n) % size
	k := copy(r.buf[end:], p)
	copy(r.buf, p[k:])
	r.n += len(p)
	if r.n > size {
		r.start = (r.start + r.n - size) % size
		r.n = size
	}
}

// Bytes içeriği sıralı bir kopya olarak döndürür.
func (r *Ring) Bytes() []byte {
	out := make([]byte, r.n)
	k := copy(out, r.buf[r.start:min(r.start+r.n, len(r.buf))])
	copy(out[k:], r.buf[:r.n-k])
	return out
}

// Len tutulan bayt sayısıdır.
func (r *Ring) Len() int { return r.n }
