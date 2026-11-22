// MakerOS web masaüstü kabuğu — açılış.
// Yalnız masaüstünün ilk karesi için gerekenler doğrudan içe aktarılır; Bul, Uygulamalar ızgarası,
// değiştirici, Genel Bakış, Kontrol Merkezi, kilit/giriş, mobil düzen, kurulum sihirbazı ve perf HUD
// ilk kullanımda dinamik import ile yüklenir (bkz. M).
import { $, h, store, lazy } from "./lib.js";
import { t } from "./i18n.js";
import { prefs, pref, setPref, isDark, syncPrefs } from "./prefs.js";
import { wallURL, themeById, brand, setFavicon, widgetReady, pluginWidget } from "./themes.js";
import { get, post, bus, Events } from "./api.js";
import { APPS, appById, loadApps, isBrowser } from "./apps.js";
import { keys, attach } from "./keys.js";
import { WindowManager } from "./wm/index.js";
import { WebBackend, bridge } from "./wm/web.js";
import { S } from "./shell.js";
import { mountMenubar } from "./ui/menubar.js";
import { mountDock } from "./ui/dock.js";
import { initNotifications, toast, toggleNC } from "./ui/control.js";
import { closeOverlay, overlayOpen } from "./ui/layer.js";
import { closePopup, menu } from "./ui/popup.js";
import { registerSW } from "./pwa.js";

registerSW();

const html = document.documentElement;
const mq = matchMedia("(max-width: 699px)");
const qs = new URLSearchParams(location.search);
const C = "/shell/css/";
const M = {
  find: lazy(() => import("./ui/find.js"), C + "overlays.css"),
  launchpad: lazy(() => import("./ui/launchpad.js"), C + "overlays.css"),
  switcher: lazy(() => import("./ui/switcher.js"), C + "overlays.css"),
  overview: lazy(() => import("./ui/overview.js"), C + "overlays.css"),
  lock: lazy(() => import("./ui/lock.js"), C + "lock.css"),
  mobile: lazy(() => import("./ui/mobile.js"), C + "mobile.css"),
  native: lazy(() => import("./wm/native.js")),
  setup: lazy(() => import("../setup/setup.js"), "/shell/setup/setup.css"),
};
S.lazy = M;
const locked = () => !$("#lock").hidden;
const lockScreen = (...a) => M.lock().then(m => m.lockScreen(...a));
const confirmDialog = (...a) => M.lock().then(m => m.confirmDialog(...a));

// Duvar kâğıdı: tema + fx katmanı (lite/flat → önceden bulanık sürüm yüzeylerde kullanılır)
function wallpaper() {
  const w = pref("wallpaper"), id = w && w !== "auto" ? w : pref("look"), d = isDark();
  // Temaya özel marka (logo + menü çubuğu işareti) ve uygulamalar (ör. Konya HackerSpace sitesi raf'ta)
  const b = brand(pref("look"));
  html.style.setProperty("--brand-mark", `url("${b.mark || "/shell/icons/mark.svg"}")`);
  document.querySelectorAll("img.brand-logo").forEach(i => { i.src = b.logo; });
  setFavicon(b.favicon); // sekme simgesi de temaya göre (ör. KHS)
  html.style.setProperty("--wall", `url("${wallURL(id, d)}")`);
  html.style.setProperty("--wall-blur", `url("${wallURL(id, d, false)}")`);
  html.dataset.look = themeById(pref("look")).id;
}
wallpaper(); prefs.on("change", wallpaper);

// Çalışma alanı (menü çubuğu altı, raf üstü). Raf yüksekliği ResizeObserver ile önbelleğe alınır:
// render() her çağrıda offsetHeight okumaz (zorunlu eşzamanlı yerleşim yok).
let dockH = 70;
new ResizeObserver(([e]) => { const v = Math.round(e.borderBoxSize?.[0]?.blockSize ?? e.target.offsetHeight); if (v && v !== dockH) { dockH = v; S.wm?.relayout(); } }).observe($("#dock"));
const area = () => {
  const vw = innerWidth, vh = innerHeight, mob = mq.matches;
  const full = S.wm?.focused?.state === "full";
  const showMb = pref("uiMenubar") !== false;
  const showDock = pref("uiDock") !== false;
  const top = mob || !showMb ? 0 : 28, bottom = mob || full || !showDock ? 0 : dockH + 4;
  return { x: 0, y: top, w: vw, h: vh - top - bottom, vw, vh, mt: mob ? 26 : 0, mb: mob ? 20 : 0 };
};

