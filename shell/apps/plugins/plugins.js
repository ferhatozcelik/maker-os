// Eklentiler: zip olarak paylaşılan Python uygulamaları.
// "Kurulu": şablonu indir → düzenle → zip'le → içe aktar; dışa aktar aynı biçimi üretir.
// "Mağaza": GitHub'daki makeros-plugins deposunun plugins/<id>/ klasörleri (makerd /plugins/catalog); Kur/Güncelle
// klasörü indirir, her dosyayı git özetiyle denetler.
// Komut satırı: mos plugin new|import|export|start|stop|ls.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, post, api, BASE } from "/shell/js/api.js";
import { fail, toast, dialog, confirmDlg } from "/shell/js/appui.js";
import { syncPluginThemes, OFFICIAL_PLUGINS } from "/shell/js/themes.js";
import { pref, setPref, prefs } from "/shell/js/prefs.js";
import { createEditor, EDITOR_CSS } from "/shell/apps/python/editor.js";
// Durdurma/silme/başlatma sonrası: eklenti teması kalktıysa varsayılan temaya dön (tercih tüm pencerelere yayılır).
const resyncThemes = () => get("/plugins").then(syncPluginThemes).catch(() => {});

app.init({ title: t("app.plugins") });
app.chrome({ title: false });
$(".h").textContent = t("app.plugins");
$(".b-tpl").append(h("span", { html: icon("download", 14, 2) }), t("pl.template"));
$(".b-imp").prepend(h("span", { html: icon("upload", 14, 2) }), t("pl.import"));
$(".b-re").innerHTML = icon("refresh-cw", 14, 2);
$(".b-re").title = t("pl.refresh");
const btn = (l, fn, cls = "") => h("button.btn.sm" + cls, { onclick: fn }, l);

let tab = "installed";
try { if (localStorage.getItem("mk.plugins.tab") === "store") tab = "store"; } catch {}
const tabs = h("div.seg", ...[["installed", t("pl.tabInstalled")], ["store", t("pl.tabStore")]].map(([v, l]) =>
  h("button", { "data-v": v, onclick: () => show(v) }, l)));
$(".tabs").replaceWith(tabs);
function show(v) {
  tab = v;
  try { localStorage.setItem("mk.plugins.tab", v); } catch {}
  tabs.querySelectorAll("button").forEach(b => b.classList.toggle("on", b.dataset.v === v));
  document.body.dataset.tab = v;
  $(".n-top").textContent = t(v === "store" ? "pl.storeTop" : "pl.top");
  document.getElementById("pl-empty")?.remove();
  $(".cards").replaceChildren(h("div.empty", t("loading")));
  (v === "store" ? loadStore() : load());
}
$(".b-re").onclick = () => (tab === "store" ? loadStore(true) : load());

$(".b-imp input").onchange = async e => {
  const f = e.target.files[0];
  e.target.value = "";
  if (!f) return;
  try {
    const r = await fetch(BASE + "/plugins/import", { method: "POST", headers: { "X-Maker": "1", "Content-Type": "application/zip" }, body: f });
    const j = await r.json();
    if (!r.ok) throw new Error(j.error?.message || r.statusText);
    toast(t("pl.imported", { name: j.name }));
    show("installed");
  } catch (x) { fail(x); }
};

