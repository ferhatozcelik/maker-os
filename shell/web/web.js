// MakerOS Web arayüzü (uzak tarayıcı varsayılanı): masaüstü ve pencere yok; kenar çubuğundan uygulamalar
// tam sayfa açılır (her uygulama kendi sayfasıdır, iframe). "Ekran" geçişi MakerOS masaüstüne (/) gider.
// Uygulama ↔ kabuk mesajları masaüstüyle aynıdır ({mk:"title"|"open"|"close"|"path"}).
import { h, $, bytes, duration } from "/shell/js/lib.js";
import { t } from "/shell/js/i18n.js";
import { icon } from "/shell/js/icons.js";
import { get, post, Events, bus } from "/shell/js/api.js";
import { prefs, pref, setPref, isDark, syncPrefs } from "/shell/js/prefs.js";
import { wallURL, brand, themeById, setFavicon, widgetReady, pluginWidget } from "/shell/js/themes.js";
import { APPS, loadApps, SETTINGS_PAGES, SETTINGS_COLORS, PYTHON_PAGES } from "/shell/js/apps.js";
import { watchThemeStatus } from "/shell/js/status.js";
import { registerSW } from "/shell/js/pwa.js";

registerSW();

const html = document.documentElement;
html.dataset.ui = "web";
const view = $(".view"), nav = $(".nav"), ttl = $(".ttl");
// Web arayüzünde gösterilmeyenler.
const HIDDEN = new Set(["chromium"]);
// Kenar çubuğunda "Ana sayfa"nın hemen altında (uygulama listesinde ayrıca gösterilmez).
const PINNED = "makeros.assistant";
const SYSTEM = new Set(["makeros.activity", "makeros.settings"]);
const ORDER = ["makeros.files", "makeros.browser", "makeros.terminal", "makeros.python", "makeros.agents", "makeros.camera", "makeros.gpio", "makeros.plugins"];
const frames = new Map(); // uygulama → iframe (açık kalır: terminal oturumu, kamera durumu korunur)
let cur = "home", sys = null;
// Alt menülü uygulamalar: seçilince kenar çubuğunda altında bölümleri açılır (#settings/wifi, #python/versions).
const SUBNAV = {
  "makeros.settings": { def: "wifi", items: () => SETTINGS_PAGES.map(([id, ic]) => [id, ic, t("set." + id), SETTINGS_COLORS[id]]) },
  "makeros.python": { def: "projects", items: () => PYTHON_PAGES.map(([id, ic, c]) => [id, ic, t("py." + id), c]) },
};
const subCur = new Map(); // uygulama → seçili bölüm (uygulamanın {mk:"path"} iletisiyle güncellenir)
const subOf = id => { const s = SUBNAV[id], p = subCur.get(id); if (!s) return null; return s.items().some(([x]) => x === p) ? p : s.def; };

// Ana sayfa görseli: temanın koyu (keskin) duvar kâğıdı; açık modda da yazı okunur kalır.
function wall() {
  html.style.setProperty("--wall", `url("${wallURL(pref("wallpaper") !== "auto" && pref("wallpaper") ? pref("wallpaper") : pref("look"), true)}")`);
  const b = brand(pref("look"));
  document.querySelectorAll("img.brand-logo").forEach(i => { i.src = b.logo; }); // temaya özel logo
  setFavicon(b.favicon);
}
prefs.on("change", k => { wall(); if (k === "look" || k === "desktopWidgets") setTimeout(() => { renderNav(); renderTabbar(); if (cur === "home") renderHome(); }); });

const short = id => id.replace(/^makeros\./, "");
const apps = () => APPS.filter(a => a.kind === "web" && !HIDDEN.has(a.id));
const sorted = list => list.sort((a, b) => ((ORDER.indexOf(a.id) + 1) || 99) - ((ORDER.indexOf(b.id) + 1) || 99) || a.name.localeCompare(b.name));

