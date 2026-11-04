package httpd

import (
	"net/http"

	"github.com/ferhatozcelik/maker-os/makerd/internal/agents"
	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/pyman"
	"github.com/ferhatozcelik/maker-os/makerd/internal/term"
)

// Python, projeler ve AI ajanları. Uzun işler terminal oturumunda çalışır;
// yanıt {"term":{"id","ticket"}} döner ve kabuk Terminal penceresini o oturuma bağlar
// (aynı oturum Pi ekranından da izlenebilir).

func (s *Server) termRun(w http.ResponseWriter, r *http.Request, cmd, title string) {
	sess, err := s.Term.CreateWith(sessionOf(r).User, 100, 30, term.Opts{Cmd: cmd, Title: title})
	if err != nil {
		writeErr(w, err)
		return
	}
	t, err := s.Term.Ticket(sess.ID, sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, map[string]any{"term": termResp{ID: sess.ID, Ticket: t}})
}

func (s *Server) needAgent(w http.ResponseWriter) bool {
	if s.Agent == nil || s.Term == nil {
		writeErr(w, apierr.Unavailable("Kullanıcı hizmeti yok."))
		return false
	}
	return true
}

func (s *Server) pythonList(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	v, err := pyman.Versions(r.Context(), s.Agent, sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, v)
}

func (s *Server) pythonOp(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	var req struct {
		Version string `json:"version"`
	}
	if !decode(w, r, &req) {
		return
	}
	cmd, err := pyman.PythonCmd(r.PathValue("op"), req.Version)
	if err != nil {
		writeErr(w, err)
		return
	}
	s.termRun(w, r, cmd, "Python "+req.Version)
}

func (s *Server) projectTemplates(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, 200, pyman.Templates())
}

func (s *Server) projectList(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	p, err := pyman.Projects(r.Context(), s.Agent, sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, p)
}

func (s *Server) projectCreate(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	var req struct {
		Name, Template, Python string
	}
	if !decode(w, r, &req) {
		return
	}
	cmd, err := pyman.Create(r.Context(), s.Agent, sessionOf(r).User, req.Name, req.Template, req.Python)
	if err != nil {
		writeErr(w, err)
		return
	}
	s.termRun(w, r, cmd, req.Name+" · kurulum")
}

func (s *Server) projectRun(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	cmd, err := pyman.RunCmd(r.Context(), s.Agent, sessionOf(r).User, r.PathValue("name"))
	if err != nil {
		writeErr(w, err)
		return
	}
	s.termRun(w, r, cmd, r.PathValue("name"))
}

func (s *Server) projectDelete(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	if err := pyman.Delete(r.Context(), s.Agent, sessionOf(r).User, r.PathValue("name")); err != nil {
		writeErr(w, err)
		return
	}
	noContent(w)
}

func (s *Server) agentList(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	l, err := agents.List(r.Context(), s.Agent, sessionOf(r).User)
	if err != nil {
		writeErr(w, err)
		return
	}
	writeJSON(w, 200, l)
}

func (s *Server) agentOp(w http.ResponseWriter, r *http.Request) {
	if !s.needAgent(w) {
		return
	}
	cmd, title, err := agents.Cmd(r.PathValue("id"), r.PathValue("op"))
	if err != nil {
		writeErr(w, err)
		return
	}
	s.termRun(w, r, cmd, title)
}
