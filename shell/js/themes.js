// Tema paketleri: duvar kâğıdı (açık/koyu + bulanık), varsayılan vurgu rengi ve isteğe bağlı marka/simge/uygulamalar.
// Yerleşik duvar kâğıtları tools/gen-wallpaper.mjs ile kodla üretilir (özgün, lisans temiz).
// Eklentiler de tema getirebilir (plugin.json "theme"): ör. MakerOS ile gelen Konya HackerSpace eklentisi
// logo, menü çubuğu işareti, sekme simgesi (favicon), uygulama simge paketi ve raf uygulamaları getirir.
// Açık/koyu görünüm temadan bağımsızdır (pref "theme"); duvar kâğıdı temadan ayrı da seçilebilir.
import { store } from "./lib.js";

export const THEMES = [
  { id: "maker", accent: "orange" },       // grafit, kesim altlığı, teknik çizim (varsayılan)
  { id: "hardware", accent: "yellow" },    // devre kartı, bakır yollar
  { id: "cyber", accent: "cyan" },         // ağ grafiği, kilit
  { id: "print3d", accent: "orange" },     // katman çizgileri, baskı tablası
];
export const DEFAULT_THEME = "maker";
const LEGACY = { hackerspace: "khs" };
const find = id => THEMES.find(t => t.id === (LEGACY[id] || id));
export const themeById = id => find(id) || THEMES[0];

// Giriş/açılış ekranı oturumsuz açılır; eklenti varlıkları oturum ister. Bu yüzden etkin eklenti temasının
// logo/işaret/favicon/bulanık duvar kâğıdı veri URL'si olarak saklanır (mk.brand) ve oturum yokken kullanılır.
const cached = id => { const c = store.get("brand"); return c && c.look === (LEGACY[id] || id) ? c : null; };

// Etkin temanın markası (tema özel değilse MakerOS).
export function brand(id) {
  const t = find(id), c = !t ? cached(id) : null;
  const logo = t?.logo || c?.logo || "/shell/icons/logo.svg";
  return { logo, mark: t?.mark || c?.mark || "", favicon: t?.favicon || c?.favicon || (t?.logo || c?.logo) || "/shell/icons/logo.svg" };
}
export const wallURL = (id, dark, blur = false) => {
  // Kullanıcının yüklediği resim: "custom:<sürüm>" (makerd /api/v1/wallpaper; sürüm önbelleği kırar).
  if (typeof id === "string" && id.startsWith("custom:")) return `/api/v1/wallpaper/${blur ? "desktop-blur" : "desktop"}?v=${encodeURIComponent(id.slice(7))}`;
  const t = find(id);
  if (!t) { const c = cached(id); if (c?.wall) return c.wall; }
  const th = t || THEMES[0];
  if (th.wall) return th.wall[(dark ? "dark" : "light") + (blur ? "_blur" : "")] || th.wall[dark ? "dark" : "light"] || th.wall.dark;
  return `/shell/wallpapers/${th.id}-${dark ? "dark" : "light"}${blur ? "-blur.png" : ".svg"}`;
};
// Uygulama simgesi: etkin tema simge paketi getiriyorsa ondan (ör. KHS), yoksa yerleşik.
const BUILTIN_ICONS = new Set(["settings", "terminal", "activity", "files", "python", "agents", "camera", "gpio", "plugins", "browser", "chromium", "launchpad", "web", "app"]);
// Aynı simgenin diğer adı: tema paketi yalnız birini getirebilir (KHS: chromium.svg var, browser.svg yok).
const ICON_ALIAS = { browser: "chromium", chromium: "browser" };
export function themeIcon(id, name, fallback) {
  const t = find(id);
  return t?.icons && BUILTIN_ICONS.has(name) ? t.icons + name + ".svg" : fallback;
}
// Tema paketinde eksik simge: önce diğer adı, o da yoksa yerleşik simge (/shell/icons/<ad>.svg). Kabuk, web
// arayüzü ve uygulamalar aynı modülü yükler; <img> hataları belge düzeyinde yakalanır (yakalama evresi).
if (typeof document !== "undefined") {
  document.addEventListener("error", e => {
    const img = e.target;
    if (!(img instanceof HTMLImageElement)) return;
    const m = /\/api\/v1\/plugins\/[^/]+\/asset\/(?:.*\/)?([a-z-]+)\.svg$/.exec(img.src);
    if (!m || !BUILTIN_ICONS.has(m[1])) return;
    const tried = (img.dataset.iconTried || "").split(",");
    const alias = ICON_ALIAS[m[1]];
    if (alias && !tried.includes(alias)) {
      img.dataset.iconTried = [...tried, m[1], alias].filter(Boolean).join(",");
      img.src = img.src.replace(new RegExp(m[1] + "\\.svg$"), alias + ".svg");
    } else {
      img.src = `/shell/icons/${m[1]}.svg`;
    }
  }, true);
}
// Tema adı: yerleşikler i18n'den, eklenti temaları kendi adlarıyla ({tr,en} veya metin).
export const themeName = (t, tr, lang) => t.plugin ? (typeof t.name === "object" ? (t.name[lang] || t.name.tr) : t.name) : tr("theme." + t.id);
export const themeDesc = (t, tr, lang) => t.plugin ? (typeof t.desc === "object" ? (t.desc[lang] || t.desc.tr) : (t.desc || "")) : tr("theme." + t.id + ".d");

