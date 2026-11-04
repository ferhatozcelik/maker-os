// Üst menü çubuğu: logo menüsü, etkin uygulama menüleri, durum öğeleri, saat.
import { h, $, lazy } from "../lib.js";
import { icon, wifiIcon } from "../icons.js";
import { t, locale } from "../i18n.js";
import { menu, rectOf, popupOpen, closePopup } from "./popup.js";
import { macLabel, shortcutFor } from "../keys.js";
import { S } from "../shell.js";
import { toggleNC } from "./control.js";
import { watchThemeStatus } from "../status.js";

const cc = lazy(() => import("./cc.js"));

const K = id => macLabel(shortcutFor(id));
// Marka işareti: CSS maskesi (--brand-mark), böylece temaya özel işaret (ör. Konya HackerSpace) metin rengini alır.
export const MARK = `<span class="ic brand-mark" style="width:15px;height:15px"></span>`;

function editCmd(cmd) {
  const w = S.wm.focused; if (!w) return;
  try {
    const d = w.view.frame.contentDocument; w.view.focusContent();
    if (cmd === "paste") navigator.clipboard?.readText().then(s => d.execCommand("insertText", false, s)).catch(() => {});
    else d.execCommand(cmd);
  } catch { /* yok */ }
}

function menus() {
  const wm = S.wm, f = wm.focused, app = f?.app, name = app?.name || t("menu.desktop");
  const user = S.user?.user || "";
  return {
    logo: [
      { label: t("menu.about"), action: () => S.openSettings("general") }, "-",
      { label: t("menu.settings"), kbd: "", action: () => S.openSettings() },
      { label: t("menu.apps"), kbd: K("apps"), action: () => S.emit("launchpad") }, "-",
      { label: t("menu.lock"), kbd: K("lock"), action: () => S.lock() },
      { label: t("menu.logout", { user }), action: () => S.logout() }, "-",
      { label: t("menu.restart"), action: () => S.power("reboot") },
      { label: t("menu.shutdown"), action: () => S.power("poweroff") },
    ],
    app: app ? [
      { label: t("menu.aboutApp", { app: name }), action: () => S.about(app) }, "-",
      { label: t("menu.appSettings"), kbd: K("prefs"), action: () => S.act("prefs") }, "-",
      { label: t("menu.hide", { app: name }), kbd: K("hide"), action: () => S.act("hide") },
      { label: t("menu.quit", { app: name }), kbd: K("quit"), action: () => S.act("quit") },
    ] : [
      { label: t("menu.about"), action: () => S.openSettings("general") }, "-",
      { label: t("menu.settings"), action: () => S.openSettings() },
    ],
    file: [
      { label: t("menu.newWindow"), kbd: K("new"), action: () => S.act("new"), disabled: !app },
      { label: t("menu.closeWindow"), kbd: K("close"), action: () => S.act("close"), disabled: !f },
    ],
    edit: [
      { label: t("menu.undo"), kbd: "⌘Z", action: () => editCmd("undo"), disabled: !f },
      { label: t("menu.redo"), kbd: "⇧⌘Z", action: () => editCmd("redo"), disabled: !f }, "-",
      { label: t("menu.cut"), kbd: "⌘X", action: () => editCmd("cut"), disabled: !f },
      { label: t("menu.copy"), kbd: "⌘C", action: () => editCmd("copy"), disabled: !f },
      { label: t("menu.paste"), kbd: "⌘V", action: () => editCmd("paste"), disabled: !f },
      { label: t("menu.selectAll"), kbd: "⌘A", action: () => editCmd("selectAll"), disabled: !f },
    ],
    view: [
      { label: f?.state === "full" ? t("menu.exitFullscreen") : t("menu.fullscreen"), kbd: K("full"), action: () => S.act("full"), disabled: !f }, "-",
      { label: t("menu.overview"), kbd: "F3", action: () => S.act("overview") },
      { label: t("menu.showDesktop"), kbd: K("desktop"), action: () => S.act("desktop") },
      { label: t("menu.apps"), kbd: "F4", action: () => S.emit("launchpad") },
    ],
    window: [
      { label: t("menu.minimize"), kbd: K("min"), action: () => S.act("min"), disabled: !f },
      { label: t("menu.zoom"), action: () => wm.zoom(f), disabled: !f },
      { label: t("menu.tileLeft"), kbd: K("left"), action: () => S.act("left"), disabled: !f },
      { label: t("menu.tileRight"), kbd: K("right"), action: () => S.act("right"), disabled: !f }, "-",
      { label: t("menu.bringAll"), action: () => wm.list().filter(w => w.app === app).forEach(w => wm.restore(w)), disabled: !app }, "-",
      ...(wm.list().length ? wm.mru().map(w => ({ label: w.title, checked: w === f, icon: w.min ? "minus" : null, action: () => wm.focus(w) }))
        : [{ label: t("menu.noWindows"), disabled: true }]),
    ],
    help: [
      { label: t("menu.shortcuts"), kbd: K("help"), action: () => S.openSettings("keyboard") },
      { label: t("menu.helpAbout"), action: () => S.openSettings("general") },
    ],
  };
}

