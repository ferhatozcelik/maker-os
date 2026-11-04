// Oturum açma / kilit ekranı ve onay iletişim kutusu.
import { h, $, animate } from "../lib.js";
import { icon } from "../icons.js";
import { t, locale } from "../i18n.js";
import { api, ApiError } from "../api.js";
import { pref } from "../prefs.js";
import { brand } from "../themes.js";

// mode: "login" (kullanıcı adı + parola) | "lock" (yalnız parola). Başarıda {user, admin} döner.
export function lockScreen(mode, user = "") {
  return new Promise(resolve => {
    const root = $("#lock");
    const timeEl = h("div.lk-time"), dateEl = h("div.lk-date");
    const tick = () => { const d = new Date(); timeEl.textContent = d.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" }); dateEl.textContent = d.toLocaleDateString(locale, { weekday: "long", day: "numeric", month: "long" }); };
    tick(); const iv = setInterval(tick, 5000);
    const uIn = h("input", { name: "user", placeholder: t("login.user"), autocomplete: "username", autocapitalize: "none", spellcheck: "false", value: user || "maker", "aria-label": t("login.user") });
    const pIn = h("input", { name: "password", type: "password", placeholder: t("login.pass"), autocomplete: "current-password", "aria-label": t("login.pass") });
    const go = h("button.lk-go", { type: "submit", "aria-label": t("login.go"), html: icon("chevron-right", 18, 2.4) });
    const msg = h("p.lk-msg", { role: "alert" });
    let showUser = mode === "login";
    const userRow = h("div.lk-field", uIn);
    const nameEl = h("div.lk-name", user || "");
    const form = h("form.lk-card", { autocomplete: "on" },
      h("div.lk-avatar", { html: mode === "login" ? `<img src="${brand(pref("look")).logo}" alt="">` : icon("user", 34, 1.6) }),
      mode === "lock" ? nameEl : h("div.lk-name", t("login.title")),
      h("div.lk-sub", mode === "lock" ? t("login.locked") : t("login.sub")),
      showUser ? userRow : "", h("div.lk-field", pIn, go), msg,
      mode === "lock" ? h("button.lk-other", { type: "button", onclick: () => { form.replaceWith(lockCardSwap()); } }, t("login.other")) : "");
    const lockCardSwap = () => { done(); lockScreen("login").then(resolve); return h("div"); };
    form.onsubmit = async e => {
      e.preventDefault();
      msg.textContent = ""; go.disabled = true; go.innerHTML = icon("loader-circle", 18, 2.4); go.classList.add("busy");
      try {
        const r = await api("POST", "/auth/login", { user: mode === "lock" ? user : uIn.value.trim(), password: pIn.value });
        done(true); resolve(r);
      } catch (x) {
        msg.textContent = x instanceof ApiError && x.status === 401 ? t("login.bad") : x.message;
        animate(form, [{ transform: "translateX(0)" }, { transform: "translateX(-10px)" }, { transform: "translateX(10px)" }, { transform: "translateX(-6px)" }, { transform: "none" }], { duration: 360 });
        pIn.select();
      } finally { go.disabled = false; go.innerHTML = icon("chevron-right", 18, 2.4); go.classList.remove("busy"); }
    };
    const done = (anim) => {
      clearInterval(iv);
      if (anim) animate(root, [{ opacity: 1 }, { opacity: 0, transform: "scale(1.04)" }], { duration: 300 }).then(() => { root.hidden = true; root.replaceChildren(); });
      else { root.hidden = true; root.replaceChildren(); }
    };
    root.replaceChildren(h("div.lk-clock", timeEl, dateEl), form);
    root.hidden = false;
    animate(root, [{ opacity: 0 }, { opacity: 1 }], { duration: 260 });
    (showUser && !uIn.value ? uIn : pIn).focus();
  });
}
export const locked = () => !$("#lock").hidden;

// Onay: Promise<boolean>
export function confirmDialog(text, okText = t("ok"), danger = false) {
  return new Promise(res => {
    const host = $("#overlays");
    const close = v => { animate(wrap, [{ opacity: 1 }, { opacity: 0 }], { duration: 120 }).then(() => wrap.remove()); res(v); };
    const ok = h("button.btn" + (danger ? ".danger" : ".primary"), { onclick: () => close(true) }, okText);
    const wrap = h("div.dlg-wrap", { onkeydown: e => { if (e.key === "Escape") close(false); } },
      h("div.dlg.mat-pop", { role: "alertdialog" }, h("img", { src: brand(pref("look")).logo, alt: "", width: 48, height: 48 }), h("p", text),
        h("div.dlg-btns", h("button.btn", { onclick: () => close(false) }, t("cancel")), ok)));
    host.append(wrap);
    animate(wrap.firstChild, [{ opacity: 0, transform: "scale(.95)" }, { opacity: 1, transform: "none" }], { duration: 180 });
    ok.focus();
  });
}

// "Hakkında" kutusu
export function aboutDialog(app, sys) {
  const host = $("#overlays");
  const close = () => wrap.remove();
  const wrap = h("div.dlg-wrap", { onclick: e => { if (e.target === wrap) close(); }, onkeydown: e => { if (e.key === "Escape") close(); } },
    h("div.dlg.mat-pop", h("img", { src: app.icon, alt: "", width: 64, height: 64 }), h("b", app.name),
      h("p.muted", `${sys?.os?.name || "MakerOS"} ${sys?.os?.version || ""}`), h("p.muted", app.id),
      h("div.dlg-btns", h("button.btn.primary", { onclick: close }, t("ok")))));
  host.append(wrap); wrap.querySelector("button").focus();
}