const openUI = p => app.open("makeros.plugin." + p.id);
const isAuto = p => (pref("autostartPlugins") || []).includes(p.id);
function toggleAuto(p, v) {
  const cur = pref("autostartPlugins") || [];
  const enable = v !== undefined ? !!v : !isAuto(p);
  setPref("autostartPlugins", enable ? [...new Set([...cur, p.id])] : cur.filter(x => x !== p.id));
  toast(enable ? t("pl.autostartOn", { name: p.name }) : t("pl.autostartOff", { name: p.name }));
  load();
}
async function act(p, o) {
  try {
    const r = await post(`/plugins/${p.id}/${o}`);
    if (o === "start") {
      const cur = pref("autostartPlugins") || [];
      if (!cur.includes(p.id)) setPref("autostartPlugins", [...cur, p.id]);
      if (p.web_port) setTimeout(() => openUI(p), 1500);
    }
    load(); resyncThemes();
    return r;
  } catch (e) { fail(e); }
}
async function logs(p) {
  const pre = h("pre.log", t("loading"));
  const tick = async () => { try { pre.textContent = (await post(`/plugins/${p.id}/logs`)).log || t("pl.noLog"); pre.scrollTop = pre.scrollHeight; } catch (e) { pre.textContent = e.message; } };
  tick(); const iv = setInterval(tick, 2000);
  await dialog(p.name + " · " + t("pl.logs"), pre, [{ label: t("close"), primary: true }]);
  clearInterval(iv);
}
function empty(ic, text, extra) {
  $(".cards").replaceChildren();
  $(".cards").after(Object.assign(h("div.empty2", h("span", { html: icon(ic, 34, 1.6) }), h("p", text), extra || ""), { id: "pl-empty" }));
}
// Masaüstü widget'ı (plugin.json "widget"): kabuk "desktopWidgets" tercihini izler; eklenti kapalıysa başlatır.
const onDesk = p => (pref("desktopWidgets") || []).includes(p.id);
function toggleDesk(p) {
  const cur = pref("desktopWidgets") || [];
  setPref("desktopWidgets", onDesk(p) ? cur.filter(x => x !== p.id) : [...cur, p.id]);
  toast(onDesk(p) ? t("pl.widgetAdded", { name: p.name }) : t("pl.widgetRemoved", { name: p.name }));
  load();
}
prefs.on("change", k => { if (k === "desktopWidgets" && tab === "installed") load(); });
// Resmî eklenti (MakerOS ekibi, makeros-plugins deposu): adın yanında rozet.
const official = p => OFFICIAL_PLUGINS.has(p.id) && p.author === "MakerOS" ? h("span.badge.acc.official", { title: t("pl.officialHint") }, t("pl.official")) : "";
async function load() {
  try {
    const list = await get("/plugins");
    if (tab !== "installed") return;
    document.getElementById("pl-empty")?.remove();
    if (!list.length) return empty("puzzle", t("pl.empty"), btn(t("pl.toStore"), () => show("store"), ".primary"));
    $(".cards").replaceChildren(...list.map(p => h("div.card2.pl" + (p.running ? ".run" : ""),
      h("div.hd", h("span.ico", p.icon ? h("img", { src: `${BASE}/plugins/${p.id}/asset/${p.icon}`, alt: "" }) : { html: icon("puzzle", 18, 2) }),
        h("div", h("div.ti", p.name, official(p)), h("div.ds", `v${p.version}${p.author ? " · " + p.author : ""} · ${p.running ? t("pl.running") : t("pl.stopped")}`))),
      h("div.ds", p.description),
      h("div.act",
        p.running ? [p.web_port ? btn(t("pl.open"), () => openUI(p), ".primary") : "", btn(t("pl.stop"), () => act(p, "stop"))] : btn(t("pl.start"), () => act(p, "start"), ".primary"),
        btn(isAuto(p) ? t("pl.autostart") + " ✓" : t("pl.autostart"), () => toggleAuto(p), isAuto(p) ? ".primary" : ""),
        p.widget && p.web_port ? btn(onDesk(p) ? t("pl.widgetOff") : t("pl.widgetOn"), () => toggleDesk(p), onDesk(p) ? "" : ".primary") : "",
        btn(t("pl.logs"), () => logs(p)),
        btn(t("pl.edit"), () => editPlugin(p)),
        h("a.btn.sm", { href: `${BASE}/plugins/${p.id}/export.zip` }, t("pl.export")),
        btn(t("delete"), async () => { if (await confirmDlg(p.name, t("pl.deleteQ", { name: p.name }))) {
          await api("DELETE", "/plugins/" + p.id).catch(fail);
          setPref("autostartPlugins", (pref("autostartPlugins") || []).filter(x => x !== p.id));
          setPref("desktopWidgets", (pref("desktopWidgets") || []).filter(x => x !== p.id));
          load(); resyncThemes();
        } })))));
  } catch (e) { if (tab === "installed") $(".cards").replaceChildren(h("div.empty.err", e.message)); }
}

