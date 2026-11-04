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
async function act(p, o) {
  try {
    const r = await post(`/plugins/${p.id}/${o}`);
    if (o === "start" && p.web_port) setTimeout(() => openUI(p), 1500);
    load();
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
async function load() {
  try {
    const list = await get("/plugins");
    if (tab !== "installed") return;
    document.getElementById("pl-empty")?.remove();
    if (!list.length) return empty("puzzle", t("pl.empty"), btn(t("pl.toStore"), () => show("store"), ".primary"));
    $(".cards").replaceChildren(...list.map(p => h("div.card2.pl" + (p.running ? ".run" : ""),
      h("div.hd", h("span.ico", p.icon ? h("img", { src: `${BASE}/plugins/${p.id}/asset/${p.icon}`, alt: "" }) : { html: icon("puzzle", 18, 2) }),
        h("div", h("div.ti", p.name), h("div.ds", `v${p.version}${p.author ? " · " + p.author : ""} · ${p.running ? t("pl.running") : t("pl.stopped")}`))),
      h("div.ds", p.description),
      h("div.act",
        p.running ? [p.web_port ? btn(t("pl.open"), () => openUI(p), ".primary") : "", btn(t("pl.stop"), () => act(p, "stop"))] : btn(t("pl.start"), () => act(p, "start"), ".primary"),
        btn(t("pl.logs"), () => logs(p)),
        h("a.btn.sm", { href: `${BASE}/plugins/${p.id}/export.zip` }, t("pl.export")),
        btn(t("delete"), async () => { if (await confirmDlg(p.name, t("pl.deleteQ", { name: p.name }))) { await api("DELETE", "/plugins/" + p.id).catch(fail); load(); } })))));
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
          h("div", h("div.ti", p.name), h("div.ds", [`v${p.version}`, p.author, p.size ? size(p.size) : ""].filter(Boolean).join(" · ")))),
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
