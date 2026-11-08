// Terminal: xterm.js (vendor/xterm, 5.5.0) + /api/v1/term PTY, sekmeler, dokunmatik ek tuş çubuğu.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { post, get, api, wsURL } from "/shell/js/api.js";

app.init({ title: "Terminal", standaloneBar: false });
app.chrome({ tone: "dark" });
const enc = new TextEncoder();
const touch = matchMedia("(pointer: coarse)").matches;
const list = $(".tablist"), host = $(".terms"), xkeys = $(".xkeys");
$(".tab-add").innerHTML = icon("plus", 15, 2);
$(".tab-add").onclick = () => newTab();
const css = getComputedStyle(document.documentElement);
const THEME = { background: css.getPropertyValue("--term-bg").trim() || "#1b1b1f", foreground: "#e6e6ea", cursor: "#ff5a6e", cursorAccent: "#1b1b1f",
  selectionBackground: "#4a4a58", black: "#2a2a2e", red: "#ff5f6d", green: "#4fd06c", yellow: "#f5c451", blue: "#5b9dff", magenta: "#c38bff", cyan: "#4fd3d6", white: "#d8d8de",
  brightBlack: "#6e6e77", brightRed: "#ff8a94", brightGreen: "#7be38f", brightYellow: "#ffd97a", brightBlue: "#8bb9ff", brightMagenta: "#d9b0ff", brightCyan: "#7fe6e8", brightWhite: "#ffffff" };

let tabs = [], cur = null, n = 0, ctrlOn = false, windowClosing = false;
const closedIds = new Set();
await document.fonts?.load("13px 'JetBrains Mono'").catch(() => {});