const wm = S.wm = new WindowManager(area);
wm.register(new WebBackend($("#windows")));
let native = null;
S.toast = toast;
S.apps = APPS;

S.launch = async (app, o = {}) => {
  if (typeof app === "string") {
    // Tarayıcı istenirse (ör. terminaldeki bağlantı): Chromium girdisi yoksa /web'deki Tarayıcı, o da yoksa pencerenin kendisi.
    const br = app === "chromium" || app === "makeros.browser";
    app = appById(app) || (br ? appById("makeros.browser") || appById("chromium") || { id: "makeros.browser", name: t("app.browser"), kind: "web", categories: ["WebBrowser"] } : null);
  }
  if (!app) return;
  closeOverlay();
  // MakerOS yerleşik tarayıcısı: hem Pi ekranında hem uzaktan her zaman pencere içinde açılır.
  if (isBrowser(app)) {
    let p = o.path || "";
    if (p) {
      try {
        const u = new URL(p, location.origin);
        const LOOP = /^(localhost|0\.0\.0\.0|127(\.\d{1,3}){3}|\[::1?\])$/i;
        if (u.port && (LOOP.test(u.hostname) || u.hostname === location.hostname)) {
          p = `/port/${u.port}${u.pathname || "/"}${u.search}${u.hash}`;
        }
      } catch {}
    }
    const br = { ...app, kind: "web", url: "/shell/apps/browser/", w: 1180, h: 760, single: true };
    // Adresle açılış (ör. terminaldeki bağlantı): açık pencere varsa o adrese gider.
    return p ? wm.open(br, { path: p }) : wm.activateApp(br);
  }
  if (app.kind === "native") {
    // Diğer yerel uygulamalar Pi'nin kendi ekranında (labwc) açılır.
    toast(t("launch.native", { app: app.name }), "app-window");
    try { native ??= new (await M.native()).NativeBackend(); await native.launch(app); } catch (e) { toast(t("launch.fail", { app: app.name }) + ": " + e.message, "circle-alert"); return; }
    if (!S.local) toast(t("launch.onPi", { app: app.name }), "monitor");
    return;
  }
  if (o.force || o.path) return wm.open(app, o);
  return wm.activateApp(app);
};
// Pi'nin kendi ekranında (cog/WPE) mı çalışıyoruz? Orada Web↔Ekran geçişi anlamsız.
S.local = (() => {
  // Pi'nin kendi ekranı: makerd yerel girişi ?local=1 ile açar; oturum boyunca hatırlanır.
  try {
    if (new URLSearchParams(location.search).get("local") === "1") sessionStorage.setItem("mk.local", "1");
    if (sessionStorage.getItem("mk.local") === "1") return true;
  } catch { /* depolama yok */ }
  return /WPE|cog/i.test(navigator.userAgent);
})();
// Web arayüzüne geç (raftaki düğme, Kontrol Merkezi).
S.webMode = () => location.assign("/web");
S.openSettings = page => S.launch("makeros.settings", { path: page || "" });
S.about = app => M.lock().then(m => m.aboutDialog(app, S.sys));
// Kilit sunucu tarafında da geçerli: oturum kapatılır (çerez geçersiz), kullanıcı adı yalnız bu tarayıcıda kalır.
// Sayfa yenilense de kilit atlanamaz: açılışta oturum yoktur ve doğrudan o kullanıcının kilit ekranı gelir.
// Pencere düzeni (wm.session) korunur: parola girilince masaüstü kaldığı yerden açılır.
S.lock = async () => {
  closeOverlay(); closePopup();
  if (locked()) return;
  const u = S.user?.user;
  store.set("lockUser", u || null);
  S.events.stop();
  try { await post("/auth/logout"); } catch { /* ağ yoksa da kilit ekranı açılır */ }
  const r = await lockScreen("lock", u);
  store.set("lockUser", null);
  if (r.user !== u) location.reload();
  else { S.user = r; S.events.start(); }
};
S.logout = async () => {
  if (!(await confirmDialog(t("logout.confirm"), t("logout.ok")))) return;
  try { await post("/auth/logout"); } catch { /* yine de çık */ }
  store.set("wm.session", []);
  location.reload();
};
S.power = async action => {
  const ok = await confirmDialog(t(action === "reboot" ? "power.confirmReboot" : "power.confirmOff"), t(action === "reboot" ? "power.reboot" : "power.off"), true);
  if (!ok) return;
  try { await post("/system/power", { action }); toast(t("power.sent"), "power"); } catch (e) { toast(e.message, "circle-alert"); }
};

