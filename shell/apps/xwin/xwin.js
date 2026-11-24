// Tek bir yerel (X11) pencerenin <mk-window> içeriği. Pencere kimliği: ?id=<x pencere>
import { XView } from "/shell/js/xwin.js";

const id = new URLSearchParams(location.search).get("id");
const canvas = document.querySelector("canvas");
const view = new XView(canvas, id, {
  onGone: () => { document.body.classList.add("closed"); document.querySelector(".gone").textContent = "Pencere kapandı"; },
  onError: m => { document.body.classList.add("closed"); document.querySelector(".gone").textContent = m || "Bağlanılamadı"; },
});

// İçerik alanının boyutu (kabuktaki pencere boyutlandırılınca) → programın penceresi.
let t = 0, last = "";
new ResizeObserver(() => {
  clearTimeout(t);
  t = setTimeout(() => {
    const w = innerWidth, h = innerHeight, k = w + "x" + h;
    if (k !== last && w > 8 && h > 8) { last = k; view.resize(w, h); }
  }, 90);
}).observe(document.documentElement);

// Kabuktan: görünürlük (küçültülmüş/gizli pencerede çizim durur) ve odak.
addEventListener("message", e => {
  if (e.origin !== location.origin) return;
  if (e.data?.mk === "visibility") view.visible(!!e.data.visible);
});
addEventListener("focus", () => view.focus());
view.focus();
