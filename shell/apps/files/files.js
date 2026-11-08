// Dosyalar: masaüstü dosya yöneticisi (Finder / Dosya Gezgini gibi). Tüm işlemler kullanıcının kendi
// hesabıyla makerd-user üzerinden yapılır (POST /files/op: list, copy, move, trash, restore, search, places…).
//   Kenar çubuğu: Ev, Masaüstü, Belgeler, İndirilenler, Resimler, Videolar, Projeler, USB sürücüler, Çöp Kutusu
//   Görünüm: ızgara (küçük resimli) / liste (sıralanabilir); çoklu seçim (⌘/Ctrl, Shift), sağ tık menüsü
//   Kısayollar: ⌘C/⌘X/⌘V, ⌘A, ⌫/Delete (çöpe), F2 (yeniden adlandır), Boşluk (hızlı bakış), Enter (aç), ⌘↑ (üst)
//   Sürükle-bırak: öğeleri klasöre/kenar çubuğuna (taşı; Alt ile kopyala), bilgisayardan dosya (yükle)
import { app, t } from "/shell/js/app-kit.js";
import { h, $, bytes, store } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, api, post, BASE } from "/shell/js/api.js";
import { toast, fail, dialog, confirmDlg, openTerm } from "/shell/js/appui.js";

app.init({ title: t("app.files") });
app.chrome({ title: false });

const area = $(".area"), side = $(".side"), crumbs = $(".crumbs"), q = $(".q");
const op = (o, path = "", to = "") => api("POST", "/files/op", { op: o, path, to }, { timeout: 120000 });
const raw = (p, dl) => `${BASE}/files/raw?path=${encodeURIComponent(p)}${dl ? "&download=1" : ""}`;
const join = (a, b) => (a.endsWith("/") ? a : a + "/") + b;
const parent = p => p.replace(/\/[^/]+\/?$/, "") || "/";
const base = p => p.replace(/\/+$/, "").split("/").pop();
const mac = /Mac|iPhone|iPad/.test(navigator.platform);
const MOD = mac ? "⌘" : "Ctrl+";

const EXT = {
  img: /\.(png|jpe?g|gif|webp|bmp|svg|avif)$/i, vid: /\.(mp4|webm|mov|mkv|m4v|ogv)$/i, aud: /\.(mp3|wav|ogg|m4a|flac|opus)$/i,
  pdf: /\.pdf$/i, code: /\.(py|js|mjs|ts|sh|c|cpp|h|hpp|rs|go|java|json|toml|ya?ml|ini|cfg|conf|html|css|xml|csv|log|md|txt|ino)$/i,
  arc: /\.(zip|tar|gz|tgz|xz|bz2|7z|rar|whl)$/i,
};
const kind = e => e.dir ? "dir" : EXT.img.test(e.name) ? "img" : EXT.vid.test(e.name) ? "vid" : EXT.aud.test(e.name) ? "aud" : EXT.pdf.test(e.name) ? "pdf" : EXT.code.test(e.name) ? "code" : EXT.arc.test(e.name) ? "arc" : "file";
const ICON = { dir: "folder", img: "image", vid: "file-video", aud: "music", pdf: "file-text", code: "file-code", arc: "file-archive", file: "file" };
const kicon = (e, s = 18) => h("span", { class: `ic ${kind(e)}`, html: icon(ICON[kind(e)], s, 1.7) });

// ------------------------------------------------------------ durum
let home = "", cur = "", data = { entries: [] }, places = [], trash = null;
let view = store.get("files.view") || "grid", sortBy = store.get("files.sort") || "name", sortDesc = false;
let showHidden = !!store.get("files.hidden"), query = "";
let sel = new Set(), anchor = null, hist = [], hpos = -1;
// Pano: aynı tarayıcıdaki tüm Dosyalar pencereleri paylaşır (localStorage).
const clip = { get: () => store.get("files.clip"), set: v => store.set("files.clip", v) };
const TRASH = "trash:";
const inTrash = () => cur === TRASH;

const rel = p => (home && (p === home || p.startsWith(home + "/")) ? "~" + p.slice(home.length) : p);

