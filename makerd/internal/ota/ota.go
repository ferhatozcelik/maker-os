// Package ota, MakerOS yazılım güncellemesidir: GitHub Releases'te yayınlanan imzalı RAUC paketini
// (makeros-<sürüm>-rpi4.raucb) bulur, indirir (SHA-256 doğrulaması), RAUC ile boştaki A/B slotuna kurar
// ve Pi'yi tek seferlik deneme açılışıyla (tryboot) yeni slottan başlatır. Yeni slot sağlıklı açılırsa
// kalıcı olur (makeros-mark-good); açılmazsa bir sonraki açılışta eski slota dönülür.
// İmza doğrulamasını RAUC yapar (/etc/rauc/keyring.pem); makerd yalnız taşıyıcıdır.
package ota

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Board paket adındaki kart son eki.
const Board = "rpi4"

// Release, kurulabilir bir sürümdür.
type Release struct {
	Version   string    `json:"version"`
	Tag       string    `json:"tag"`
	Name      string    `json:"name"`
	Notes     string    `json:"notes"`
	Published time.Time `json:"published"`
	URL       string    `json:"url"` // GitHub sürüm sayfası
	Size      int64     `json:"size"`
	bundleURL string
	sumURL    string
}

// Status, GET /api/v1/system/update yanıtıdır.
type Status struct {
	Current   string    `json:"current"`
	Repo      string    `json:"repo"`
	Checked   time.Time `json:"checked,omitzero"`
	Available *Release  `json:"available,omitempty"` // yalnız mevcut sürümden yeniyse
	Error     string    `json:"error,omitempty"`     // son denetim hatası
	Phase     string    `json:"phase"`               // idle | downloading | verifying | installing | rebooting | error
	Progress  int       `json:"progress"`
	Message   string    `json:"message,omitempty"`
	Job       string    `json:"job,omitempty"`
	CanApply  bool      `json:"can_apply"` // bu sistem kurulum yapabilir (gerçek cihaz; mock'ta benzetim)
}

// InstallState, kurulum aracının (RAUC) anlık durumudur.
type InstallState struct {
	Operation string // idle | installing
	Progress  int
	Message   string
	LastError string
}

// Installer, paketi kuran ve cihazı deneme açılışıyla başlatan arka uçtur (gerçek: makerd-priv → RAUC).
type Installer interface {
	Install(ctx context.Context, path string) error
	State(ctx context.Context) (InstallState, error)
	Reboot(ctx context.Context) error
}

// Progress iş ilerleme bildirimidir (jobs.Handle.Step ile uyumlu).
type Progress func(pct int, line string)

// Updater güncelleme durum makinesidir.
type Updater struct {
	Repo      string // ör. ferhatozcelik/maker-os
	Current   string // çalışan sürüm (makerd -X main.Version)
	Dir       string // indirme dizini (makerd-priv bunu kabul eder: /var/lib/makerd/ota)
	API       string // https://api.github.com (testte değiştirilir)
	HTTP      *http.Client
	Installer Installer
	Log       *slog.Logger
	// RebootDelay kurulumdan sonra yeniden başlatmadan önce bekleme (arayüz "yeniden başlatılıyor" gösterir).
	RebootDelay time.Duration

	mu      sync.Mutex
	st      Status
	latest  *Release
	running bool
}

func (u *Updater) api() string {
	if u.API != "" {
		return strings.TrimRight(u.API, "/")
	}
	return "https://api.github.com"
}

func (u *Updater) client() *http.Client {
	if u.HTTP != nil {
		return u.HTTP
	}
	return &http.Client{Timeout: 30 * time.Minute}
}

// Status anlık durumu döndürür.
func (u *Updater) Status() Status {
	u.mu.Lock()
	defer u.mu.Unlock()
	s := u.st
	s.Current, s.Repo, s.CanApply = u.Current, u.Repo, u.Installer != nil
	if s.Phase == "" {
		s.Phase = "idle"
	}
	if u.latest != nil && Newer(u.latest.Version, u.Current) {
		r := *u.latest
		s.Available = &r
	}
	return s
}

type ghAsset struct {
	Name string `json:"name"`
	Size int64  `json:"size"`
	URL  string `json:"browser_download_url"`
}
type ghRelease struct {
	Tag        string    `json:"tag_name"`
	Name       string    `json:"name"`
	Body       string    `json:"body"`
	Draft      bool      `json:"draft"`
	Prerelease bool      `json:"prerelease"`
	Published  time.Time `json:"published_at"`
	HTMLURL    string    `json:"html_url"`
	Assets     []ghAsset `json:"assets"`
}