// --- Mağaza ------------------------------------------------------------------
const TAGS = { theme: "palette", web: "globe", widget: "layout-grid", status: "activity" };
const size = n => n >= 1 << 20 ? (n / (1 << 20)).toFixed(1) + " MB" : Math.max(1, Math.round(n / 1024)) + " KB";
async function install(p, b) {
  b.disabled = true;
  b.replaceChildren(h("span.spin", { html: icon("loader-circle", 14, 2) }), t(p.update ? "pl.updating" : "pl.installing"));
  try {
    const m = await post(`/plugins/catalog/${p.id}/install`);
    const cur = pref("autostartPlugins") || [];
    if (!cur.includes(p.id)) setPref("autostartPlugins", [...cur, p.id]);
    await post(`/plugins/${p.id}/start`).catch(() => {});
    toast(t(p.update ? "pl.updated" : "pl.installed", { name: m.name, v: m.version }));
  } catch (e) { fail(e); }
  loadStore();
}
async function loadStore(refresh) {
  try {
    const c = await get("/plugins/catalog" + (refresh ? "?refresh=1" : ""));
    if (tab !== "store") return;
    document.getElementById("pl-empty")?.remove();
    if (!c.plugins.length) return empty("shopping-bag", t("pl.storeEmpty"));
    $(".cards").replaceChildren(...c.plugins.map(p => {
      const state = p.update ? t("pl.updateTo", { from: p.installed, to: p.version }) : p.installed ? t("pl.isInstalled") : "";
      const b = p.installed && !p.update
        ? h("button.btn.sm", { disabled: true }, h("span", { html: icon("check", 14, 2) }), t("pl.isInstalled"))
        : h("button.btn.sm.primary", { onclick: () => install(p, b) }, t(p.update ? "pl.update" : "pl.install"));
      return h("div.card2.pl.st" + (p.installed ? ".inst" : ""),
        h("div.hd", h("span.ico", p.icon ? h("img", { src: p.icon, alt: "" }) : { html: icon("puzzle", 18, 2) }),
          h("div", h("div.ti", p.name, official(p)), h("div.ds", [`v${p.version}`, p.author, p.size ? size(p.size) : ""].filter(Boolean).join(" · ")))),
        h("div.ds", p.description),
        p.tags?.length ? h("div.tags", ...p.tags.map(g => h("span.tag", { html: (TAGS[g] ? icon(TAGS[g], 12, 2) : "") }, t("pl.tag." + g) === "pl.tag." + g ? g : t("pl.tag." + g)))) : "",
        state ? h("div.ds.state", state) : "",
        h("div.act", b, p.installed ? btn(t("pl.manage"), () => show("installed")) : "",
          p.url ? h("a.btn.sm", { href: p.url, target: "_blank", rel: "noopener" }, t("pl.source")) : ""));
    }));
  } catch (e) {
    if (tab !== "store") return;
    empty("wifi-off", e.message, btn(t("pl.retry"), () => { document.getElementById("pl-empty")?.remove(); loadStore(true); }, ".primary"));
  }
}
show(tab);
// Başka pencere, Web arayüzü ya da `mos plugin` ile yapılan değişiklikler de listeye yansısın.
let plTimer = 0;
app.events.subscribe("plugins", () => { clearTimeout(plTimer); plTimer = setTimeout(() => { if (tab === "installed") load(); }, 400); });

