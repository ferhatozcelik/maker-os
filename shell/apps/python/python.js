// Python yöneticisi: projeler (şablonlu, uv sanal ortamlı), şablonlar ve Python sürümleri; proje ekranında
// kod düzenleyici, paketler (uv add/remove/upgrade), arka planda çalıştır/durdur + canlı çıktı, proje başına
// Python sürümü ve açılışta otomatik başlatma. Yalnız uv kullanılır (pip yok).
// Aynı işlemler komut satırından: mos project new|ls|run, mos python ls|install|default.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, post, put, api, BASE } from "/shell/js/api.js";
import { createEditor, EDITOR_CSS } from "./editor.js";
import { lang } from "/shell/js/i18n.js";
import { fail, dialog, confirmDlg, openTerm, toast } from "/shell/js/appui.js";

app.init({ title: t("app.python") });
app.chrome({ title: false });
const page = $(".page"), L = o => (o && (o[lang] || o.tr)) || "";
let tab = "projects", templates = [];
const TABS = [["projects", t("py.projects")], ["templates", t("py.templates")], ["versions", t("py.versions")]];
const tabs = $(".tabs");
tabs.append(...TABS.map(([id, l]) => h("button" + (id === tab ? ".on" : ""), { "data-t": id, onclick: () => show(id) }, l)));
$(".b-new").append(h("span", { html: icon("plus", 14, 2.2) }), t("py.new"));
$(".b-new").onclick = () => newProject();

async function show(id) {
  tab = id;
  tabs.querySelectorAll("button").forEach(b => b.classList.toggle("on", b.dataset.t === id));
  page.replaceChildren(h("div.empty", t("loading")));
  try { await ({ projects, templates: tplPage, versions })[id](); } catch (e) { page.replaceChildren(h("div.empty.err", e.message)); }
}
const card = (cls, ic, title, desc, ...acts) => h("div.card2" + cls, h("div.hd", h("span.ico", { html: icon(ic, 18, 2) }), h("span.ti", title)), desc ? h("div.ds", desc) : "", h("div.act", ...acts));
const btn = (l, fn, cls = "") => h("button.btn.sm" + cls, { onclick: fn }, l);

async function projects() {
  const [list] = await Promise.all([get("/projects"), loadTemplates()]);
  if (!list.length) {
    page.replaceChildren(h("div.empty2", h("span", { html: icon("file-code", 34, 1.6) }), h("p", t("py.noProjects")), btn(t("py.new"), () => newProject(), ".primary")));
    return;
  }
  page.replaceChildren(h("div.cards", ...list.map(p => {
    const tp = templates.find(x => x.id === p.template);
    const c = card(".cat-" + (tp?.category || "temel"), tp?.icon || "file-code", p.name,
      `${tp ? L(tp.title) : t("py.custom")} · ${p.python === "system" || !p.python ? t("py.system") : "Python " + p.python}${p.venv ? "" : " · " + t("py.noVenv")}`,
      btn(t("py.open"), () => openProject(p.name), ".primary"),
      btn(t("py.folder"), () => app.open("makeros.files", "~/Projeler/" + p.name)));
    c.style.cursor = "pointer";
    c.addEventListener("click", ev => { if (!ev.target.closest("button")) openProject(p.name); });
    return c;
  })), h("p.note", t("py.cli")));
}
async function loadTemplates() { if (!templates.length) templates = await get("/projects/templates"); return templates; }

async function tplPage() {
  await loadTemplates();
  const cats = [...new Set(templates.map(x => x.category))];
  page.replaceChildren(...cats.flatMap(c => [h("div.sect", t("py.cat." + c)), h("div.cards", ...templates.filter(x => x.category === c).map(tp =>
    card(".cat-" + c, tp.icon || "file-code", L(tp.title), L(tp.desc), btn(t("py.use"), () => newProject(tp.id), ".primary"))))]));
}

