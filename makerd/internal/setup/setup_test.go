package setup

import (
	"sync"
	"testing"
)

func TestStoreOneShot(t *testing.T) {
	s := &Store{Dir: t.TempDir()}
	if s.Done() || s.PrimaryUser() != "" {
		t.Fatal("fresh store done")
	}
	unlock, err := s.Begin()
	if err != nil {
		t.Fatal(err)
	}
	// Eşzamanlı ikinci istek (aynı süreç ya da başka süreç) reddedilir.
	if _, err := s.Begin(); err != ErrBusy {
		t.Fatalf("want busy, got %v", err)
	}
	other := &Store{Dir: s.Dir}
	if _, err := other.Begin(); err != ErrBusy {
		t.Fatalf("flock: want busy, got %v", err)
	}
	if err := s.MarkDone("ayse"); err != nil {
		t.Fatal(err)
	}
	unlock()
	if !s.Done() || s.PrimaryUser() != "ayse" {
		t.Fatal("not done")
	}
	if _, err := s.Begin(); err != ErrDone {
		t.Fatalf("want done, got %v", err)
	}
	_ = s.Reset()
	var wg sync.WaitGroup
	ok := 0
	var mu sync.Mutex
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			u, err := s.Begin()
			if err != nil {
				return
			}
			mu.Lock()
			ok++
			mu.Unlock()
			_ = s.MarkDone("x")
			u()
		}()
	}
	wg.Wait()
	if ok != 1 {
		t.Fatalf("%d completions", ok)
	}
}

func TestLocalAddrAndHostname(t *testing.T) {
	for ip, want := range map[string]bool{
		"127.0.0.1": true, "10.42.0.23": true, "192.168.1.5": true, "172.17.0.1": true, "169.254.3.4": true,
		"::1": true, "fe80::1": true, "fd00::5": true, "::ffff:192.168.1.2": true,
		"8.8.8.8": false, "2001:4860::8888": false, "100.64.0.1": false, "bogus": false,
	} {
		if LocalAddr(ip) != want {
			t.Errorf("%s: want %v", ip, want)
		}
	}
	if SuggestHostname("DC:A6:32:12:AB:CD") != "makeros-abcd" || SuggestHostname("") != "makeros" {
		t.Error("hostname suggestion")
	}
}
