// Uygulama çerçevesi: hem kabuk penceresinde (iframe) hem telefonda bağımsız sayfa olarak çalışır.
//   import { app } from "/shell/js/app-kit.js";  app.title("…"); app.events.subscribe("system", fn)
import { syncPrefs, pref } from "./prefs.js";
import { brand, setFavicon } from "./themes.js";
import { bus, Events } from "./api.js";
import { h } from "./lib.js";
import { icon } from "./icons.js";
import { t } from "./i18n.js";

const html = document.documentElement;
const embedded = (() => { try { return window.parent !== window && !!window.frameElement; } catch { return false; } })();
const post = m => { if (embedded) parent.postMessage(m, location.origin); };
const cmds = {};
// Görünürlük: kabuk {mk:"visibility"} gönderir (küçültme/gizleme/masaüstü); sekme gizlenmesi de sayılır
let shellVis = true, lastVis = true;
const visFns = new Set();
const visible = () => shellVis && document.visibilityState !== "hidden";
const visChanged = () => { const v = visible(); if (v !== lastVis) { lastVis = v; for (const f of visFns) try { f(v); } catch (x) { console.error(x); } } };
document.addEventListener("visibilitychange", visChanged);

function sharedEvents() {
  // Kabuğun tek WS bağlantısını paylaş (Pi'de bağlantı ve bellek tasarrufu)
  let ev = null;
  try { ev = embedded && parent.__mk?.events; } catch { /* yok */ }
  if (!ev) return new Events().start();
  const subs = new Set();
  addEventListener("pagehide", () => { for (const u of subs) u(); subs.clear(); });
  return { subscribe(topic, fn) { const u = ev.subscribe(topic, d => fn(d)); subs.add(u); return () => { u(); subs.delete(u); }; } };
}

function layout() {
  let mobile = false, web = false;
  try {
    mobile = embedded && parent.document.documentElement.dataset.layout === "mobile";
    web = embedded && parent.document.documentElement.dataset.ui === "web"; // Web arayüzü: tam sayfa, pencere çerçevesi yok
  } catch { /* yok */ }
  html.dataset.embed = embedded ? (web ? "page" : mobile ? "mobile" : "window") : "standalone";
}

export const app = {
  embedded,
  events: null,
  title(s) { document.title = s; post({ mk: "title", title: s }); },
  chrome(o) { post({ mk: "chrome", ...o }); },
  path(p) { post({ mk: "path", path: p }); },
  open(appId, path) { embedded ? post({ mk: "open", app: appId, path }) : (location.href = `/?open=${encodeURIComponent(appId)}`); },
  close() { embedded ? post({ mk: "close" }) : history.back(); },
  on(cmd, fn) { cmds[cmd] = fn; },
  get visible() { return visible(); },
  // fn(görünür) — gizliyken yoklama/çizim durdurulmalı (Pi'de CPU tasarrufu)
  onVisibility(fn) { visFns.add(fn); return () => visFns.delete(fn); },
  init({ title, standaloneBar = true } = {}) {
    layout();
    if (embedded) { try { parent.addEventListener("resize", layout); } catch { /* yok */ } }
    this.events = sharedEvents();
    if (title) this.title(title);
    if (!embedded) { syncPrefs(); setFavicon(brand(pref("look")).favicon); } // telefonda doğrudan açılan uygulama: tercihler + sekme simgesi
    if (!embedded && standaloneBar) {
      document.body.prepend(h("header.sa-bar", h("a.sa-home", { href: "/", html: icon("chevron-left", 18, 2.2) + "<span>MakerOS</span>" }), h("b.sa-title", title || "")));
    }
    return this;
  },
};

addEventListener("message", e => {
  if (e.origin !== location.origin || !e.data?.mk) return;
  const m = e.data;
  if (m.mk === "command") cmds[m.cmd]?.(m);
  else if (m.mk === "visibility") { shellVis = !!m.visible; visChanged(); }
  else if (m.mk === "navigate") { if (m.path != null) location.hash = m.path; cmds.navigate?.(m.path); }
  else if (m.mk === "closing") cmds.closing?.();
});
bus.on("unauthorized", () => {
  if (embedded) post({ mk: "unauthorized" });
  else location.href = "/?next=" + encodeURIComponent(location.pathname + location.hash);
});
export { t };
