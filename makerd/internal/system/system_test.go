package system

import (
	"context"
	"testing"
)

func TestRealInfoFixtures(t *testing.T) {
	r := &Real{Root: "testdata"}
	in, err := r.Info(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if in.OS.Name != "MakerOS" || in.OS.Version != "2.0.0" || in.Model != "Raspberry Pi 4 Model B Rev 1.5" || in.UptimeS != 3600 {
		t.Errorf("%+v", in)
	}
	if in.CPU.TempC == nil || *in.CPU.TempC != 47.2 || in.CPU.Load != [3]float64{0.52, 0.40, 0.31} {
		t.Errorf("cpu %+v", in.CPU)
	}
	if in.Mem.Total != 3981000*1024 || in.Mem.Available != 3000000*1024 || in.Mem.Used != 981000*1024 || in.Mem.ZramUsed != 8192 {
		t.Errorf("mem %+v", in.Mem)
	}
	if !in.Throttled {
		t.Error("throttled bit 0 set but false")
	}
	if in.Disk.Total <= 0 {
		t.Error("disk")
	}
}

func TestValidators(t *testing.T) {
	for s, want := range map[string]bool{"tr_TR.UTF-8": true, "C.UTF-8": true, "en_US": true, "x;rm": false} {
		if ValidLang(s) != want {
			t.Errorf("lang %q", s)
		}
	}
	for s, want := range map[string]bool{"Europe/Istanbul": true, "America/Argentina/Buenos_Aires": true, "UTC": true, "../etc": false} {
		if ValidTimezone(s) != want {
			t.Errorf("tz %q", s)
		}
	}
}

func TestCPUUsage(t *testing.T) {
	a, ok := ParseCPUStat("cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3 4 5\n")
	if !ok || a.Total != 1000 || a.Idle != 800 {
		t.Fatalf("%+v %v", a, ok)
	}
	b, _ := ParseCPUStat("cpu  150 0 150 750 150 0 0 0 5 5\n")
	if got := CPUUsage(a, b); got != 50 {
		t.Fatalf("usage %v", got)
	}
	if CPUUsage(b, a) != 0 || CPUUsage(a, a) != 0 {
		t.Fatal("non-increasing sample")
	}
	if _, ok := ParseCPUStat("intr 1\n"); ok {
		t.Fatal("no cpu line")
	}
}
