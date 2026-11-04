// Pencere yöneticisi (arka uçtan bağımsız). Durum, odak, z-sırası, yerleşim, kalıcılık.
//
// Arka uç arayüzü (web: wm/web.js; M3'te native: wm/native.js, makerd WS köprüsü):
//   backend.kind                 "web" | "native"
//   backend.create(win) → view   pencereyi oluşturur
//   view.render(win)             win.rect/state/z/focused/min/hidden durumunu uygular
//   view.play(kind, from?)    "open" | "close" | "min" | "restore" | "flip" (Promise)
//   view.send(msg)               uygulamaya komut ({mk:"command",cmd}) iletir
//   view.focusContent()          klavye odağını içeriğe verir
//   view.destroy()
// Native pencerelerde render/animate compositor'a IPC isteği olarak gider (window.move/resize/...).
import { Emitter, store, clamp } from "../lib.js";

let seq = 0;
export class WindowManager extends Emitter {
  wins = []; focused = null; z = 10; backends = {}; mobile = false; overviewOn = false; desktopOn = false;
  constructor(area) { super(); this.area = area; }
  register(b) { this.backends[b.kind] = b; }

  list() { return this.wins; }
  byApp(appId) { return this.wins.filter(w => w.app.id === appId); }
  visible() { return this.wins.filter(w => !w.min && !w.hidden); }
  // En son odaklanan önce (uygulama değiştirici için)
  mru() { return [...this.wins].sort((a, b) => b.z - a.z); }
  appsMRU() { const s = new Map(); for (const w of this.mru()) if (!s.has(w.app.id)) s.set(w.app.id, w.app); return [...s.values()]; }

  open(app, o = {}) {
    if (app.single && !o.force) {
      const ex = this.byApp(app.id)[0];
      if (ex) { if (o.path) ex.view.send({ mk: "navigate", path: o.path }); this.focus(ex); return ex; }
    }
    const b = this.backends[app.kind] || this.backends.web;
    const win = { id: "w" + ++seq, app, title: app.name, path: o.path || "", state: o.state || "normal", min: !!o.min, hidden: false, z: ++this.z, focused: false };
    win.rect = this.#fit(o.rect || store.get("wm.rect." + app.id) || this.#defaultRect(app));
    if (o.rect == null && this.byApp(app.id).length) win.rect = this.#fit({ ...win.rect, x: win.rect.x + 26 * this.byApp(app.id).length, y: win.rect.y + 26 * this.byApp(app.id).length });
    if (win.state !== "normal") win.saved = { ...win.rect };
    win.view = b.create(win, this);
    this.wins.push(win);
    this.render(win);
    if (!win.min) { this.focus(win); win.view.play("open"); }
    this.emit("open", win); this.changed();
    return win;
  }
  async close(win) {
    if (!win || win.closing) return;
    win.closing = true;
    await win.view.play("close");
    win.view.destroy();
    this.wins = this.wins.filter(w => w !== win);
    if (this.focused === win) { this.focused = null; const n = this.mru().find(w => !w.min && !w.hidden); n ? this.focus(n) : this.emit("focus", null); }
    this.emit("close", win); this.changed();
  }
  focus(win) {
    if (this.desktopOn) this.showDesktop(false);
    if (win && (win.min || win.hidden)) return this.restore(win);
    if (this.focused === win) { win?.view.focusContent(); return; }
    const prev = this.focused;
    this.focused = win;
    if (prev) { prev.focused = false; this.render(prev); }
    if (win) { win.focused = true; win.z = ++this.z; this.render(win); win.view.focusContent(); }
    this.emit("focus", win); this.changed();
  }
  blur() { this.focus(null); }
  async minimize(win) {
    if (!win || win.min) return;
    await win.view.play("min");
    win.min = true; win.focused = false; this.render(win);
    if (this.focused === win) { this.focused = null; const n = this.mru().find(w => !w.min && !w.hidden); n ? this.focus(n) : this.emit("focus", null); }
    this.changed();
  }
  restore(win) {
    const wasMin = win.min;
    win.min = false; win.hidden = false; win.z = ++this.z; this.render(win);
    if (wasMin) win.view.play("restore");
    this.focus(win);
  }
  hideApp(appId) {
    for (const w of this.byApp(appId)) { w.hidden = true; w.focused = false; this.render(w); }
    if (this.focused?.app.id === appId) { this.focused = null; const n = this.mru().find(w => !w.min && !w.hidden); n ? this.focus(n) : this.emit("focus", null); }
    this.changed();
  }
  activateApp(app) {
    const ws = this.byApp(app.id);
    if (!ws.length) return this.open(app);
    const top = ws.sort((a, b) => b.z - a.z);
    for (const w of top) if (w.hidden) { w.hidden = false; this.render(w); }
    const vis = top.find(w => !w.min);
    this.focus(vis || top[0]);
  }
  quitApp(appId) { for (const w of this.byApp(appId)) this.close(w); }
  cycleApp() {
    const f = this.focused; if (!f) return;
    const ws = this.byApp(f.app.id).filter(w => !w.min && !w.hidden).sort((a, b) => a.z - b.z);
    if (ws.length > 1) this.focus(ws[0]);
  }

