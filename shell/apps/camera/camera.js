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
$(".b-gal").onclick = () => app.open("makeros.files", "~/Resimler/Kamera");
const pauseBtn = $(".b-pause");
const setPause = p => { paused = p; pauseBtn.replaceChildren(h("span", { html: icon(p ? "play" : "pause", 14, 2) }), p ? t("cam.resume") : t("cam.pause")); p ? stop() : start(); };
pauseBtn.onclick = () => setPause(!paused);

function start() { if (!paused && app.visible) img.src = `${BASE}/camera/stream?t=${Date.now()}`; }
function stop() { img.removeAttribute("src"); img.src = "data:,"; }
img.onerror = () => { if (img.src.includes("/camera/stream")) { msg.hidden = false; msg.textContent = t("cam.noStream"); } };
img.onload = () => { msg.hidden = true; };
app.onVisibility(v => (v ? start() : stop()));

$(".shot").onclick = async () => {
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
