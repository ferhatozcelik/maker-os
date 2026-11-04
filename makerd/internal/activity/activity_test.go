package activity

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestParseStat(t *testing.T) {
	cases := []struct {
		in       string
		pid      int
		comm     string
		ticks    uint64
		start    uint64
		rss      int64
		wantFail bool
	}{
		{"1 (systemd) S 0 1 1 0 -1 4194560 100 0 10 0 50 30 0 0 20 0 1 0 5 170000000 2800 0", 1, "systemd", 80, 5, 2800, false},
		{"1240 (Web Content) (x)) R 1 1240 1240 0 -1 0 0 0 0 0 1000 500 0 0 20 0 4 0 5000 900000000 16384 0", 1240, "Web Content) (x)", 1500, 5000, 16384, false},
		{"garbage", 0, "", 0, 0, 0, true},
		{"5 (short) S 1 2 3", 0, "", 0, 0, 0, true},
	}
	for _, c := range cases {
		st, err := ParseStat(c.in)
		if c.wantFail {
			if err == nil {
				t.Errorf("%q: expected error", c.in)
			}
			continue
		}
		if err != nil || st.PID != c.pid || st.Comm != c.comm || st.Ticks != c.ticks || st.Start != c.start || st.RSS != c.rss {
			t.Errorf("%q: got %+v %v", c.in, st, err)
		}
	}
}

func TestSliceAndUID(t *testing.T) {
	for in, want := range map[string]string{
		"0::/user.slice/user-1000.slice/session-2.scope": "user-1000.slice",
		"0::/system.slice/makerd.service":                "system.slice",
		"0::/apps.slice/app-foo.scope":                   "apps.slice",
		"0::/":                                           "",
		"12:cpu:/x\n0::/shell.slice/cog.service\n":       "shell.slice",
	} {
		if got := SliceOf(in); got != want {
			t.Errorf("SliceOf(%q) = %q want %q", in, got, want)
		}
	}
	if uid, ok := UIDOf("Name:\tx\nUid:\t1000\t1000\t1000\t1000\n"); !ok || uid != 1000 {
		t.Error("UIDOf failed")
	}
	if _, ok := UIDOf("Name: x\n"); ok {
		t.Error("UIDOf accepted missing Uid")
	}
}

func copyTree(t *testing.T, src, dst string) {
	t.Helper()
	_ = filepath.Walk(src, func(p string, info os.FileInfo, err error) error {
		rel, _ := filepath.Rel(src, p)
		if info.IsDir() {
			return os.MkdirAll(filepath.Join(dst, rel), 0o755)
		}
		b, _ := os.ReadFile(p)
		return os.WriteFile(filepath.Join(dst, rel), b, 0o644)
	})
}

func TestProcessesCPUDelta(t *testing.T) {
	root := t.TempDir()
	copyTree(t, "testdata/proc", filepath.Join(root, "proc"))
	now := time.Unix(1000, 0)
	p := &Proc{Root: root, Passwd: "testdata/passwd", PageSize: 4096, SelfUID: 1000, now: func() time.Time { return now }}
	first, err := p.take(nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(first.list) != 4 {
		t.Fatalf("want 4 procs, got %d", len(first.list))
	}
	// 2 sn sonra makerd 100 tik (1 sn CPU) daha kullanmış: %50.
	now = now.Add(2 * time.Second)
	stat := filepath.Join(root, "proc/431/stat")
	b, _ := os.ReadFile(stat)
	_ = os.WriteFile(stat, []byte(strings.Replace(string(b), " 200 100 ", " 260 140 ", 1)), 0o644)
	second, err := p.take(first)
	if err != nil {
		t.Fatal(err)
	}
	top := second.list[0]
	if top.PID != 431 || top.CPUPct != 50 || top.User != "maker" || top.Slice != "system.slice" || top.RSS != 3584*4096 {
		t.Fatalf("top = %+v", top)
	}
	for _, pr := range second.list[1:] {
		if pr.CPUPct != 0 {
			t.Errorf("%d cpu %v", pr.PID, pr.CPUPct)
		}
	}
	// Ölen süreç meta'dan temizlenir; aynı pid yeni starttime ile sıfırdan.
	_ = os.RemoveAll(filepath.Join(root, "proc/77"))
	third, _ := p.take(second)
	if len(third.list) != 3 || p.meta[77].start != 0 {
		t.Fatalf("dead pid not pruned: %d", len(third.list))
	}
	// Processes() önbellek: 1 sn içinde aynı liste.
	p.last = third
	got, _ := p.Processes(context.Background())
	if len(got) != 3 {
		t.Fatal(len(got))
	}
}

func TestSignalRules(t *testing.T) {
	root := t.TempDir()
	copyTree(t, "testdata/proc", filepath.Join(root, "proc"))
	p := &Proc{Root: root, SelfUID: 1000, SelfPID: 431}
	cases := []struct {
		pid  int
		sig  string
		code string
	}{
		{431, "TERM", "forbidden"}, // makerd'nin kendisi
		{1, "TERM", "forbidden"},
		{77, "KILL", "forbidden"}, // root süreci
		{1240, "HUP", "invalid"},
		{99999, "TERM", "not_found"},
	}
	for _, c := range cases {
		err := p.Signal(context.Background(), c.pid, c.sig)
		if err == nil || !strings.HasPrefix(err.Error(), c.code) {
			t.Errorf("Signal(%d,%s) = %v, want %s", c.pid, c.sig, err, c.code)
		}
	}
}
