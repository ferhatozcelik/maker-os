// Ekran Modu: tarayıcı sayfasının tamamı Pi'nin gerçek ekranı olur (yerel uygulamalar, ör. Chromium, dahil).
// Arka plan: noVNC 1.7 (vendor/novnc) → makerd /api/v1/vnc (WS↔TCP) → wayvnc (yalnız 127.0.0.1). Bu ayrıntı
// arayüzde görünmez; kullanıcı yalnız Pi'nin ekranını ve üstte kendiliğinden gizlenen küçük bir kontrol hapını görür.
// Telefonda dokunmatik + ekran klavyesi ve yatay görünüm (rotate.js).
import { app, t } from "/shell/js/app-kit.js";
import { $, h } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { wsURL, get } from "/shell/js/api.js";
import RFB from "/shell/vendor/novnc/core/rfb.js";
import { patchRFB, attach, setupRotation, rotated, isPhone } from "./rotate.js";

patchRFB(RFB);
// ?src=browser: MakerOS'taki "Chromium" penceresi. Pi'nin ekranı değil, ayrı monitörsüz tarayıcı oturumu
// (makerd /api/v1/browser/vnc): pencere = Chromium, boyutu pencereyle birlikte değişir; kontrol hapı yok.
const BROWSER = new URLSearchParams(location.search).get("src") === "browser";
document.documentElement.dataset.src = BROWSER ? "browser" : "screen";
app.init({ title: BROWSER ? "Chromium" : t("app.screen"), standaloneBar: false });
app.chrome(BROWSER ? { tone: "dark" } : { tone: "dark", title: false });

const host = $(".screen"), msg = $(".msg"), dot = $(".pill .dot"), pill = $(".pill"), menu = $(".menu"), kbd = $(".kbd");
const touch = matchMedia("(pointer: coarse)").matches;
const Q = { auto: [6, 2], high: [8, 1], low: [3, 6] }; // JPEG kalitesi + sıkıştırma (Tight): Pi 4'te CPU/bant dengesi
let quality = "auto";
try { quality = localStorage.getItem("mk.screen.q") || "auto"; } catch { /* özel pencere */ }

// Geri dönüş: geldiği MakerOS sayfasına (masaüstü "/" ya da Web arayüzü "/web").
const back = (() => {
  const q = new URLSearchParams(location.search).get("back");
  if (q === "/" || q === "/web") return q;
  try { const r = new URL(document.referrer); if (r.origin === location.origin && (r.pathname === "/" || r.pathname === "/web")) return r.pathname; } catch { /* yok */ }
  return isPhone() ? "/web" : "/";
})();
const leave = () => (app.embedded ? app.close() : location.assign(back));

const set = (b, ic, label, fn, withLabel = false) => {
  b.innerHTML = icon(ic, 15, 2) + (withLabel ? `<span class="lbl">${label}</span>` : "");
  b.title = label; b.setAttribute("aria-label", label); b.onclick = fn; return b;
};
set($(".p-back"), "chevron-left", t("scr.back"), leave, true);
set($(".p-full"), "maximize-2", t("scr.full"), () => {
  const p = document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen?.({ navigationUI: "hide" });
  p?.catch?.(() => {});
});
set($(".p-more"), "ellipsis", t("scr.more"), e => { e.stopPropagation(); toggleMenu(); });
if (touch) { const k = set($(".p-keys"), "keyboard", t("scr.kbd"), () => kbd.focus()); k.hidden = false; }
if (BROWSER) { for (const s of [".p-back", ".p-full", ".p-more", ".dot"]) $(s).hidden = true; pill.hidden = !touch; }

let rfb = null, retry = 0, timer = 0, headless = true, everConnected = false;
const rot = BROWSER ? { on: false, toggle() {} } : setupRotation({ change: on => { if (rfb) rfb.showDotCursor = touch && !on; } });
if (isPhone() && !BROWSER) {
  const b = set($(".p-rot"), "rotate-cw", t("scr.rotate"), () => b.classList.toggle("on", rot.toggle()));
  b.hidden = false; b.classList.toggle("on", rot.on);
}

// --- kontrol hapı: açılışta görünür, birkaç saniye sonra gizlenir --------------------------
let hideT = 0;
const reveal = (ms = 2600) => { pill.classList.remove("hide"); clearTimeout(hideT); hideT = setTimeout(() => { if (menu.hidden && !pill.matches(":hover")) pill.classList.add("hide"); }, ms); };
const grab = h("div.grab", { onpointerdown: e => { e.stopPropagation(); reveal(4000); }, onpointerenter: () => reveal() });
if (!BROWSER) $(".rot").append(grab);
pill.addEventListener("pointerleave", () => reveal(1200));
// Fare üst kenara yaklaşınca (döndürülmemiş görünüm).
addEventListener("pointermove", e => { if (!BROWSER && e.pointerType === "mouse" && !rotated() && e.clientY < 28) reveal(); }, { capture: true, passive: true });
if (BROWSER) pill.classList.remove("hide"); else reveal(4000);

