// Eklentiler: zip ile paylaşılan Python uygulamaları (Mağaza yerine topluluk ekosistemi).
// Şablonu indir → düzenle → zip'le → içe aktar. Dışa aktar aynı biçimi üretir.
// Komut satırı: mos plugin new|import|export|start|stop|ls.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, post, api, BASE } from "/shell/js/api.js";
import { fail, toast, dialog, confirmDlg } from "/shell/js/appui.js";

app.init({ title: t("app.plugins") });
app.chrome({ title: false });
$(".h").textContent = t("app.plugins");
$(".n-top").textContent = t("pl.top");
$(".b-tpl").append(h("span", { html: icon("download", 14, 2) }), t("pl.template"));
$(".b-imp").prepend(h("span", { html: icon("upload", 14, 2) }), t("pl.import"));
const btn = (l, fn, cls = "") => h("button.btn.sm" + cls, { onclick: fn }, l);

$(".b-imp input").onchange = async e => {
  const f = e.target.files[0];
  e.target.value = "";
  if (!f) return;
  try {
    const r = await fetch(BASE + "/plugins/import", { method: "POST", headers: { "X-Maker": "1", "Content-Type": "application/zip" }, body: f });
    const j = await r.json();
    if (!r.ok) throw new Error(j.error?.message || r.statusText);
    toast(t("pl.imported", { name: j.name }));
    load();
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
async function load() {
  try {
    const list = await get("/plugins");
    if (!list.length) {
      $(".cards").replaceChildren();
      if (!document.getElementById("pl-empty")) $(".cards").after(Object.assign(h("div.empty2", h("span", { html: icon("puzzle", 34, 1.6) }), h("p", t("pl.empty"))), { id: "pl-empty" }));
      return;
    }
    document.getElementById("pl-empty")?.remove();
    $(".cards").replaceChildren(...list.map(p => h("div.card2.pl" + (p.running ? ".run" : ""),
      h("div.hd", h("span.ico", p.icon ? h("img", { src: `${BASE}/files/raw?path=${encodeURIComponent("~/.local/share/makeros/plugins/" + p.id + "/" + p.icon)}`, alt: "" }) : { html: icon("puzzle", 18, 2) }),
        h("div", h("div.ti", p.name), h("div.ds", `v${p.version}${p.author ? " · " + p.author : ""} · ${p.running ? t("pl.running") : t("pl.stopped")}`))),
      h("div.ds", p.description),
      h("div.act",
        p.running ? [p.web_port ? btn(t("pl.open"), () => openUI(p), ".primary") : "", btn(t("pl.stop"), () => act(p, "stop"))] : btn(t("pl.start"), () => act(p, "start"), ".primary"),
        btn(t("pl.logs"), () => logs(p)),
        h("a.btn.sm", { href: `${BASE}/plugins/${p.id}/export.zip` }, t("pl.export")),
        btn(t("delete"), async () => { if (await confirmDlg(p.name, t("pl.deleteQ", { name: p.name }))) { await api("DELETE", "/plugins/" + p.id).catch(fail); load(); } })))));
  } catch (e) { $(".cards").replaceChildren(h("div.empty.err", e.message)); }
}
load();
