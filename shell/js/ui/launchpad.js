// Uygulamalar ızgarası (tam ekran başlatıcı). Tembel yüklenir.
import { h, animate } from "../lib.js";
import { t } from "../i18n.js";
import { APPS } from "../apps.js";
import { show, closeOverlay, overlayOpen } from "./layer.js";
import { score } from "./find.js";
import { S } from "../shell.js";

export function launchpad() {
  if (overlayOpen("launchpad")) return closeOverlay();
  const input = h("input.lp-search", { placeholder: t("launch.search"), type: "search", autocomplete: "off", spellcheck: "false" });
  const grid = h("div.lp-grid");
  const render = () => {
    const q = input.value, list = APPS.map(a => [score(q, a.keys), a]).filter(([s]) => s > 0).sort((a, b) => q ? b[0] - a[0] : 0).map(x => x[1]);
    grid.replaceChildren(...list.map(a => h("button.lp-app", { onclick: () => { closeOverlay(); S.launch(a); } }, h("img", { src: a.icon, alt: "" }), h("span", a.name))));
  };
  input.oninput = render;
  input.onkeydown = e => { if (e.key === "Enter") grid.querySelector("button")?.click(); };
  const el = show("launchpad", h("div.lp", { onclick: e => { if (e.target === e.currentTarget) closeOverlay(); } }, input, grid), null, ".lp-layer");
  animate(el.firstChild, [{ transform: "scale(1.06)" }, { transform: "none" }], { duration: 260 });
  render(); input.focus();
}

