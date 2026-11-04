// Bildirim Merkezi (sağ panel), bildirim afişleri ve kısa durum iletileri. Kontrol Merkezi: ui/cc.js (tembel).
import { h, $, animate } from "../lib.js";
import { icon } from "../icons.js";
import { t, locale } from "../i18n.js";
import { closePopup } from "./popup.js";
import { get, post, api } from "../api.js";
import { S } from "../shell.js";

// ------------------------------------------------------------ bildirimler
let list = [];
const panel = () => $("#nc");
const fmt = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" });
const unread = () => S.emit("unread", list.filter(n => !n.read).length);

function card(n, banner = false) {
  const el = h("div.nc-card.mat-pop" + (n.read ? ".read" : ""),
    h("div.nc-h", h("span.nc-app", { html: icon("bell", 12) }), h("span", appName(n.app)), h("time", fmt.format(new Date(n.time)))),
    h("b", n.title), h("p", n.body));
  el.onclick = () => { markRead(n); if (banner) el.remove(); };
  return el;
}
const appName = id => S.apps?.find(a => a.id === id)?.name || "MakerOS";
function markRead(n) { if (n.read) return; n.read = true; post(`/notifications/${encodeURIComponent(n.id)}/read`).catch(() => {}); render(); unread(); }
function clearAll() { list = []; render(); unread(); $("#toasts")?.replaceChildren(); api("DELETE", "/notifications").catch(() => {}); }
function render() {
  const p = panel(); if (!p) return;
  const items = list.map(n => card(n));
  p.replaceChildren(h("div.nc-head", h("b", t("nc.title")), list.length ? h("button.link", { onclick: clearAll }, t("nc.clear")) : ""),
    ...(items.length ? items : [h("p.nc-empty", t("nc.empty"))]));
}
export function toggleNC(force) {
  const p = panel(); const on = force ?? !p.classList.contains("open");
  closePopup();
  p.classList.toggle("open", on);
  if (on) render();
}
export async function initNotifications() {
  try { list = await get("/notifications") || []; } catch { list = []; }
  render(); unread();
  S.events.subscribe("notifications", n => { if (n.cleared) { list = []; render(); unread(); return; } list = [n, ...list.filter(x => x.id !== n.id)].slice(0, 50); render(); unread(); banner(n); });
  addEventListener("pointerdown", e => { const p = panel(); if (p.classList.contains("open") && !p.contains(e.target) && !e.target.closest(".mb-clock")) toggleNC(false); }, true);
}
export function banner(n) {
  const host = $("#toasts"), el = card(n, true);
  host.prepend(el);
  animate(el, [{ opacity: 0, transform: "translateX(40px)" }, { opacity: 1, transform: "none" }], { duration: 280 });
  setTimeout(() => animate(el, [{ opacity: 1 }, { opacity: 0, transform: "translateX(40px)" }], { duration: 220, easing: "ease-in" }).then(() => el.remove()), 5500);
}
// Kısa durum iletisi (alt orta HUD)
export function toast(text, ic = "info") {
  const el = h("div.toast.mat-pop", { html: icon(ic, 15) }, text);
  $("#hud").append(el);
  animate(el, [{ opacity: 0, transform: "translateY(8px)" }, { opacity: 1, transform: "none" }], { duration: 180 });
  setTimeout(() => animate(el, [{ opacity: 1 }, { opacity: 0 }], { duration: 200 }).then(() => el.remove()), 2600);
}
