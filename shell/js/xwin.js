// Yerel (X11) pencere görünümü: Pi'de çalışan pencereli bir programın (tkinter, PySide6/Qt, pygame,
// OpenCV) tek penceresini bir <canvas>'a çizer; fare ve klavyeyi programa iletir.
// Akış: WS /api/v1/xwin/<id> (makerd → makerd-user X köprüsü). Yalnız değişen bölgeler gelir:
//   ikili [1, kodlama, x, y, w, h] + yük (0: ham RGBA deflate, 1: JPEG), [2, seq] toplu sonu (onaylanır)
//   metin {"t":"geom"|"cursor"|"gone"|"error"}

const NAMED = {
  Enter: 0xff0d, Tab: 0xff09, Backspace: 0xff08, Escape: 0xff1b, Delete: 0xffff, Insert: 0xff63,
  Home: 0xff50, End: 0xff57, PageUp: 0xff55, PageDown: 0xff56, ArrowLeft: 0xff51, ArrowUp: 0xff52,
  ArrowRight: 0xff53, ArrowDown: 0xff54, ContextMenu: 0xff67, Pause: 0xff13, ScrollLock: 0xff14, PrintScreen: 0xff61,
};
const MODS = new Set(["Shift", "Control", "Alt", "Meta", "AltGraph", "OS", "CapsLock", "NumLock", "Fn", "FnLock", "Hyper", "Super", "Symbol"]);
const MAC = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

export function keysymOf(ch) {
  const cp = ch.codePointAt(0);
  return (cp >= 0x20 && cp <= 0x7e) || (cp >= 0xa0 && cp <= 0xff) ? cp : 0x01000000 | cp;
}

