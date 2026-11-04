package priv

import "testing"

func TestValidOTAPath(t *testing.T) {
	ok := []string{OTADir + "/makeros-2.0.1-rpi4.raucb"}
	bad := []string{"", "/tmp/x.raucb", OTADir + "/../x.raucb", OTADir + "/a/b.raucb", OTADir + "/.gizli.raucb",
		OTADir + "/x.raucb.sha256", OTADir + "/x.tar", OTADir + "/", "/var/lib/makerd/otax/x.raucb", OTADir + "/x\x00.raucb"}
	for _, p := range ok {
		if !ValidOTAPath(p) {
			t.Errorf("kabul edilmeliydi: %q", p)
		}
	}
	for _, p := range bad {
		if ValidOTAPath(p) {
			t.Errorf("reddedilmeliydi: %q", p)
		}
	}
}