export function mountMenubar(root) {
  let openKey = null;
  const btn = (key, label, cls = "") => {
    const b = h("button.mb-item" + cls, { "data-k": key }, label);
    if (key === "logo") b.innerHTML = MARK;
    b.addEventListener("pointerdown", e => { e.preventDefault(); openKey === key ? closePopup() : open(key, b); });
    b.addEventListener("pointerenter", () => { if (openKey && openKey !== key && popupOpen()) open(key, b); });
    return b;
  };
  const open = (key, b) => {
    const items = menus()[key];
    openKey = key; root.querySelectorAll(".mb-item").forEach(x => x.classList.toggle("open", x === b));
    menu(items, rectOf(b), { owner: root, onClose: () => { openKey = null; b.classList.remove("open"); } });
  };
  const appName = btn("app", "", ".mb-app");
  const left = h("div.mb-left", btn("logo", "", ".mb-logo"), appName,
    btn("file", t("menu.file")), btn("edit", t("menu.edit")), btn("view", t("menu.view")), btn("window", t("menu.window")), btn("help", t("menu.help")));
  const temp = h("button.mb-st.mb-temp", { title: t("status.temp"), onclick: () => S.act("activity") });
  // Tema durumu (eklenti): ör. Konya HackerSpace kapısı açık/kapalı. Tıklayınca eklentinin sayfası açılır.
  const door = h("button.mb-st.mb-door", { hidden: true });
  watchThemeStatus(s => {
    door.hidden = !s;
    if (!s) return;
    const txt = s.state === "open" ? t("door.open") : s.state === "closed" ? t("door.closed") : t("door.unknown");
    door.className = "mb-st mb-door " + s.state;
    door.replaceChildren(h("i.dd"), h("span", `${s.label} · ${txt}`));
    door.title = s.since ? t("door.since", { when: new Date(s.since).toLocaleString(locale, { weekday: "short", hour: "2-digit", minute: "2-digit" }) }) : txt;
    door.onclick = () => S.launch(s.app || "makeros.plugin." + s.plugin);
  });
  const cpuBar = h("i"), cpu = h("span.mb-cpu", cpuBar);
  const wifi = h("button.mb-st", { title: t("status.wifi"), onclick: () => S.openSettings("wifi") });
  const find = h("button.mb-st", { title: t("k.find"), html: icon("search", 15), onclick: () => S.emit("find") });
  const ccBtn = h("button.mb-st", { title: t("status.cc"), html: icon("sliders-horizontal", 15) });
  ccBtn.onclick = () => cc().then(m => m.toggleControl(ccBtn, root));
  const clock = h("button.mb-st.mb-clock", { title: t("status.nc"), onclick: () => toggleNC() });
  const badge = h("span.mb-badge"), ctext = h("span");
  clock.append(ctext, badge);
  root.append(left, h("div.mb-right", door, temp, wifi, find, ccBtn, clock));

  const update = () => {
    const f = S.wm.focused;
    appName.textContent = f ? f.app.name : t("menu.desktop");
  };
  S.wm.on("focus", update); S.wm.on("change", update); update();

  const fmtD = new Intl.DateTimeFormat(locale, { weekday: "short", day: "numeric", month: "short" });
  const fmtT = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" });
  const tick = () => { const d = new Date(); ctext.textContent = `${fmtD.format(d)}  ${fmtT.format(d)}`; };
  tick(); setInterval(tick, 10000);

  S.on("system", s => {
    // CPU: makerd cpu.usage_pct (M2); yoksa 1 dk yük / çekirdek tahmini
    const c = s?.cpu?.temp_c, u = s?.cpu ? s.cpu.usage_pct ?? Math.min(100, 100 * (s.cpu.load?.[0] || 0) / (s.cpu.cores || 1)) : null;
    temp.hidden = c == null && u == null;
    if (temp.hidden) return;
    if (!temp.firstChild) temp.append(cpu, h("span.mb-tc"));
    cpuBar.style.transform = `scaleY(${(u ?? 0) / 100})`;
    temp.lastChild.textContent = c != null ? `${Math.round(c)}°` : "";
    temp.classList.toggle("hot", c >= 70);
    temp.title = [u != null && `CPU %${Math.round(u)}`, c != null && `${t("status.temp")} ${c.toFixed(1)} °C`].filter(Boolean).join(" · ");
  });
  S.on("net", n => {
    const ifs = n?.ifaces || [], w = ifs.find(i => i.type === "wifi" && i.state === "connected"), e = ifs.find(i => i.type === "ethernet" && i.state === "connected");
    const radio = n?.wifi_enabled !== false;
    wifi.innerHTML = icon(e && !w ? "ethernet-port" : wifiIcon(w?.signal, !!w && radio), 16);
    wifi.title = w ? `${t("status.wifi")}: ${w.ssid}` : e ? t("status.ethernet") : n?.online === false ? t("status.offline") : t("status.wifiOff");
  });
  S.on("unread", n => { badge.hidden = !n; });
  S.emit("net", S.net); badge.hidden = true;
  return { el: root };
}
export const menubarHeight = () => $("#menubar")?.offsetHeight || 28;