async function newProject(pre) {
  await loadTemplates();
  let sel = pre || templates[0].id;
  const grid = h("div.tpls", ...templates.map(tp => h("button.tpl.cat-" + tp.category + (tp.id === sel ? ".on" : ""), { "data-id": tp.id, onclick: e => {
    sel = tp.id; grid.querySelectorAll(".tpl").forEach(b => b.classList.toggle("on", b === e.currentTarget));
    if (!name.dataset.touched) name.value = suggest(tp.id); } },
    h("span.ti", h("span", { html: icon(tp.icon || "file-code", 15, 2) }), L(tp.title)), h("span.ds", L(tp.desc)))));
  const suggest = id => id.replace(/[^a-z0-9-]/g, "") + "-" + Math.floor(Math.random() * 900 + 100);
  const name = h("input", { value: suggest(sel), oninput: () => (name.dataset.touched = 1), autocapitalize: "off", spellcheck: "false" });
  const py = h("select", h("option", { value: "system" }, t("py.systemLong")));
  get("/python").then(vs => vs.filter(v => v.installed && !v.system).forEach(v => py.append(h("option", { value: v.minor }, "Python " + v.version)))).catch(() => {});
  const r = await dialog(t("py.new"), [grid, h("div.field", h("label", t("py.name")), name), h("div.field", h("label", t("py.python")), py), h("p.note", t("py.pyHint"))],
    [{ label: t("cancel"), value: null }, { label: t("py.create"), primary: true, value: "ok" }]);
  if (r !== "ok") return;
  try {
    openTerm(await post("/projects", { name: name.value.trim(), template: sel, python: py.value }));
    toast(t("py.creating"));
    setTimeout(() => show("projects"), 800);
  } catch (e) { fail(e); }
}

async function versions() {
  const vs = await get("/python");
  const op = (o, v) => post("/python/" + o, { version: v }).then(openTerm).then(() => setTimeout(() => show("versions"), 4000)).catch(fail);
  const row = v => h("div.row",
    h("div.grow", h("div.t", "Python " + v.version, " ", v.system ? h("span.badge", t("py.system")) : "", v.default ? h("span.badge.acc", t("py.default")) : ""),
      h("div.s", v.installed ? v.path : t("py.downloadable"))),
    v.installed ? [!v.system && !v.default ? btn(t("py.makeDefault"), () => op("default", v.minor)) : "", !v.system ? btn(t("py.remove"), () => op("remove", v.minor)) : ""]
      : btn(t("py.install"), () => op("install", v.minor), ".primary"));
  page.replaceChildren(h("div.sect", t("py.installed")), h("div.group", ...vs.filter(v => v.installed).map(row)),
    h("div.sect", t("py.available")), h("div.group", ...vs.filter(v => !v.installed).slice(0, 8).map(row)), h("p.note", t("py.uvHint")));
}
app.on("new", () => newProject());