  // Yakınlaştır (yeşil düğme): çalışma alanını doldurur / önceki boyuta döner
  zoom(win, state = "max") {
    if (!win) return;
    const from = { ...this.rectOf(win) };
    if (win.state === state) win.state = "normal";
    else { if (win.state === "normal") win.saved = { ...win.rect }; win.state = state; }
    if (win.state === "normal" && win.saved) win.rect = this.#fit(win.saved);
    this.render(win); win.view.play("flip", from);
    this.emit("state", win); this.changed();
  }
  fullscreen(win) { this.zoom(win, "full"); }
  tile(win, side) { this.zoom(win, side); }
  // Sürükleme/boyutlandırma sonucu (view'dan)
  move(win, rect, state = "normal") {
    if (state !== "normal" && win.state === "normal") win.saved = { ...win.rect };
    win.state = state;
    if (state === "normal") win.rect = rect;
    this.render(win); this.changed();
  }
  // Duruma göre ekrandaki gerçek dikdörtgen
  rectOf(win) {
    const a = this.area();
    if (this.mobile) return { x: 0, y: a.mt, w: a.vw, h: a.vh - a.mt - a.mb };
    switch (win.state) {
      case "max": return { x: a.x, y: a.y, w: a.w, h: a.h };
      case "full": return { x: 0, y: 0, w: a.vw, h: a.vh };
      case "left": return { x: a.x, y: a.y, w: Math.round(a.w / 2), h: a.h };
      case "right": return { x: a.x + Math.round(a.w / 2), y: a.y, w: a.w - Math.round(a.w / 2), h: a.h };
      default: return win.rect;
    }
  }
  render(win) { win.view.render(win); }
  // M3: native pencereyi (compositor'dan gelen) yönetime al
  adopt(app, view, o = {}) {
    const win = { id: "w" + ++seq, app, title: o.title || app.name, path: "", state: "normal", min: false, hidden: false, z: ++this.z, focused: false, rect: this.#fit(o.rect || this.#defaultRect(app)), view };
    this.wins.push(win); this.render(win); this.focus(win); this.emit("open", win); this.changed();
    return win;
  }
  relayout() { for (const w of this.wins) { if (w.state === "normal") w.rect = this.#fit(w.rect); this.render(w); } }

  // Genel bakış (Mission Control benzeri): pencereleri ızgaraya ölçekler (yalnız transform)
  layoutOverview() {
    const a = this.area(), ws = this.visible().sort((x, y) => x.id.localeCompare(y.id, "en", { numeric: true }));
    const n = ws.length; if (!n) return [];
    const top = a.y + 40, bottom = a.vh - 110, left = 40, right = a.vw - 40, W = right - left, H = bottom - top;
    let best = null;
    for (let cols = 1; cols <= n; cols++) {
      const rows = Math.ceil(n / cols), cw = W / cols, ch = H / rows;
      const s = Math.min(...ws.map(w => { const r = this.rectOf(w); return Math.min(.92 * cw / r.w, .82 * ch / r.h, 1); }));
      if (!best || s > best.s) best = { cols, rows, cw, ch, s };
    }
    return ws.map((w, i) => {
      const r = this.rectOf(w), row = Math.floor(i / best.cols), inRow = Math.min(best.cols, n - row * best.cols);
      const s = Math.min(.92 * best.cw / r.w, .82 * best.ch / r.h, 1), offX = (W - inRow * best.cw) / 2;
      const cx = left + offX + (i % best.cols + .5) * best.cw, cy = top + (row + .5) * best.ch - 10;
      return { win: w, s, x: cx - r.w * s / 2, y: cy - r.h * s / 2, w: r.w * s, h: r.h * s };
    });
  }
  showDesktop(on = !this.desktopOn) { this.desktopOn = on; for (const w of this.wins) this.render(w); this.emit("desktop", on); }

  #defaultRect(app) {
    const a = this.area(), w = Math.min(app.w, a.w - 40), h = Math.min(app.h, a.h - 20);
    return { x: Math.round(a.x + (a.w - w) / 2), y: Math.round(a.y + Math.max(10, (a.h - h) / 2.6)), w, h };
  }
  #fit(r) {
    const a = this.area(), w = clamp(r.w, 320, a.vw), h = clamp(r.h, 200, a.vh - a.y);
    return { w, h, x: clamp(r.x, -w + 120, a.vw - 120), y: clamp(r.y, a.y, a.vh - 60) };
  }

  // Kalıcılık: oturum pencereleri ve uygulama başına son boyut
  #t = 0;
  changed() {
    this.emit("change");
    clearTimeout(this.#t);
    this.#t = setTimeout(() => {
      for (const w of this.wins) if (w.state === "normal") store.set("wm.rect." + w.app.id, w.rect);
      store.set("wm.session", this.wins.map(w => ({ app: w.app.id, rect: w.state === "normal" ? w.rect : w.saved || w.rect, state: w.state, min: w.min, path: w.path })));
    }, 400);
  }
  restoreSession(appById) {
    const s = store.get("wm.session", []);
    if (!Array.isArray(s)) return;
    for (const e of s) { const app = appById(e.app); if (app && app.kind === "web") try { this.open(app, { rect: e.rect, state: e.state, min: e.min, path: e.path, force: true }); } catch (x) { console.warn(x); } }
  }
}