function newTab(existing) {
  if (windowClosing) return null;
  if (existing?.id) {
    if (closedIds.has(existing.id)) return null;
    const found = tabs.find(x => x.id === existing.id);
    if (found) { select(found); return found; }
  }
  if (!window.Terminal) { host.replaceChildren(h("div.term-msg", t("term.missing"))); return null; }
  const tab = {
    n: ++n,
    ws: null,
    id: existing?.id || null,
    ticket: existing?.ticket || null,
    el: h("div.term"),
    title: existing?.title || `${t("term.shell")} ${n > 1 ? n : ""}`.trim(),
  };
  tab.term = new Terminal({ cursorBlink: true, fontSize: touch ? 13 : 13.5, lineHeight: 1.15, scrollback: 5000, allowProposedApi: true,
    fontFamily: '"JetBrains Mono", ui-monospace, monospace', theme: THEME, macOptionIsMeta: true });
  tab.fit = new FitAddon.FitAddon(); tab.term.loadAddon(tab.fit);
  host.append(tab.el); tab.term.open(tab.el);
  // http(s) adresleri tıklanabilir: MakerOS Tarayıcı penceresinde açılır.
  tab.term.registerLinkProvider({ provideLinks(y, cb) {
    const line = tab.term.buffer.active.getLine(y - 1)?.translateToString(true) || "";
    const out = [];
    for (const m of line.matchAll(/https?:\/\/[^\s"'<>()]+[^\s"'<>().,;:]/g)) {
      out.push({ text: m[0], range: { start: { x: m.index + 1, y }, end: { x: m.index + m[0].length, y } }, activate: (_, u) => app.open("chromium", u) });
    }
    cb(out.length ? out : undefined);
  } });
  // Komut satırından gelen tarayıcı açma istekleri (OSC 777: open;<url>)
  try {
    tab.term.parser?.registerOscHandler(777, data => {
      const s = String(data || "").trim();
      if (s.startsWith("open;")) {
        const u = s.slice(5).trim();
        app.open("chromium", u);
        return true;
      }
      return false;
    });
  } catch {}
  tab.term.onTitleChange(s => { tab.title = s || tab.title; renderTabs(); });
  tab.term.onData(d => {
    if (!tab.ws || tab.ws.readyState !== 1) return;
    if (ctrlOn && d.length === 1) { const c = d.toUpperCase().charCodeAt(0); if (c >= 64 && c <= 95) d = String.fromCharCode(c - 64); setCtrl(false); }
    tab.ws.send(enc.encode(d));
  });
  tab.term.onBinary(d => tab.ws?.readyState === 1 && tab.ws.send(Uint8Array.from(d, c => c.charCodeAt(0))));
  tab.term.onResize(() => sendSize(tab));
  // ⌘C / ⌘V (Super) — Ctrl terminale aittir
  tab.term.attachCustomKeyEventHandler(e => {
    if (e.type !== "keydown" || !e.metaKey) return true;
    const k = e.key.toLowerCase();
    if (k === "c" && tab.term.hasSelection()) { navigator.clipboard?.writeText(tab.term.getSelection()); return false; }
    if (k === "v") { navigator.clipboard?.readText().then(s => tab.term.paste(s)).catch(() => {}); return false; }
    if (k === "t") { newTab(); return false; }
    if (k === "w") { closeTab(tab, true); return false; }
    if (k === "k") { tab.term.clear(); return false; }
    return true;
  });
  tabs.push(tab); select(tab); connect(tab);
  return tab;
}

function select(tab) {
  if (!tab || tab.closing) return;
  cur = tab;
  for (const x of tabs) x.el.classList.toggle("on", x === tab);
  renderTabs(); refit(); tab.term.focus();
  app.title(tab.title);
}

function closeTab(tab, killServer = true) {
  if (!tab || tab.closing) return;
  tab.closing = true;
  clearTimeout(tab.retryTimer);
  if (tab.id) {
    closedIds.add(tab.id);
    if (killServer) api("DELETE", "/term/" + encodeURIComponent(tab.id)).catch(() => {});
  }
  try { tab.ws?.close(); } catch { /* yok */ }
  try { tab.term.dispose(); } catch { /* yok */ }
  tab.el.remove();
  tabs = tabs.filter(x => x !== tab);
  if (!tabs.length) {
    windowClosing = true;
    clearInterval(pollTimer);
    app.path("");
    return app.close();
  }
  if (cur === tab) select(tabs[tabs.length - 1]); else renderTabs();
}

function renderTabs() {
  list.replaceChildren(...tabs.map(x => h("button.tab" + (x === cur ? ".on" : ""), { role: "tab", onclick: () => select(x), onauxclick: e => e.button === 1 && closeTab(x, true) },
    h("i.st" + (x.ws?.readyState === 1 ? ".ok" : "")), h("span", x.title),
    h("i.x", { html: icon("x", 11, 2.2), title: t("close"), onclick: e => { e.stopPropagation(); closeTab(x, true); } }))));
  if (cur) app.title(cur.title);
}

function sendSize(tab) { if (tab.ws?.readyState === 1) tab.ws.send(JSON.stringify({ type: "resize", cols: tab.term.cols, rows: tab.term.rows })); }

async function connect(tab) {
  if (tab.connecting || tab.closing || windowClosing) return;
  tab.connecting = true;
  try {
    tab.fit.fit();
    let r;
    if (tab.id && tab.ticket) {
      r = { id: tab.id, ticket: tab.ticket };
      tab.ticket = null;
    } else if (tab.id) {
      try {
        r = await post(`/term/${encodeURIComponent(tab.id)}/ticket`);
      } catch {
        // Oturum sunucuda kapanmış ya da başka sekmeden silinmiş: yeni oturum açma, bu sekmeyi kapat!
        closeTab(tab, false);
        return;
      }
      tab.term.reset();
    } else {
      r = await post("/term", { cols: tab.term.cols, rows: tab.term.rows });
      tab.id = r.id;
    }
    if (tab.closing || windowClosing) return;
    const ws = tab.ws = new WebSocket(wsURL(`/term/${encodeURIComponent(r.id)}?ticket=${encodeURIComponent(r.ticket)}`));
    ws.binaryType = "arraybuffer";
    ws.onopen = () => { tab.retry = 0; sendSize(tab); renderTabs(); };
    ws.onmessage = e => tab.term.write(typeof e.data === "string" ? e.data : new Uint8Array(e.data));
    ws.onclose = e => {
      renderTabs();
      if (tab.closing || windowClosing) return;
      // Süreç normal çıktı (1000) ya da sonlandırıldıysa sekmeyi kapat; asla yeni oturum uydurma!
      if (e.code === 1000) {
        closeTab(tab, false);
        return;
      }
      if (tab.id && !closedIds.has(tab.id) && (tab.retry = (tab.retry || 0) + 1) <= 2) {
        tab.retryTimer = setTimeout(() => connect(tab), 800 * tab.retry);
        return;
      }
      closeTab(tab, false);
    };
  } catch (e) {
    if (!tab.closing && !windowClosing) {
      tab.term.write(`\r\n\x1b[31m${e.message}\x1b[0m\r\n`);
    }
  } finally { tab.connecting = false; }
}

const refit = () => { if (cur) try { cur.fit.fit(); } catch { /* gizli */ } };
new ResizeObserver(refit).observe(host);
visualViewport?.addEventListener("resize", refit);

// Dokunmatik ek tuşlar
const KEYS = [["Esc", "\x1b"], ["Tab", "\t"], ["Ctrl", null], ["↑", "\x1b[A"], ["↓", "\x1b[B"], ["←", "\x1b[D"], ["→", "\x1b[C"],
  ["|", "|"], ["/", "/"], ["~", "~"], ["-", "-"], ["Home", "\x1b[H"], ["End", "\x1b[F"], ["^C", "\x03"], ["^D", "\x04"]];
const setCtrl = v => { ctrlOn = v; xkeys.querySelector("[data-k='2']")?.classList.toggle("on", v); };
if (touch) {
  xkeys.hidden = false;
  xkeys.append(...KEYS.map(([l], i) => h("button", { "data-k": i, onpointerdown: e => e.preventDefault(), onclick: () => {
    const s = KEYS[i][1];
    if (s === null) setCtrl(!ctrlOn); else if (cur?.ws?.readyState === 1) cur.ws.send(enc.encode(s));
    cur?.term.focus();
  } }, l)));
}
app.on("new", () => newTab());
app.on("settings", () => app.open("makeros.settings", "appearance"));
// Pencere başlık çubuğundaki kırmızı × düğmesiyle kapatılınca açık oturumları da temizle
app.on("closing", () => {
  windowClosing = true;
  clearInterval(pollTimer);
  for (const tab of [...tabs]) {
    tab.closing = true;
    clearTimeout(tab.retryTimer);
    if (tab.id) {
      closedIds.add(tab.id);
      api("DELETE", "/term/" + encodeURIComponent(tab.id)).catch(() => {});
    }
    try { tab.ws?.close(); } catch { /* yok */ }
  }
});
addEventListener("focus", () => cur?.term.focus());

// #s=<id> veya #<id>:<ticket> biçimlerini ayrıştırır.
function parseTarget(raw) {
  const h = String(raw ?? location.hash.slice(1)).replace(/^#/, "").trim();
  if (!h) return null;
  const sp = new URLSearchParams(h);
  if (sp.get("s")) return { id: sp.get("s"), ticket: sp.get("t") || null };
  if (h.includes(":")) {
    const [id, ticket] = h.split(":");
    return id ? { id, ticket: ticket || null } : null;
  }
  if (!h.includes("=")) return { id: h, ticket: null };
  return null;
}

function attach(target, title) {
  if (!target?.id || closedIds.has(target.id)) return;
  const ex = tabs.find(x => x.id === target.id);
  if (ex) return select(ex);
  newTab({ id: target.id, ticket: target.ticket, title });
}

let syncing = null;
async function sync(navPath) {
  if (windowClosing) return;
  const target = parseTarget(navPath);
  if (target) {
    history.replaceState(null, "", location.pathname);
    app.path("");
  }
  if (syncing) {
    await syncing;
    if (target) attach(target);
    return;
  }
  syncing = (async () => {
    let list = [];
    try { list = await get("/term"); } catch { /* çevrimdışı */ }
    if (windowClosing) return;
    for (const s of list) {
      if (!closedIds.has(s.id) && !tabs.some(x => x.id === s.id)) {
        newTab({ id: s.id, title: s.title });
      }
    }
    if (target && !closedIds.has(target.id)) {
      attach(target, list.find(s => s.id === target.id)?.title);
    }
    if (!tabs.length && !windowClosing) newTab();
  })().finally(() => { syncing = null; });
  return syncing;
}

app.on("navigate", path => sync(path));
sync();

const pollTimer = setInterval(() => {
  if (!app.visible || windowClosing || syncing) return;
  get("/term").then(list => {
    if (windowClosing) return;
    for (const s of list) {
      if (!closedIds.has(s.id) && !tabs.some(x => x.id === s.id)) {
        newTab({ id: s.id, title: s.title });
      }
    }
  }).catch(() => {});
}, 5000);