// ------------------------------------------------------------ yükleme / gezinme
async function load(p, { push = true, keepSel = false } = {}) {
  try {
    if (p === TRASH) {
      trash = await op("trash-list");
      data = { path: TRASH, entries: trash.map(x => ({ name: x.name, id: x.id, dir: x.dir, size: x.size, mtime: x.deleted, origin: x.origin })) };
      cur = TRASH;
    } else if (query) {
      data = await op("search", p, query);
      home = data.home; cur = data.path;
    } else {
      data = await get("/files?path=" + encodeURIComponent(p || "~"));
      home = data.home; cur = data.path;
    }
    if (!keepSel) { sel.clear(); anchor = null; }
    if (push && hist[hpos] !== cur) { hist = hist.slice(0, hpos + 1); hist.push(cur); hpos = hist.length - 1; }
    render();
    if (!inTrash()) app.path(rel(cur));
  } catch (e) { fail(e); if (p !== "~" && !hist.length) load("~"); }
}
const reload = () => load(cur, { push: false, keepSel: true });

function sorted() {
  let list = data.entries.filter(e => showHidden || !base(e.name).startsWith("."));
  const dir = sortDesc ? -1 : 1;
  const cmp = {
    name: (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }),
    size: (a, b) => (a.size || 0) - (b.size || 0),
    date: (a, b) => String(a.mtime).localeCompare(String(b.mtime)),
  }[sortBy];
  return list.sort((a, b) => (b.dir - a.dir) || cmp(a, b) * dir);
}
let shown = [];

// ------------------------------------------------------------ çizim
function renderSide() {
  const label = { home: t("fm.home"), desktop: t("fm.desktop"), documents: t("fm.documents"), downloads: t("fm.downloads"), pictures: t("fm.pictures"), videos: t("fm.videos"), projects: t("fm.projects") };
  const pic = { home: "house", desktop: "monitor", documents: "file-text", downloads: "download", pictures: "image", videos: "file-video", projects: "code" };
  const btn = (p, ic, name, extra = {}) => {
    const b = h("button" + (cur === p || (p !== TRASH && cur === p) ? ".on" : ""), { title: p === TRASH ? name : rel(p), onclick: () => { query = ""; q.value = ""; document.body.classList.remove("side-open"); load(p); }, ...extra },
      h("span", { class: "ic", html: icon(ic, 16, 1.9) }), h("span", name));
    if (p !== TRASH) dropTarget(b, () => p); else dropTarget(b, () => TRASH);
    return b;
  };
  const fav = places.filter(p => !p.drive), drives = places.filter(p => p.drive);
  side.replaceChildren(
    h("h4", t("fm.places")), ...fav.map(p => btn(p.path, pic[p.id] || "folder", label[p.id] || base(p.path))),
    ...(drives.length ? [h("h4", t("fm.drives")), ...drives.map(p => btn(p.path, "usb", p.id))] : []),
    h("h4", t("fm.other")), btn("/", "hard-drive", t("fm.system")), btn(TRASH, "trash-2", t("fm.trash")));
}

function renderCrumbs() {
  if (inTrash()) { crumbs.replaceChildren(h("button", t("fm.trash"))); return; }
  const r = rel(cur), parts = r.split("/").filter(Boolean);
  const out = [];
  let acc = r.startsWith("~") ? home : "";
  parts.forEach((s, i) => {
    if (i === 0 && s === "~") { acc = home; } else acc = join(acc || "/", s);
    const target = acc;
    if (i) out.push(h("span.sep", { html: icon("chevron-right", 12, 2) }));
    const b = h("button", { onclick: () => { query = ""; q.value = ""; load(target); } }, i === 0 && s === "~" ? t("fm.home") : s);
    dropTarget(b, () => target);
    out.push(b);
  });
  if (!out.length) out.push(h("button", { onclick: () => load("/") }, "/"));
  crumbs.replaceChildren(...out);
  crumbs.scrollLeft = crumbs.scrollWidth; // yer yoksa son klasör (bulunulan yer) görünsün
}