// --- Kod düzenleyici -----------------------------------------------------------
// Eklenti dosyaları (~/.local/share/makeros/plugins/<id>) Python Yöneticisi'ndeki düzenleyiciyle açılır;
// kaydedilen değişiklik "Kaydet ve yeniden başlat" ile hemen denenir. Dosya erişimi: /files, /files/raw.
document.head.append(Object.assign(document.createElement("style"), { textContent: EDITOR_CSS + `
.ped { position: fixed; inset: 0; z-index: 40; background: var(--bg-window); display: flex; flex-direction: column; }
.ped-hd { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: .5px solid var(--separator); flex-wrap: wrap; }
html[data-embed=window] .ped-hd { min-height: var(--tb); padding-left: 90px; }
.ped-hd .nm { font-weight: 700; font-size: 15px; } .ped-hd .fn { color: var(--text-2); font-family: var(--font-mono); font-size: 12px; }
.ped-hd .dirty { color: var(--accent); }
.ped-bd { flex: 1; min-height: 0; display: grid; grid-template-columns: 210px 1fr; }
.ped-files { border-right: .5px solid var(--separator); overflow: auto; padding: 8px; display: grid; align-content: start; gap: 1px; background: var(--bg-sidebar); }
.ped-files button.f { display: flex; align-items: center; gap: 7px; border: 0; background: none; text-align: left; padding: 5px 8px; border-radius: 7px; color: var(--text); font: 12.5px var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ped-files button.f:hover { background: var(--bg-hover); } .ped-files button.f.on { background: var(--accent); color: var(--on-accent, #fff); }
.ped-files .ft { margin-top: 8px; display: grid; gap: 6px; } .ped-files .hint2 { font-size: 11px; color: var(--text-2); line-height: 1.35; padding: 0 4px; }
.ped-code { min-width: 0; min-height: 0; display: flex; } .ped-code .ed { flex: 1; }
@media (max-width: 699px) { .ped-bd { grid-template-columns: 1fr; grid-template-rows: auto 1fr; } .ped-files { max-height: 30vh; border-right: 0; border-bottom: .5px solid var(--separator); } }
` }));
const EDIT_OK = n => !n.startsWith(".") && n !== "__pycache__" && !/\.(pyc|so|bin|png|jpe?g|gif|webp|ico|woff2?|ttf|zip|mp[34])$/i.test(n);
async function editPlugin(p) {
  const dir = `~/.local/share/makeros/plugins/${p.id}`;
  const E = { file: "", dirty: false };
  const fname = h("span.fn"), dirty = h("span.dirty"), list = h("div.ped-files");
  const mark = () => { dirty.textContent = E.dirty ? "●" : ""; };
  const ed = createEditor({ onSave: () => save(), onChange: () => { if (!E.dirty) { E.dirty = true; mark(); } } });
  const close = () => { if (E.dirty && !confirm(t("pl.unsaved"))) return; root.remove(); load(); };
  async function save() {
    if (!E.file) return false;
    try {
      const r = await fetch(`${BASE}/files/raw?path=${encodeURIComponent(E.file)}`, { method: "PUT", headers: { "X-Maker": "1" }, body: ed.value });
      if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error?.message || r.statusText);
      E.dirty = false; mark(); toast(t("pl.saved")); return true;
    } catch (e) { fail(e); return false; }
  }
  async function restart() {
    if (!(await save())) return;
    await post(`/plugins/${p.id}/stop`).catch(() => {});
    try { await post(`/plugins/${p.id}/start`); toast(t("pl.restarted", { name: p.name })); resyncThemes(); } catch (e) { fail(e); }
  }
  async function open(rel) {
    if (E.dirty && !confirm(t("pl.unsaved"))) return;
    try {
      const r = await fetch(`${BASE}/files/raw?path=${encodeURIComponent(dir + "/" + rel)}`);
      if (!r.ok) throw new Error(r.statusText);
      ed.set(await r.text(), rel); E.file = dir + "/" + rel; E.dirty = false; mark(); fname.textContent = rel;
      list.querySelectorAll("button.f").forEach(b => b.classList.toggle("on", b.dataset.f === rel));
    } catch (e) { fail(e); }
  }
  async function refresh(want) {
    const files = [];
    const walk = async (rel, depth) => {
      const l = await get("/files?path=" + encodeURIComponent(dir + (rel ? "/" + rel : ""))).catch(() => null);
      for (const e of (l?.entries || []).filter(e => EDIT_OK(e.name))) {
        const r = rel ? rel + "/" + e.name : e.name;
        if (e.dir) { if (depth < 2) await walk(r, depth + 1); } else if (files.length < 200) files.push(r);
      }
    };
    await walk("", 0);
    const first = ["plugin.json", p.entrypoint || "main.py"];
    files.sort((a, b) => ((first.indexOf(a) + 1) || 99) - ((first.indexOf(b) + 1) || 99) || a.localeCompare(b));
    list.replaceChildren(...files.map(f => h("button.f", { "data-f": f, title: f, onclick: () => open(f) },
      h("span", { html: icon(/\.(py|js|json|html|css|toml)$/.test(f) ? "file-code" : /\.svg$/.test(f) ? "file-image" : "file-text", 14, 1.8) }), f)),
      h("div.ft", btn(t("pl.newFile"), async () => {
        const inp = h("input", { value: "yeni.py" });
        if (await dialog(t("pl.newFile"), inp, [{ label: t("cancel"), value: null }, { label: t("f.create"), primary: true, value: "ok" }]) !== "ok") return;
        const n = inp.value.trim().replace(/^\/+/, "");
        if (!n || n.includes("..")) return;
        await fetch(`${BASE}/files/raw?path=${encodeURIComponent(dir + "/" + n)}`, { method: "PUT", headers: { "X-Maker": "1" }, body: "" }).catch(fail);
        refresh(n);
      }), btn(t("pl.folder"), () => app.open("makeros.files", dir)), h("p.hint2", t("pl.editHint"))));
    const pick = want || (files.includes(first[1]) ? first[1] : files[0]);
    if (pick) open(pick);
  }
  const root = h("div.ped",
    h("div.ped-hd", h("button.btn.icon.sm", { html: icon("chevron-left", 16, 2), "aria-label": t("close"), onclick: close }),
      h("span.nm", p.name), fname, dirty, h("span.grow"),
      btn(t("save"), () => save()), btn(t("pl.saveRestart"), () => restart(), ".primary")),
    h("div.ped-bd", list, h("div.ped-code", ed.el)));
  document.body.append(root);
  refresh();
}