// Değiştirici: modül yüklenirken ⌘ bırakılırsa yüklenince hemen seçilir
let modUp = 0;
const switcher = dir => { const at = performance.now(); M.switcher().then(m => { m.switcher(dir); if (modUp > at) m.commit(); }); };
const find = () => M.find().then(m => m.find());
const launchpad = () => M.launchpad().then(m => m.launchpad());
const overview = on => M.overview().then(m => m.overview(on));

// Kısayol eylemleri
S.act = id => {
  const f = wm.focused;
  switch (id) {
    case "find": return find();
    case "switch": return switcher(1);
    case "switchBack": return switcher(-1);
    case "nextWin": return wm.cycleApp();
    case "quit": return f && wm.quitApp(f.app.id);
    case "close": return f && wm.close(f);
    case "min": return f && wm.minimize(f);
    case "hide": return f && wm.hideApp(f.app.id);
    case "prefs": return f && f.app.id !== "makeros.settings" ? f.view.send({ mk: "command", cmd: "settings" }) : S.openSettings();
    case "new": return f ? (f.app.single ? wm.focus(f) : wm.open(f.app, { force: true })) : null;
    case "overview": return overview();
    case "desktop": html.classList.add("wm-ov-anim"); wm.showDesktop(); return setTimeout(() => !wm.overviewOn && html.classList.remove("wm-ov-anim"), 400);
    case "apps": return launchpad();
    case "full": return f && wm.fullscreen(f);
    case "lock": return S.lock();
    case "nc": return toggleNC();
    case "left": case "right": return f && wm.tile(f, id);
    case "terminal": {
      const ex = wm.byApp("makeros.terminal")[0];
      if (ex) { ex.view.send({ mk: "command", cmd: "new" }); return wm.restore(ex); }
      return S.launch("makeros.terminal");
    }
    case "activity": return S.launch("makeros.activity");
    case "help": return S.openSettings("keyboard");
  }
};
keys.on("action", id => { if (!locked() && started) S.act(id); });
keys.on("modup", () => { modUp = performance.now(); if (M.switcher.m?.switching()) M.switcher.m.commit(); });
keys.on("escape", e => {
  if (locked()) return;
  if (wm.overviewOn) { e.preventDefault(); overview(false); }
  else if (overlayOpen()) { e.preventDefault(); closeOverlay(); }
  else if (wm.focused?.state === "full") wm.fullscreen(wm.focused);
});
attach(window, () => false);

// Uygulama → kabuk köprüsü
bridge(wm, {
  open: m => S.launch(m.app, { path: m.path }),
  notify: m => toast(m.title || m.body, "bell"),
  unauthorized: () => onUnauthorized(),
  shortcut: m => S.act(m.id),
  power: m => S.power(m.action),
});

// Tam ekran penceresi odaktayken menü çubuğu ve raf gizlenir
const syncFull = () => html.classList.toggle("wm-full", wm.focused?.state === "full" && !mq.matches);
wm.on("focus", syncFull); wm.on("state", syncFull);
wm.on("snap", r => {
  const s = $("#snap"); s.hidden = !r;
  if (r) { s.style.width = r.w + "px"; s.style.height = r.h + "px"; s.style.transform = `translate(${r.x}px, ${r.y}px)`; }
});
$("#desktop").addEventListener("pointerdown", () => { closePopup(); wm.blur(); if (wm.desktopOn) S.act("desktop"); });

