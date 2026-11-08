// Tarayıcı: MakerOS web masaüstünde açılan yerleşik tarayıcı penceresi.
// Tüm sayfalar ve aramalar host PC tarayıcısına çıkmadan doğrudan MakerOS Tarayıcı penceresi içinde açılır.
import { app, t } from "/shell/js/app-kit.js";
import { $, h } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { post } from "/shell/js/api.js";
import { BOOKMARKS } from "/shell/js/bookmarks.js";

app.init({ title: t("app.browser"), standaloneBar: false });
app.chrome({ title: false });

const SEARCH = q => "https://www.google.com/search?igu=1&hl=tr&q=" + encodeURIComponent(q);
const EMBED = new URLSearchParams(location.search).get("embed") === "1";
const KIOSK = /^(localhost|127(\.\d{1,3}){3}|\[::1\])$/i.test(location.hostname);

async function openExt(u) {
  if (!u) return;
  if (!KIOSK) { window.open(u, "_blank", "noopener"); return; }
  try { await post("/apps/chromium/launch", { url: u }); showHint(t("br.inChromium")); }
  catch (e) { showHint(t("br.failed") + ": " + e.message); }
}

const page = $(".page"), home = $(".home"), addr = $(".addr"), hint = $(".hint");
if (EMBED) { $(".tb").hidden = true; $(".bm").hidden = true; }

const back = $(".b-back"), fwd = $(".b-fwd"), reloadBtn = $(".b-reload"), homeBtn = $(".b-home"), extBtn = $(".b-ext");
const addrIcon = $(".addr-icon"), progress = $(".progress-bar");
const loadingOverlay = $(".loading-overlay"), loadingSpinner = $(".loading-spinner"), loadingText = $(".loading-text"), loadingHost = $(".loading-host");

loadingSpinner.innerHTML = icon("loader-circle", 28, 2.2);
loadingText.textContent = t("br.loading") || "Yükleniyor…";
loadingOverlay.onclick = () => {
  loadingOverlay.style.opacity = "0";
  setTimeout(() => { loadingOverlay.hidden = true; }, 200);
};

const btn = (el, ic, label, fn) => {
  el.innerHTML = icon(ic, 16, 2);
  el.title = label;
  el.setAttribute("aria-label", label);
  el.onclick = fn;
};

let frame = null, cur = "", hist = [""], pos = 0, tk = null;
let loading = false, progressTimer = null, loadTimeout = null, overlayTimer = null;

function updateNav() {
  back.disabled = pos <= 0;
  fwd.disabled = pos >= hist.length - 1;
}

btn(back, "chevron-left", (t("br.back") || "Geri") + " (Alt+←)", () => {
  if (pos > 0) go(hist[--pos], false);
});
btn(fwd, "chevron-right", (t("br.forward") || "İleri") + " (Alt+→)", () => {
  if (pos < hist.length - 1) go(hist[++pos], false);
});
btn(reloadBtn, "rotate-cw", (t("br.reload") || "Yenile") + " (Ctrl+R)", () => cur && load(cur));
btn(homeBtn, "house", t("br.home") || "Başlangıç", () => go("", true));
if (extBtn) btn(extBtn, "external-link", t("br.newTab") || "Yeni sekmede aç", () => openExt(cur));

addr.placeholder = t("br.placeholder") || "Adres yazın ya da arayın";

// Başlangıç: Google araması + maker sık kullanılanları.
const q = home.querySelector(".q");
q.placeholder = t("br.placeholder") || "Adres yazın ya da arayın";
home.querySelector(".go").textContent = t("br.searchGoogle") || "Ara";
home.querySelector("p").textContent = "";
home.querySelector(".hs").onsubmit = e => { e.preventDefault(); submit(q.value); };
home.querySelector(".groups").append(...BOOKMARKS.map(g => h("div.grp", h("h3", g.name),
  ...g.items.map(b => h("button", { onclick: () => go(b.url, true), title: b.url }, h("span.fav", { html: icon("globe", 14, 1.8) }), b.name)))));

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

hint.querySelector("span").textContent = t("br.blocked") || "Sayfa düzgün görünmüyor mu?";
hint.querySelector(".h-open").textContent = t("br.openTab") || "Yeni sekmede aç";
hint.querySelector(".h-open").onclick = () => openExt(cur);
hint.querySelector(".x").innerHTML = icon("x", 12, 2.4);
hint.querySelector(".x").onclick = () => { hint.hidden = true; };

function updateAddrIcon(u) {
  if (loading) {
    addrIcon.className = "addr-icon spin";
    addrIcon.innerHTML = icon("loader-circle", 15, 2.2);
    addrIcon.title = t("br.loading") || "Yükleniyor…";
    return;
  }
  addrIcon.className = "addr-icon";
  if (!u) {
    addrIcon.innerHTML = icon("search", 14, 2);
    addrIcon.title = "";
  } else if (/^https:/i.test(u)) {
    addrIcon.className = "addr-icon secure";
    addrIcon.innerHTML = icon("lock", 13, 2);
    addrIcon.title = "HTTPS";
  } else {
    addrIcon.innerHTML = icon("globe", 14, 1.8);
    addrIcon.title = "HTTP";
  }
}

