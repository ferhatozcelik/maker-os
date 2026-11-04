// AI Ajanları: terminal tabanlı kodlama ajanları ve yerel model çalıştırıcı (Ollama).
// Kurulum/güncelleme terminalde görünür çalışır; komut satırı: mos agent ls|install|run <id>.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import BRANDS from "/shell/js/brand-icons.js";
import { get, post } from "/shell/js/api.js";
import { fail, openTerm, confirmDlg } from "/shell/js/appui.js";

app.init({ title: t("app.agents") });
app.chrome({ title: false });
$(".h").textContent = t("app.agents");
$(".n-top").textContent = t("ag.top");
$(".n-cli").textContent = t("ag.cli");
$(".b-re").innerHTML = icon("refresh-cw", 14, 2); $(".b-re").title = t("ag.refresh");
$(".b-re").onclick = load;
const op = (id, o) => post(`/agents/${id}/${o}`).then(openTerm).then(() => setTimeout(load, 5000)).catch(fail);
const btn = (l, fn, cls = "") => h("button.btn.sm" + cls, { onclick: fn }, l);
// Marka logosu (Simple Icons, CC0); logosu olmayan ajan (Codex) nötr simgeyle gösterilir.
const logo = id => BRANDS[id]
  ? `<svg class="ic" width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="${BRANDS[id]}"/></svg>`
  : icon(id === "codex" ? "square-terminal" : "bot", 20, 2);

async function load() {
  try {
    const list = await get("/agents");
    $(".cards").replaceChildren(...list.map(a => h("div.card2.ag" + (a.installed ? ".inst" : ""),
      h("div.hd", h("span.ico.br-" + a.id, { html: logo(a.id), title: a.name }), h("div", h("div.ti", a.name, a.installed ? h("span.chip", t("ag.installed")) : null), h("div.meta", a.vendor))),
      h("div.ds", a.summary), h("div.ds.tip", h("span", { html: icon("lightbulb", 13, 2) }), a.tip), h("div.ds", h("b", t("ag.account") + ": "), a.account),
      h("div.act",
        a.installed ? [btn(t("ag.run"), () => op(a.id, "run"), ".primary"), btn(t("ag.update"), () => op(a.id, "update")),
          btn(t("ag.remove"), async () => (await confirmDlg(a.name, t("ag.removeQ", { name: a.name }), t("ag.remove"))) && op(a.id, "remove"))]
          : btn(t("ag.install"), () => op(a.id, "install"), ".primary"),
        h("a.btn.sm", { href: a.docs, target: "_blank", rel: "noopener" }, t("ag.docs"))))));
  } catch (e) { $(".cards").replaceChildren(h("div.empty.err", e.message)); }
}
load();