// Check GitHub'daki sürümleri okur: imzalı RAUC paketi olan en yeni (taslak/ön sürüm olmayan) sürüm.
// v1 (Pardus) sürümlerinde RAUC paketi yoktur; onlar hiç önerilmez.
func (u *Updater) Check(ctx context.Context) (Status, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", u.api()+"/repos/"+u.Repo+"/releases?per_page=20", nil)
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "makerd/"+u.Current)
	cctx, cancel := context.WithTimeout(ctx, 20*time.Second)
	defer cancel()
	res, err := u.client().Do(req.WithContext(cctx))
	if err == nil && res.StatusCode != 200 {
		err = fmt.Errorf("GitHub yanıtı %s", res.Status)
		res.Body.Close()
	}
	if err != nil {
		u.mu.Lock()
		u.st.Error, u.st.Checked = "Güncellemeler denetlenemedi: "+err.Error(), time.Now()
		u.mu.Unlock()
		return u.Status(), err
	}
	defer res.Body.Close()
	var list []ghRelease
	if err := json.NewDecoder(io.LimitReader(res.Body, 4<<20)).Decode(&list); err != nil {
		return u.Status(), err
	}
	var best *Release
	for _, r := range list {
		if r.Draft || r.Prerelease {
			continue
		}
		v := strings.TrimPrefix(r.Tag, "v")
		rel := &Release{Version: v, Tag: r.Tag, Name: r.Name, Notes: r.Body, Published: r.Published, URL: r.HTMLURL}
		want := "makeros-" + v + "-" + Board + ".raucb"
		for _, a := range r.Assets {
			switch a.Name {
			case want:
				rel.bundleURL, rel.Size = a.URL, a.Size
			case want + ".sha256":
				rel.sumURL = a.URL
			}
		}
		if rel.bundleURL == "" || rel.sumURL == "" {
			continue
		}
		if best == nil || Newer(rel.Version, best.Version) {
			best = rel
		}
	}
	u.mu.Lock()
	u.latest, u.st.Error, u.st.Checked = best, "", time.Now()
	u.mu.Unlock()
	return u.Status(), nil
}

// Apply mevcut en yeni sürümü indirir, doğrular, kurar ve cihazı yeniden başlatır (uzun iş).
func (u *Updater) Apply(ctx context.Context, job string, step Progress) error {
	u.mu.Lock()
	if u.running {
		u.mu.Unlock()
		return errors.New("Bir güncelleme zaten sürüyor.")
	}
	if u.Installer == nil {
		u.mu.Unlock()
		return errors.New("Bu sistem güncelleme kuramıyor.")
	}
	rel := u.latest
	if rel == nil || !Newer(rel.Version, u.Current) {
		u.mu.Unlock()
		return errors.New("Kurulacak yeni bir sürüm yok.")
	}
	u.running = true
	u.st.Job = job
	u.mu.Unlock()
	defer func() { u.mu.Lock(); u.running = false; u.mu.Unlock() }()

	set := func(phase string, pct int, msg string) {
		u.mu.Lock()
		u.st.Phase, u.st.Progress, u.st.Message = phase, pct, msg
		u.mu.Unlock()
		step(pct, msg)
	}
	fail := func(err error) error {
		u.mu.Lock()
		u.st.Phase, u.st.Message = "error", err.Error()
		u.mu.Unlock()
		return err
	}

	if err := os.MkdirAll(u.Dir, 0o750); err != nil {
		return fail(err)
	}
	// Eski paketler (önceki denemeler) silinir: /data alanı boşa gitmesin.
	if old, _ := filepath.Glob(filepath.Join(u.Dir, "*.raucb*")); len(old) > 0 {
		for _, f := range old {
			_ = os.Remove(f)
		}
	}
	path := filepath.Join(u.Dir, "makeros-"+rel.Version+"-"+Board+".raucb")

	set("downloading", 1, "MakerOS "+rel.Version+" indiriliyor…")
	want, err := u.fetchSum(ctx, rel.sumURL)
	if err != nil {
		return fail(fmt.Errorf("özet dosyası alınamadı: %w", err))
	}
	got, err := u.download(ctx, rel, path, func(done, total int64) {
		if total > 0 {
			set("downloading", 1+int(done*59/total), fmt.Sprintf("İndiriliyor: %d / %d MB", done>>20, total>>20))
		}
	})
	if err != nil {
		_ = os.Remove(path)
		return fail(fmt.Errorf("indirme başarısız: %w", err))
	}
	set("verifying", 61, "Paket doğrulanıyor…")
	if got != want {
		_ = os.Remove(path)
		return fail(errors.New("paket özeti eşleşmiyor (bozuk ya da yarım indirme)"))
	}

	set("installing", 62, "Yedek bölüme kuruluyor…")
	if err := u.Installer.Install(ctx, path); err != nil {
		return fail(fmt.Errorf("kurulum başlatılamadı: %w", err))
	}
	saw := false
	deadline := time.Now().Add(25 * time.Minute)
	for {
		if time.Now().After(deadline) {
			return fail(errors.New("kurulum zaman aşımına uğradı"))
		}
		select {
		case <-ctx.Done():
			return fail(ctx.Err())
		case <-time.After(time.Second):
		}
		s, err := u.Installer.State(ctx)
		if err != nil {
			continue
		}
		if s.Operation == "installing" {
			saw = true
			set("installing", 62+s.Progress*36/100, strings.TrimSpace("Kuruluyor: "+s.Message))
			continue
		}
		if !saw && s.Progress < 100 && s.LastError == "" {
			continue // henüz başlamadı
		}
		if s.LastError != "" {
			_ = os.Remove(path)
			return fail(errors.New("RAUC: " + s.LastError))
		}
		break
	}
	_ = os.Remove(path)
	set("rebooting", 100, "Kuruldu. Yeni sürümle yeniden başlatılıyor…")
	u.Log.Info("güncelleme kuruldu, deneme açılışı", "version", rel.Version)
	d := u.RebootDelay
	if d == 0 {
		d = 4 * time.Second
	}
	time.Sleep(d)
	if err := u.Installer.Reboot(context.WithoutCancel(ctx)); err != nil {
		return fail(fmt.Errorf("yeniden başlatılamadı: %w", err))
	}
	return nil
}

