package network

import "testing"

func TestSecurity(t *testing.T) {
	cases := []struct {
		flags, wpa, rsn uint32
		want            string
	}{
		{0, 0, 0, "open"}, {1, 0, 0, "wpa2"}, {1, 0, secPSK, "wpa2"}, {1, secPSK, secPSK, "wpa2"},
		{1, 0, secSAE, "wpa3"}, {1, 0, secSAE | secPSK, "wpa2"}, {1, 0, sec8021X, "enterprise"},
	}
	for _, c := range cases {
		if got := Security(c.flags, c.wpa, c.rsn); got != c.want {
			t.Errorf("Security(%x,%x,%x) = %s want %s", c.flags, c.wpa, c.rsn, got, c.want)
		}
	}
}

func TestDedupeAPs(t *testing.T) {
	in := []AP{
		{SSID: "A", Signal: 30, Active: true}, {SSID: "A", Signal: 70}, {SSID: "B", Signal: 90},
		{SSID: "", Signal: 99}, {SSID: HotspotSSID, Signal: 99}, {SSID: "C", Signal: 50, Known: true},
	}
	out := DedupeAPs(in)
	if len(out) != 3 || out[0].SSID != "A" || out[0].Signal != 70 || !out[0].Active || out[1].SSID != "B" || out[2].SSID != "C" {
		t.Fatalf("%+v", out)
	}
	if !ValidSSID("Kafe") || ValidSSID("") || ValidSSID("çççççççççççççççççç") {
		t.Error("ValidSSID")
	}
}