// =============================================================== proje ekranı
document.head.append(Object.assign(document.createElement("style"), { textContent: EDITOR_CSS + `
body.detail .bar2, body.detail .wrap { display: none; }
.pd { display: none; flex-direction: column; height: 100vh; }
body.detail .pd { display: flex; }
.pd-hd { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: .5px solid var(--separator); flex-wrap: wrap; }
/* pencere düğmeleri (trafik ışıkları) başlık satırının solunda: üstlerine binmesin (liste görünümündeki .bar2 gibi) */
html[data-embed=window] .pd-hd { min-height: var(--tb); padding-left: 90px; }
.pd-hd .nm { font-weight: 700; font-size: 15px; }
.pd-hd .dot { width: 8px; height: 8px; border-radius: 50%; background: #9aa0a6; }
.pd-hd .dot.on { background: #22c55e; box-shadow: 0 0 0 3px rgba(34,197,94,.25); }
.pd-hd .meta { color: var(--text-2); font-size: 12px; }
.pd-tabs { padding: 6px 12px 0; }
.pd-body { flex: 1; min-height: 0; display: flex; padding: 10px 12px; gap: 10px; }
.pd-files { width: 190px; flex: none; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
.pd-files button.f { display: flex; align-items: center; gap: 6px; border: 0; background: none; text-align: left; padding: 5px 8px; border-radius: 6px; color: var(--text); font: inherit; font-size: 13px; cursor: pointer; }
.pd-files button.f:hover { background: var(--bg-hover); }
.pd-files button.f.on { background: color-mix(in srgb, var(--accent) 20%, transparent); font-weight: 600; }
.pd-files .ft { display: flex; gap: 4px; margin-top: 6px; }
.pd-code { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
.pd-code .bar { display: flex; align-items: center; gap: 6px; }
.pd-code .bar .fn { font-weight: 600; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pd-code .bar .dirty { color: var(--accent); }
.pd-out { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
.pd-out pre { flex: 1; margin: 0; overflow: auto; background: #111317; color: #ddd; padding: 10px 12px; border-radius: 10px; font: 12px/1.5 var(--font-mono, monospace); white-space: pre-wrap; user-select: text; }
.pd-scroll { flex: 1; overflow: auto; }
.pkg { display: grid; grid-template-columns: 1fr 110px auto; gap: 8px; align-items: center; padding: 6px 10px; border-top: .5px solid var(--separator); }
.pkg:first-child { border-top: 0; }
.pkg .v { color: var(--text-2); font-variant-numeric: tabular-nums; font-size: 12px; }
.term-drawer { height: 42%; min-height: 180px; border-top: .5px solid var(--separator); display: flex; flex-direction: column; }
.term-drawer .hd { display: flex; align-items: center; padding: 4px 10px; gap: 8px; font-size: 12px; color: var(--text-2); }
.term-drawer iframe { flex: 1; border: 0; width: 100%; background: #000; }
@media (max-width: 640px) { .pd-files { display: none; } }
` }));
const pd = h("div.pd");
document.body.append(pd);
let P = null; // { name, status, file, editor, dirty, logPos, logTimer, tab }

function closeProject() {
  if (P?.dirty && !confirm(t("py.unsaved"))) return;
  clearInterval(P?.logTimer); clearInterval(P?.stTimer);
  P = null; document.body.classList.remove("detail"); pd.replaceChildren(); app.path(""); show("projects");
}
// Uzun işler (paket kurulumu, ortam) alt çekmecede canlı terminal olarak görünür.
function termDrawer(r, onDone) {
  pd.querySelector(".term-drawer")?.remove();
  if (!r?.term) return;
  const fr = h("iframe", { src: `/shell/apps/terminal/#s=${encodeURIComponent(r.term.id)}`, title: "Terminal" });
  // İşlem sürerken liste/durum birkaç saniyede bir yenilenir (kurulan paket hemen görünsün).
  const tick = onDone ? setInterval(() => { if (!d.isConnected) return clearInterval(tick); onDone(); }, 3000) : 0;
  const d = h("div.term-drawer", h("div.hd", h("span", { html: icon("terminal", 13, 2) }), h("span.grow", t("py.termHint")),
    btn(t("close"), () => { clearInterval(tick); d.remove(); onDone?.(); })), fr);
  pd.append(d);
}