func (u *Updater) fetchSum(ctx context.Context, url string) (string, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", url, nil)
	req.Header.Set("User-Agent", "makerd/"+u.Current)
	res, err := u.client().Do(req)
	if err != nil {
		return "", err
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		return "", fmt.Errorf("%s", res.Status)
	}
	b, _ := io.ReadAll(io.LimitReader(res.Body, 4096))
	f := strings.Fields(string(b))
	if len(f) == 0 || len(f[0]) != 64 {
		return "", errors.New("geçersiz özet dosyası")
	}
	return strings.ToLower(f[0]), nil
}

func (u *Updater) download(ctx context.Context, rel *Release, path string, prog func(done, total int64)) (string, error) {
	req, _ := http.NewRequestWithContext(ctx, "GET", rel.bundleURL, nil)
	req.Header.Set("User-Agent", "makerd/"+u.Current)
	res, err := u.client().Do(req)
	if err != nil {
		return "", err
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		return "", fmt.Errorf("%s", res.Status)
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o640)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	total := res.ContentLength
	if total <= 0 {
		total = rel.Size
	}
	buf := make([]byte, 256<<10)
	var done int64
	last := time.Time{}
	for {
		n, rerr := res.Body.Read(buf)
		if n > 0 {
			if _, err := f.Write(buf[:n]); err != nil {
				return "", err
			}
			h.Write(buf[:n])
			done += int64(n)
			if time.Since(last) > 500*time.Millisecond {
				prog(done, total)
				last = time.Now()
			}
		}
		if rerr == io.EOF {
			break
		}
		if rerr != nil {
			return "", rerr
		}
	}
	prog(done, total)
	if err := f.Sync(); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

var verRe = regexp.MustCompile(`^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$`)

// Newer a, b'den yeni mi (anlamsal sürüm; ön sürüm eki olan aynı numaralı sürümden eskidir:
// 2.0.0-dev < 2.0.0). Çözümlenemeyen sürüm hiçbir zaman yeni sayılmaz.
func Newer(a, b string) bool {
	ma, mb := verRe.FindStringSubmatch(a), verRe.FindStringSubmatch(b)
	if ma == nil {
		return false
	}
	if mb == nil {
		return true
	}
	for i := 1; i <= 3; i++ {
		x, _ := strconv.Atoi(ma[i])
		y, _ := strconv.Atoi(mb[i])
		if x != y {
			return x > y
		}
	}
	// Aynı numara: ön sürümsüz olan daha yenidir.
	switch {
	case ma[4] == "" && mb[4] != "":
		return true
	case ma[4] != "" && mb[4] == "":
		return false
	default:
		return ma[4] > mb[4]
	}
}
