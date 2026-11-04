// Uygulamalar için küçük ortak yardımcılar: diyalog, bildirim, terminal açma, hata gösterme.
import { h } from "./lib.js";
import { app, t } from "./app-kit.js";

export function toast(msg, ms = 3200) {
  const el = h("div.toast2", msg);
  document.body.append(el);
  setTimeout(() => el.remove(), ms);
}

export const fail = e => toast((e && e.message) || String(e), 5000);

// Uzun işin terminal oturumu: Terminal penceresi açılır ve oturuma bağlanır (ortak oturum).
export function openTerm(r) {
  if (r && r.term) app.open("makeros.terminal", "s=" + r.term.id);
  return r;
}

// dialog(başlık, gövde düğümleri, [{label, primary, danger, onclick}]) → Promise<düğme etiketi|null>
export function dialog(title, body, buttons) {
  return new Promise(resolve => {
    const close = v => { bg.remove(); resolve(v); };
    const dlg = h("div.dlg", { role: "dialog", "aria-modal": "true" }, h("h2", title), ...[].concat(body),
      h("div.act", ...buttons.map(b => h("button.btn" + (b.primary ? ".primary" : b.danger ? ".danger" : ""), {
        onclick: async () => { if (b.onclick && (await b.onclick()) === false) return; close(b.value ?? b.label); } }, b.label))));
    const bg = h("div.dlg-bg", { onclick: e => e.target === bg && close(null) }, dlg);
    document.body.append(bg);
    addEventListener("keydown", function k(e) { if (e.key === "Escape") { removeEventListener("keydown", k); close(null); } });
    dlg.querySelector("input,select,textarea")?.focus();
  });
}

export const confirmDlg = (title, text, danger = t("delete")) =>
  dialog(title, h("p", text), [{ label: t("cancel"), value: false }, { label: danger, danger: true, value: true }]).then(v => v === true);