function render() {
  renderSide(); renderCrumbs();
  $(".b-back").disabled = hpos <= 0; $(".b-fwd").disabled = hpos >= hist.length - 1;
  $(".b-up").disabled = inTrash() || cur === "/";
  $(".b-mk").disabled = inTrash(); $(".b-up2").classList.toggle("dis", inTrash());
  $(".v-grid").classList.toggle("on", view === "grid"); $(".v-list").classList.toggle("on", view === "list");
  shown = sorted();
  const c = clip.get(), cutSet = c?.mode === "cut" ? new Set(c.paths) : null;
  if (!shown.length) {
    area.replaceChildren(h("div.empty", query ? t("fm.noResults") : inTrash() ? t("fm.trashEmpty") : t("f.empty")));
  } else if (view === "grid") {
    area.replaceChildren(h("div.grid", ...shown.map((e, i) => item(e, i, cutSet))));
  } else {
    const hb = (k, label, r) => h("button" + (r ? ".r" : ""), { onclick: () => { if (sortBy === k) sortDesc = !sortDesc; else { sortBy = k; sortDesc = false; } store.set("files.sort", sortBy); render(); } },
      label + (sortBy === k ? (sortDesc ? " ↓" : " ↑") : ""));
    area.replaceChildren(h("div.list", h("div.hd", h("span"), hb("name", t("fm.name")), hb("size", t("fm.size"), 1), hb("date", inTrash() ? t("fm.deleted") : t("fm.modified"), 1)),
      ...shown.map((e, i) => item(e, i, cutSet))));
  }
  if (inTrash() && shown.length) area.prepend(h("div", { style: { display: "flex", justifyContent: "flex-end", marginBottom: "8px" } },
    h("button.btn.sm", { onclick: emptyTrash }, h("span", { html: icon("trash-2", 13, 2) }), t("fm.emptyTrash"))));
  status();
}

const pathOf = e => inTrash() ? e.id : join(cur, e.name);
function item(e, i, cutSet) {
  const p = pathOf(e);
  const k = kind(e);
  const fmtDate = d => { const x = new Date(d); return isNaN(x) ? String(d || "") : x.toLocaleString(document.documentElement.lang, { dateStyle: "medium", timeStyle: "short" }); };
  let el;
  if (view === "grid") {
    const thumb = k === "img" && !inTrash() && e.size < 6e6 ? h("img", { src: raw(p), loading: "lazy", alt: "", draggable: "false" }) : kicon(e, 44);
    el = h("div.it", h("div.th", thumb), h("div.nm", { title: e.name }, e.name));
  } else {
    el = h("div.it", kicon(e, 16), h("span.nm", { title: inTrash() ? e.origin : e.name }, e.name), h("span.sz", e.dir ? "—" : bytes(e.size)), h("span.tm", fmtDate(e.mtime)));
  }
  el.dataset.i = i;
  el.classList.toggle("sel", sel.has(e.name));
  if (cutSet?.has(p)) el.classList.add("cut");
  el.onclick = ev => { ev.stopPropagation(); select(i, ev); if (matchMedia("(pointer: coarse)").matches && !ev.metaKey && !ev.ctrlKey && !ev.shiftKey) open(e); };
  el.ondblclick = () => open(e);
  el.oncontextmenu = ev => { ev.preventDefault(); ev.stopPropagation(); if (!sel.has(e.name)) select(i, {}); menu(ev, e); };
  if (!inTrash()) {
    el.draggable = true;
    el.ondragstart = ev => {
      if (!sel.has(e.name)) select(i, {});
      const paths = selected().map(pathOf);
      ev.dataTransfer.setData("application/x-makeros-files", JSON.stringify(paths));
      ev.dataTransfer.effectAllowed = "copyMove";
    };
    if (e.dir) dropTarget(el, () => p);
  }
  return el;
}

function status() {
  const n = shown.length, s = sel.size, size = selected().reduce((a, e) => a + (e.dir ? 0 : e.size || 0), 0);
  $(".st-n").textContent = t("fm.items", { n });
  $(".st-sel").textContent = s ? t("fm.selected", { n: s }) + (size ? ` · ${bytes(size)}` : "") : "";
}

// ------------------------------------------------------------ seçim
const selected = () => shown.filter(e => sel.has(e.name));
function select(i, ev) {
  const e = shown[i];
  if (!e) return;
  if (ev.shiftKey && anchor != null) {
    const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
    if (!(ev.metaKey || ev.ctrlKey)) sel.clear();
    for (let k = a; k <= b; k++) sel.add(shown[k].name);
  } else if (ev.metaKey || ev.ctrlKey) {
    sel.has(e.name) ? sel.delete(e.name) : sel.add(e.name);
    anchor = i;
  } else { sel = new Set([e.name]); anchor = i; }
  area.querySelectorAll(".it").forEach(el => el.classList.toggle("sel", sel.has(shown[+el.dataset.i]?.name)));
  status();
}
area.onclick = () => { sel.clear(); area.querySelectorAll(".it.sel").forEach(el => el.classList.remove("sel")); status(); };
area.oncontextmenu = ev => { ev.preventDefault(); sel.clear(); render(); menu(ev, null); };