async function openProject(name) {
  document.body.classList.add("detail");
  P = { name, tab: "code", dirty: false, logPos: 0 };
  app.path(name);
  const dot = h("span.dot"), meta = h("span.meta"), runB = btn(t("py.run"), toggleRun, ".primary");
  // Web arayüzlü projeler (şablonda "ui": port): çalışınca arayüz MakerOS Tarayıcı penceresinde açılır (/port/<port>/ vekili).
  const uiURL = () => `/port/${P.status.ui}/`;
  const uiB = btn("", () => app.open("chromium", location.origin + uiURL()), ".primary");
  uiB.innerHTML = icon("app-window", 13, 2.2) + " " + t("py.openUi"); uiB.hidden = true;
  const tabsEl = h("div.seg", ...[["code", t("py.tab.code")], ["packages", t("py.tab.packages")], ["output", t("py.tab.output")], ["settings", t("py.tab.settings")]]
    .map(([id, l]) => h("button" + (id === P.tab ? ".on" : ""), { "data-t": id, onclick: () => setTab(id) }, l)));
  P.body = h("div.pd-body");
  pd.replaceChildren(
    h("div.pd-hd", h("button.btn.sm", { onclick: closeProject, html: icon("chevron-left", 14, 2.2) + " " + t("py.projects") }), dot, h("span.nm", name), meta, h("span.grow"),
      uiB, runB, btn(t("py.runTerm"), () => post(`/projects/${name}/run`).then(openTerm).catch(fail)), btn(t("py.folder"), () => app.open("makeros.files", "~/Projeler/" + name))),
    h("div.pd-tabs", tabsEl), P.body);
  P.refresh = async () => {
    try {
      P.status = await get(`/projects/${name}/status`);
      dot.classList.toggle("on", P.status.running);
      uiB.hidden = !(P.status.ui && P.status.running);
      runB.replaceChildren(h("span", { html: icon(P.status.running ? "square" : "play", 13, 2.2) }), P.status.running ? t("py.stop") : t("py.run"));
      meta.textContent = [P.status.running ? t("py.running") : t("py.stopped"), P.status.python && P.status.python !== "system" ? "Python " + P.status.python : t("py.system"), P.status.autostart ? t("py.autoOn") : ""].filter(Boolean).join(" · ");
    } catch (e) { fail(e); }
  };
  P.stTimer = setInterval(() => app.visible && P?.refresh(), 4000);
  await P.refresh();
  async function toggleRun() {
    if (P.dirty) await saveFile();
    try { await post(`/projects/${name}/${P.status?.running ? "stop" : "start"}`); } catch (e) { return fail(e); }
    await P.refresh();
    if (P.status.running) { setTab("output"); P.openUiWhenReady(); }
  }
  // Sunucu ayağa kalkana dek bekle (ilk çalıştırmada paket/model indirmesi sürebilir), sonra arayüzü aç.
  P.openUiWhenReady = async () => {
    if (!P.status?.ui) return;
    const me = P, relUrl = uiURL(), fullUrl = location.origin + relUrl, until = Date.now() + 5 * 60e3;
    toast(t("py.uiWait"), "app-window");
    while (P === me && P.status?.running && Date.now() < until) {
      try {
        const r = await fetch(relUrl, { cache: "no-store" });
        if (r.status < 500) { app.open("chromium", fullUrl); return; }
      } catch { /* henüz hazır değil */ }
      await new Promise(r => setTimeout(r, 1500));
      if (P === me) await P.refresh().catch(() => {});
    }
  };
  function setTab(id) {
    P.tab = id;
    tabsEl.querySelectorAll("button").forEach(b => b.classList.toggle("on", b.dataset.t === id));
    clearInterval(P.logTimer);
    ({ code: codeTab, packages: pkgTab, output: outTab, settings: setTab2 })[id]();
  }
  P.setTab = setTab;
  setTab("code");
}

