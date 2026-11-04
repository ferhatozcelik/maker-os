// Açılır menü ve popover. Aynı anda tek açılır öğe; dış tıklama / Esc kapatır.
import { h, animate } from "../lib.js";
import { icon } from "../icons.js";

let current = null;
export const closePopup = () => { const c = current; current = null; c?.close(); };
export const popupOpen = () => !!current;
addEventListener("pointerdown", e => { if (current && !current.el.contains(e.target) && !current.owner?.contains(e.target)) closePopup(); }, true);
addEventListener("blur", () => setTimeout(() => { if (document.activeElement?.tagName === "IFRAME") closePopup(); }));

function place(el, at, align = "left") {
  const layer = document.getElementById("popups") || document.body;
  layer.append(el);
  const w = el.offsetWidth, hh = el.offsetHeight;
  let x = align === "right" ? at.x + (at.w || 0) - w : at.x, y = at.y + (at.h || 0) + 4;
  x = Math.max(6, Math.min(x, innerWidth - w - 6));
  if (y + hh > innerHeight - 6) y = Math.max(6, at.y - hh - 4);
  el.style.transform = `translate(${x}px, ${y}px)`;
  return { x, y };
}

// items: [{label, kbd, action, disabled, checked, icon}, "-" (ayraç)]
export function menu(items, at, { owner, align, onClose, cls = "" } = {}) {
  closePopup();
  const el = h("div.menu.mat-pop" + (cls ? "." + cls : ""), { role: "menu" });
  const rows = [];
  for (const it of items) {
    if (it === "-") { el.append(h("div.sep")); continue; }
    if (it.header) { el.append(h("div.mh", it.header)); continue; }
    const b = h("button.mi", { role: "menuitem", disabled: it.disabled, tabindex: "-1" },
      h("span.ck", { html: it.checked ? icon("check", 12, 2.4) : it.icon ? icon(it.icon, 13) : "" }),
      h("span.ml", it.label), it.kbd ? h("kbd", it.kbd) : "");
    b.onclick = () => { if (it.disabled) return; closePopup(); it.action?.(); };
    b.onpointerenter = () => b.focus({ preventScroll: true });
    rows.push(b); el.append(b);
  }
  el.addEventListener("keydown", e => {
    const i = rows.indexOf(document.activeElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); const en = rows.filter(r => !r.disabled); const k = en.indexOf(rows[i]); en[(k + (e.key === "ArrowDown" ? 1 : -1) + en.length) % en.length]?.focus(); }
    else if (e.key === "Escape") { e.preventDefault(); closePopup(); }
    else if (e.key === "Enter" && i >= 0) { e.preventDefault(); rows[i].click(); }
  });
  place(el, at, align);
  animate(el, [{ opacity: 0 }, { opacity: 1 }], { duration: 120 });
  current = { el, owner, close() { el.remove(); onClose?.(); } };
  el.tabIndex = -1; el.focus({ preventScroll: true });
  return current;
}

export function popover(content, at, { owner, align = "right", onClose, cls = "" } = {}) {
  closePopup();
  const el = h("div.popover.mat-pop" + (cls ? "." + cls : ""), content);
  place(el, at, align);
  const o = el.style.transform;
  animate(el, [{ opacity: 0, transform: o + " translateY(-6px)" }, { opacity: 1, transform: o }], { duration: 160 });
  current = { el, owner, close() { el.remove(); onClose?.(); } };
  return current;
}
export const rectOf = el => { const r = el.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; };
