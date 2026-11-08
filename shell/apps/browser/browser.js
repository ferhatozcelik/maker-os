// Tarayıcı: MakerOS web masaüstünde (uzaktan) "Chromium"un yerine açılan tarayıcı penceresi.
// Sayfa, bağlanan bilgisayarın kendi tarayıcısında bir iframe içinde çizilir; görüntü aktarımı (VNC) yoktur.
// Çoğu site iframe'de gösterilmeyi reddettiği için HTML makerd üzerinden gelir (GET /api/v1/browse, bkz.
// makerd/src/browse.rs): engel başlıkları atılır, sayfa opak kökende (sandbox) çalışır. Giriş gerektiren ya da
// JS ağırlıklı siteler eksik çalışabilir: "Yeni sekmede aç" asıl siteyi açar. Google araması pencerede çalışmaz
// (JS + çerez ister): başlangıç sayfasındaki ve adres çubuğundaki aramalar Google'ı yeni sekmede açar.
// Pi'de çalışan uygulamalar (http://0.0.0.0:8000, localhost:5000…) aktarılmaz: adres Pi'nin adına çevrilir
// (bağlanan bilgisayar için "localhost" kendisidir) ve iframe'de doğrudan açılır.
import { app, t } from "/shell/js/app-kit.js";
import { $, h } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { post } from "/shell/js/api.js";
import { BOOKMARKS } from "/shell/js/bookmarks.js";

app.init({ title: t("app.browser"), standaloneBar: false });

const GOOGLE = q => "https://www.google.com/search?q=" + encodeURIComponent(q);
const favicon = u => { try { return `https://www.google.com/s2/favicons?sz=32&domain=${new URL(u).hostname}`; } catch { return ""; } };

const page = $(".page"), home = $(".home"), addr = $(".addr"), hint = $(".hint");
const back = $(".b-back"), fwd = $(".b-fwd");
const btn = (el, ic, label, fn) => { el.innerHTML = icon(ic, 16, 2); el.title = label; el.setAttribute("aria-label", label); el.onclick = fn; };
btn(back, "chevron-left", t("br.back"), () => go(hist[--pos], false));
btn(fwd, "chevron-right", t("br.forward"), () => go(hist[++pos], false));
btn($(".b-reload"), "rotate-cw", t("br.reload"), () => cur && load(cur));
btn($(".b-home"), "house", t("br.home"), () => go("", true));
btn($(".b-ext"), "external-link", t("br.newTab"), () => openTab(cur || addr.value));
addr.placeholder = t("br.placeholder");
// Başlangıç: Google araması (yeni sekme) + maker sık kullanılanları (pencerede).
const q = home.querySelector(".q");
q.placeholder = t("br.searchGoogle");
home.querySelector(".go").textContent = t("br.googleBtn");
home.querySelector("p").textContent = t("br.about");
home.querySelector(".hs").onsubmit = e => { e.preventDefault(); submit(q.value); };
home.querySelector(".groups").append(...BOOKMARKS.map(g => h("div.grp", h("h3", g.name),
  ...g.items.map(b => h("button", { onclick: () => go(b.url, true), title: b.url }, h("img", { src: favicon(b.url), alt: "", loading: "lazy", referrerpolicy: "no-referrer" }), b.name)))));
// Sık kullanılanlar çubuğu: klasör → açılır menü.
let menu = null;
const closeMenu = () => { menu?.remove(); menu = null; document.querySelectorAll(".bm button.on").forEach(b => b.classList.remove("on")); };
$(".bm").append(...BOOKMARKS.map(g => {
  const b = h("button", { html: icon("folder", 13, 2) }, g.name);
  b.onclick = e => {
    e.stopPropagation();
    const was = b.classList.contains("on"); closeMenu(); if (was) return;
    const r = b.getBoundingClientRect();
    b.classList.add("on");
    menu = h("div.bm-menu", { style: { left: Math.min(r.left, innerWidth - 240) + "px", top: r.bottom + 4 + "px" } },
      ...g.items.map(i => h("button", { onclick: () => { closeMenu(); go(i.url, true); } }, i.name)));
    document.body.append(menu);
  };
  return b;
}));
addEventListener("click", closeMenu);
addEventListener("blur", closeMenu);
hint.querySelector("span").textContent = t("br.blocked");
hint.querySelector(".h-open").textContent = t("br.newTab");
hint.querySelector(".h-open").onclick = () => openTab(cur);
hint.querySelector(".x").innerHTML = icon("x", 12, 2.4);
hint.querySelector(".x").onclick = () => { hint.hidden = true; };

let frame = null, cur = "", hist = [""], pos = 0, tk = null, walled = [];
// Aktarım bileti (makerd, oturuma bağlı; 12 sa) + bot korumalı siteler. Saatte bir yenilenir.
async function ticket() {
  if (!tk || Date.now() - tk.at > 3600e3) { const r = await post("/browse/ticket"); tk = { v: r.ticket, at: Date.now() }; walled = r.walled || []; }
  return tk.v;
}
ticket().catch(() => {}); // liste tıklamadan önce hazır olsun
// Bot korumalı (Cloudflare) siteler aktarılamaz: tıklamayla aynı anda yeni sekmede açılır.
const isWalled = u => { try { const h = new URL(u).hostname; return walled.some(d => h === d || h.endsWith("." + d)); } catch { return false; } };
// Pi'nin kendi adresi mi? Öyleyse Pi'ye bağlanılan ada (location.hostname) çevrilmiş URL, değilse null.
const LOOP = /^(localhost|0\.0\.0\.0|127(\.\d{1,3}){3}|\[::1?\])$/i;
function piLocal(u) {
  try {
    const x = new URL(u);
    if (!/^https?:$/.test(x.protocol) || !(LOOP.test(x.hostname) || x.hostname === location.hostname)) return null;
    x.hostname = location.hostname;
    return x;
  } catch { return null; }
}
const proxied = (v, u) => `/api/v1/browse?t=${encodeURIComponent(v)}&url=${encodeURIComponent(u)}`;
const showHint = msg => { hint.querySelector("span").textContent = msg || t("br.blocked"); hint.hidden = false; };