// ------------------------------------------------------------ açma / önizleme
function open(e) {
  if (inTrash()) return restore([e]);
  const p = pathOf(e);
  if (e.dir) { query = ""; q.value = ""; return load(p); }
  const k = kind(e);
  if (["img", "vid", "aud", "pdf", "code"].includes(k) || e.size < 64000) return quickLook(e);
  location.href = raw(p, true);
}
const isPy = n => /\.py$/i.test(n);
const runPy = p => post("/files/run", { path: rel(p) }).then(openTerm).catch(fail);

let qlEl = null;
function closeQL() { qlEl?.remove(); qlEl = null; area.focus(); }
async function quickLook(e) {
  closeQL();
  const p = pathOf(e), k = kind(e);
  let body;
  if (k === "img") body = h("img", { src: raw(p), alt: e.name });
  else if (k === "vid") body = h("video", { src: raw(p), controls: true, autoplay: true });
  else if (k === "aud") body = h("audio", { src: raw(p), controls: true, autoplay: true });
  else if (k === "pdf") body = h("iframe", { src: raw(p) });
  else if (e.dir) return;
  else {
    try { const txt = await (await fetch(raw(p))).text(); body = h("pre", txt.slice(0, 300000)); } catch (x) { return fail(x); }
  }
  const actions = h("span", { style: { display: "flex", gap: "6px" } },
    ...(isPy(e.name) ? [h("button.btn.sm.primary", { onclick: () => { closeQL(); runPy(p); } }, h("span", { html: icon("play", 13, 2) }), t("f.run"))] : []),
    h("button.btn.sm", { onclick: () => (location.href = raw(p, true)) }, h("span", { html: icon("download", 13, 2) }), t("f.download")),
    h("button.ib", { onclick: closeQL, html: icon("x", 16, 2), title: t("close") }));
  qlEl = h("div.ql", { onclick: ev => { if (ev.target === qlEl) closeQL(); } }, h("div.box", h("div.hd", kicon(e, 16), h("span", e.name), actions), h("div.bd", body)));
  document.body.append(qlEl);
}

