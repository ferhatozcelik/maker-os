// Küçük ortak yardımcılar (kabuk + uygulamalar).
export const $ = (s, r = document) => r.querySelector(s);
export const $$ = (s, r = document) => [...r.querySelectorAll(s)];
export const esc = t => String(t ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

// h("div.cls#id", {attr}, ...children) — hafif DOM üretici
export function h(tag, attrs, ...kids) {
  const [, name, rest] = tag.match(/^([a-z0-9-]+)(.*)$/i);
  const el = document.createElement(name);
  for (const m of rest.matchAll(/([.#])([\w-]+)/g)) m[1] === "." ? el.classList.add(m[2]) : (el.id = m[2]);
  if (attrs && (typeof attrs !== "object" || attrs instanceof Node || Array.isArray(attrs))) { kids.unshift(attrs); attrs = null; }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "html") el.innerHTML = v;
    else if (k === "style" && typeof v === "object") Object.assign(el.style, v);
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const k of kids.flat()) if (k != null && k !== false) el.append(k instanceof Node ? k : String(k));
  return el;
}

// localStorage her zaman try/catch içinde (gizli pencere, WPE kısıtları)
export const store = {
  get(k, d) { try { const v = localStorage.getItem("mk." + k); return v == null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem("mk." + k, JSON.stringify(v)); } catch { /* yok say */ } },
};

// Basit olay yayıcı
export class Emitter {
  #m = new Map();
  on(e, f) { (this.#m.get(e) || this.#m.set(e, new Set()).get(e)).add(f); return () => this.#m.get(e).delete(f); }
  emit(e, ...a) { for (const f of this.#m.get(e) || []) try { f(...a); } catch (x) { console.error(x); } }
}

export const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
export function bytes(n) {
  if (n == null) return "—";
  const u = ["B", "KB", "MB", "GB", "TB"]; let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return (i ? n.toFixed(n < 10 ? 1 : 0) : n) + " " + u[i];
}
export function duration(s) {
  const d = Math.floor(s / 86400), hh = Math.floor(s % 86400 / 3600), m = Math.floor(s % 3600 / 60);
  return [d && d + "g", (d || hh) && hh + "s", m + "dk"].filter(Boolean).join(" ");
}
// Türkçe duyarlı katlama: "Ayarlar" ~ "ayarlar", "Etkinlik Monitörü" ~ "monitoru"
export const fold = s => String(s).toLocaleLowerCase("tr").normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/ı/g, "i");

// Animasyon: yalnız transform/opacity, bitince will-change kaldırılır
export function animate(el, frames, opts = {}) {
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (!el.animate || reduce) return Promise.resolve();
  el.style.willChange = "transform, opacity";
  const o = { duration: 240, easing: "cubic-bezier(.2,.8,.2,1)", ...opts };
  const a = el.animate(frames, o);
  // Güvenlik: kare döngüsü durursa (WPE/cog'da yük altında, gizli sayfa) "finished" hiç çözülmeyebilir;
  // akış (sihirbaz "Başla", kilit ekranı kapanışı) animasyona takılı kalmasın.
  const cap = new Promise(r => setTimeout(r, (o.duration || 0) + (o.delay || 0) + 400));
  return Promise.race([a.finished.catch(() => {}), cap]).finally(() => { el.style.willChange = ""; });
}
export const nextFrame = () => new Promise(r => requestAnimationFrame(() => r()));

// Tembel stil dosyası: aynı href bir kez yüklenir; yüklenince çözülür
const sheets = new Map();
export function loadCSS(href) {
  if (!sheets.has(href)) sheets.set(href, new Promise(r => {
    const l = h("link", { rel: "stylesheet", href }); l.onload = l.onerror = () => r(); document.head.append(l);
  }));
  return sheets.get(href);
}
// Tembel modül + stil: lazy(() => import("./x.js"), "/shell/css/x.css") → () => Promise<modül>
export function lazy(load, css) {
  let p = null;
  const f = () => p ??= Promise.all([load(), css && loadCSS(css)]).then(([m]) => (f.m = m));
  return f;
}