// Tarayıcı değişkenleri → köprü bitleri (1 Shift, 2 Ctrl, 4 Alt, 8 Super). Mac'te ⌘ = Ctrl
// (⌘C/⌘V alışkanlığı); AltGr (Windows'ta Ctrl+Alt olarak gelir) değişken sayılmaz, karakter zaten üretilmiştir.
function mods(e) {
  let m = (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
  if (MAC && e.metaKey) m = (m & ~8) | 2;
  if (e.getModifierState?.("AltGraph")) m &= ~6;
  return m;
}

export class XView {
  closed = false; gone = false; acc = { x: 0, y: 0 }; pend = null; raf = 0; down = 0;

  // o: { onGeom(w,h), onGone(), onError(msg), input: true }
  constructor(canvas, id, o = {}) {
    this.c = canvas; this.id = id; this.o = o;
    this.g = canvas.getContext("2d", { alpha: true, desynchronized: true });
    this.q = Promise.resolve();
    this.connect();
    if (o.input !== false) this.bind();
  }

  connect() {
    if (this.closed || this.gone) return;
    const ws = this.ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/v1/xwin/${this.id}`);
    ws.binaryType = "arraybuffer";
    ws.onmessage = e => typeof e.data === "string" ? this.json(JSON.parse(e.data)) : this.bin(e.data);
    ws.onclose = () => { if (!this.closed && !this.gone) setTimeout(() => this.connect(), 1000); };
  }

  send(m) { if (this.ws?.readyState === 1) this.ws.send(JSON.stringify(m)); }

  json(m) {
    if (m.t === "geom") {
      if (this.c.width !== m.w || this.c.height !== m.h) { this.c.width = m.w; this.c.height = m.h; }
      this.c.style.width = m.w + "px"; this.c.style.height = m.h + "px";
      this.o.onGeom?.(m.w, m.h);
    } else if (m.t === "cursor") this.cursor(m);
    else if (m.t === "gone") { this.gone = true; this.o.onGone?.(); }
    else if (m.t === "error") { this.gone = true; this.o.onError?.(m.message); }
  }

  bin(buf) {
    const d = new DataView(buf), type = d.getUint8(0);
    if (type === 1) {
      const enc = d.getUint8(1), x = d.getUint16(2, true), y = d.getUint16(4, true), w = d.getUint16(6, true), h = d.getUint16(8, true);
      const p = new Uint8Array(buf, 10);
      // Sıra korunur (üst üste binen bölgeler); her toplu güncelleme çizilince onaylanır.
      this.q = this.q.then(() => enc === 1 ? this.jpeg(p, x, y) : this.raw(p, x, y, w, h)).catch(() => {});
    } else if (type === 2) this.q = this.q.then(() => this.send({ t: "ack" }));
  }

  async raw(p, x, y, w, h) {
    const s = new Blob([p]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
    const b = new Uint8ClampedArray(await new Response(s).arrayBuffer());
    if (b.length === w * h * 4) this.g.putImageData(new ImageData(b, w, h), x, y);
  }

  async jpeg(p, x, y) {
    const bmp = await createImageBitmap(new Blob([p], { type: "image/jpeg" }));
    this.g.drawImage(bmp, x, y); bmp.close?.();
  }

  cursor(m) {
    try {
      if (!m.w || !m.h) { this.c.style.cursor = "none"; return; }
      const cv = document.createElement("canvas"); cv.width = m.w; cv.height = m.h;
      const bin = atob(m.rgba), a = new Uint8ClampedArray(bin.length);
      for (let i = 0; i < bin.length; i++) a[i] = bin.charCodeAt(i);
      cv.getContext("2d").putImageData(new ImageData(a, m.w, m.h), 0, 0);
      this.c.style.cursor = `url(${cv.toDataURL()}) ${m.hx} ${m.hy}, default`;
    } catch { this.c.style.cursor = "default"; }
  }

  // Tuvaldeki nokta → pencere pikseli (Genel Bakış gibi ölçekli görünümlerde de doğru).
  pt(e) {
    const r = this.c.getBoundingClientRect();
    return { x: Math.round((e.clientX - r.left) * this.c.width / (r.width || 1)), y: Math.round((e.clientY - r.top) * this.c.height / (r.height || 1)) };
  }

  bind() {
    const c = this.c;
    // Klavye gizli bir metin alanından alınır: tuşlar keydown/keyup ile (oyunlarda basılı tutma), tuşa
    // karşılık gelmeyen metin (IME, ölü tuşlar ^ ´, mobil klavye, yapıştırma) "input" ile programa yazılır.
    let ta = null;
    if (this.o.keyboard !== false) {
      ta = this.ta = document.createElement("textarea");
      ta.setAttribute("autocapitalize", "off"); ta.setAttribute("autocomplete", "off"); ta.spellcheck = false;
      ta.setAttribute("aria-hidden", "true");
      ta.style.cssText = "position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;padding:0;border:0;resize:none;overflow:hidden;z-index:-1";
      (c.parentNode || document.body).append(ta);
      ta.addEventListener("keydown", e => this.key(e, 1));
      ta.addEventListener("keyup", e => this.key(e, 0));
      ta.addEventListener("input", e => { if (e.isComposing) return; const s = ta.value; ta.value = ""; if (s) this.send({ t: "text", s }); });
      ta.addEventListener("compositionend", e => { ta.value = ""; if (e.data) this.send({ t: "text", s: e.data }); });
      ta.addEventListener("paste", e => { const s = e.clipboardData?.getData("text/plain"); e.preventDefault(); if (s) this.send({ t: "text", s }); });
      ta.addEventListener("focus", () => this.send({ t: "focus" }));
      ta.addEventListener("blur", () => this.send({ t: "blur" }));
    }
    c.addEventListener("contextmenu", e => e.preventDefault());
    c.addEventListener("pointermove", e => {
      const p = this.pt(e); this.pend = { t: "m", ...p, m: mods(e) };
      if (!this.raf) this.raf = requestAnimationFrame(() => { this.raf = 0; if (this.pend) { this.send(this.pend); this.pend = null; } });
    });
    const btn = e => [1, 2, 3, 8, 9][e.button] || 1;
    c.addEventListener("pointerdown", e => {
      ta?.focus({ preventScroll: true });
      try { c.setPointerCapture(e.pointerId); } catch { /* yok */ }
      this.flush();
      this.send({ t: "b", ...this.pt(e), b: btn(e), d: 1, m: mods(e) });
      e.preventDefault();
    });
    c.addEventListener("pointerup", e => { this.flush(); this.send({ t: "b", ...this.pt(e), b: btn(e), d: 0, m: mods(e) }); });
    c.addEventListener("pointercancel", e => this.send({ t: "b", ...this.pt(e), b: btn(e), d: 0, m: 0 }));
    c.addEventListener("wheel", e => {
      e.preventDefault();
      const k = e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? 800 : 1, STEP = 50;
      this.acc.x += e.deltaX * k; this.acc.y += e.deltaY * k;
      const dx = Math.trunc(this.acc.x / STEP), dy = Math.trunc(this.acc.y / STEP);
      if (!dx && !dy) return;
      this.acc.x -= dx * STEP; this.acc.y -= dy * STEP;
      this.send({ t: "w", ...this.pt(e), dx, dy, m: mods(e) });
    }, { passive: false });
  }

  focus() { this.ta?.focus({ preventScroll: true }); }

  flush() { if (this.pend) { this.send(this.pend); this.pend = null; } }

  key(e, d) {
    if (e.isComposing || MODS.has(e.key)) return;
    const m = mods(e);
    // ⌘V / Ctrl+V: tarayıcının "paste" olayına bırak (yerel pano programa metin olarak yazılır).
    if (d && (m & 2) && (e.key === "v" || e.key === "V") && !(m & 4)) return;
    let ks = NAMED[e.key], p = 0;
    if (!ks && /^F\d{1,2}$/.test(e.key)) ks = 0xffbe + Number(e.key.slice(1)) - 1;
    if (!ks && [...e.key].length === 1) { ks = keysymOf(e.key); p = 1; }
    // Bırakırken basılan keysym kullanılır (Shift önce bırakılınca 'A' basılıp 'a' bırakılır).
    this.codes ??= {};
    if (d) { if (ks) this.codes[e.code] = { ks, p }; }
    else if (this.codes[e.code]) { ({ ks, p } = this.codes[e.code]); delete this.codes[e.code]; }
    if (!ks) return;
    e.preventDefault(); e.stopPropagation();
    this.flush();
    this.send({ t: "k", ks, d, m, p });
  }

  visible(v) { this.send({ t: "vis", v }); if (v) this.send({ t: "refresh" }); }
  resize(w, h) { this.send({ t: "size", w: Math.max(1, Math.round(w)), h: Math.max(1, Math.round(h)) }); }
  close() { this.closed = true; this.ta?.remove(); try { this.ws?.close(); } catch { /* yok */ } }
}