let mobileUI = null;
const layout = () => {
  html.dataset.layout = mq.matches ? "mobile" : "desktop"; wm.mobile = mq.matches; wm.relayout(); syncFull();
  if (mq.matches && started && !mobileUI) mobileUI = M.mobile().then(m => m.mountMobile());
};
mq.addEventListener("change", layout);
let rt = 0; addEventListener("resize", () => { cancelAnimationFrame(rt); rt = requestAnimationFrame(() => wm.relayout()); });

// ------------------------------------------------------------ oturum
let started = false, relogin = null;
async function onUnauthorized() {
  if (relogin || !started || locked()) return; // kilitliyken 401 beklenir (oturum kapalı)
  relogin = lockScreen("login", S.user?.user).then(r => { relogin = null; S.user = r; S.events.stop(); S.events.start(); });
}
bus.on("unauthorized", onUnauthorized);

// İlk açılış: /session 401 + GET /setup 200 (kurulum bekliyor) ya da setup_done:false → sihirbaz
async function pendingSetup(session) {
  if (session && session.setup_done !== false) return null;
  try { return await get("/setup"); } catch { return null; } // 409/401/404 → kurulum yok, giriş ekranı
}

async function start() {
  layout();
  if (qs.get("perf") === "1") import("./perf.js").then(m => m.perfHUD());
  let session = null;
  for (let i = 0; ; i++) {
    try { session = await get("/session"); break; }
    catch (e) {
      if (e.status === 401) break;
      if (i === 0) $("#boot-msg").textContent = t("login.offline");
      await new Promise(r => setTimeout(r, Math.min(5000, 800 * (i + 1))));
    }
  }
  const setup = await pendingSetup(session);
  if (setup) {
    const { runSetup } = await M.setup();
    $("#boot").remove();
    await runSetup(setup);
    location.replace("/"); // dil ve oturum çerezi yeni sayfada geçerli
    return;
  }
  if (!session) {
    const m = await M.lock(); $("#boot").remove();
    // Kilitliyken yenilendiyse: aynı kullanıcının kilit ekranı (pencereler parola sonrası geri gelir).
    const lu = store.get("lockUser");
    session = await m.lockScreen(lu ? "lock" : "login", lu || "");
    store.set("lockUser", null);
  }
  else $("#boot").remove();
  S.user = session;
  const next = qs.get("next"); // bağımsız uygulamadan gelen yönlendirme
  if (next && /^\/shell\/apps\/[\w-]+\/[^/]*$/.test(next.split("#")[0])) { location.href = next; return; }
  // İki adres: "/" = masaüstü (doğrudan tarayıcıda çizilir; giriş ve kurulum da burada), "/web" = Web arayüzü.
  if (next === "/web" || next?.startsWith("/web#")) { location.replace(next); return; }
  const [changed] = await Promise.all([syncPrefs(), loadApps()]);
  if (changed.includes("lang")) { location.reload(); return; } // sunucudaki dil farklı → i18n yeniden yüklenir
  S.apps = APPS;

  S.events = new Events().start();
  let offline = false;
  S.events.on("close", () => { if (!offline && started) { offline = true; setTimeout(() => { if (!S.events.connected) toast(t("api.offline"), "wifi-off"); }, 3000); } });
  S.events.on("open", () => { if (offline) { offline = false; toast(t("api.back"), "check"); } });
  S.events.subscribe("system", s => { S.sys = s; S.emit("system", s); });
  S.events.subscribe("net", n => { S.net = n; S.emit("net", n); });
  // Eklenti başlatıldı/durduruldu/kuruldu/kaldırıldı: uygulama listesi, raf, menüler ve tema yenilenir;
  // artık listede olmayan eklentinin açık penceresi kapanır (arayüzü çalışmıyor).
  let appsTimer = 0;
  const refreshApps = () => { clearTimeout(appsTimer); appsTimer = setTimeout(async () => {
    const before = APPS.map(a => a.id);
    await loadApps();
    S.apps = APPS;
    for (const id of before) if (id.startsWith("makeros.plugin.") && !appById(id)) S.wm.quitApp(id);
    S.emit("apps", APPS);
  }, 600); };
  S.events.subscribe("plugins", refreshApps);
  S.events.on("open", () => { if (started) refreshApps(); }); // bağlantı kopukken kaçan değişiklikler
  get("/net").then(n => { S.net = n; S.emit("net", n); }).catch(() => {});

  mountMenubar($("#menubar"));
  mountDock($("#dock"));
  initNotifications();
  S.on("launchpad", launchpad); S.on("find", find);
  started = true;
  if (mq.matches) layout(); // mobil başlatıcıyı yükle
  else wm.restoreSession(appById); // telefonda her zaman ana ekrandan başla
  const page = qs.get("open") || pref("startupApp"); // ?open=makeros.terminal ya da otomatik başlatma uygulaması
  const openPath = qs.get("path") || "";
  if (page) {
    S.launch(page, { path: openPath }).then(w => {
      if (w && !qs.get("open") && pref("startupFullscreen") && w.state !== "full") wm.fullscreen(w);
    });
  }
  // Çoklu başlangıç uygulamaları
  const extraApps = (pref("startupApps") || []).filter(a => a && a !== page);
  for (const aId of extraApps) {
    S.launch(aId).catch(() => {});
  }
  // Otomatik başlatılan eklentiler
  const autoPlugs = pref("autostartPlugins") || [];
  if (Array.isArray(autoPlugs) && autoPlugs.length) {
    get("/plugins").then(list => {
      const runningIds = new Set((list || []).filter(p => p.running).map(p => p.id));
      for (const pid of autoPlugs) {
        if (!runningIds.has(pid)) {
          post(`/plugins/${pid}/start`).catch(() => {});
        }
      }
    }).catch(() => {});
  }
  if (!mq.matches) $("#menubar").animate?.([{ transform: "translateY(-100%)" }, { transform: "none" }], { duration: 300, easing: "cubic-bezier(.2,.8,.2,1)" });
  // Sık kullanılan katmanları boşta önceden yükle (⌘Tab/Bul ilk basışta gecikmesin)
  const idle = window.requestIdleCallback ? f => requestIdleCallback(f, { timeout: 4000 }) : f => setTimeout(f, 1500);
  idle(() => { M.switcher(); M.find(); });
}
start();
// geliştirme/hata ayıklama için
window.__mk = S;