// --- ⋯ menüsü: kalite, Ctrl+Alt+Del, yeniden bağlan -----------------------------------
function toggleMenu(force) {
  const open = force ?? menu.hidden;
  menu.hidden = !open;
  if (!open) return reveal(1500);
  clearTimeout(hideT); pill.classList.remove("hide");
  const item = (ic, label, fn, ck) => h("button", { onclick: () => { toggleMenu(false); fn(); } }, h("span", { html: icon(ic, 14, 2) }), label, ck ? h("span.ck", { html: icon("check", 14, 2.4) }) : "");
  menu.replaceChildren(
    h("div.lb", t("scr.quality")),
    ...Object.keys(Q).map(k => item("image", t("scr.q." + k), () => setQuality(k), k === quality)),
    h("hr"),
    ...(touch ? [] : [item("keyboard", "Ctrl+Alt+Del", () => rfb?.sendCtrlAltDel())]),
    item("refresh-cw", t("scr.reconnect"), () => connect()),
    item("layout-grid", t("scr.back"), leave));
}
addEventListener("pointerdown", e => { if (!menu.hidden && !e.target.closest(".menu, .p-more")) toggleMenu(false); }, true);
function setQuality(k) {
  quality = k;
  try { localStorage.setItem("mk.screen.q", k); } catch { /* yok */ }
  apply();
}
function apply() { if (!rfb) return; const [q, c] = Q[quality] || Q.auto; rfb.qualityLevel = q; rfb.compressionLevel = c; }

// --- bağlantı ----------------------------------------------------------------------------
function state(kind, text, withRetry = false) {
  dot.className = "dot " + kind; dot.title = text || "";
  $(".m-t").textContent = text || "";
  const re = $(".m-re");
  re.hidden = !withRetry;
  if (withRetry) { re.innerHTML = icon("refresh-cw", 14, 2) + t("scr.reconnect"); re.onclick = () => connect(); }
  msg.hidden = kind === "ok";
}
function connect() {
  clearTimeout(timer);
  if (rfb) { rfb._noRetry = true; rfb.disconnect(); rfb = null; }
  host.querySelectorAll(":scope > div:not(.msg)").forEach(n => n.remove());
  state("wait", t(BROWSER ? "scr.browserStarting" : "scr.connecting"));
  const r = rfb = new RFB(host, wsURL(BROWSER ? "/browser/vnc" : "/vnc"), { wsProtocols: ["binary"], shared: true });
  // Monitör yoksa Pi'nin sanal ekranı bu pencerenin boyutuna geçer (wayvnc: istemci boyutu); monitör varsa ölçeklenir.
  // Tarayıcı oturumu her zaman monitörsüzdür: Chromium pencerenin tam boyutunda çizilir.
  r.scaleViewport = true; r.resizeSession = BROWSER || headless; r.clipViewport = false; r.focusOnClick = true;
  r.showDotCursor = touch && !rotated(); r.background = "#000";
  attach(r);
  apply();
  r.addEventListener("connect", () => { retry = 0; everConnected = true; state("ok", ""); r.focus(); });
  r.addEventListener("clipboard", e => navigator.clipboard?.writeText(e.detail.text).catch(() => {}));
  r.addEventListener("disconnect", e => {
    if (r._noRetry) return;
    rfb = null;
    // Tarayıcı: kullanıcı Chromium'u kapattıysa kendiliğinden yeniden açılmaz.
    if (BROWSER && everConnected) { everConnected = false; state("err", t("scr.browserClosed"), true); $(".m-re").innerHTML = icon("refresh-cw", 14, 2) + t("scr.browserReopen"); return; }
    // Ekran oturumu henüz açılmamış olabilir (açılışta): kısa aralıklarla yeniden dene.
    if (++retry <= 5) { state("wait", t("scr.retrying")); timer = setTimeout(connect, 800 * retry); }
    else { state("err", e.detail?.clean ? t("scr.closed") : t("scr.unavailable"), true); reveal(8000); }
  });
}
// Yerel pano → Pi: sayfa odaklanınca.
addEventListener("focus", async () => { try { const s = await navigator.clipboard?.readText(); if (s && rfb) rfb.clipboardPasteFrom(s); } catch { /* izin yok */ } });
// Telefon ekran klavyesi: gizli giriş kutusundaki karakterleri tuş olarak gönder.
kbd.addEventListener("input", () => { for (const ch of kbd.value) rfb?.sendKey(ch.codePointAt(0) < 256 ? ch.codePointAt(0) : 0x01000000 + ch.codePointAt(0)); kbd.value = ""; });
kbd.addEventListener("keydown", e => {
  const map = { Backspace: [0xff08, "Backspace"], Enter: [0xff0d, "Enter"], Tab: [0xff09, "Tab"], Escape: [0xff1b, "Escape"] };
  if (map[e.key]) { e.preventDefault(); rfb?.sendKey(...map[e.key]); }
});
app.on("new", () => connect());
// Ekran Modu kapalıysa (Ayarlar) bağlanmayı denemeden açıklama gösterilir. Tarayıcı bundan bağımsızdır.
if (BROWSER) connect();
else get("/screen").then(s => {
  headless = s.headless !== false;
  if (!s.enabled) { state("err", t("scr.off")); return; }
  connect();
}).catch(() => connect());
