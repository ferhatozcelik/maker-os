// Web arka ucu: her uygulama <mk-window> içinde aynı kökenli bir iframe.
import { h, animate, clamp } from "../lib.js";
import { icon } from "../icons.js";
import { t } from "../i18n.js";
import { attach } from "../keys.js";

const px = v => Math.round(v) + "px";
// Başlık şeridi (sürükleme alanı) uygulamanın kendi üst çubuğunun üstündedir (title:false, sekmeler, düğmeler).
// Şeridin altında tıklanabilir bir öğe varsa tıklama ona gider; boş yerden pencere sürüklenir.
const INTERACTIVE = "button, a[href], select, input, textarea, label, summary, [role=button], [role=tab], [contenteditable=true], [tabindex]:not([tabindex='-1'])";
const T = (r, s = 1, dx = 0, dy = 0) => `translate(${px(r.x + r.w * (1 - s) / 2 + dx)}, ${px(r.y + r.h * (1 - s) / 2 + dy)}) scale(${s})`;

class MkWindow extends HTMLElement {
  setup(win, wm) {
    this.win = win; this.wm = wm;
    const tb = h("div.tb", { part: "titlebar" },
      h("div.traffic",
        h("button.tl.close", { title: t("win.close"), "aria-label": t("win.close"), html: icon("x", 8, 3), onclick: () => wm.close(win) }),
        h("button.tl.min", { title: t("win.minimize"), "aria-label": t("win.minimize"), html: icon("minus", 8, 3), onclick: () => wm.minimize(win) }),
        h("button.tl.zoom", { title: t("win.zoom"), "aria-label": t("win.zoom"), html: icon("maximize-2", 7, 3), onclick: e => e.altKey ? wm.fullscreen(win) : wm.zoom(win) })),
      this.titleEl = h("div.title", win.title));
    tb.addEventListener("pointerdown", e => this.#hit(e) ? this.#tap(e) : this.#drag(e));
    tb.addEventListener("pointermove", e => { if (!e.buttons && e.pointerType === "mouse" && this.#hit(e)) tb.classList.add("pass"); });
    tb.addEventListener("dblclick", e => { if (!e.target.closest(".traffic") && !this.#hit(e)) wm.zoom(win); });
    this.tb = tb;
    // Dış site (ör. Konya HackerSpace): host PC internetini değil, Raspberry Pi'nin internetini (/api/v1/browse) kullanır.
    let ext = false;
    try { ext = new URL(win.app.url, location.href).origin !== location.origin; } catch { /* geçersiz adres */ }
    if (ext) this.classList.add("ext");
    const frameSrc = ext
      ? `/shell/apps/browser/?embed=1#${encodeURIComponent(win.app.url)}`
      : win.app.url + (win.path ? "#" + win.path : "");
    this.frame = h("iframe", { src: frameSrc, title: win.app.name, allow: "clipboard-read; clipboard-write; fullscreen; camera; microphone" });
    this.frame.addEventListener("load", () => this.#bind());
    this.append(tb, h("div.body", this.frame, h("div.spin", { html: icon("loader-circle", 22) })));
    for (const d of ["n", "s", "e", "w", "ne", "nw", "se", "sw"]) this.append(h("div.rz", { "data-d": d, onpointerdown: e => this.#resize(e, d) }));
    this.addEventListener("pointerdown", () => wm.focus(win), true);
    return this;
  }
  #bind() {
    this.classList.add("loaded");
    if (this._vis === false) this.send({ mk: "visibility", visible: false });
    try {
      const cw = this.frame.contentWindow;
      attach(cw, () => this.win.app.rawCtrl);
      cw.addEventListener("pointerdown", () => this.wm.focus(this.win), true);
      // Şerit geçirgenken (pass) fare tıklanabilir öğeden ya da şerit yüksekliğinden çıkınca şerit geri gelir.
      cw.addEventListener("pointermove", e => {
        if (this.tb.classList.contains("pass") && (e.clientY > this.tb.offsetHeight || !e.target.closest?.(INTERACTIVE))) this.tb.classList.remove("pass");
      }, { passive: true });
      cw.document.documentElement.addEventListener("pointerleave", () => this.tb.classList.remove("pass"));
      cw.addEventListener("focus", () => this.wm.focused !== this.win && this.wm.focus(this.win));
    } catch { /* farklı köken: yalnız postMessage köprüsü */ }
  }
  // Şeridin altındaki uygulama öğesi (aynı köken). Trafik ışıkları ve dış siteler hariç.
  #hit(e) {
    if (e.target.closest(".traffic") || this.classList.contains("ext")) return null;
    try {
      const fr = this.frame.getBoundingClientRect(), doc = this.frame.contentDocument;
      const x = e.clientX - fr.left, y = e.clientY - fr.top;
      if (!doc || x < 0 || y < 0) return null;
      return doc.elementFromPoint(x, y)?.closest(INTERACTIVE) || null;
    } catch { return null; }
  }
  // Dokunmatik/kalem (geçirgen şerit yok): sürüklemesiz dokunuş öğeye iletilir.
  #tap(e) {
    const el = this.#hit(e), tb = e.currentTarget, sx = e.clientX, sy = e.clientY;
    e.preventDefault();
    const up = ev => {
      tb.removeEventListener("pointerup", up);
      if (Math.hypot(ev.clientX - sx, ev.clientY - sy) > 6 || this.#hit(ev) !== el) return;
      this.wm.focus(this.win);
      try { this.frame.contentWindow.focus(); el.focus({ preventScroll: true }); } catch { /* yok */ }
      if (el.matches("select")) { try { el.showPicker(); } catch { /* eski tarayıcı */ } }
      else if (!el.matches("input:not([type=checkbox]):not([type=radio]):not([type=button]):not([type=submit]), textarea, [contenteditable=true]")) el.click();
    };
    tb.addEventListener("pointerup", up);
  }
  // Sürükleme: pointermove yalnız son konumu saklar; DOM'a rAF başına bir kez yalnız transform yazılır.
  // WindowManager durumu (wm.move) pointerup'ta bir kez güncellenir.
  #drag(e) {
    if (e.button !== 0 || e.target.closest(".traffic") || this.wm.mobile || this.wm.overviewOn) return;
    const wm = this.wm, win = this.win, a = wm.area(), sx = e.clientX, sy = e.clientY;
    let r = { ...wm.rectOf(win) }, moved = false, snap = null, lastSnap = null, pend = null, raf = 0;
    const tb = e.currentTarget; tb.setPointerCapture(e.pointerId);
    const flush = () => {
      raf = 0; if (!pend) return;
      const { nr, sn } = pend; pend = null;
      this.style.transform = T(nr); r = nr;
      if (sn !== lastSnap) { lastSnap = sn; wm.emit("snap", sn && wm.rectOf({ state: sn })); }
    };
    const move = ev => {
      const dx = ev.clientX - sx, dy = ev.clientY - sy;
      if (!moved && Math.hypot(dx, dy) < 4) return;
      if (!moved) {
        moved = true; document.documentElement.classList.add("wm-drag");
        if (win.state !== "normal") { // büyütülmüş pencereyi sürükleyince eski boyutuna dön
          const n = win.saved || win.rect, fx = (sx - r.x) / r.w;
          r = { x: sx - n.w * fx, y: r.y, w: n.w, h: n.h }; win.state = "normal"; win.rect = r; this.render(win);
        }
        this.base = { ...r };
      }
      snap = ev.clientY <= a.y + 2 ? "max" : ev.clientX <= 2 ? "left" : ev.clientX >= a.vw - 3 ? "right" : null;
      pend = { nr: { ...this.base, x: this.base.x + dx, y: clamp(this.base.y + dy, a.y, a.vh - 40) }, sn: snap };
      if (!raf) raf = requestAnimationFrame(flush);
    };
    const up = () => {
      tb.removeEventListener("pointermove", move); tb.removeEventListener("pointerup", up); tb.removeEventListener("pointercancel", up);
      if (raf) { cancelAnimationFrame(raf); flush(); }
      document.documentElement.classList.remove("wm-drag");
      if (!moved) return;
      wm.emit("snap", null);
      if (snap) { const from = { ...r }; win.rect = r; wm.move(win, r, snap); this.play("flip", from); }
      else wm.move(win, r);
    };
    tb.addEventListener("pointermove", move); tb.addEventListener("pointerup", up); tb.addEventListener("pointercancel", up);
  }
  // Boyutlandırma: fx=full'da canlı (rAF başına bir kez); lite/flat'te (Pi) yalnız ince bir çerçeve
  // transform/boyutla izlenir, iframe içeriği pointerup'ta bir kez yeniden yerleşir.
  #resize(e, d) {
    if (e.button !== 0 || this.wm.mobile) return;
    e.stopPropagation(); e.preventDefault();
    const wm = this.wm, win = this.win, a = wm.area(), s = { ...wm.rectOf(win) }, sx = e.clientX, sy = e.clientY;
    const minW = 360, minH = 220, live = document.documentElement.dataset.fx === "full";
    let r = s, raf = 0, ghost = null;
    if (!live) { ghost = h("div.rz-ghost"); this.parentNode.append(ghost); ghost.style.zIndex = win.z + 1; this.#ghost(ghost, s); }
    const tgt = e.target; tgt.setPointerCapture(e.pointerId);
    document.documentElement.classList.add("wm-drag");
    const flush = () => { raf = 0; live ? this.#apply(r) : this.#ghost(ghost, r); };
    const move = ev => {
      const dx = ev.clientX - sx, dy = ev.clientY - sy; r = { ...s };
      if (d.includes("e")) r.w = Math.max(minW, s.w + dx);
      if (d.includes("s")) r.h = Math.max(minH, s.h + dy);
      if (d.includes("w")) { r.w = Math.max(minW, s.w - dx); r.x = s.x + s.w - r.w; }
      if (d.includes("n")) { const ny = Math.max(a.y, s.y + dy); r.h = Math.max(minH, s.h - (ny - s.y)); r.y = s.y + s.h - r.h; }
      if (!raf) raf = requestAnimationFrame(flush);
    };
    const up = () => {
      tgt.removeEventListener("pointermove", move); tgt.removeEventListener("pointerup", up); tgt.removeEventListener("pointercancel", up);
      if (raf) cancelAnimationFrame(raf);
      ghost?.remove(); document.documentElement.classList.remove("wm-drag");
      wm.move(win, r);
    };
    tgt.addEventListener("pointermove", move); tgt.addEventListener("pointerup", up); tgt.addEventListener("pointercancel", up);
  }
  #ghost(g, r) { g.style.width = px(r.w); g.style.height = px(r.h); g.style.transform = `translate(${px(r.x)}, ${px(r.y)})`; }
  #apply(r) {
    if (this._w !== r.w) this.style.width = px(this._w = r.w);
    if (this._h !== r.h) this.style.height = px(this._h = r.h);
    this.style.transform = T(r);
  }
  render(win) {
    const wm = this.wm, r = wm.rectOf(win);
    this.#apply(r);
    if (win.ov) this.style.transform = `translate(${px(win.ov.x)}, ${px(win.ov.y)}) scale(${win.ov.s})`;
    else if (wm.desktopOn && !wm.mobile) { const a = wm.area(), left = r.x + r.w / 2 < a.vw / 2; this.style.transform = T(r, 1, left ? -(r.x + r.w) - 30 : a.vw - r.x + 30, 0); }
    this.style.zIndex = win.z;
    this.dataset.state = win.state;
    this.classList.toggle("focused", win.focused);
    this.classList.toggle("min", win.min || win.hidden);
    this.classList.toggle("tone-dark", win.tone === "dark");
    if (this.titleEl.textContent !== win.title) this.titleEl.textContent = win.title;
    this.titleEl.style.paddingLeft = win.sidebar && !wm.mobile ? px(win.sidebar) : "";
    this.classList.toggle("has-sidebar", !!win.sidebar);
    this.titleEl.hidden = !!win.noTitle;
    this.firstChild.style.right = win.tbRight && !wm.mobile ? px(win.tbRight) : ""; // sağdaki uygulama araç çubuğu tıklanabilir kalır
    // Görünürlük uygulamaya bildirilir: gizli/küçültülmüş pencerede yoklama ve çizim durur
    const vis = !win.min && !win.hidden && !wm.desktopOn && (!wm.mobile || win.focused);
    if (vis !== this._vis) { this._vis = vis; this.send({ mk: "visibility", visible: vis }); }
  }
  async play(kind, from) {
    const r = this.wm.rectOf(this.win);
    if (this.wm.mobile) {
      if (kind === "open" || kind === "restore") return animate(this, [{ opacity: 0, transform: T(r, .9) }, { opacity: 1, transform: T(r) }], { duration: 260 });
      if (kind === "close" || kind === "min") return animate(this, [{ opacity: 1, transform: T(r) }, { opacity: 0, transform: T(r, .9) }], { duration: 200, easing: "ease-in" });
      return;
    }
    if (kind === "open") return animate(this, [{ opacity: 0, transform: T(r, .94) }, { opacity: 1, transform: T(r) }], { duration: 220 });
    if (kind === "close") return animate(this, [{ opacity: 1, transform: T(r) }, { opacity: 0, transform: T(r, .94) }], { duration: 150, easing: "ease-in" });
    if (kind === "flip" && from) {
      return animate(this, [{ transform: `translate(${px(from.x)}, ${px(from.y)}) scale(${from.w / r.w}, ${from.h / r.h})` }, { transform: T(r) }], { duration: 260 });
    }
    if (kind === "min" || kind === "restore") {
      const d = this.wm.dockRect?.(this.win.app.id) || { x: r.x + r.w / 2, y: innerHeight, w: 0, h: 0 };
      const s = .06, tx = d.x + d.w / 2 - r.w * s / 2, ty = d.y + d.h / 2 - r.h * s / 2;
      const a = { opacity: 1, transform: T(r) }, b = { opacity: 0, transform: `translate(${px(tx)}, ${px(ty)}) scale(${s})` };
      return animate(this, kind === "min" ? [a, b] : [b, a], { duration: 300, easing: kind === "min" ? "cubic-bezier(.4,0,.8,.6)" : "cubic-bezier(.2,.8,.2,1)" });
    }
  }
  focusContent() { try { this.frame.focus(); this.frame.contentWindow.focus(); } catch { /* yok */ } }
  send(msg) { try { this.frame.contentWindow.postMessage(msg, location.origin); } catch { /* yok */ } }
  destroy() { this.frame.src = "about:blank"; this.remove(); }
}
customElements.define("mk-window", MkWindow);

export class WebBackend {
  kind = "web";
  constructor(layer) { this.layer = layer; }
  create(win, wm) { const el = new MkWindow().setup(win, wm); this.layer.append(el); return el; }
}

// Uygulama → kabuk mesajları ({mk:"title"|"chrome"|"open"|"notify"|"unauthorized"|"close"})
export function bridge(wm, handlers) {
  addEventListener("message", e => {
    if (e.origin !== location.origin || !e.data?.mk) return;
    const win = wm.list().find(w => w.view.frame?.contentWindow === e.source), m = e.data;
    if (win && m.mk === "title") { win.title = m.title || win.app.name; wm.render(win); wm.emit("change"); }
    else if (win && m.mk === "chrome") { win.sidebar = m.sidebar || 0; win.tone = m.tone; win.tbRight = m.tbRight || 0; win.noTitle = m.title === false; wm.render(win); }
    else if (win && m.mk === "path") { win.path = m.path || ""; wm.changed(); }
    else if (win && m.mk === "close") wm.close(win);
    else handlers[m.mk]?.(m, win);
  });
}