// ---- Kod
const FILE_OK = n => !n.startsWith(".") && !/^(__pycache__|\.venv)$/.test(n) && !/\.(pyc|so|bin|param|jpg|png|mp4|onnx|tflite)$/i.test(n);
async function saveFile() {
  if (!P?.file) return;
  try {
    const r = await fetch(`${BASE}/files/raw?path=${encodeURIComponent(P.file)}`, { method: "PUT", headers: { "X-Maker": "1" }, body: P.editor.value });
    if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error?.message || r.statusText);
    P.dirty = false; P.markDirty?.(); toast(t("py.saved"));
  } catch (e) { fail(e); }
}
async function codeTab() {
  const name = P.name, dir = `~/Projeler/${name}`;
  const list = h("div.pd-files"), fname = h("span.fn"), dirtyEl = h("span.dirty");
  P.markDirty = () => { dirtyEl.textContent = P.dirty ? "●" : ""; };
  P.editor = createEditor({ onSave: saveFile, onChange: () => { if (!P.dirty) { P.dirty = true; P.markDirty(); } } });
  const saveB = btn(t("save"), saveFile), runB = btn(t("py.saveRun"), async () => { await saveFile(); if (P.status?.running) await post(`/projects/${name}/stop`).catch(() => {}); await post(`/projects/${name}/start`).catch(fail); await P.refresh(); P.setTab("output"); if (P.status?.running) P.openUiWhenReady(); }, ".primary");
  P.body.replaceChildren(list, h("div.pd-code", h("div.bar", fname, dirtyEl, saveB, runB), P.editor.el));
  async function openFile(rel) {
    if (P.dirty && !confirm(t("py.unsaved"))) return;
    try {
      const txt = await (await fetch(`${BASE}/files/raw?path=${encodeURIComponent(dir + "/" + rel)}`)).text();
      P.file = dir + "/" + rel; P.dirty = false; P.markDirty();
      P.editor.set(txt, rel); fname.textContent = rel;
      list.querySelectorAll("button.f").forEach(b => b.classList.toggle("on", b.dataset.f === rel));
    } catch (e) { fail(e); }
  }
  async function refresh(open) {
    let files = [];
    try {
      const l = await get("/files?path=" + encodeURIComponent(dir));
      files = l.entries.filter(e => !e.dir && FILE_OK(e.name)).map(e => e.name);
      // Bir düzey alt klasör (ör. static/, templates/)
      for (const d of l.entries.filter(e => e.dir && FILE_OK(e.name)).slice(0, 6)) {
        const s = await get("/files?path=" + encodeURIComponent(dir + "/" + d.name)).catch(() => null);
        s?.entries.filter(e => !e.dir && FILE_OK(e.name)).slice(0, 30).forEach(e => files.push(d.name + "/" + e.name));
      }
    } catch (e) { fail(e); }
    files.sort((a, b) => (b === "main.py") - (a === "main.py") || a.localeCompare(b));
    list.replaceChildren(...files.map(f => h("button.f", { "data-f": f, onclick: () => openFile(f) }, h("span", { html: icon(/\.py$/.test(f) ? "file-code" : "file-text", 14, 1.8) }), f)),
      h("div.ft", btn(t("py.newFile"), async () => {
        const inp = h("input", { value: "yeni.py" });
        if (await dialog(t("py.newFile"), inp, [{ label: t("cancel"), value: null }, { label: t("f.create"), primary: true, value: "ok" }]) !== "ok") return;
        const n = inp.value.trim().replace(/^\/+/, "");
        if (!n || n.includes("..")) return;
        await fetch(`${BASE}/files/raw?path=${encodeURIComponent(dir + "/" + n)}`, { method: "PUT", headers: { "X-Maker": "1" }, body: "" });
        refresh(n);
      })));
    if (open || files.length) openFile(open || (files.includes("main.py") ? "main.py" : files[0]));
  }
  refresh();
}

