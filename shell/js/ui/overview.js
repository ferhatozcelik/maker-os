// Genel Bakış: pencereleri transform ölçeğiyle döşer. Tembel yüklenir.
import { h, $, animate } from "../lib.js";
import { t } from "../i18n.js";
import { closeOverlay } from "./layer.js";
import { closePopup } from "./popup.js";
import { S } from "../shell.js";

let ov = null;
export function overview(on = !S.wm.overviewOn) {
  const wm = S.wm, html = document.documentElement;
  if (on === wm.overviewOn) return;
  closeOverlay(); closePopup();
  if (on) {
    wm.overviewOn = true; html.classList.add("wm-ov", "wm-ov-anim");
    const bg = h("div.ov-bg", { onclick: () => overview(false) });
    const labels = h("div.ov-labels");
    $("#windows").prepend(bg);
    const place = () => {
      const lay = wm.layoutOverview();
      for (const w of wm.list()) delete w.ov;
      labels.replaceChildren(...lay.map(l => { l.win.ov = l; return h("div.ov-label", { style: { transform: `translate(${l.x + l.w / 2}px, ${l.y + l.h + 10}px) translateX(-50%)` } }, h("img", { src: l.win.app.icon, alt: "" }), l.win.title); }));
      if (!lay.length) labels.append(h("div.ov-empty", t("ov.empty")));
      for (const w of wm.list()) wm.render(w);
    };
    place();
    $("#overlays").append(labels);
    animate(bg, [{ opacity: 0 }, { opacity: 1 }], { duration: 300 });
    animate(labels, [{ opacity: 0 }, { opacity: 1 }], { duration: 300, delay: 120, fill: "backwards" });
    const pick = e => { const el = e.target.closest("mk-window"); if (!el) return; e.stopPropagation(); e.preventDefault(); overview(false); wm.focus(el.win); };
    $("#windows").addEventListener("click", pick, true);
    addEventListener("resize", place);
    ov = { bg, labels, pick, place };
  } else {
    wm.overviewOn = false;
    for (const w of wm.list()) { delete w.ov; wm.render(w); }
    html.classList.remove("wm-ov");
    const { bg, labels, pick, place } = ov; ov = null;
    $("#windows").removeEventListener("click", pick, true); removeEventListener("resize", place);
    labels.remove();
    animate(bg, [{ opacity: 1 }, { opacity: 0 }], { duration: 260 }).then(() => { bg.remove(); if (!wm.overviewOn) html.classList.remove("wm-ov-anim"); });
  }
}