// ------------------------------------------------------------ masaüstü widget'ları
// Sağ üstte: etkin temanın eklenti widget'ı (ör. Konya HackerSpace kapı durumu) ve yerleşik "AI Asistan"
// (yerel Türkçe Linux asistanı; Ayarlar › Görünüm'den ya da widget'ın × düğmesiyle gizlenir).
const AI_WIDGET = { url: "/shell/apps/ai-widget/", w: 340, h: 170 };
let themeKey = "", aiOn = null;
// Her widget bir kutuda: iframe + düzenleme katmanı (iframe fare olaylarını yuttuğu için sürükleme katmandan yapılır).
// Kutu anahtarı: "ai", "theme" ya da eklenti kimliği; konum tercihi widgetPos[anahtar] = [sol, üst] (px).
function widgetFrame(cls, w, key) {
  const f = document.createElement("iframe");
  Object.assign(f, { title: "widget", allowTransparency: true });
  f.className = cls;
  f.style.cssText = `width:${w.w}px;height:${w.h}px`;
  const box = h("div.wbox", { "data-key": key }, f,
    h("div.wgrip", { title: t("wid.drag") }, h("button.wx", { type: "button", "aria-label": t("wid.remove"), title: t("wid.remove"), onclick: e => { e.stopPropagation(); removeWidget(key); } }, "×")));
  box.querySelector(".wgrip").addEventListener("pointerdown", e => dragWidget(e, box));
  placeWidget(box);
  f.box = box;
  return f;
}
const boxOf = sel => $("#widgets").querySelector(sel)?.closest(".wbox");
function placeWidget(box) {
  const p = (pref("widgetPos") || {})[box.dataset.key];
  if (!p) { box.style.position = ""; box.style.left = box.style.top = ""; return; }
  box.style.position = "fixed";
  box.style.left = Math.max(0, Math.min(p[0], innerWidth - 80)) + "px";
  box.style.top = Math.max(0, Math.min(p[1], innerHeight - 60)) + "px";
}
function removeWidget(key) {
  if (key === "ai") setPref("aiWidget", false);
  else if (key === "theme") setPref("uiThemeWidget", false);
  else setPref("desktopWidgets", (pref("desktopWidgets") || []).filter(x => x !== key));
}
function dragWidget(e, box) {
  if (!document.documentElement.classList.contains("wedit") || e.button !== 0 || e.target.closest(".wx")) return;
  e.preventDefault();
  const r = box.getBoundingClientRect(), dx = e.clientX - r.left, dy = e.clientY - r.top, grip = e.currentTarget;
  Object.assign(box.style, { position: "fixed", left: r.left + "px", top: r.top + "px" });
  box.classList.add("dragging");
  grip.setPointerCapture(e.pointerId);
  const move = ev => {
    box.style.left = Math.max(0, Math.min(ev.clientX - dx, innerWidth - r.width)) + "px";
    box.style.top = Math.max(0, Math.min(ev.clientY - dy, innerHeight - 40)) + "px";
  };
  const up = () => {
    grip.removeEventListener("pointermove", move);
    box.classList.remove("dragging");
    setPref("widgetPos", { ...(pref("widgetPos") || {}), [box.dataset.key]: [parseInt(box.style.left), parseInt(box.style.top)] });
  };
  grip.addEventListener("pointermove", move);
  grip.addEventListener("pointerup", up, { once: true });
  grip.addEventListener("pointercancel", up, { once: true });
}
// Düzenleme modu: masaüstüne ya da bir widget'a basılı tutma (LONG_PRESS) ya da sağ tık → "Widget'ları düzenle";
// Bitti / Esc ile çıkılır. Widget içindeki basılı tutma plugin-kit / AI widget'ından "widget-edit" iletisiyle gelir.
export const LONG_PRESS = 1000;
let editBar = null, editAt = 0;
function widgetEdit(on) {
  if (on && mq.matches) return;
  if (on) editAt = Date.now();
  document.documentElement.classList.toggle("wedit", on);
  if (on && !editBar) {
    editBar = h("div#wedit.mat-pop", h("span", t("wid.editing")),
      h("button.btn.sm", { type: "button", onclick: () => { setPref("widgetPos", {}); } }, t("wid.reset")),
      h("button.btn.sm", { type: "button", onclick: () => S.launch("makeros.plugins") }, t("wid.add")),
      h("button.btn.sm.primary", { type: "button", onclick: () => widgetEdit(false) }, t("wid.done")));
    document.body.append(editBar);
  } else if (!on && editBar) { editBar.remove(); editBar = null; }
}
addEventListener("keydown", e => { if (e.key === "Escape" && editBar) widgetEdit(false); });
function widgetMenu(x, y) {
  if (mq.matches || Date.now() - editAt < 1500) return; // dokunmatikte basılı tutma hem düzenlemeyi hem menüyü tetikler

  const on = document.documentElement.classList.contains("wedit");
  menu([
    { label: on ? t("wid.done") : t("wid.edit"), icon: "layout-grid", action: () => widgetEdit(!on) },
    { label: t("wid.add"), icon: "plus", action: () => S.launch("makeros.plugins") },
    { label: t("wid.reset"), icon: "rotate-ccw", action: () => setPref("widgetPos", {}) },
  ], { x, y, w: 0, h: 0 });
}
$("#desktop").addEventListener("contextmenu", e => { e.preventDefault(); widgetMenu(e.clientX, e.clientY); });
// Masaüstünde basılı tutma (fare ya da dokunma): parmak/fare 10 px'ten fazla kayarsa iptal.
{
  let timer = 0, x0 = 0, y0 = 0;
  const stop = () => { clearTimeout(timer); timer = 0; };
  const d = $("#desktop");
  d.addEventListener("pointerdown", e => {
    if (e.button !== 0) return;
    x0 = e.clientX; y0 = e.clientY; stop();
    timer = setTimeout(() => { timer = 0; widgetEdit(true); }, LONG_PRESS);
  });
  d.addEventListener("pointermove", e => { if (timer && Math.hypot(e.clientX - x0, e.clientY - y0) > 10) stop(); });
  for (const ev of ["pointerup", "pointercancel", "pointerleave"]) d.addEventListener(ev, stop);
}
async function renderWidgets() {
  const host = $("#widgets");
  // AI Asistan (telefonda ve Pi'nin kendi ekranında da çalışır; masaüstü dar ise gizli).
  const ai = pref("aiWidget") !== false && !mq.matches;
  if (ai !== aiOn) {
    aiOn = ai;
    boxOf("iframe.w-ai")?.remove();
    if (ai) {
      const f = widgetFrame("w-ai", AI_WIDGET, "ai");
      f.addEventListener("load", () => f.classList.add("on"));
      f.src = AI_WIDGET.url;
      host.append(f.box);
    }
  }
  // Tema widget'ı
  const w = pref("uiThemeWidget") !== false ? themeById(pref("look")).widget : null;
  const key = w ? w.url : "";
  if (key !== themeKey) {
    themeKey = key;
    boxOf("iframe.w-theme")?.remove();
    if (w) {
      const f = widgetFrame("w-theme", w, "theme");
      host.prepend(f.box);
      widgetReady(w, id => post(`/plugins/${id}/start`)).then(() => {
        if (themeKey !== key) return;
        f.addEventListener("load", () => f.classList.add("on")); // boş iframe'in ilk "load"ı sayılmaz
        f.src = w.url;
      });
    }
  }
  // Kullanıcının eklediği eklenti widget'ları (Eklentiler → Masaüstüne ekle). Eklenti durdurulur/kaldırılırsa
  // listeden düşer (loadApps → "look" değişimi → burası); yeniden başlatılınca geri gelir.
  const mine = mq.matches ? [] : (pref("desktopWidgets") || []).map(pluginWidget).filter(x => x && x.url !== key);
  const want = new Set(mine.map(x => x.url));
  for (const f of host.querySelectorAll("iframe.w-plugin")) if (!want.has(f.dataset.url)) f.closest(".wbox").remove();
  for (const x of mine) {
    if (host.querySelector(`iframe.w-plugin[data-url="${CSS.escape(x.url)}"]`)) continue;
    const f = widgetFrame("w-plugin", x, x.plugin);
    f.dataset.url = x.url;
    f.dataset.plugin = x.plugin;
    host.insertBefore(f.box, boxOf("iframe.w-ai") || null);
    widgetReady(x, id => post(`/plugins/${id}/start`)).then(ok => {
      if (!f.isConnected) return;
      f.addEventListener("load", () => f.classList.add("on"));
      if (ok) f.src = x.url; else f.box.remove();
    });
  }
}
addEventListener("message", e => {
  if (e.origin !== location.origin || !e.data?.mk) return;
  const f = [...document.querySelectorAll("#widgets iframe")].find(x => x.contentWindow === e.source);
  if (!f) return;
  if (e.data.mk === "open" && e.data.app) S.launch(e.data.app, e.data.path ? { path: e.data.path } : {});
  else if (e.data.mk === "shortcut" && e.data.id) S.act(e.data.id);
  else if (e.data.mk === "power" && e.data.action) S.power(e.data.action);
  else if (e.data.mk === "notify") toast(e.data.title || e.data.body, "sparkles");
  else if (e.data.mk === "widget-close" && f.dataset.plugin) setPref("desktopWidgets", (pref("desktopWidgets") || []).filter(x => x !== f.dataset.plugin));
  else if (e.data.mk === "widget-size" && e.data.h > 40 && e.data.h < 800) f.style.height = e.data.h + "px"; // içeriğe göre (boşluk yok)
  else if (e.data.mk === "widget-edit") widgetEdit(true);
  else if (e.data.mk === "widget-menu") { const r = f.getBoundingClientRect(); widgetMenu(r.left + (+e.data.x || 0), r.top + (+e.data.y || 0)); }
});
prefs.on("change", k => {
  if (k === "look" || k === "aiWidget" || k === "uiThemeWidget" || k === "desktopWidgets") renderWidgets();
  if (k === "widgetPos") document.querySelectorAll("#widgets .wbox").forEach(placeWidget);
  if (k === "uiMenubar" || k === "uiDock") wm.relayout();
});
