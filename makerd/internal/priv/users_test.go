package priv

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func userFixture(t *testing.T) (*Commands, string) {
	t.Helper()
	d := t.TempDir()
	w := func(name, data string, perm os.FileMode) string {
		p := filepath.Join(d, name)
		if err := os.WriteFile(p, []byte(data), perm); err != nil {
			t.Fatal(err)
		}
		return p
	}
	c := &Commands{MinUID: 1000}
	c.Shadow = w("shadow", "root:*:1::::::\nmaker:!:1:0:99999:7:::\n", 0o600)
	c.Users = UserFiles{
		Passwd:  w("passwd", "root:x:0:0:root:/root:/bin/sh\nmaker:x:1000:1000:MakerOS:"+filepath.Join(d, "home", "maker")+":/bin/sh\nsvc:x:999:999::/:/sbin/nologin\n", 0o644),
		Group:   w("group", "root:x:0:\nwheel:x:10:root\nvideo:x:44:\nmaker:x:1000:\nsvc:x:999:\ndialout:x:20:maker\n", 0o644),
		Gshadow: w("gshadow", "root:::\nwheel:::root\nvideo:::\n", 0o600),
		Lock:    filepath.Join(d, ".pwd.lock"), HomeBase: filepath.Join(d, "home"), Skel: filepath.Join(d, "skel"),
		SetupFlag: filepath.Join(d, "setup-done"), Shells: []string{"/bin/sh"},
	}
	_ = os.MkdirAll(filepath.Join(d, "skel", ".config"), 0o755)
	w("skel/.profile", "export X=1\n", 0o644)
	return c, d
}

func create(c *Commands, a UserCreateArgs) (UserCreateResult, error) {
	raw, _ := json.Marshal(a)
	var res UserCreateResult
	out, err := c.userCreate(context.Background(), Peer{}, raw)
	if err == nil {
		res = out.(UserCreateResult)
	}
	return res, err
}

func TestUserCreate(t *testing.T) {
	c, d := userFixture(t)
	for _, bad := range []UserCreateArgs{
		{User: "Root", Password: "parola1"}, {User: "root", Password: "parola1"}, {User: "ali", Password: "kısa"},
		{User: "ali", Password: "parola1", Fullname: "a:b"}, {User: "1ali", Password: "parola1"},
	} {
		if _, err := create(c, bad); err == nil {
			t.Errorf("accepted %+v", bad)
		}
	}
	if _, err := create(c, UserCreateArgs{User: "svc", Password: "parola1"}); err == nil {
		t.Error("system account adopted")
	}
	res, err := create(c, UserCreateArgs{User: "ayse", Password: "gizli-parola", Fullname: "Ayşe Yılmaz"})
	if err != nil {
		t.Fatal(err)
	}
	if !res.Created || res.UID != 1001 {
		t.Fatalf("%+v", res)
	}
	pw, _ := os.ReadFile(c.Users.Passwd)
	if !strings.Contains(string(pw), "ayse:x:1001:1001:Ayşe Yılmaz:"+filepath.Join(d, "home", "ayse")+":/bin/sh\n") {
		t.Fatalf("passwd:\n%s", pw)
	}
	gr, _ := os.ReadFile(c.Users.Group)
	for _, want := range []string{"wheel:x:10:root,ayse", "video:x:44:ayse", "dialout:x:20:maker,ayse", "ayse:x:1001:"} {
		if !strings.Contains(string(gr), want+"\n") {
			t.Errorf("group missing %q:\n%s", want, gr)
		}
	}
	gs, _ := os.ReadFile(c.Users.Gshadow)
	if !strings.Contains(string(gs), "wheel:::root,ayse\n") || !strings.Contains(string(gs), "ayse:!::\n") {
		t.Errorf("gshadow:\n%s", gs)
	}
	if ok, _ := CheckShadow(c.Shadow, "ayse", "gizli-parola"); !ok {
		t.Error("password not set")
	}
	if st, err := os.Stat(c.Shadow); err != nil || st.Mode().Perm() != 0o600 {
		t.Error("shadow perms changed")
	}
	if b, err := os.ReadFile(filepath.Join(d, "home", "ayse", ".profile")); err != nil || string(b) != "export X=1\n" {
		t.Errorf("skel not copied: %v", err)
	}
	if _, err := create(c, UserCreateArgs{User: "ayse", Password: "yeni-parola"}); err != nil {
		t.Fatalf("retry before setup-done: %v", err)
	}
	if ok, _ := CheckShadow(c.Shadow, "ayse", "yeni-parola"); !ok {
		t.Error("retry did not update password")
	}
	// Kilitli imaj kullanıcısı devralınır.
	res, err = create(c, UserCreateArgs{User: "maker", Password: "parola123", Fullname: "Maker"})
	if err != nil || res.Created || res.UID != 1000 {
		t.Fatalf("adopt: %+v %v", res, err)
	}
	if strings.Count(readFile(c.Users.Group), "dialout:x:20:maker,ayse\n") != 1 {
		t.Error("duplicate group member")
	}
	// Kurulum tamamlandıktan sonra reddedilir.
	_ = os.WriteFile(c.Users.SetupFlag, []byte("{}"), 0o644)
	if _, err := create(c, UserCreateArgs{User: "mehmet", Password: "parola123"}); err == nil {
		t.Fatal("user.create allowed after setup")
	}
}

func readFile(p string) string { b, _ := os.ReadFile(p); return string(b) }