// Yazılanı adrese çevir: tam adres, alan adı (örn. "wikipedia.org") ya da arama.
function toURL(s) {
  s = s.trim();
  if (!s) return "";
  if (/^https?:\/\//i.test(s)) return s;
  // Yerel adresler (localhost, 0.0.0.0, IP, *.local) çoğunlukla https değildir.
  if (/^(localhost|\d{1,3}(\.\d{1,3}){3}|[\w-]+\.local)(:\d+)?(\/.*)?$/i.test(s)) return "http://" + s;
  if (/^(localhost|[\w-]+(\.[\w-]+)+|\d{1,3}(\.\d{1,3}){3})(:\d+)?(\/.*)?$/i.test(s) && !/\s/.test(s)) return "https://" + s;
  return GOOGLE(s);
}
function openTab(u) { if (u) window.open(piLocal(u)?.href || u, "_blank", "noopener"); }

let retried = false;
async function load(u) {
  hint.hidden = true;
  const local = piLocal(u);
  if (local) {
    frame?.remove();
    // Farklı port = farklı köken: uygulama kendi çerezini/depolamasını kullanabilsin. MakerOS'un kendisiyse (aynı köken) verilmez.
    const own = local.origin === location.origin;
    frame = h("iframe", { src: local.href, allow: "fullscreen; clipboard-write; autoplay; camera; microphone",
      sandbox: "allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-modals allow-downloads" + (own ? "" : " allow-same-origin") });
    page.append(frame);
    return;
  }
  let v;
  try { v = await ticket(); } catch (e) { showHint(e.message); return; }
  if (u !== cur) return; // bu arada başka adrese geçildi
  frame?.remove();
  // allow-same-origin YOK: aktarılan sayfa opak kökende çalışır, MakerOS'a erişemez (makerd de CSP sandbox gönderir).
  frame = h("iframe", { src: proxied(v, u), allow: "fullscreen; clipboard-write; autoplay",
    sandbox: "allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-modals allow-downloads" });
  page.append(frame);
}

// Aktarılan sayfa her açılışta adresini/başlığını bildirir (sayfa içi bağlantılar da aktarımdan geçer).
addEventListener("message", e => {
  if (!frame || e.source !== frame.contentWindow || !e.data?.mkBrowse) return;
  const { url, title, error, expired } = e.data;
  if (typeof url !== "string" || !/^https?:/i.test(url)) return;
  // makerd yeniden başladı: bilet eskidi. Bir kez yeni biletle dene.
  if (expired) { if (!retried) { retried = true; tk = null; load(cur); } else showHint(t("br.failed")); return; }
  retried = false;
  if (url !== cur) {
    if (url !== hist[pos]) { hist = hist.slice(0, pos + 1); hist.push(url); pos = hist.length - 1; }
    cur = url; addr.value = url; back.disabled = pos <= 0; fwd.disabled = pos >= hist.length - 1; app.path(url);
  }
  if (typeof title === "string" && title.trim()) app.title(title.trim().slice(0, 80));
  if (error) {
    try { const d = new URL(url).hostname.replace(/^www\./, ""); if (/bot/i.test(error) && !walled.includes(d)) walled.push(d); } catch { /* yok */ }
    showHint(t("br.failed") + ": " + error);
  }
});

function go(u, push) {
  u = u || "";
  if (u && isWalled(u)) { openTab(u); return; }
  const local = u && piLocal(u);
  if (local) u = local.href;
  if (push && u !== hist[pos]) { hist = hist.slice(0, pos + 1); hist.push(u); pos = hist.length - 1; }
  cur = u; addr.value = u;
  back.disabled = pos <= 0; fwd.disabled = pos >= hist.length - 1;
  home.hidden = !!u;
  if (!u) { frame?.remove(); frame = null; hint.hidden = true; app.title(t("app.browser")); return; }
  try { const x = new URL(u); app.title(x.hostname.replace(/^www\./, "") + (piLocal(u) && x.port ? ":" + x.port : "")); } catch { /* geçersiz adres */ }
  load(u);
  app.path(u);
}

// Adres → pencerede; arama sözcükleri → Google (yeni sekme; pencerede çalışmaz).
function submit(v) {
  v = (v || "").trim();
  if (!v) return;
  const u = toURL(v);
  if (u.startsWith(GOOGLE(""))) openTab(u); else go(u, true);
}
$(".tb form").onsubmit = e => { e.preventDefault(); addr.blur(); submit(addr.value); };
addr.addEventListener("focus", () => addr.select());
app.on("navigate", p => submit(p));
app.on("new", () => go("", true));

const start = decodeURIComponent(location.hash.slice(1));
if (start) submit(start); else go("", false);
if (!start) q.focus();
