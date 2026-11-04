// Raf (dock): sabitlenmiş + çalışan uygulamalar, çalışma göstergesi, sağ tık menüsü,
// isteğe bağlı büyütme (yalnız fx=full, transform ile).
import { h, animate } from "../lib.js";
import { t } from "../i18n.js";
import { menu, rectOf } from "./popup.js";
import { prefs, pref, setPref, fxTier } from "../prefs.js";
import { APPS, appById, DEFAULT_DOCK } from "../apps.js";
import { themeIcon } from "../themes.js";
import { S } from "../shell.js";

export function mountDock(root) {
  const inner = h("div.dock-inner.mat-bar");
  root.append(inner);
  const items = new Map(); // appId → button
  // Sabitlenenler + etkin temanın uygulamaları (en başta; tema değişince kalkar).
  const pinned = () => {
    const base = (pref("dockApps") || DEFAULT_DOCK).filter(id => appById(id));
    return [...APPS.filter(a => a.theme && !base.includes(a.id)).map(a => a.id), ...base];
  };

  const item = (app, launchpad = false) => {
    const b = h("button.di", { "aria-label": app.name, "data-app": app.id },
      h("img", { src: app.icon, alt: "", draggable: "false" }), h("span.tip", app.name), h("span.dot"));
    b.onclick = () => {
      if (launchpad) return S.emit("launchpad");
      const running = S.wm.byApp(app.id).length;
      if (!running) bounce(b);
      S.launch(app);
    };
    b.oncontextmenu = e => { e.preventDefault(); if (!launchpad) ctx(app, b); };
    let lp = 0; // dokunmatik uzun basış = sağ tık
    b.addEventListener("pointerdown", e => { if (e.pointerType === "touch") lp = setTimeout(() => ctx(app, b), 550); });
    b.addEventListener("pointerup", () => clearTimeout(lp)); b.addEventListener("pointerleave", () => clearTimeout(lp));
    return b;
  };
  const bounce = b => animate(b.querySelector("img"), [{ transform: "none" }, { transform: "translateY(-14px)" }, { transform: "none" }, { transform: "translateY(-6px)" }, { transform: "none" }], { duration: 700, easing: "ease-out" });

  function ctx(app, b) {
    const ws = S.wm.byApp(app.id), pins = pinned(), isPin = pins.includes(app.id);
    menu([
      ...ws.map(w => ({ label: w.title, checked: w === S.wm.focused, icon: w.min ? "minus" : null, action: () => S.wm.focus(w) })), ws.length ? "-" : null,
      !app.single || !ws.length ? { label: ws.length ? t("dock.newWindow") : t("dock.open"), action: () => S.launch(app, { force: !!ws.length }) } : null,
      { label: isPin ? t("dock.remove") : t("dock.keep"), action: () => { setPref("dockApps", isPin ? pins.filter(i => i !== app.id) : [...pins, app.id]); render(); } },
      ...(ws.length ? ["-", { label: t("dock.hide"), action: () => S.wm.hideApp(app.id) }, { label: t("dock.quit"), action: () => S.wm.quitApp(app.id) }] : []),
    ].filter(Boolean), { ...rectOf(b), y: rectOf(b).y - 8 }, { owner: inner, cls: "up" });
  }

  function render() {
    const pins = pinned(), running = [...new Set(S.wm.list().map(w => w.app.id))].filter(id => !pins.includes(id));
    items.clear();
    const lp = item({ id: "launchpad", name: t("dock.apps"), icon: themeIcon(pref("look"), "launchpad", "/shell/icons/launchpad.svg") }, true);
    const kids = [lp, ...pins.map(id => item(appById(id))), ...(running.length ? [h("div.dsep")] : []), ...running.map(id => item(appById(id)))];
    // Web arayüzüne geçiş (Ekran modunda izleyen tarayıcıyı /web'e geçirir; bkz. S.webMode).
    kids.push(h("div.dsep"), h("button.di.mode", { "aria-label": t("dock.web"), title: t("dock.web"), onclick: () => S.webMode() },
      h("img", { src: themeIcon(pref("look"), "web", "/shell/icons/web.svg"), alt: "", draggable: "false" }), h("span.tip", t("dock.web"))));
    // Ekran Modu (yalnız Ayarlar'dan açıksa ve uzaktan bakılıyorsa listede vardır): Pi'nin ekranı bu sayfada.
    const scr = appById("makeros.screen");
    if (scr) kids.push(h("button.di.mode", { "aria-label": scr.name, title: scr.name, onclick: () => S.launch(scr) },
      h("img", { src: themeIcon(pref("look"), "screen", "/shell/icons/screen.svg"), alt: "", draggable: "false" }), h("span.tip", scr.name)));
    for (const k of kids) if (k.dataset?.app) items.set(k.dataset.app, k);
    inner.replaceChildren(...kids);
    state();
  }
  function state() {
    for (const [id, b] of items) {
      const ws = S.wm.byApp(id);
      b.classList.toggle("run", ws.length > 0);
      b.classList.toggle("active", S.wm.focused?.app.id === id);
    }
  }
  let lastCount = -1;
  S.wm.on("change", () => { const ids = new Set(S.wm.list().map(w => w.app.id)).size; if (ids !== lastCount) { lastCount = ids; render(); } else state(); });

  // Büyütme: imleç uzaklığına göre ölçek + yatay kaydırma (yerleşim değişmez)
  let raf = 0;
  inner.addEventListener("pointermove", e => {
    if (!pref("magnify") || fxTier() !== "full" || e.pointerType === "touch") return;
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => {
      const bs = [...inner.querySelectorAll(".di")], sz = bs[0]?.offsetWidth || 52;
      const sc = bs.map(b => { const r = b.getBoundingClientRect(), d = Math.abs(e.clientX - (r.left + r.width / 2)); return 1 + .55 * Math.max(0, 1 - d / (sz * 2.4)) ** 1.6; });
      const extra = sc.map(s => (s - 1) * sz), total = extra.reduce((a, b) => a + b, 0);
      let acc = 0;
      bs.forEach((b, i) => { const off = acc + extra[i] / 2 - total / 2; acc += extra[i]; b.firstChild.style.transform = `translateX(${off}px) translateY(${-(sc[i] - 1) * sz * .5}px) scale(${sc[i]})`; });
      inner.classList.add("mag");
    });
  });
  inner.addEventListener("pointerleave", () => { cancelAnimationFrame(raf); inner.classList.remove("mag"); inner.querySelectorAll(".di img").forEach(i => i.style.transform = ""); });

  S.wm.dockRect = id => { const b = items.get(id) || inner.firstChild; return b ? rectOf(b) : null; };
  render();
  prefs.on("change", k => { if (k === "dockApps" || k === "look") setTimeout(render); }); // başka cihazdan/sunucudan gelen raf düzeni
  return { render };
}
