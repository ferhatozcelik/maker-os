// Ekran Modu — telefonda yatay görünüm.
// 1) Yapabiliyorsa gerçek kilit: screen.orientation.lock("landscape") (Android Chrome tam ekranda,
//    kurulu PWA her zaman). 2) Yapamıyorsa (iPhone Safari, iframe içi) ve telefon dik tutuluyorsa
//    sayfa CSS ile 90° döndürülür (html[data-rot]); noVNC'nin dokunma/fare koordinatları burada
//    döndürülmüş çerçeveye çevrilir. noVNC dosyalarına (vendor/) dokunulmaz.
const html = document.documentElement;
const real = Element.prototype.getBoundingClientRect;
export const isPhone = () => matchMedia("(pointer: coarse)").matches && Math.min(screen.width, screen.height) < 700;
const portrait = () => innerHeight > innerWidth;
let locked = false, enabled = true;
export const rotated = () => html.hasAttribute("data-rot");

// Döndürülmüş çerçevede: yerel (lx, ly) → ekran (r.right - ly, r.top + lx). Tersine:
// lx = y - r.top, ly = r.right - x. noVNC "x - left, y - top" hesapladığı için sahte istemci
// koordinatı X = r.left + lx, Y = r.top + ly verilir ve dikdörtgen yerel boyutlarla bildirilir.
function localRect(el) {
  const r = real.call(el);
  if (!rotated()) return r;
  return { left: r.left, top: r.top, width: r.height, height: r.width, right: r.left + r.height, bottom: r.top + r.width, x: r.left, y: r.top };
}
function map(el, x, y) {
  const r = real.call(el);
  return { x: r.left + (y - r.top), y: r.top + (r.right - x) };
}
const proxy = (obj, over) => new Proxy(obj, { get: (t, k) => (k in over ? over[k] : typeof t[k] === "function" ? t[k].bind(t) : t[k]) });

// RFB oluşturulmadan önce çağrılır: olay işleyicileri kurucuda bağlandığı için prototip sarılır.
export function patchRFB(RFB) {
  const P = RFB.prototype, mouse = P._handleMouse, wheel = P._handleWheel, gesture = P._handleGesture;
  P._handleMouse = function (ev) {
    if (!rotated() || ev.clientX == null) return mouse.call(this, ev);
    const p = map(this._canvas, ev.clientX, ev.clientY);
    return mouse.call(this, proxy(ev, { clientX: p.x, clientY: p.y }));
  };
  P._handleWheel = function (ev) {
    if (!rotated()) return wheel.call(this, ev);
    const p = map(this._canvas, ev.clientX, ev.clientY);
    return wheel.call(this, proxy(ev, { clientX: p.x, clientY: p.y, deltaX: ev.deltaY, deltaY: -ev.deltaX }));
  };
  P._handleGesture = function (ev) {
    if (!rotated()) return gesture.call(this, ev);
    const d = ev.detail, p = map(this._canvas, d.clientX, d.clientY);
    const nd = { ...d, clientX: p.x, clientY: p.y };
    if ("magnitudeX" in d) { nd.magnitudeX = d.magnitudeY; nd.magnitudeY = -d.magnitudeX; }
    return gesture.call(this, proxy(ev, { detail: nd }));
  };
}
// Ölçekleme (scaleViewport) ve koordinat kırpma yerel boyutları görsün.
export function attach(rfb) {
  rfb._screen.getBoundingClientRect = () => localRect(rfb._screen);
  rfb._canvas.getBoundingClientRect = () => localRect(rfb._canvas);
}

async function tryLock() {
  if (locked || !screen.orientation?.lock) return locked;
  try { await screen.orientation.lock("landscape"); locked = true; } catch { locked = false; }
  return locked;
}
// Android: kilit için tam ekran gerekir → ilk dokunuşta tam ekran + kilit dene.
async function fullAndLock() {
  if (await tryLock()) return update();
  if (top !== self) return update(); // iframe içinde tam ekran/kilit yok
  try { if (!document.fullscreenElement) await html.requestFullscreen?.({ navigationUI: "hide" }); } catch { /* izin yok */ }
  await tryLock();
  update();
}
let onChange = () => {};
function update() {
  const want = enabled && isPhone() && portrait() && !locked;
  if (want !== rotated()) { html.toggleAttribute("data-rot", want); onChange(want); dispatchEvent(new Event("resize")); }
}
export function setupRotation({ change } = {}) {
  if (!isPhone()) return { toggle() {}, get on() { return false; } };
  onChange = change || onChange;
  try { enabled = localStorage.getItem("mk.screen.rot") !== "0"; } catch { /* özel pencere */ }
  addEventListener("resize", () => requestAnimationFrame(update));
  screen.orientation?.addEventListener?.("change", update);
  document.addEventListener("fullscreenchange", () => { if (!document.fullscreenElement) locked = false; update(); });
  if (enabled) { tryLock().then(update); addEventListener("pointerdown", () => enabled && fullAndLock(), { once: true, capture: true }); }
  update();
  return {
    get on() { return enabled; },
    toggle() {
      enabled = !enabled;
      try { localStorage.setItem("mk.screen.rot", enabled ? "1" : "0"); } catch { /* yok */ }
      if (enabled) fullAndLock();
      else { if (locked) { try { screen.orientation.unlock(); } catch { /* yok */ } locked = false; } update(); }
      return enabled;
    },
  };
}
