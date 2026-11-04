// Dosyalar: kullanıcının ev dizini (makerd-user üzerinden, kullanıcının kendi izinleriyle).
import { app, t } from "/shell/js/app-kit.js";
import { h, $, bytes } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, api, post, BASE } from "/shell/js/api.js";
import { toast, fail, dialog, confirmDlg, openTerm } from "/shell/js/appui.js";

app.init({ title: t("app.files") });
app.chrome({ title: false });
const list = $(".list"), crumbs = $(".crumbs");
let cur = "~", home = "", data = null;
$(".b-up").innerHTML = icon("arrow-up", 15, 2); $(".b-up").title = t("f.up");
$(".b-mk").append(h("span", { html: icon("folder-plus", 14, 2) }), t("f.mkdir"));
$(".b-up2").prepend(h("span", { html: icon("upload", 14, 2) }), t("f.upload"));
$(".n-hint").textContent = t("f.hint");

const rel = p => (home && p.startsWith(home) ? "~" + p.slice(home.length) : p) || "~";
const join = (a, b) => (a.endsWith("/") ? a : a + "/") + b;
const raw = (p, dl) => `${BASE}/files/raw?path=${encodeURIComponent(p)}${dl ? "&download=1" : ""}`;
const op = (o, path, to) => api("POST", "/files/op", { op: o, path, to });
const ICON = n => /\.(png|jpe?g|gif|webp|svg)$/i.test(n) ? "image" : /\.(py|js|sh|c|cpp|h|json|toml|ya?ml|md|txt|html|css)$/i.test(n) ? "file-code" : /\.(zip|tar|gz|xz)$/i.test(n) ? "file-archive" : "file";

async function load(p = cur) {
  try {
    data = await get("/files?path=" + encodeURIComponent(p));
    home = data.home; cur = rel(data.path); render();
    app.path(cur);
  } catch (e) { fail(e); if (p !== "~") load("~"); }
}
function render() {
  const parts = cur.split("/").filter(Boolean);
  crumbs.replaceChildren(...parts.map((s, i) => h("button", { onclick: () => load(parts.slice(0, i + 1).join("/")) }, i === 0 && s === "~" ? t("f.home") : s)));
  $(".b-up").disabled = cur === "~" || cur === "/";
  if (!data.entries.length) { list.replaceChildren(h("div.empty", t("f.empty"))); return; }
  list.replaceChildren(...data.entries.filter(e => !e.name.startsWith(".") || showHidden).map(e => h("div.it", { ondblclick: () => openEntry(e), onclick: ev => ev.detail === 1 && matchMedia("(pointer: coarse)").matches && openEntry(e) },
    h("span", { html: icon(e.dir ? "folder" : ICON(e.name), 18, 1.7), style: e.dir ? "color:var(--accent)" : "" }),
    h("span.nm", e.name), h("span.sz", e.dir ? "" : bytes(e.size)), h("span.tm", new Date(e.mtime).toLocaleString(document.documentElement.lang, { dateStyle: "short", timeStyle: "short" })),
    h("button.mn", { html: icon("ellipsis", 16, 2), onclick: ev => { ev.stopPropagation(); menu(ev, e); } }))));
}
let showHidden = false;
const isPy = n => /\.py$/i.test(n);
// Python betiği: tıklayınca doğrudan terminalde çalışır (önizleme ⋯ menüsünde).
const runPy = p => post("/files/run", { path: p }).then(openTerm).catch(fail);
function openEntry(e) {
  const p = join(cur, e.name);
  if (e.dir) return load(p);
  if (isPy(e.name)) return runPy(p);
  preview(e, p);
}
function preview(e, p) {
  if (/\.(png|jpe?g|gif|webp)$/i.test(e.name)) return dialog(e.name, h("div.prev", h("img", { src: raw(p) })), [{ label: t("f.download"), onclick: () => (location.href = raw(p, true)) }, { label: t("close"), primary: true }]);
  if (/\.(py|js|sh|txt|md|json|toml|ya?ml|log|csv|ini|cfg|html|css)$/i.test(e.name) || e.size < 64000) {
    return fetch(raw(p)).then(r => r.text()).then(txt => dialog(e.name, h("div.prev", h("pre", txt.slice(0, 200000))), [{ label: t("f.download"), onclick: () => (location.href = raw(p, true)) },
      ...(isPy(e.name) ? [{ label: t("f.run"), onclick: () => { runPy(p); } }] : []), { label: t("close"), primary: true }]));
  }
  location.href = raw(p, true);
}
function menu(ev, e) {
  document.querySelector(".menu2")?.remove();
  const p = join(cur, e.name);
  const items = [
    !e.dir && isPy(e.name) && [t("f.run"), "play", () => runPy(p)],
    !e.dir && isPy(e.name) && [t("f.view"), "file-code", () => preview(e, p)],
    !e.dir && [t("f.download"), "download", () => (location.href = raw(p, true))],
    [t("f.rename"), "file", async () => {
      const inp = h("input", { value: e.name });
      if (await dialog(t("f.rename"), inp, [{ label: t("cancel"), value: null }, { label: t("save"), primary: true, value: "ok" }]) !== "ok") return;
      try { await op("rename", p, join(cur, inp.value.trim())); load(); } catch (x) { fail(x); }
    }],
    [t("f.copyPath"), "copy", () => navigator.clipboard?.writeText(home + p.slice(1)).then(() => toast(t("f.copied")))],
    [t("delete"), "trash-2", async () => { if (await confirmDlg(t("f.delete"), t("f.deleteQ", { name: e.name }))) try { await op("remove", p); load(); } catch (x) { fail(x); } }],
  ].filter(Boolean);
  const m = h("div.menu2", ...items.map(([l, ic, fn]) => h("button", { onclick: () => { m.remove(); fn(); } }, h("span", { html: icon(ic, 14, 2) }), l)));
  m.style.left = Math.min(ev.clientX, innerWidth - 180) + "px"; m.style.top = Math.min(ev.clientY, innerHeight - 160) + "px";
  document.body.append(m);
  setTimeout(() => addEventListener("click", () => m.remove(), { once: true }));
}
$(".b-up").onclick = () => load(cur.split("/").slice(0, -1).join("/") || "/");
$(".b-mk").onclick = async () => {
  const inp = h("input", { placeholder: t("f.newFolder") });
  if (await dialog(t("f.mkdir"), inp, [{ label: t("cancel"), value: null }, { label: t("f.create"), primary: true, value: "ok" }]) !== "ok" || !inp.value.trim()) return;
  try { await op("mkdir", join(cur, inp.value.trim())); load(); } catch (x) { fail(x); }
};
async function upload(files) {
  for (const f of files) {
    toast(t("f.uploading", { name: f.name }));
    try {
      const r = await fetch(`${BASE}/files/raw?path=${encodeURIComponent(join(cur, f.name))}`, { method: "PUT", headers: { "X-Maker": "1" }, body: f });
      if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error?.message || r.statusText);
    } catch (x) { fail(x); }
  }
  load();
}
$(".b-up2 input").onchange = e => upload(e.target.files);
const wrap = $(".wrap");
wrap.addEventListener("dragover", e => { e.preventDefault(); wrap.classList.add("drop"); });
wrap.addEventListener("dragleave", () => wrap.classList.remove("drop"));
wrap.addEventListener("drop", e => { e.preventDefault(); wrap.classList.remove("drop"); upload(e.dataTransfer.files); });
addEventListener("keydown", e => { if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === ".") { showHidden = !showHidden; render(); } });
app.on("navigate", p => load(p || "~"));
load(decodeURIComponent(location.hash.slice(1)) || "~");
