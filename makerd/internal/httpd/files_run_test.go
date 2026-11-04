package httpd

import "testing"

func TestRunFileCmd(t *testing.T) {
	for in, want := range map[string]string{
		"~/Projeler/a/main.py": `cd ~ && cd 'Projeler/a/' && if [ -f pyproject.toml ]; then uv run 'main.py'; else python3 'main.py'; fi`,
		"x.py":                 `cd ~ && cd '.' && if [ -f pyproject.toml ]; then uv run 'x.py'; else python3 'x.py'; fi`,
		"/tmp/it's.py":         `cd '/tmp/' && if [ -f pyproject.toml ]; then uv run 'it'\''s.py'; else python3 'it'\''s.py'; fi`,
	} {
		if got, err := runFileCmd(in); err != nil || got != want {
			t.Errorf("%q:\n got %s (%v)\nwant %s", in, got, err, want)
		}
	}
	for _, bad := range []string{"a.sh", "a\n.py", ""} {
		if _, err := runFileCmd(bad); err == nil {
			t.Errorf("%q kabul edilmemeliydi", bad)
		}
	}
}