function startLoading(targetUrl) {
  loading = true;
  clearTimeout(loadTimeout);
  clearTimeout(overlayTimer);
  clearInterval(progressTimer);

  // Reload butonu durdurma butonuna dönüşür
  btn(reloadBtn, "x", (t("br.stop") || "Durdur") + " (Esc)", stopLoading);

  // Adres simgesi döner
  updateAddrIcon(targetUrl);

  // İlerleme çubuğu
  progress.className = "progress-bar active";
  progress.style.width = "20%";
  let pct = 20;
  progressTimer = setInterval(() => {
    if (pct < 88) {
      pct += (88 - pct) * 0.12 + Math.random() * 2;
      progress.style.width = Math.min(pct, 90) + "%";
    }
  }, 180);

  // Sayfa yükleme örtüsü (sayfa akışını tıkamamak için en geç 2.4 sn sonra yumuşakça kalkar)
  if (targetUrl) {
    loadingHost.textContent = hostOf(targetUrl) || targetUrl;
    loadingOverlay.hidden = false;
    loadingOverlay.style.opacity = "1";
    overlayTimer = setTimeout(() => {
      loadingOverlay.style.opacity = "0";
      setTimeout(() => { if (!loading) loadingOverlay.hidden = true; }, 250);
    }, 2400);
  }

  // 15 sn içinde yanıt gelmezse yükleme durumunu kapat
  loadTimeout = setTimeout(() => {
    finishLoading();
  }, 15000);
}

function finishLoading() {
  if (!loading) return;
  loading = false;
  clearInterval(progressTimer);
  clearTimeout(loadTimeout);
  clearTimeout(overlayTimer);

  progress.style.width = "100%";
  setTimeout(() => {
    progress.classList.add("done");
    setTimeout(() => {
      progress.className = "progress-bar";
      progress.style.width = "0%";
    }, 300);
  }, 160);

  btn(reloadBtn, "rotate-cw", (t("br.reload") || "Yenile") + " (Ctrl+R)", () => cur && load(cur));
  updateAddrIcon(cur);

  loadingOverlay.style.opacity = "0";
  setTimeout(() => {
    loadingOverlay.hidden = true;
  }, 200);
}

function stopLoading() {
  if (!loading) return;
  if (frame) {
    try { frame.contentWindow.stop(); } catch {}
  }
  finishLoading();
}

const walled = new Set();
const hostOf = u => { try { return new URL(u).hostname.replace(/^www\./, ""); } catch { return ""; } };

async function ticket() {
  if (!tk || Date.now() - tk.at > 3600e3) {
    const r = await post("/browse/ticket"); tk = { v: r.ticket, at: Date.now() };
    (r.walled || []).forEach(w => walled.add(String(w).replace(/^www\./, "")));
  }
  return tk.v;
}
ticket().catch(() => {});

const LOOP = /^(localhost|0\.0\.0\.0|127(\.\d{1,3}){3}|\[::1?\])$/i;
function piLocal(u) {
  try {
    const x = new URL(u, location.origin);
    if (!/^https?:$/.test(x.protocol)) return null;
    if (x.pathname.startsWith("/port/")) return x;
    const isLoop = LOOP.test(x.hostname);
    const isSameHost = x.hostname === location.hostname;
    if (!isLoop && !isSameHost) return null;
    // Yerel bir port varsa /port/<port>/ vekil yoluna çevir (Cloudflare Tunnel ve uzak bağlantılarda çalışabilmesi için)
    const p = x.port;
    if (p && p !== "80" && p !== "443" && p !== (location.port || (location.protocol === "https:" ? "443" : "80"))) {
      return new URL(`/port/${p}${x.pathname || "/"}${x.search}${x.hash}`, location.origin);
    }
    if (isLoop) {
      return new URL(`${x.pathname || "/"}${x.search}${x.hash}`, location.origin);
    }
    return x;
  } catch { return null; }
}
const proxied = (v, u) => `/api/v1/browse?t=${encodeURIComponent(v)}&url=${encodeURIComponent(u)}`;
const showHint = msg => { hint.querySelector("span").textContent = msg || t("br.failed"); hint.hidden = false; };

