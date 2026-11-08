// Kamera: Raspberry Pi kamera modülleri (Pi 4: unicam+ISP, Pi 5: CFE+PiSP) ve USB kameralar (libcamera).
// Canlı görüntü makerd'nin tek rpicam-vid akışıdır; pencere gizlenince akış durur (CPU tasarrufu).
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, post, BASE } from "/shell/js/api.js";
import { toast, fail } from "/shell/js/appui.js";

app.init({ title: t("app.camera") });
app.chrome({ title: false });
app.chrome({ tone: "dark" });
const img = $(".view img"), msg = $(".msg"), info = $(".info");
let paused = false;
$(".b-gal").append(h("span", { html: icon("image", 14, 2) }), t("cam.gallery"));
$(".b-gal").onclick = () => app.open("makeros.files", mode === "video" ? "~/Videolar/Kamera" : "~/Resimler/Kamera");
const pauseBtn = $(".b-pause");
const setPause = p => { paused = p; pauseBtn.replaceChildren(h("span", { html: icon(p ? "play" : "pause", 14, 2) }), p ? t("cam.resume") : t("cam.pause")); p ? stop() : start(); };
pauseBtn.onclick = () => setPause(!paused);

function start() { if (!paused && app.visible) img.src = `${BASE}/camera/stream?t=${Date.now()}`; }
function stop() { img.removeAttribute("src"); img.src = "data:,"; }
img.onerror = () => { if (img.src.includes("/camera/stream")) { msg.hidden = false; msg.textContent = t("cam.noStream"); } };
img.onload = () => { msg.hidden = true; };
app.onVisibility(v => (v ? start() : stop()));

// --- Video kaydı: canlı MJPEG görüntüsü tarayıcıda kaydedilir (MediaRecorder; MP4, yoksa WebM) ve
// ~/Videolar/Kamera altına yüklenir. Pi'de ikinci bir kamera süreci açılmaz, işlemci yorulmaz.
const canRecord = typeof MediaRecorder !== "undefined" && !!HTMLCanvasElement.prototype.captureStream;
const MIME = ["video/mp4;codecs=avc1", "video/mp4", "video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"].find(m => canRecord && MediaRecorder.isTypeSupported(m));
let mode = "photo", rec = null;
const shot = $(".shot"), timer = $(".timer"), modes = $(".modes");
$(".m-photo").textContent = t("cam.photo"); $(".m-video").textContent = t("cam.video");
if (!MIME) modes.hidden = true;
const setMode = m => {
  if (rec) return;
  mode = m;
  modes.querySelectorAll("button").forEach(b => b.classList.toggle("on", b.classList.contains("m-" + m)));
  shot.classList.toggle("video", m === "video");
  shot.setAttribute("aria-label", m === "video" ? t("cam.record") : t("cam.photo"));
};
$(".m-photo").onclick = () => setMode("photo");
$(".m-video").onclick = () => setMode("video");
const mmss = s => `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;

function startRec() {
  if (!img.naturalWidth) { toast(t("cam.noStream")); return; }
  const c = Object.assign(document.createElement("canvas"), { width: img.naturalWidth, height: img.naturalHeight });
  const g = c.getContext("2d");
  const draw = setInterval(() => { try { g.drawImage(img, 0, 0, c.width, c.height); } catch { /* kare yok */ } }, 1000 / 30);
  const r = new MediaRecorder(c.captureStream(30), { mimeType: MIME, videoBitsPerSecond: 4_000_000 });
  const parts = [];
  r.ondataavailable = e => e.data.size && parts.push(e.data);
  const t0 = Date.now();
  const tick = setInterval(() => { timer.textContent = mmss(Math.floor((Date.now() - t0) / 1000)); }, 500);
  r.onstop = async () => {
    clearInterval(draw); clearInterval(tick); timer.hidden = true; shot.classList.remove("rec"); modes.hidden = false; rec = null;
    const blob = new Blob(parts, { type: MIME.split(";")[0] });
    const ext = MIME.startsWith("video/mp4") ? "mp4" : "webm";
    const d = new Date(), z = n => String(n).padStart(2, "0"); // yerel saat (fotoğraflarla aynı biçim)
    const name = `${d.getFullYear()}-${z(d.getMonth() + 1)}-${z(d.getDate())}_${z(d.getHours())}-${z(d.getMinutes())}-${z(d.getSeconds())}`;
    const path = `~/Videolar/Kamera/${name}.${ext}`;
    toast(t("cam.saving"));
    try {
      const res = await fetch(`${BASE}/files/raw?path=${encodeURIComponent(path)}`, { method: "PUT", headers: { "X-Maker": "1" }, body: blob });
      if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error?.message || res.statusText);
      toast(t("cam.saved", { path }));
    } catch (e) { fail(e); }
  };
  r.start(1000);
  rec = r; timer.textContent = "00:00"; timer.hidden = false; shot.classList.add("rec"); modes.hidden = true;
}
// Pencere gizlenirse akış durur: kayıt da düzgünce biter.
app.onVisibility(v => { if (!v && rec) rec.stop(); });

shot.onclick = async () => {
  if (mode === "video") { if (rec) rec.stop(); else startRec(); return; }
  try {
    const r = await post("/camera/photo");
    $(".view").classList.remove("flash"); void $(".view").offsetWidth; $(".view").classList.add("flash");
    toast(t("cam.saved", { path: r.path }));
  } catch (e) { fail(e); }
};

(async () => {
  try {
    const s = await get("/camera");
    if (!s.cams.length) { msg.hidden = false; msg.textContent = t("cam.none"); info.textContent = ""; return; }
    const c = s.cams[0];
    info.textContent = `${c.sensor}${c.ai ? " · AI Camera (IMX500)" : ""}${s.sim ? " · " + t("cam.sim") : ""} · ${s.width}x${s.height} @ ${s.fps}`;
    setPause(false);
  } catch (e) { msg.hidden = false; msg.textContent = e.message; }
})();