function renderNav() {
  const user = sorted(apps().filter(a => !SYSTEM.has(a.id) && a.id !== PINNED));
  const pin = APPS.find(a => a.id === PINNED);
  const link = (id, name, img) => {
    const s = SUBNAV[id], open = s && cur === id;
    const a = h("a" + (cur === id ? ".on" : "") + (s ? ".has-sub" : "") + (open ? ".open" : ""), { href: "#" + (id === "home" ? "" : short(id)), "data-id": id },
      img, h("span.nl", name), s ? h("span.chev", { html: icon("chevron-right", 14, 2.2) }) : "");
    if (!s) return a;
    const sel = subOf(id);
    const list = h("div.sub", { hidden: !open }, ...s.items().map(([sid, ic, label, color]) =>
      h("a.sub-a" + (open && sid === sel ? ".on" : ""), { href: `#${short(id)}/${sid}`, "data-sub": sid }, h("span.sb-ic", { style: { background: color }, html: icon(ic, 13, 2) }), label)));
    return [a, list];
  };
  nav.replaceChildren(
    link("home", t("web.home"), h("span", { html: icon("house", 20, 1.9), style: "width:24px;display:grid;place-items:center" })),
    pin ? link(pin.id, pin.name, h("img", { src: pin.icon, alt: "" })) : "",
    h("div.grp", t("web.apps")), ...user.flatMap(a => link(a.id, a.name, h("img", { src: a.icon, alt: "" }))),
    h("div.grp", t("web.system")), ...sorted(apps().filter(a => SYSTEM.has(a.id))).flatMap(a => link(a.id, a.name, h("img", { src: a.icon, alt: "" }))));
}

// Telefon: alt sekme çubuğu (sık kullanılanlar). CSS yalnız dar ekranda gösterir.
const TABBAR = [["home", "house", () => t("web.home")], ["makeros.files"], ["makeros.browser"], ["makeros.terminal"], ["makeros.python"]];
function renderTabbar() {
  const bar = $(".tabbar");
  bar.replaceChildren(...TABBAR.map(([id, ic, label]) => {
    const app = APPS.find(a => a.id === id);
    if (!ic && !app) return "";
    const href = id === "home" ? "#" : "#" + short(id);
    return h("a" + (cur === id ? ".on" : ""), { href, "data-id": id },
      ic ? h("span.ti", { html: icon(ic, 22, 1.9) }) : h("img", { src: app.icon, alt: "" }), h("span.tl", label ? label() : app.name));
  }));
}

function show(id, path = "") {
  if (id === "makeros.chromium" || id === "chromium") id = "makeros.browser";
  if (id !== "home" && !APPS.some(a => a.id === id)) id = "home";
  const prev = cur;
  cur = id;
  if (SUBNAV[id] && path && SUBNAV[id].items().some(([x]) => x === path)) subCur.set(id, path);
  document.body.classList.remove("drawer");
  if (prev !== id || SUBNAV[id]) renderNav(); else nav.querySelectorAll("a[data-id]").forEach(a => a.classList.toggle("on", a.dataset.id === id));
  document.querySelectorAll(".tabbar a").forEach(a => a.classList.toggle("on", a.dataset.id === id));
  for (const [k, f] of frames) f.hidden = k !== id;
  $(".home")?.toggleAttribute("hidden", id !== "home");
  if (id === "home") { ttl.textContent = t("web.home"); document.title = "MakerOS"; renderHome(); return; }
  const app = APPS.find(a => a.id === id);
  ttl.textContent = app.name; document.title = app.name + " · MakerOS";
  let targetPath = path;
  if (id === "makeros.browser" && targetPath) {
    try {
      const u = new URL(targetPath, location.origin);
      const LOOP = /^(localhost|0\.0\.0\.0|127(\.\d{1,3}){3}|\[::1?\])$/i;
      if (u.port && (LOOP.test(u.hostname) || u.hostname === location.hostname)) {
        targetPath = `/port/${u.port}${u.pathname || "/"}${u.search}${u.hash}`;
      }
    } catch {}
  }
  let f = frames.get(id);
  if (!f) {
    f = h("iframe", { src: app.url + (targetPath ? "#" + targetPath : ""), title: app.name, allow: "clipboard-read; clipboard-write; fullscreen; camera; microphone" });
    frames.set(id, f); view.append(f);
  } else if (targetPath) f.contentWindow?.postMessage({ mk: "navigate", path: targetPath }, location.origin);
}