function toURL(s) {
  s = s.trim();
  if (!s) return "";
  if (s.startsWith("/port/")) return new URL(s, location.origin).href;
  if (/^https?:\/\//i.test(s)) return s;
  if (/^(localhost|0\.0\.0\.0|\d{1,3}(\.\d{1,3}){3}|[\w-]+\.local)(:\d+)?(\/.*)?$/i.test(s)) return "http://" + s;
  if (/^(localhost|[\w-]+(\.[\w-]+)+|\d{1,3}(\.\d{1,3}){3})(:\d+)?(\/.*)?$/i.test(s) && !/\s/.test(s)) return "https://" + s;
  return SEARCH(s);
}

let retried = false;
async function load(u) {
  hint.hidden = true;
  startLoading(u);

  const local = piLocal(u);
  if (local) {
    frame?.remove();
    frame = h("iframe", {
      onload: () => finishLoading(),
      onerror: () => { finishLoading(); showHint(t("br.failed")); },
      src: local.href,
      allow: "fullscreen; clipboard-write; autoplay; camera; microphone",
      sandbox: "allow-scripts allow-forms allow-same-origin allow-popups allow-popups-to-escape-sandbox allow-modals allow-downloads"
    });
    page.append(frame);
    return;
  }

  if (/^https:\/\/(www\.)?google\.com\/search\b/i.test(u)) {
    frame?.remove();
    frame = h("iframe", {
      onload: () => finishLoading(),
      onerror: () => { finishLoading(); showHint(t("br.failed")); },
      src: u,
      allow: "fullscreen; clipboard-write; autoplay",
      sandbox: "allow-same-origin allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-modals allow-downloads"
    });
    page.append(frame);
    return;
  }

  let v;
  try { v = await ticket(); } catch (e) { finishLoading(); showHint(e.message); return; }
  if (u !== cur) return;

  frame?.remove();
  frame = h("iframe", {
    onload: () => finishLoading(),
    onerror: () => { finishLoading(); showHint(t("br.failed")); },
    src: proxied(v, u),
    allow: "fullscreen; clipboard-write; autoplay",
    sandbox: "allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-modals allow-downloads"
  });
  page.append(frame);
}

addEventListener("message", e => {
  if (!frame || e.source !== frame.contentWindow || !e.data?.mkBrowse) return;
  finishLoading();
  const { url, title, error, expired, open, walled: wall } = e.data;
  if (typeof url !== "string" || !/^https?:/i.test(url)) return;
  if (open) { openExt(url); return; }
  if (wall) { const x = hostOf(url); if (x) walled.add(x); if (KIOSK) { openExt(url); return; } }
  if (expired) { if (!retried) { retried = true; tk = null; load(cur); } else showHint(t("br.failed")); return; }
  retried = false;
  if (url !== cur) {
    if (url !== hist[pos]) { hist = hist.slice(0, pos + 1); hist.push(url); pos = hist.length - 1; }
    cur = url; addr.value = url; updateNav(); updateAddrIcon(url); app.path(url);
  }
  if (typeof title === "string" && title.trim()) {
    app.title(title.trim().slice(0, 80));
    if (/just a moment|security verification|performing security/i.test(title)) {
      showHint("Bot doğrulamasını tamamlamak için sayfayı sekmede açın");
    }
  }
  if (error) showHint(t("br.failed") + ": " + error);
});

function go(u, push) {
  u = u || "";
  const local = u && piLocal(u);
  if (local) u = local.href;
  if (push && u !== hist[pos]) { hist = hist.slice(0, pos + 1); hist.push(u); pos = hist.length - 1; }
  cur = u; addr.value = u;
  updateNav();
  updateAddrIcon(u);
  home.hidden = !!u;
  if (!u) {
    stopLoading();
    frame?.remove();
    frame = null;
    hint.hidden = true;
    app.title(t("app.browser"));
    return;
  }
  try {
    const x = new URL(u);
    if (x.pathname.startsWith("/port/")) {
      const p = x.pathname.split("/")[2] || "";
      app.title("localhost:" + p);
    } else {
      app.title(x.hostname.replace(/^www\./, "") + (piLocal(u) && x.port ? ":" + x.port : ""));
    }
  } catch {}
  load(u);
  app.path(u);
}

function submit(v) {
  v = (v || "").trim();
  if (!v) return;
  go(toURL(v), true);
}

$(".tb form").onsubmit = e => { e.preventDefault(); addr.blur(); submit(addr.value); };
addr.addEventListener("focus", () => addr.select());
app.on("navigate", p => submit(p));
app.on("new", () => go("", true));

addEventListener("keydown", e => {
  if ((e.altKey && e.key === "ArrowLeft") || e.key === "BrowserBack") {
    if (pos > 0) { e.preventDefault(); go(hist[--pos], false); }
  } else if ((e.altKey && e.key === "ArrowRight") || e.key === "BrowserForward") {
    if (pos < hist.length - 1) { e.preventDefault(); go(hist[++pos], false); }
  } else if ((e.key === "r" && (e.ctrlKey || e.metaKey)) || e.key === "F5") {
    e.preventDefault();
    if (cur) load(cur);
  } else if (e.key === "Escape" && loading) {
    e.preventDefault();
    stopLoading();
  }
});

const start = decodeURIComponent(location.hash.slice(1));
if (start) submit(start); else go("", false);
if (!start) q.focus();
