// Python: projeler (şablonlu, uv sanal ortamlı), şablonlar ve Python sürümleri.
// Aynı işlemler komut satırından: mos project new|ls|run, mos python ls|install|default.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, post, api } from "/shell/js/api.js";
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
    return card(".cat-" + (tp?.category || "temel"), tp?.icon || "file-code", p.name,
      `${tp ? L(tp.title) : t("py.custom")} · ${p.python === "system" || !p.python ? t("py.system") : "Python " + p.python}${p.venv ? "" : " · " + t("py.noVenv")}`,
      btn(t("py.run"), () => post(`/projects/${p.name}/run`).then(openTerm).catch(fail), ".primary"),
      btn(t("py.folder"), () => app.open("makeros.files", "~/Projeler/" + p.name)),
      btn(t("delete"), async () => { if (await confirmDlg(t("py.delete"), t("py.deleteQ", { name: p.name }))) { await api("DELETE", "/projects/" + p.name).catch(fail); show("projects"); } }));
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
show(location.hash.slice(1) || "projects");