// ------------------------------------------------------------ işlemler
async function run(label, list, fn) {
  const prog = $(".prog");
  let done = 0, errs = 0;
  for (const x of list) {
    prog.textContent = `${label} ${++done}/${list.length}`;
    try { await fn(x); } catch (e) { errs++; fail(e); }
  }
  prog.textContent = "";
  return errs;
}
function copySel(mode) {
  const paths = selected().map(pathOf);
  if (!paths.length || inTrash()) return;
  clip.set({ mode, paths });
  toast(t(mode === "cut" ? "fm.cutN" : "fm.copiedN", { n: paths.length }));
  render();
}
async function paste(dest = cur) {
  const c = clip.get();
  if (!c?.paths?.length || inTrash()) return;
  await run(t(c.mode === "cut" ? "fm.moving" : "fm.copying"), c.paths, p => op(c.mode === "cut" ? "move" : "copy", p, dest));
  if (c.mode === "cut") clip.set(null);
  reload();
}
async function transfer(paths, dest, copy) {
  if (dest === TRASH) return toTrash(paths);
  const list = paths.filter(p => p !== dest && parent(p) !== dest || copy);
  if (!list.length) return;
  await run(t(copy ? "fm.copying" : "fm.moving"), list, p => op(copy ? "copy" : "move", p, dest));
  reload();
}
async function toTrash(paths) {
  if (!paths.length) return;
  const errs = await run(t("fm.trashing"), paths, p => op("trash", p));
  if (!errs) toast(t("fm.trashedN", { n: paths.length }));
  reload();
}
async function restore(list) {
  await run(t("fm.restoring"), list, e => op("restore", e.id));
  toast(t("fm.restoredN", { n: list.length }));
  reload();
}
async function deleteForever(list) {
  if (!(await confirmDlg(t("fm.deleteForever"), t("fm.deleteForeverQ", { n: list.length })))) return;
  await run(t("fm.deleting"), list, e => op("trash-delete", e.id));
  reload();
}
async function emptyTrash() {
  if (!(await confirmDlg(t("fm.emptyTrash"), t("fm.emptyTrashQ")))) return;
  try { await op("trash-delete"); } catch (e) { fail(e); }
  reload();
}
async function rename(e) {
  const inp = h("input", { value: e.name });
  setTimeout(() => { inp.focus(); const dot = e.dir ? -1 : e.name.lastIndexOf("."); inp.setSelectionRange(0, dot > 0 ? dot : e.name.length); });
  if (await dialog(t("f.rename"), inp, [{ label: t("cancel"), value: null }, { label: t("save"), primary: true, value: "ok" }]) !== "ok") return;
  const nn = inp.value.trim();
  if (!nn || nn === e.name || nn.includes("/")) return;
  try { await op("rename", pathOf(e), join(parent(pathOf(e)), nn)); sel = new Set([nn]); reload(); } catch (x) { fail(x); }
}
async function mkdir() {
  if (inTrash()) return;
  const inp = h("input", { placeholder: t("f.newFolder"), value: t("fm.untitled") });
  setTimeout(() => inp.select());
  if (await dialog(t("f.mkdir"), inp, [{ label: t("cancel"), value: null }, { label: t("f.create"), primary: true, value: "ok" }]) !== "ok" || !inp.value.trim()) return;
  try { await op("mkdir", join(cur, inp.value.trim())); sel = new Set([inp.value.trim()]); reload(); } catch (x) { fail(x); }
}
async function duplicate() {
  await run(t("fm.copying"), selected().map(pathOf), p => op("copy", p, parent(p)));
  reload();
}

// Yükleme: XHR ile ilerleme gösterilir.
function uploadOne(f, dest) {
  return new Promise((res, rej) => {
    const x = new XMLHttpRequest();
    x.open("PUT", `${BASE}/files/raw?path=${encodeURIComponent(join(dest, f.name))}`);
    x.setRequestHeader("X-Maker", "1");
    x.upload.onprogress = ev => { if (ev.lengthComputable) $(".prog").textContent = `${t("f.uploading", { name: f.name })} %${Math.round(ev.loaded / ev.total * 100)}`; };
    x.onload = () => (x.status < 300 ? res() : rej(new Error((() => { try { return JSON.parse(x.responseText).error.message; } catch { return x.statusText; } })())));
    x.onerror = () => rej(new Error(t("fm.uploadFail")));
    x.send(f);
  });
}
async function upload(files, dest = cur) {
  if (inTrash() || !files?.length) return;
  await run(t("f.upload"), [...files], f => uploadOne(f, dest));
  toast(t("fm.uploadedN", { n: files.length }));
  reload();
}

// ------------------------------------------------------------ sürükle-bırak
function dropTarget(el, dest) {
  el.addEventListener("dragover", ev => {
    const internal = ev.dataTransfer.types.includes("application/x-makeros-files"), ext = ev.dataTransfer.types.includes("Files");
    if (!internal && !ext) return;
    ev.preventDefault(); ev.stopPropagation();
    ev.dataTransfer.dropEffect = ev.altKey || (ext && !internal) ? "copy" : "move";
    el.classList.add("dropok");
  });
  el.addEventListener("dragleave", () => el.classList.remove("dropok"));
  el.addEventListener("drop", ev => {
    ev.preventDefault(); ev.stopPropagation(); el.classList.remove("dropok");
    const d = dest(), s = ev.dataTransfer.getData("application/x-makeros-files");
    if (s) transfer(JSON.parse(s), d, ev.altKey);
    else if (ev.dataTransfer.files.length && d !== TRASH) upload(ev.dataTransfer.files, d);
  });
}
area.addEventListener("dragover", ev => { if (ev.dataTransfer.types.includes("Files") || ev.dataTransfer.types.includes("application/x-makeros-files")) { ev.preventDefault(); area.classList.add("drop"); } });
area.addEventListener("dragleave", ev => { if (ev.target === area) area.classList.remove("drop"); });
area.addEventListener("drop", ev => {
  ev.preventDefault(); area.classList.remove("drop");
  const s = ev.dataTransfer.getData("application/x-makeros-files");
  if (s) transfer(JSON.parse(s), cur, ev.altKey);
  else upload(ev.dataTransfer.files);
});

