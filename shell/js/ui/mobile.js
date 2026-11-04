// Dar ekran (<700px): başlatıcı ızgarası + tam ekran uygulamalar + alt ana çubuk. Yalnız mobil düzende tembel yüklenir.
import { h, $, animate } from "../lib.js";
import { icon, wifiIcon } from "../icons.js";
import { t, locale } from "../i18n.js";
import { APPS } from "../apps.js";
import { S } from "../shell.js";

export function mountMobile() {
  const home = $("#home"), bar = $("#homebar"), status = $("#mstatus");
  const clock = h("b"), right = h("span.ms-r");
  status.replaceChildren(clock, right);
  const tick = () => { clock.textContent = new Date().toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" }); };
  tick(); setInterval(tick, 10000);
  const stat = () => {
    const w = S.net?.ifaces?.find(i => i.type === "wifi" && i.state === "connected"), c = S.sys?.cpu?.temp_c;
    right.innerHTML = (c != null ? `<span>${Math.round(c)}°</span>` : "") + icon(wifiIcon(w?.signal, !!w), 15, 2.2);
  };
  S.on("net", stat); S.on("system", stat);

  const running = h("div.m-running");
  const render = () => {
    const open = [...new Set(S.wm.list().map(w => w.app))];
    running.replaceChildren(...(open.length ? [h("div.m-label", t("m.running")), h("div.m-chips", ...open.map(a =>
      h("span.m-chip", h("button", { onclick: () => S.wm.activateApp(a) }, h("img", { src: a.icon, alt: "" }), a.name),
        h("button.m-x", { "aria-label": t("close"), html: icon("x", 12, 2.4), onclick: () => S.wm.quitApp(a.id) }))))] : []));
    home.replaceChildren(
      h("button.m-search", { onclick: () => S.emit("find") }, h("span", { html: icon("search", 16) }), t("launch.search")),
      h("div.m-grid", ...APPS.map(a => h("button.m-app", { onclick: () => S.launch(a) }, h("img", { src: a.icon, alt: "" }), h("span", a.name)))),
      running);
  };
  render();
  S.wm.on("change", () => { if (!S.wm.focused) render(); });
  S.wm.on("focus", w => {
    document.documentElement.classList.toggle("m-app-open", !!w);
    if (!w) { render(); animate(home, [{ opacity: 0, transform: "scale(.96)" }, { opacity: 1, transform: "none" }], { duration: 240 }); }
  });
  // Ana çubuk: dokun → ana ekran; yukarı kaydır → ana ekran
  let sy = 0;
  bar.onpointerdown = e => { sy = e.clientY; };
  bar.onpointerup = e => { if (sy - e.clientY > -10) goHome(); };
  return { render };
}
export function goHome() { const f = S.wm.focused; if (f) S.wm.minimize(f); }