function route() {
  const [a, ...rest] = location.hash.slice(1).split("/");
  let appId = a ? (a.startsWith("makeros.") ? a : "makeros." + a) : "home";
  if (appId === "makeros.chromium") appId = "makeros.browser";
  const path = decodeURIComponent(rest.join("/"));
  show(appId, path);
}
addEventListener("hashchange", route);

// Uygulamalardan gelen mesajlar
addEventListener("message", e => {
  if (e.origin !== location.origin || !e.data?.mk) return;
  const m = e.data, src = [...frames].find(([, f]) => f.contentWindow === e.source)?.[0];
  if (m.mk === "title" && src === cur && m.title) ttl.textContent = m.title;
  else if (m.mk === "path" && SUBNAV[src]) {
    // Uygulama içinde bölüm değişti: alt menüde işaretle, adresi güncelle (yenileyince aynı bölüm açılır).
    const p = String(m.path || "");
    if (SUBNAV[src].items().some(([x]) => x === p)) subCur.set(src, p);
    const sel = subOf(src);
    nav.querySelectorAll(`a[data-id="${src}"] + .sub .sub-a`).forEach(a => a.classList.toggle("on", a.dataset.sub === sel));
    if (src === cur) history.replaceState(null, "", "#" + short(src) + "/" + encodeURIComponent(p && !SUBNAV[src].items().some(([x]) => x === p) ? p : sel));
  }
  else if (m.mk === "open") {
    let id = m.app;
    if (id === "chromium") id = "makeros.browser";
    let p = m.path || "";
    if (id === "makeros.browser" && p) {
      try {
        const u = new URL(p, location.origin);
        const LOOP = /^(localhost|0\.0\.0\.0|127(\.\d{1,3}){3}|\[::1?\])$/i;
        if (u.port && (LOOP.test(u.hostname) || u.hostname === location.hostname)) {
          p = `/port/${u.port}${u.pathname || "/"}${u.search}${u.hash}`;
        }
      } catch {}
    }
    location.hash = short(id) + (p ? "/" + encodeURIComponent(p) : "");
    if (p && frames.has(id)) frames.get(id).contentWindow?.postMessage({ mk: "navigate", path: p }, location.origin);
  }
  else if (widgetEl && (m.mk === "widget-size" || m.mk === "widget-close")) {
    const f = widgetEl.find(x => x.contentWindow === e.source);
    if (f && m.mk === "widget-size" && m.h > 40 && m.h < 800) f.style.height = m.h + "px";
    if (f && m.mk === "widget-close" && f.dataset.plugin) setPref("desktopWidgets", (pref("desktopWidgets") || []).filter(x => x !== f.dataset.plugin));
  }
  else if (m.mk === "close") location.hash = "";
  else if (m.mk === "unauthorized") location.replace("/?next=/web");
});