// ------------------------------------------------------------ sağ tık menüsü
function menu(ev, e) {
  document.querySelector(".menu2")?.remove();
  const s = selected(), one = s.length === 1 ? s[0] : null, c = clip.get();
  const it = (label, ic, fn, { kbd = "", disabled = false } = {}) => h("button", { disabled, onclick: () => { m.remove(); fn(); } }, h("span", { html: icon(ic, 14, 2) }), label, kbd ? h("kbd", kbd) : "");
  const hr = () => h("hr");
  let items;
  if (inTrash()) {
    items = s.length ? [it(t("fm.restore"), "rotate-ccw", () => restore(s)), it(t("fm.deleteForever"), "trash-2", () => deleteForever(s))]
      : [it(t("fm.emptyTrash"), "trash-2", emptyTrash, { disabled: !shown.length })];
  } else if (!e) {
    items = [it(t("f.mkdir"), "folder-plus", mkdir), it(t("fm.paste"), "clipboard-paste", () => paste(), { kbd: MOD + "V", disabled: !c?.paths?.length }), hr(),
      it(t("f.upload"), "upload", () => $(".b-up2 input").click()), it(showHidden ? t("fm.hideHidden") : t("fm.showHidden"), "eye", toggleHidden, { kbd: mac ? "⌘⇧." : "Ctrl+H" })];
  } else {
    items = [
      it(t("fm.open"), e.dir ? "folder-open" : "external-link", () => open(e), { kbd: "Enter" }),
      ...(one && !one.dir ? [it(t("fm.quickLook"), "eye", () => quickLook(one), { kbd: "Space" })] : []),
      ...(one && isPy(one.name) ? [it(t("f.run"), "play", () => runPy(pathOf(one)))] : []),
      ...(one && !one.dir ? [it(t("f.download"), "download", () => (location.href = raw(pathOf(one), true)))] : []),
      hr(),
      it(t("fm.copy"), "copy", () => copySel("copy"), { kbd: MOD + "C" }),
      it(t("fm.cut"), "scissors", () => copySel("cut"), { kbd: MOD + "X" }),
      it(t("fm.paste"), "clipboard-paste", () => paste(one?.dir ? pathOf(one) : cur), { kbd: MOD + "V", disabled: !c?.paths?.length }),
      it(t("fm.duplicate"), "copy", duplicate, { kbd: MOD + "D" }),
      ...(one ? [it(t("f.rename"), "pencil", () => rename(one), { kbd: "F2" })] : []),
      it(t("f.copyPath"), "clipboard-paste", () => navigator.clipboard?.writeText(s.map(pathOf).join("\n")).then(() => toast(t("f.copied")))),
      hr(),
      it(t("fm.toTrash"), "trash-2", () => toTrash(s.map(pathOf)), { kbd: mac ? "⌘⌫" : "Del" }),
    ];
  }
  const m = h("div.menu2", ...items);
  document.body.append(m);
  const r = m.getBoundingClientRect();
  m.style.left = Math.min(ev.clientX, innerWidth - r.width - 6) + "px";
  m.style.top = Math.min(ev.clientY, innerHeight - r.height - 6) + "px";
  setTimeout(() => addEventListener("click", () => m.remove(), { once: true }));
}

