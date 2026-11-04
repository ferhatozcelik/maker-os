package httpd

import (
	"context"
	"net/http"
	"time"

	"github.com/ferhatozcelik/maker-os/makerd/internal/apierr"
	"github.com/ferhatozcelik/maker-os/makerd/internal/jobs"
)

// Yazılım güncellemesi (internal/ota): durum, denetim (GitHub Releases) ve kurulum (yönetici; uzun iş).

func (s *Server) updateAvailable(w http.ResponseWriter) bool {
	if s.Update == nil {
		writeErr(w, apierr.Unavailable("Yazılım güncellemesi bu sistemde yok."))
		return false
	}
	return true
}

func (s *Server) updateStatus(w http.ResponseWriter, r *http.Request) {
	if !s.updateAvailable(w) {
		return
	}
	writeJSON(w, 200, s.Update.Status())
}

func (s *Server) updateCheck(w http.ResponseWriter, r *http.Request) {
	if !s.updateAvailable(w) {
		return
	}
	// GitHub API'si kimliksiz istemciye saatte 60 istek verir: bir dakikadan yeni denetim yeniden yapılmaz.
	if st := s.Update.Status(); time.Since(st.Checked) < time.Minute && st.Error == "" {
		writeJSON(w, 200, st)
		return
	}
	st, err := s.Update.Check(r.Context())
	if err != nil {
		writeErr(w, apierr.Unavailable(st.Error))
		return
	}
	writeJSON(w, 200, st)
}

func (s *Server) updateApply(w http.ResponseWriter, r *http.Request) {
	if !s.updateAvailable(w) {
		return
	}
	st := s.Update.Status()
	if st.Available == nil {
		writeErr(w, apierr.Conflict("Kurulacak yeni bir sürüm yok; önce denetleyin."))
		return
	}
	if st.Phase != "idle" && st.Phase != "error" {
		writeErr(w, apierr.Conflict("Bir güncelleme zaten sürüyor."))
		return
	}
	ready := make(chan string, 1) // iş kimliği, iş başladıktan sonra iletilir (yarış yok)
	id := s.Jobs.StartFor("update", sessionOf(r).User, 45*time.Minute, func(ctx context.Context, h *jobs.Handle) error {
		return s.Update.Apply(ctx, <-ready, h.Step)
	})
	ready <- id
	s.Log.Info("yazılım güncellemesi başladı", "version", st.Available.Version, "user", sessionOf(r).User)
	writeJSON(w, 202, map[string]string{"job": id})
}
