// Uygulama değiştirici (⌘Tab). Tembel yüklenir; main.js ilk boşta önceden yükler.
import { h } from "../lib.js";
import { show, closeOverlay } from "./layer.js";
import { S } from "../shell.js";

let sw = null;
export function switcher(dir = 1) {
  const apps = S.wm.appsMRU();
  if (!apps.length) return;
  if (!sw) {
    const row = h("div.sw-row"), name = h("div.sw-name");
    const el = show("switch", h("div.sw-wrap", h("div.sw.mat-hud", row, name)), () => { clearTimeout(sw?.timer); sw = null; });
    sw = { apps, idx: 0, row, name, el };
    row.replaceChildren(...apps.map((a, i) => h("button.sw-app", { onclick: () => { sw.idx = i; commit(); } }, h("img", { src: a.icon, alt: "" }))));
  }
  sw.idx = (sw.idx + dir + sw.apps.length) % sw.apps.length;
  sw.row.querySelectorAll(".sw-app").forEach((b, i) => b.classList.toggle("sel", i === sw.idx));
  sw.name.textContent = sw.apps[sw.idx].name;
  clearTimeout(sw.timer); sw.timer = setTimeout(commit, 4000); // keyup kaybolursa (uzak tarayıcı)
}
export function commit() {
  if (!sw) return;
  const a = sw.apps[sw.idx]; closeOverlay(); S.wm.activateApp(a);
}
export const switching = () => !!sw;

