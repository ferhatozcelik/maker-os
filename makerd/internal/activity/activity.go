// Package activity: /proc ayrıştırma ile süreç listesi. CPU yüzdesi iki örnek
// arasındaki tik farkından (tek çekirdek = %100, top gibi) hesaplanır; cgroup
// dilimi /proc/<pid>/cgroup'tan okunur.
package activity

import (
	"bufio"
	"context"
	"errors"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
)

// Process, GET /processes öğesi.
type Process struct {
	PID    int     `json:"pid"`
	Name   string  `json:"name"`
	User   string  `json:"user"`
	CPUPct float64 `json:"cpu_pct"`
	RSS    int64   `json:"rss"`
	Slice  string  `json:"slice"`
}

// Source süreç arka ucudur.
type Source interface {
	Processes(ctx context.Context) ([]Process, error)
	Signal(ctx context.Context, pid int, sig string) error
}

// MaxList dönen en fazla süreç sayısı.
const MaxList = 100

const clkTck = 100 // Linux arm64/amd64 USER_HZ

// Stat, /proc/<pid>/stat'ın gerekli alanları.
type Stat struct {
	PID   int
	Comm  string
	Ticks uint64 // utime+stime
	Start uint64 // starttime
	RSS   int64  // sayfa
}

// ParseStat bir /proc/<pid>/stat satırını ayrıştırır. comm boşluk ve ')' içerebilir.
func ParseStat(s string) (Stat, error) {
	open := strings.IndexByte(s, '(')
	end := strings.LastIndexByte(s, ')')
	if open < 0 || end < open || end+2 > len(s) {
		return Stat{}, errors.New("geçersiz stat")
	}
	pid, err := strconv.Atoi(strings.TrimSpace(s[:open]))
	if err != nil {
		return Stat{}, err
	}
	f := strings.Fields(s[end+2:]) // f[0] = alan 3 (state)
	if len(f) < 22 {
		return Stat{}, errors.New("eksik stat alanı")
	}
	ut, _ := strconv.ParseUint(f[11], 10, 64)
	st, _ := strconv.ParseUint(f[12], 10, 64)
	start, _ := strconv.ParseUint(f[19], 10, 64)
	rss, _ := strconv.ParseInt(f[21], 10, 64)
	return Stat{PID: pid, Comm: s[open+1 : end], Ticks: ut + st, Start: start, RSS: max(0, rss)}, nil
}

// SliceOf, cgroup v2 yolundaki en derin ".slice" bileşenini döndürür.
// "0::/user.slice/user-1000.slice/session-2.scope" → "user-1000.slice".
func SliceOf(cgroup string) string {
	path := ""
	for _, l := range strings.Split(cgroup, "\n") {
		if p, ok := strings.CutPrefix(l, "0::"); ok {
			path = p
			break
		}
	}
	slice := ""
	for _, part := range strings.Split(path, "/") {
		if strings.HasSuffix(part, ".slice") {
			slice = part
		}
	}
	return slice
}

// UIDOf /proc/<pid>/status içindeki gerçek UID'yi okur.
func UIDOf(status string) (int, bool) {
	for _, l := range strings.Split(status, "\n") {
		if v, ok := strings.CutPrefix(l, "Uid:"); ok {
			if f := strings.Fields(v); len(f) > 0 {
				n, err := strconv.Atoi(f[0])
				return n, err == nil
			}
		}
	}
	return 0, false
}

type meta struct {
	start uint64
	uid   int
	slice string
}

type sample struct {
	at    time.Time
	ticks map[int][2]uint64 // pid → (ticks, start)
	list  []Process
}

// Proc gerçek /proc okuyucusudur.
type Proc struct {
	Root     string // "/" (testte fikstür)
	Passwd   string // /etc/passwd
	PageSize int64
	SelfUID  int // sinyal yetkisi: yalnız bu UID'nin süreçleri
	SelfPID  int

	mu     sync.Mutex
	last   *sample
	meta   map[int]meta
	users  map[int]string
	usersT time.Time
	now    func() time.Time
}

// NewProc sistem için okuyucu oluşturur.
func NewProc() *Proc {
	return &Proc{Root: "/", Passwd: "/etc/passwd", PageSize: int64(os.Getpagesize()), SelfUID: os.Getuid(), SelfPID: os.Getpid()}
}

func (p *Proc) clock() time.Time {
	if p.now != nil {
		return p.now()
	}
	return time.Now()
}

func (p *Proc) userName(uid int) string {
	if p.users == nil || p.clock().Sub(p.usersT) > time.Minute {
		p.users = map[int]string{}
		p.usersT = p.clock()
		if f, err := os.Open(p.Passwd); err == nil {
			sc := bufio.NewScanner(f)
			for sc.Scan() {
				parts := strings.Split(sc.Text(), ":")
				if len(parts) > 2 {
					if id, err := strconv.Atoi(parts[2]); err == nil {
						p.users[id] = parts[0]
					}
				}
			}
			f.Close()
		}
	}
	if n, ok := p.users[uid]; ok {
		return n
	}
	return strconv.Itoa(uid)
}

