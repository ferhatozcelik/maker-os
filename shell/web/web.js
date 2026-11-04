// MakerOS Web arayüzü (uzak tarayıcı varsayılanı): masaüstü ve pencere yok; kenar çubuğundan uygulamalar
// tam sayfa açılır (her uygulama kendi sayfasıdır, iframe). "Ekran" modu Pi'nin gerçek ekranını (VNC) açar.
// Uygulama ↔ kabuk mesajları masaüstüyle aynıdır ({mk:"title"|"open"|"close"|"path"}).
import { h, $, bytes, duration } from "/shell/js/lib.js";
import { t } from "/shell/js/i18n.js";
import { icon } from "/shell/js/icons.js";
import { get, post, Events, bus } from "/shell/js/api.js";
import { prefs, pref, isDark, syncPrefs } from "/shell/js/prefs.js";
import { wallURL, brand, themeById, setFavicon, widgetReady } from "/shell/js/themes.js";
import { APPS, loadApps, SCREEN } from "/shell/js/apps.js";
import { watchThemeStatus } from "/shell/js/status.js";
import { registerSW } from "/shell/js/pwa.js";
import { openScreen } from "/shell/js/screen-overlay.js";

registerSW();

const html = document.documentElement;
html.dataset.ui = "web";
const view = $(".view"), nav = $(".nav"), ttl = $(".ttl");
// Web arayüzünde gösterilmeyenler: Ekran Modu uygulaması (üstteki "Ekran Modu" geçişi onun yerine geçer).
const HIDDEN = new Set(["makeros.screen"]);
const SYSTEM = new Set(["makeros.activity", "makeros.settings"]);
const ORDER = ["makeros.files", "makeros.terminal", "makeros.python", "makeros.agents", "makeros.camera", "makeros.gpio", "makeros.plugins"];
const frames = new Map(); // uygulama → iframe (açık kalır: terminal oturumu, kamera durumu korunur)
let cur = "home", sys = null;

// Ana sayfa görseli: temanın koyu (keskin) duvar kâğıdı; açık modda da yazı okunur kalır.
function wall() {
  html.style.setProperty("--wall", `url("${wallURL(pref("wallpaper") !== "auto" && pref("wallpaper") ? pref("wallpaper") : pref("look"), true)}")`);
  const b = brand(pref("look"));
  document.querySelectorAll("img.brand-logo").forEach(i => { i.src = b.logo; }); // temaya özel logo
  setFavicon(b.favicon);
}
prefs.on("change", k => { wall(); if (k === "look") setTimeout(() => { renderNav(); renderTabbar(); if (cur === "home") renderHome(); }); });

const short = id => id.replace(/^makeros\./, "");
const apps = () => APPS.filter(a => a.kind === "web" && !HIDDEN.has(a.id));
const sorted = list => list.sort((a, b) => ((ORDER.indexOf(a.id) + 1) || 99) - ((ORDER.indexOf(b.id) + 1) || 99) || a.name.localeCompare(b.name));

function renderNav() {
  const user = sorted(apps().filter(a => !SYSTEM.has(a.id)));
  const link = (id, name, img) => h("a" + (cur === id ? ".on" : ""), { href: "#" + (id === "home" ? "" : short(id)), "data-id": id }, img, name);
  nav.replaceChildren(
    link("home", t("web.home"), h("span", { html: icon("house", 20, 1.9), style: "width:24px;display:grid;place-items:center" })),
    h("div.grp", t("web.apps")), ...user.map(a => link(a.id, a.name, h("img", { src: a.icon, alt: "" }))),
    h("div.grp", t("web.system")), ...sorted(apps().filter(a => SYSTEM.has(a.id))).map(a => link(a.id, a.name, h("img", { src: a.icon, alt: "" }))));
}

// Telefon: alt sekme çubuğu (sık kullanılanlar + Ekran Modu). CSS yalnız dar ekranda gösterir.
const TABBAR = [["home", "house", () => t("web.home")], ["makeros.files"], ["makeros.terminal"], ["makeros.python"], ["screen", "monitor", () => t("app.screen")]];
function renderTabbar() {
  const bar = $(".tabbar");
  bar.replaceChildren(...TABBAR.map(([id, ic, label]) => {
    const app = APPS.find(a => a.id === id);
    if (!ic && !app) return "";
    if (id === "screen" && !SCREEN.enabled) return ""; // Ekran Modu kapalı (Ayarlar)
    const href = id === "home" ? "#" : id === "screen" ? "#" : "#" + short(id);
    const onclick = id === "screen" ? e => { e.preventDefault(); openScreen(); } : null;
    return h("a" + (cur === id ? ".on" : ""), { href, "data-id": id, onclick },
      ic ? h("span.ti", { html: icon(ic, 22, 1.9) }) : h("img", { src: app.icon, alt: "" }), h("span.tl", label ? label() : app.name));
  }));
}