// ------------------------------------------------------------ klavye
const toggleHidden = () => { showHidden = !showHidden; store.set("files.hidden", showHidden); render(); };
addEventListener("keydown", ev => {
  if (ev.target.closest("input, textarea, dialog, .dlg")) { if (ev.key === "Escape" && ev.target === q) { q.value = ""; setQuery(""); } return; }
  const k = ev.key, cmd = ev.metaKey || ev.ctrlKey;
  if (qlEl) { if (k === " " || k === "Escape") { ev.preventDefault(); closeQL(); } return; }
  const s = selected();
  if (cmd && k.toLowerCase() === "c") { ev.preventDefault(); copySel("copy"); }
  else if (cmd && k.toLowerCase() === "x") { ev.preventDefault(); copySel("cut"); }
  else if (cmd && k.toLowerCase() === "v") { ev.preventDefault(); paste(); }
  else if (cmd && k.toLowerCase() === "a") { ev.preventDefault(); sel = new Set(shown.map(e => e.name)); render(); }
  else if (cmd && k.toLowerCase() === "d") { ev.preventDefault(); if (s.length && !inTrash()) duplicate(); }
  else if (cmd && k.toLowerCase() === "f") { ev.preventDefault(); q.focus(); }
  else if ((cmd && ev.shiftKey && k === ".") || (ev.ctrlKey && k.toLowerCase() === "h")) { ev.preventDefault(); toggleHidden(); }
  else if (cmd && k === "ArrowUp") { ev.preventDefault(); goUp(); }
  else if ((cmd && k === "ArrowDown") || k === "Enter") { if (s.length === 1) { ev.preventDefault(); open(s[0]); } }
  else if (k === "Delete" || (cmd && k === "Backspace")) { if (s.length) { ev.preventDefault(); inTrash() ? deleteForever(s) : toTrash(s.map(pathOf)); } }
  else if (k === "Backspace" && !cmd) { ev.preventDefault(); goBack(); }
  else if (k === "F2") { if (s.length === 1 && !inTrash()) { ev.preventDefault(); rename(s[0]); } }
  else if (k === " ") { if (s.length === 1 && !s[0].dir) { ev.preventDefault(); quickLook(s[0]); } }
  else if (k.startsWith("Arrow") && shown.length) {
    ev.preventDefault();
    const cols = view === "grid" ? Math.max(1, Math.round(area.querySelector(".grid")?.clientWidth / (area.querySelector(".grid .it")?.offsetWidth + 4 || 108))) : 1;
    const d = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -cols, ArrowDown: cols }[k];
    const i = Math.min(shown.length - 1, Math.max(0, (anchor ?? -1) + d));
    select(i, ev);
    area.querySelector(`.it[data-i="${i}"]`)?.scrollIntoView({ block: "nearest" });
  } else if (k === "Escape") { sel.clear(); render(); }
});

// ------------------------------------------------------------ araç çubuğu
const ib = (sel, ic, title, fn) => { const b = $(sel); b.innerHTML = icon(ic, 16, 2); b.title = title; b.setAttribute("aria-label", title); if (fn) b.onclick = fn; return b; };
const goBack = () => { if (hpos > 0) load(hist[--hpos], { push: false }); };
const goFwd = () => { if (hpos < hist.length - 1) load(hist[++hpos], { push: false }); };
const goUp = () => { if (!inTrash() && cur !== "/") { query = ""; q.value = ""; load(parent(cur)); } };
ib(".b-back", "chevron-left", t("fm.back"), goBack);
ib(".b-fwd", "chevron-right", t("fm.forward"), goFwd);
ib(".b-up", "arrow-up", t("f.up"), goUp);
ib(".v-grid", "layout-grid", t("fm.grid"), () => { view = "grid"; store.set("files.view", view); render(); });
ib(".v-list", "list", t("fm.list"), () => { view = "list"; store.set("files.view", view); render(); });
ib(".b-mk", "folder-plus", t("f.mkdir"), mkdir);
ib(".toggle-side", "panel-left", t("fm.places"), () => document.body.classList.toggle("side-open"));
const up2 = $(".b-up2");
up2.prepend(h("span", { html: icon("upload", 16, 2) })); up2.title = t("f.upload");
$(".b-up2 input").onchange = e => { upload(e.target.files); e.target.value = ""; };
q.placeholder = t("fm.search");
let qt = 0;
const setQuery = v => { query = v.trim(); if (inTrash()) { render(); return; } load(cur, { push: false }); };
q.oninput = () => { clearTimeout(qt); qt = setTimeout(() => setQuery(q.value), 300); };

app.on("navigate", p => { query = ""; q.value = ""; load(p || "~"); });
// Başka pencere panoya kesip yapıştırınca soluk görünüm güncellensin.
addEventListener("storage", e => { if (e.key?.endsWith("files.clip")) render(); });
app.onVisibility(v => { if (v && cur) reload(); });

(async () => {
  try { places = await op("places"); } catch { places = []; }
  load(decodeURIComponent(location.hash.slice(1)) || "~");
  area.focus();
})();