// ---- Paketler (yalnız uv)
async function pkgTab() {
  const name = P.name;
  const box = h("div.pd-scroll", h("div.empty", t("loading")));
  const inp = h("input", { placeholder: t("py.pkgPh"), autocapitalize: "off", spellcheck: "false", style: { flex: 1 } });
  const op = (o, pkg = "") => post(`/projects/${name}/packages`, { op: o, pkg }).then(r => termDrawer(r, load)).catch(fail);
  const add = () => { const v = inp.value.trim(); if (v) { op("add", v); inp.value = ""; } };
  inp.onkeydown = e => { if (e.key === "Enter") add(); };
  P.body.replaceChildren(h("div.pd-out", h("div", { style: { display: "flex", gap: "8px" } }, inp, btn(t("py.pkgAdd"), add, ".primary"), btn(t("py.pkgUpAll"), () => op("upgrade")), btn(t("py.pkgSync"), () => op("sync"))), box));
  async function load() {
    try {
      const r = await get(`/projects/${name}/packages`);
      const direct = r.deps.map(d => ({ spec: d, name: d.split(/[<>=!~\[ ;]/)[0].toLowerCase() }));
      const inst = new Map(r.installed.map(p => [p.name.toLowerCase().replace(/_/g, "-"), p.version]));
      const others = r.installed.filter(p => !direct.some(d => d.name === p.name.toLowerCase().replace(/_/g, "-")));
      box.replaceChildren(
        h("div.sect", t("py.pkgDirect")),
        direct.length ? h("div.group", ...direct.map(d => h("div.pkg", h("span", d.spec), h("span.v", inst.get(d.name.replace(/_/g, "-")) || t("py.pkgNotInstalled")),
          h("span", { style: { display: "flex", gap: "6px" } }, btn(t("py.pkgUpdate"), () => op("upgrade", d.name)), btn(t("py.remove"), () => op("remove", d.name)))))) : h("p.note", t("py.pkgNone")),
        h("div.sect", t("py.pkgDeps", { n: others.length })),
        h("div.group", ...others.map(p => h("div.pkg", h("span", p.name), h("span.v", p.version), h("span")))),
        h("p.note", t("py.pkgHint")));
    } catch (e) { box.replaceChildren(h("div.empty.err", e.message)); }
  }
  load();
}

// ---- Çıktı (arka planda çalışan projenin günlüğü, canlı)
function outTab() {
  const name = P.name, pre = h("pre");
  const clearB = btn(t("py.clear"), () => { pre.textContent = ""; });
  P.body.replaceChildren(h("div.pd-out", h("div", { style: { display: "flex", gap: "8px", alignItems: "center" } }, h("span.grow.meta", t("py.outHint")), clearB), pre));
  P.logPos = 0;
  const poll = async () => {
    if (!app.visible) return;
    try {
      const r = await get(`/projects/${name}/log?from=${P.logPos}`);
      if (r.reset) pre.textContent = "";
      if (r.text) {
        const atEnd = pre.scrollTop + pre.clientHeight >= pre.scrollHeight - 30;
        pre.textContent += r.text;
        if (pre.textContent.length > 400000) pre.textContent = pre.textContent.slice(-300000);
        if (atEnd) pre.scrollTop = pre.scrollHeight;
      }
      P.logPos = r.from + new TextEncoder().encode(r.text).length;
      if (!r.text && r.size > P.logPos) P.logPos = r.size;
    } catch { /* geçici */ }
  };
  poll();
  P.logTimer = setInterval(poll, 1000);
}

// ---- Ayarlar
async function setTab2() {
  const name = P.name;
  await P.refresh();
  const st = P.status;
  const py = h("select", h("option", { value: "system" }, t("py.systemLong")));
  try { (await get("/python")).filter(v => v.installed && !v.system).forEach(v => py.append(h("option", { value: v.minor }, "Python " + v.version))); } catch { /* yok */ }
  py.value = st.python && st.python !== "system" ? st.python : "system";
  const auto = h("input", { type: "checkbox", checked: st.autostart, onchange: async () => { try { await put(`/projects/${name}/autostart`, { enabled: auto.checked }); toast(auto.checked ? t("py.autoOnToast") : t("py.autoOffToast")); P.refresh(); } catch (e) { fail(e); auto.checked = !auto.checked; } } });
  P.body.replaceChildren(h("div.pd-scroll",
    h("div.sect", t("py.envTitle")),
    h("div.group", h("div.row", h("div.grow", h("div.t", t("py.python")), h("div.s", t("py.envHint"))), py,
      btn(t("py.apply"), () => put(`/projects/${name}/python`, { python: py.value }).then(r => termDrawer(r, () => P.refresh())).catch(fail), ".primary"))),
    h("div.sect", t("py.autoTitle")),
    h("div.group", h("label.row", h("div.grow", h("div.t", t("py.autoLabel")), h("div.s", t("py.autoHint"))), auto)),
    h("div.sect", t("py.danger")),
    h("div.group", h("div.row", h("div.grow", h("div.t", t("py.delete")), h("div.s", t("py.deleteQ", { name }))),
      btn(t("delete"), async () => {
        if (!(await confirmDlg(t("py.delete"), t("py.deleteQ", { name })))) return;
        await post(`/projects/${name}/stop`).catch(() => {});
        await put(`/projects/${name}/autostart`, { enabled: false }).catch(() => {});
        await api("DELETE", "/projects/" + name).catch(fail);
        P.dirty = false; closeProject();
      }, ".danger")))));
}

app.on("navigate", p => { if (p && !["projects", "templates", "versions"].includes(p)) openProject(p); });
const start = location.hash.slice(1);
if (start && !["projects", "templates", "versions"].includes(start)) { show("projects"); openProject(decodeURIComponent(start)); }
else show(start || "projects");