// --- ana sayfa ---------------------------------------------------------------
// İskelet bir kez kurulur; sistem olayında (2 sn) yalnız istatistikler yenilenir. Widget iframe'i DOM'dan hiç
// sökülmez (sökülürse tarayıcı onu baştan yükler → titreme). Uygulama ızgarası yalnız liste değişince çizilir.
let homeParts = null, gridKey = "", widgetEl = null, widgetKey = null;
function renderWidget(sec) {
  // Etkin temanın widget'ı + kullanıcının masaüstüne eklediği eklenti widget'ları (masaüstüyle aynı liste).
  const tw = themeById(pref("look")).widget;
  const list = [tw, ...(pref("desktopWidgets") || []).map(pluginWidget)].filter((x, i, a) => x && a.findIndex(y => y?.url === x.url) === i);
  const key = list.map(x => x.url).join("|");
  if (widgetKey === key) return; // aynı widget'lar: dokunma (iframe'ler yeniden yüklenmesin)
  widgetKey = key;
  if (!list.length) { widgetEl = null; sec.replaceChildren(); return; }
  const frames = list.map(w => {
    const f = h("iframe.wdg", { title: "widget" });
    f.dataset.url = w.url;
    if (w !== tw) f.dataset.plugin = w.plugin;
    f.style.cssText = `width:${w.w}px;height:${w.h}px;opacity:0;transition:opacity .25s`;
    widgetReady(w, id => post(`/plugins/${id}/start`)).then(() => {
      if (!f.isConnected) return;
      f.onload = () => { f.style.opacity = "1"; }; // boş iframe'in ilk "load"ı sayılmaz
      f.src = w.url;
    });
    return f;
  });
  widgetEl = frames;
  sec.replaceChildren(h("h3", t("web.widgets")), h("div.widgets", ...frames));
}
function renderHome() {
  let home = $(".home");
  if (!home) {
    home = h("div.home"); view.append(home);
    homeParts = { hero: h("div.hero"), stats: h("div.stats"), wsec: h("div.wsec"), grid: h("div.grid") };
    home.append(homeParts.hero, homeParts.stats, homeParts.wsec, h("h3", t("web.apps")), homeParts.grid);
  }
  const P = homeParts, s = sys, mem = s ? s.mem.used / s.mem.total : 0, disk = s ? s.disk.used / s.disk.total : 0;
  const ip = netIP();
  const stat = (ic, k, n, frac, sub) => h("div.stat", h("div.k", h("span", { html: icon(ic, 14, 2) }), k), h("div.n", n, sub ? h("small", " / " + sub) : ""), frac != null ? h("div.bar", h("i", { style: `width:${Math.round(frac * 100)}%` })) : "");
  P.hero.replaceChildren(h("h2", t("web.hello", { user: me?.user || "" })), h("p", s ? `${s.model} · MakerOS ${s.os.version}` : "MakerOS"));
  P.stats.replaceChildren(
    stat("cpu", t("web.cpu"), s ? `%${Math.round(s.cpu.usage_pct ?? s.cpu.load[0] * 25)}` : "—", s ? (s.cpu.usage_pct ?? 0) / 100 : null),
    stat("thermometer", t("web.temp"), s?.cpu.temp_c != null ? `${s.cpu.temp_c.toFixed(1)} °C` : "—", s ? Math.min(1, (s.cpu.temp_c || 0) / 85) : null),
    stat("memory-stick", t("web.mem"), s ? bytes(s.mem.used) : "—", s ? mem : null, s && bytes(s.mem.total)),
    stat("hard-drive", t("web.disk"), s ? bytes(s.disk.used) : "—", s ? disk : null, s && bytes(s.disk.total)),
    stat("network", t("web.ip"), ip || "—"),
    stat("clock", t("web.uptime"), s ? duration(s.uptime_s) : "—"));
  renderWidget(P.wsec);
  const list = sorted(apps()), key = list.map(a => a.id + "|" + a.name + "|" + a.icon).join(",");
  if (key !== gridKey) {
    gridKey = key;
    P.grid.replaceChildren(...list.map(a => h("a.tile", { href: "#" + short(a.id) }, h("img", { src: a.icon, alt: "" }), a.name)));
  }
}
let net = null;
const netIP = () => (net?.ifaces || []).flatMap(i => i.ipv4 || [])[0]?.split("/")[0] || "—";

