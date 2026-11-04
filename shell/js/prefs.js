// Görünüm tercihleri: tema, efekt katmanı (fx), vurgu rengi, raf, dil. Kabuk ve uygulamalar paylaşır.
// Doğruluk kaynağı makerd `GET/PUT /prefs` (kullanıcı başına); localStorage yalnız önbellek.
// "storage" olayı ile açık tüm pencereler anında güncellenir; PUT'u yalnız değişikliği yapan bağlam gönderir.
import { store, Emitter } from "./lib.js";
import { api } from "./api.js";

export const prefs = new Emitter();
const DEF = { theme: "auto", look: "maker", wallpaper: "auto", fx: "auto", accent: "orange", magnify: false, ctrlCmd: "auto", dockApps: null, lang: "tr" };
const SYNC = Object.keys(DEF);
export const pref = k => store.get(k, DEF[k]);
export function setPref(k, v) { store.set(k, v); apply(); prefs.emit("change", k, v); push(); }

// ------------------------------------------------------------ sunucu eşitlemesi
let remote = true, timer = 0;
const body = () => Object.fromEntries(SYNC.map(k => [k, pref(k)]));
async function send(keepalive = false) {
  clearTimeout(timer); timer = 0;
  if (!remote) return;
  try { await api("PUT", "/prefs", body(), { keepalive }); }
  catch (e) { if (e.status === 404 || e.status === 405) remote = false; /* arka uç desteklemiyor → yalnız yerel */ }
}
function push() { clearTimeout(timer); timer = setTimeout(send, 700); }
export const flushPrefs = () => send();
addEventListener("pagehide", () => { if (timer) send(true); });

// Oturum açıldıktan sonra çağrılır. Sunucudaki değerler önbelleği ezer; sunucu boşsa yerel değerler yüklenir.
// Dönüş: değişen anahtarlar (ör. "lang" değiştiyse çağıran sayfayı yeniden yükler).
export async function syncPrefs() {
  let r;
  try { r = await api("GET", "/prefs"); }
  catch (e) { if (e.status === 404 || e.status === 405) remote = false; return []; }
  if (!r || typeof r !== "object" || !SYNC.some(k => k in r)) { send(); return []; }
  const changed = SYNC.filter(k => k in r && JSON.stringify(r[k]) !== JSON.stringify(store.get(k)));
  for (const k of changed) store.set(k, r[k]);
  if (changed.length) { apply(); for (const k of changed) prefs.emit("change", k, r[k]); }
  return changed;
}

// ------------------------------------------------------------ uygulama
const darkMQ = matchMedia("(prefers-color-scheme: dark)");
// Düşük donanım: Pi 4 (4 çekirdek, 4 GB), WPE motoru → "lite" (önceden bulanık duvar kâğıdı)
export function autoFx() {
  if (matchMedia("(prefers-reduced-transparency: reduce)").matches) return "flat";
  const hc = navigator.hardwareConcurrency || 4, dm = navigator.deviceMemory;
  if (/WPE|cog/i.test(navigator.userAgent) || hc <= 4 || (dm && dm <= 4)) return "lite";
  return "full";
}
export function fxTier() {
  const q = new URLSearchParams(location.search).get("fx");
  const v = ["full", "lite", "flat"].includes(q) ? q : pref("fx");
  return v === "auto" ? autoFx() : v;
}
export const isDark = () => { const t = pref("theme"); return t === "dark" || (t === "auto" && darkMQ.matches); };

export function apply(root = document.documentElement) {
  const t = pref("theme");
  t === "auto" ? root.removeAttribute("data-theme") : root.setAttribute("data-theme", t);
  root.dataset.fx = fxTier();
  root.dataset.accent = pref("accent");
  root.dataset.dark = isDark() ? "1" : "0";
}
darkMQ.addEventListener?.("change", () => { apply(); prefs.emit("change", "theme"); });
addEventListener("storage", e => { if (e.key?.startsWith("mk.")) { apply(); prefs.emit("change", e.key.slice(3)); } });
apply();