// Eklenti temalarını kaydet (GET /plugins sonucu). Varlıklar eklenti dizininden sunulur (süreç gerekmez).
export function registerPluginThemes(plugins) {
  const asset = (pid, rel) => rel && (/^https?:/.test(rel) ? rel : `/api/v1/plugins/${pid}/asset/${rel.replace(/^\.?\//, "")}`);
  for (let i = THEMES.length - 1; i >= 0; i--) if (THEMES[i].plugin) THEMES.splice(i, 1);
  for (const p of plugins || []) {
    const th = p.theme;
    if (!th || !th.id || THEMES.some(t => t.id === th.id)) continue;
    const w = th.wallpaper || {};
    THEMES.push({
      id: th.id, plugin: p.id, name: th.name || p.name, desc: th.desc || p.description, accent: th.accent || "orange", mode: th.mode,
      logo: asset(p.id, th.logo), mark: asset(p.id, th.mark), favicon: asset(p.id, th.favicon || th.logo),
      icons: th.icons ? asset(p.id, th.icons.replace(/\/?$/, "/")) : "",
      wall: { dark: asset(p.id, w.dark), light: asset(p.id, w.light || w.dark), dark_blur: asset(p.id, w.dark_blur || w.dark), light_blur: asset(p.id, w.light_blur || w.light || w.dark) },
      apps: (th.apps || []).map(a => ({ ...a, id: `makeros.plugin.${p.id}.${a.id}`, icon: asset(p.id, a.icon) || "/shell/icons/plugins.svg" })),
      status: p.status && p.web_port ? { ...p.status, url: `/plugin/${p.id}${p.status.api}`, plugin: p.id, autostart: !!p.autostart, running: !!p.running } : null,
      widget: p.widget && p.web_port ? { url: `/plugin/${p.id}${p.widget.path || "/"}`, w: p.widget.w || 320, h: p.widget.h || 200, plugin: p.id, running: !!p.running } : null,
    });
  }
}

// Oturum varken etkin eklenti temasının markasını veri URL'si olarak sakla (giriş ekranı için).
export async function cacheBrand(id) {
  const t = find(id);
  if (!t?.plugin) { store.set("brand", null); return; }
  const data = async u => {
    if (!u) return "";
    try { const r = await fetch(u); if (!r.ok) return ""; const b = await r.blob(); return await new Promise(res => { const fr = new FileReader(); fr.onload = () => res(fr.result); fr.readAsDataURL(b); }); }
    catch { return ""; }
  };
  const [logo, mark, favicon, wall] = await Promise.all([data(t.logo), data(t.mark), data(t.favicon), data(t.wall?.dark_blur)]);
  store.set("brand", { look: t.id, logo, mark, favicon, wall });
}

// Sekme simgesini (favicon) temaya göre ayarla.
export function setFavicon(href) {
  let l = document.querySelector('link[rel="icon"]');
  if (!l) { l = document.createElement("link"); l.rel = "icon"; document.head.append(l); }
  if (l.getAttribute("href") !== href) { l.type = href.startsWith("data:image/svg") || href.endsWith(".svg") ? "image/svg+xml" : ""; l.href = href; }
}

// Widget'ın eklenti süreci hazır mı: çalışmıyorsa başlatır, widget adresi 200 dönene kadar bekler (en çok ~20 sn).
// Hazır olmadan iframe açılırsa 503 sayfası görünür ve "load" yine tetiklenir; bu yüzden önce yoklanır.
export async function widgetReady(w, start) {
  if (!w.running) { try { await start(w.plugin); } catch { /* başlatılamadı */ } }
  for (let i = 0; i < 40; i++) {
    try { if ((await fetch(w.url, { cache: "no-store", credentials: "same-origin" })).ok) return true; } catch { /* yok */ }
    await new Promise(r => setTimeout(r, 500));
  }
  return false;
}