func (p *Proc) read(pid int, file string) string {
	b, _ := os.ReadFile(filepath.Join(p.Root, "proc", strconv.Itoa(pid), file))
	return string(b)
}

// take tek bir örnek alır (önceki örneğe göre CPU yüzdesiyle).
func (p *Proc) take(prev *sample) (*sample, error) {
	entries, err := os.ReadDir(filepath.Join(p.Root, "proc"))
	if err != nil {
		return nil, err
	}
	now := p.clock()
	s := &sample{at: now, ticks: map[int][2]uint64{}}
	var dt float64
	if prev != nil {
		dt = now.Sub(prev.at).Seconds()
	}
	if p.meta == nil {
		p.meta = map[int]meta{}
	}
	seen := map[int]bool{}
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil {
			continue
		}
		st, err := ParseStat(p.read(pid, "stat"))
		if err != nil {
			continue // okunurken sonlanan süreç
		}
		seen[pid] = true
		m, ok := p.meta[pid]
		if !ok || m.start != st.Start {
			m = meta{start: st.Start, slice: SliceOf(p.read(pid, "cgroup"))}
			m.uid, _ = UIDOf(p.read(pid, "status"))
			p.meta[pid] = m
		}
		proc := Process{PID: pid, Name: st.Comm, User: p.userName(m.uid), RSS: st.RSS * p.PageSize, Slice: m.slice}
		if prev != nil && dt > 0 {
			if old, ok := prev.ticks[pid]; ok && old[1] == st.Start && st.Ticks >= old[0] {
				proc.CPUPct = math.Round(float64(st.Ticks-old[0])/clkTck/dt*1000) / 10
			}
		}
		s.ticks[pid] = [2]uint64{st.Ticks, st.Start}
		s.list = append(s.list, proc)
	}
	for pid := range p.meta {
		if !seen[pid] {
			delete(p.meta, pid)
		}
	}
	sort.Slice(s.list, func(i, j int) bool {
		a, b := s.list[i], s.list[j]
		if a.CPUPct != b.CPUPct {
			return a.CPUPct > b.CPUPct
		}
		if a.RSS != b.RSS {
			return a.RSS > b.RSS
		}
		return a.PID < b.PID
	})
	return s, nil
}

// Processes en çok CPU kullanan ilk 100 süreci döndürür. Son örnek 1 sn'den
// yeniyse önbellekten verilir; önceki örnek yoksa kısa aralıkla iki örnek alınır.
func (p *Proc) Processes(ctx context.Context) ([]Process, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	now := p.clock()
	if p.last != nil && now.Sub(p.last.at) < time.Second {
		return limit(p.last.list), nil
	}
	prev := p.last
	if prev == nil || now.Sub(prev.at) > 30*time.Second {
		var err error
		if prev, err = p.take(nil); err != nil {
			return nil, err
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(250 * time.Millisecond):
		}
	}
	s, err := p.take(prev)
	if err != nil {
		return nil, err
	}
	p.last = s
	return limit(s.list), nil
}

func limit(l []Process) []Process {
	if len(l) > MaxList {
		l = l[:MaxList]
	}
	return append([]Process(nil), l...)
}

// ParseSignal sözleşmedeki sinyal adını çevirir.
func ParseSignal(s string) (syscall.Signal, bool) {
	switch s {
	case "TERM":
		return syscall.SIGTERM, true
	case "KILL":
		return syscall.SIGKILL, true
	}
	return 0, false
}

// Signal, yalnız makerd ile aynı kullanıcıya ait süreçlere sinyal gönderir.
func (p *Proc) Signal(_ context.Context, pid int, sig string) error {
	s, ok := ParseSignal(sig)
	if !ok {
		return apierr.Invalid("Sinyal TERM veya KILL olmalıdır.")
	}
	if pid <= 1 || pid == p.SelfPID {
		return apierr.Forbidden("Bu sürece sinyal gönderilemez.")
	}
	uid, ok := UIDOf(p.read(pid, "status"))
	if !ok {
		return apierr.NotFound("Süreç bulunamadı.")
	}
	if uid != p.SelfUID {
		return apierr.Forbidden("Yalnızca kendi süreçlerinizi sonlandırabilirsiniz.")
	}
	if err := syscall.Kill(pid, s); err != nil {
		if errors.Is(err, syscall.ESRCH) {
			return apierr.NotFound("Süreç bulunamadı.")
		}
		return apierr.Forbidden("Sinyal gönderilemedi: " + err.Error())
	}
	return nil
}