function show(id, path = "") {
  if (id !== "home" && !APPS.some(a => a.id === id)) id = "home";
  cur = id;
  document.body.classList.remove("drawer");
  nav.querySelectorAll("a").forEach(a => a.classList.toggle("on", a.dataset.id === id));
  document.querySelectorAll(".tabbar a").forEach(a => a.classList.toggle("on", a.dataset.id === id));
  for (const [k, f] of frames) f.hidden = k !== id;
  $(".home")?.toggleAttribute("hidden", id !== "home");
  if (id === "home") { ttl.textContent = t("web.home"); document.title = "MakerOS"; renderHome(); return; }
  const app = APPS.find(a => a.id === id);
  ttl.textContent = app.name; document.title = app.name + " · MakerOS";
  let f = frames.get(id);
  if (!f) {
    f = h("iframe", { src: app.url + (path ? "#" + path : ""), title: app.name, allow: "clipboard-read; clipboard-write; fullscreen; camera; microphone" });
    frames.set(id, f); view.append(f);
  } else if (path) f.contentWindow?.postMessage({ mk: "navigate", path }, location.origin);
}

function route() {
  const [a, ...rest] = location.hash.slice(1).split("/");
  const path = decodeURIComponent(rest.join("/"));
  show(a ? (a.startsWith("makeros.") ? a : "makeros." + a) : "home", path);
}
addEventListener("hashchange", route);

// Uygulamalardan gelen mesajlar
addEventListener("message", e => {
  if (e.origin !== location.origin || !e.data?.mk) return;
  const m = e.data, src = [...frames].find(([, f]) => f.contentWindow === e.source)?.[0];
  if (m.mk === "title" && src === cur && m.title) ttl.textContent = m.title;
  else if (m.mk === "open") { const id = m.app; location.hash = short(id) + (m.path ? "/" + encodeURIComponent(m.path) : ""); if (m.path && frames.has(id)) frames.get(id).contentWindow?.postMessage({ mk: "navigate", path: m.path }, location.origin); }
  else if (m.mk === "widget-size" && widgetEl && e.source === widgetEl.contentWindow && m.h > 40 && m.h < 800) widgetEl.style.height = m.h + "px";
  else if (m.mk === "close") location.hash = "";
  else if (m.mk === "unauthorized") location.replace("/?next=/web");
});

// --- ana sayfa ---------------------------------------------------------------
// İskelet bir kez kurulur; sistem olayında (2 sn) yalnız istatistikler yenilenir. Widget iframe'i DOM'dan hiç
// sökülmez (sökülürse tarayıcı onu baştan yükler → titreme). Uygulama ızgarası yalnız liste değişince çizilir.
let homeParts = null, gridKey = "", widgetEl = null;
function renderWidget(sec) {
  const w = themeById(pref("look")).widget;
  if (!w) { widgetEl = null; sec.replaceChildren(); return; }
  if (widgetEl && widgetEl.dataset.url === w.url) return; // aynı widget: dokunma
  const f = h("iframe.wdg", { title: "widget" });
  f.dataset.url = w.url;
  f.style.cssText = `width:${w.w}px;height:${w.h}px;opacity:0;transition:opacity .25s`;
  widgetEl = f;
  sec.replaceChildren(h("h3", t("web.widgets")), h("div.widgets", f));
  widgetReady(w, id => post(`/plugins/${id}/start`)).then(() => {
    if (widgetEl !== f) return;
    f.onload = () => { f.style.opacity = "1"; }; // boş iframe'in ilk "load"ı sayılmaz
    f.src = w.url;
  });
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
    stat("network", t("web.ip"), ip || t("web.offline")),
    stat("clock", t("web.uptime"), s ? duration(s.uptime_s) : "—"));
  renderWidget(P.wsec);
  const list = sorted(apps()), key = list.map(a => a.id + "|" + a.name + "|" + a.icon).join(",");
  if (key !== gridKey) {
    gridKey = key;
    P.grid.replaceChildren(...list.map(a => h("a.tile", { href: "#" + short(a.id) }, h("img", { src: a.icon, alt: "" }), a.name)));
  }
}
let net = null;
const netIP = () => (net?.ifaces || []).flatMap(i => i.ipv4 || [])[0]?.split("/")[0] || "";

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
  get("/system").then(s => { sys = s; chips(); if (cur === "home") renderHome(); }).catch(() => {});
  get("/net").then(n => { net = n; chips(); }).catch(() => {});
})();