let door = null;
watchThemeStatus(s => { door = s; chips(); });
function chips() {
  const s = sys;
  if (!s) return;
  const hot = (s.cpu.temp_c || 0) >= 75;
  $(".chips").replaceChildren(
    h("span.chip", h("span", { html: icon("cpu", 13, 2) }), `%${Math.round(s.cpu.usage_pct ?? 0)}`),
    h("span.chip" + (hot ? ".warn" : ""), h("span", { html: icon("thermometer", 13, 2) }), `${(s.cpu.temp_c || 0).toFixed(0)}°`),
    h("span.chip", h("span", { html: icon("memory-stick", 13, 2) }), bytes(s.mem.used)),
    netIP() ? h("span.chip", h("span", { html: icon("wifi", 13, 2) }), netIP()) : "",
    door ? h("a.chip.door." + door.state, { href: "#" + (door.app || "makeros.plugin." + door.plugin).replace(/^makeros\./, "") }, h("i.dd"), `${door.label} · ${t(door.state === "open" ? "door.open" : door.state === "closed" ? "door.closed" : "door.unknown")}`) : "");
  $(".host").textContent = s.hostname;
}

// --- mod geçişi ---------------------------------------------------------------
const mode = $(".mode");
mode.querySelector('[data-m="web"]').append(h("span", { html: icon("layout-grid", 14, 2) }), h("span.lb", t("web.modeWeb")));
mode.querySelector('[data-m="screen"]').append(h("span", { html: icon("monitor", 14, 2) }), h("span.lb", t("web.modeScreen")));
mode.querySelector('[data-m="web"]').title = t("web.modeWeb");
mode.querySelector('[data-m="screen"]').title = t("web.modeScreenHint");
// "Ekran": MakerOS masaüstü (pencereler, raf; Chromium penceresi dahil). Ekran Modu ayarından bağımsız, her zaman var.
mode.querySelector('[data-m="screen"]').onclick = () => location.assign("/");

$(".b-menu").innerHTML = icon("layout-grid", 16, 2);
$(".b-menu").onclick = () => document.body.classList.toggle("drawer");
$(".scrim").onclick = () => document.body.classList.remove("drawer");
$(".b-out").innerHTML = icon("log-out", 15, 2); $(".b-out").title = t("web.logout");
$(".b-out").onclick = async () => { try { await post("/auth/logout"); } catch { /* yok */ } location.replace("/"); };
bus.on("unauthorized", () => location.replace("/?next=/web"));

let me = null;
(async () => {
  try { me = await get("/session"); } catch { location.replace("/?next=/web"); return; }
  $(".who").textContent = me.user; $(".avatar").textContent = (me.user || "?")[0].toUpperCase();
  await Promise.all([syncPrefs(), loadApps()]);
  wall(); renderNav(); renderTabbar(); route();
  const ev = new Events().start();
  window.__mk = { events: ev }; // uygulamalar kabuğun tek WS bağlantısını paylaşır
  ev.subscribe("system", s => { sys = s; chips(); if (cur === "home") renderHome(); });
  ev.subscribe("net", n => { net = n; chips(); });
  // Eklenti durumu değişti: gezinme, sekmeler ve ana sayfa yenilenir; kalkan eklentinin çerçevesi kapanır.
  let appsTimer = 0;
  const refreshApps = () => { clearTimeout(appsTimer); appsTimer = setTimeout(async () => {
    await loadApps(); // "look" değişimi yayınlar → renderNav/renderTabbar/renderHome
    for (const [id, f] of frames) if (id.startsWith("makeros.plugin.") && !APPS.some(a => a.id === id)) { f.remove(); frames.delete(id); }
    if (cur !== "home" && !APPS.some(a => a.id === cur)) { history.replaceState(null, "", location.pathname); route(); } // ana sayfaya; adres de
  }, 600); };
  ev.subscribe("plugins", refreshApps);
  ev.on("open", refreshApps);
  get("/system").then(s => { sys = s; chips(); if (cur === "home") renderHome(); }).catch(() => {});
  get("/net").then(n => { net = n; chips(); }).catch(() => {});
})();
