// İlk açılış sihirbazı: yalnız Wi-Fi (atlanabilir) → Bitti. Hesap, dil, saat dilimi ve cihaz adı
// varsayılanlarla kurulur; hepsi sonradan Ayarlar'dan değiştirilir. Pi HDMI'da (WPE) otomatik oturum
// açılır; ağdan erişimde varsayılan kullanıcı/parola (DEFAULT_USER) kullanılır.
// API (docs/v2/api.md → "İlk açılış kurulumu"):
//   GET /setup → {hostname, defaults:{lang,keymap,timezone}, languages, keymaps, timezones[], online, wifi_needed, hotspot}
//   POST /setup/wifi/scan → /net/wifi/scan gövdesi (kimliksiz)
//   POST /setup/complete {user,password,fullname?,hostname,timezone,lang,keymap,wifi?} → {user,admin,job?,warnings?} + çerez; 409 = kurulu
// Metinler i18n/*.js içinde "su.*" anahtarlarıyla.
import { h, store, animate } from "../js/lib.js";
import { icon, wifiIcon } from "../js/icons.js";
import tr from "../i18n/tr.js";
import { api } from "../js/api.js";

// Varsayılan yönetici hesabı ('maker' makerd hizmet hesabıdır, kullanılamaz).
export const DEFAULT_USER = { user: "makeros", password: "makeros", fullname: "MakerOS" };

const dicts = { tr };
let ui = "tr";
const t = (k, v) => { let s = dicts[ui]?.[k] ?? tr[k] ?? k; if (v) s = s.replace(/\{(\w+)\}/g, (_, x) => v[x] ?? ""); return s; };
const uiOf = id => (String(id).toLowerCase().startsWith("tr") ? "tr" : "en");
const norm = x => (typeof x === "string" ? { id: x } : { ...x, id: x.id ?? x.code ?? x.name });

// ------------------------------------------------------------ sihirbaz
export function runSetup(info) {
  return new Promise(done => {
    const L = info.languages || info.langs, D = info.defaults || {};
    const langs = (L?.length ? L : ["tr_TR.UTF-8", "en_US.UTF-8"]).map(norm);
    const keymaps = (info.keymaps?.length ? info.keymaps : ["trq", "us"]).map(norm);
    const zones = (info.timezones?.length ? info.timezones : ["Europe/Istanbul", "UTC"]).map(z => norm(z).id);
    const here = (() => { try { return Intl.DateTimeFormat().resolvedOptions().timeZone; } catch { return ""; } })();
    const lang = (langs.find(l => uiOf(l.id) === store.get("lang")) || langs.find(l => l.id === D.lang) || langs[0]).id;
    const kmFor = l => {
      if (l === D.lang && D.keymap && keymaps.some(k => k.id === D.keymap)) return D.keymap;
      const c = String(l).split(/[_.@-]/)[0], cand = c === "en" ? ["us", "gb", "uk"] : [c + "q", c];
      return (keymaps.find(k => cand.includes(k.id)) || keymaps.find(k => k.id.startsWith(c)) || keymaps[0]).id;
    };
    const S = {
      ...DEFAULT_USER, lang, keymap: kmFor(lang),
      timezone: zones.includes(here) ? here : zones.includes(D.timezone) ? D.timezone : zones.includes("Europe/Istanbul") ? "Europe/Istanbul" : zones[0],
      hostname: info.hostname || "makeros", wifi: null,
    };
    let networks = (info.wifi_networks || []).filter(n => n.ssid), scanned = !!info.wifi_networks;
    const scan = async () => { try { const r = await api("POST", "/setup/wifi/scan", {}, { timeout: 30000 }); if (Array.isArray(r)) networks = r.filter(n => n.ssid); } catch { /* yok */ } scanned = true; };
    const kiosk = /WPE|cog/i.test(navigator.userAgent);
    const steps = ["wifi", "done"];
    let idx = 0, busy = false;

    const root = h("div#setup", { role: "dialog", "aria-modal": "true" });
    const stage = h("div.su-stage");
    const next = h("button.btn.primary.su-next", { type: "submit" });
    const status = h("p.su-err", { role: "alert" });
    const form = h("form.su-card", { autocomplete: "off", novalidate: true, onsubmit: e => { e.preventDefault(); forward(); } },
      stage, status, h("div.su-foot", h("span"), h("span.su-step-n"), next));
    root.append(h("div.su-bg"), form);
    document.body.append(root);

    // Her adım: {icon, title, sub, body(): Node, valid(): string|"" (hata metni), next: etiket}
    const VIEWS = {
      wifi: () => {
        const list = h("div.su-list.su-nets", { role: "radiogroup", "aria-label": "Wi-Fi" });
        const pw = h("input", { type: "password", placeholder: t("su.wifi.pass"), "aria-label": t("su.wifi.pass"), autocomplete: "off", value: S.wifi?.password || "" });
        const hidden = h("input", { placeholder: t("su.wifi.ssid"), "aria-label": t("su.wifi.ssid"), autocomplete: "off", spellcheck: "false", value: S.wifi?.hidden ? S.wifi.ssid : "" });
        const pwRow = h("div.su-col.su-pw", h("label.su-lbl"), pw);
        const hidRow = h("div.su-col", h("label.su-lbl", t("su.wifi.ssid")), hidden);
        const sel = () => networks.find(n => n.ssid === S.wifi?.ssid);
        const sync = () => {
          const n = sel(), isHidden = S.wifi?.hidden;
          hidRow.hidden = !isHidden;
          pwRow.hidden = !S.wifi || (n && n.security === "open");
          pwRow.firstChild.textContent = t("su.wifi.passFor", { ssid: isHidden ? hidden.value || "…" : S.wifi?.ssid || "" });
          note.hidden = !S.wifi || kiosk;
          next.textContent = S.wifi ? t("su.wifi.connect") : t("su.wifi.skipGo");
        };
        const fill = () => list.replaceChildren(
          ...networks.sort((a, b) => b.signal - a.signal).map(n => opt(n.ssid, S.wifi?.ssid === n.ssid && !S.wifi.hidden, () => { S.wifi = { ssid: n.ssid, password: "" }; pw.value = ""; fill(); sync(); pw.focus(); },
            null, null, h("span.su-sig", { html: (n.security !== "open" ? icon("lock", 12, 2) : "") + icon(wifiIcon(n.signal), 16, 2) }))),
          opt(t("su.wifi.other"), !!S.wifi?.hidden, () => { S.wifi = { ssid: hidden.value, password: pw.value, hidden: true }; fill(); sync(); hidden.focus(); }, null, null, h("span.su-sig", { html: icon("plus", 15, 2) })),
          opt(t("su.wifi.skip"), !S.wifi, () => { S.wifi = null; fill(); sync(); }, t("su.wifi.skipSub")));
        if (!networks.length && scanned) list.prepend(h("p.su-none", t("su.wifi.none")));
        pw.oninput = () => { if (S.wifi) S.wifi.password = pw.value; status.textContent = ""; };
        hidden.oninput = () => { if (S.wifi) S.wifi.ssid = hidden.value; sync(); };
        const rescan = h("button.link.su-rescan", { type: "button", onclick: async () => {
          rescan.disabled = true; rescan.innerHTML = icon("loader-circle", 13, 2) + t("su.wifi.scanning");
          await scan();
          rescan.disabled = false; rescan.innerHTML = icon("refresh-cw", 13, 2) + t("su.wifi.rescan"); fill(); sync();
        }, html: icon("refresh-cw", 13, 2) + t("su.wifi.rescan") });
        const note = h("p.su-note", { html: icon("info", 14) + `<span>${t("su.wifi.hotspot", { host: S.hostname })}</span>` });
        fill(); sync();
        if (!scanned) { rescan.click(); }
        return { icon: "wifi", title: t("su.wifi.title"), sub: t("su.wifi.sub"), next: S.wifi ? t("su.wifi.connect") : t("su.wifi.skipGo"), body: h("div.su-col", list, hidRow, pwRow, h("div.su-row-end", rescan), note),
          valid: () => {
            if (!S.wifi) return "";
            if (!S.wifi.ssid?.trim()) return t("su.wifi.needSsid");
            const n = sel(); const needPw = S.wifi.hidden ? !!S.wifi.password : n && n.security !== "open";
            if (needPw && (S.wifi.password || "").length < 8 && n?.security !== "enterprise") return t("su.wifi.short");
            return "";
          } };
      },
      done: () => {
        const rows = [[t("su.done.addr"), `${S.hostname}.local`], [t("su.done.user"), S.user], [t("su.done.pass"), S.password], ...(S.wifi ? [["Wi-Fi", S.wifi.ssid]] : [])];
        return { hero: h("div.su-hero.su-ok", h("span.su-check", { html: icon("check", 40, 2.6) })), title: t("su.done.title"), sub: t("su.done.sub"), next: t("su.done.go"),
          body: h("div.su-col", h("div.su-summary", ...rows.map(([k, v]) => h("div", h("span", k), h("b", v)))),
            h("p.su-note", { html: icon("shield", 14) + `<span>${t("su.done.change")}</span>` }),
            ...(S.warnings || []).map(w => h("p.su-note.su-warn", { html: icon("circle-alert", 14) }, h("span", w))),
            S.wifi && !kiosk ? h("p.su-note", { html: icon("info", 14) + `<span>${t("su.done.wifi", { ssid: S.wifi.ssid, host: S.hostname })}</span>` }) : "") };
      },
    };

    function opt(label, on, pick, sub, value, extra) {
      return h("button.su-opt" + (on ? ".on" : ""), { type: "button", role: "radio", "aria-checked": String(on), "data-v": value, onclick: pick },
        h("span.su-ol", h("b", label), sub ? h("small", sub) : ""), extra || "", h("span.su-tick", { html: on ? icon("check", 14, 2.6) : "" }));
    }
    async function setUI(l) {
      if (l === ui) return;
      if (!dicts[l]) dicts[l] = (await import(`../i18n/${l}.js`)).default;
      ui = l; store.set("lang", l); document.documentElement.lang = l;
    }

    let cur = null;
    function render(dir = 0, keepScroll = false) {
      const name = steps[idx], v = VIEWS[name]();
      const scroll = keepScroll && cur?.querySelector(".su-list")?.scrollTop;
      const el = h("div.su-page", { "data-step": name },
        v.hero || h("div.su-icon", { html: icon(v.icon, 30, 1.8) }),
        h("h1", v.title), h("p.su-sub", v.sub), v.body);
      el.valid = v.valid || (() => "");
      const old = cur; cur = el;
      stage.append(el);
      if (scroll) { const l = el.querySelector(".su-list"); if (l) l.scrollTop = scroll; }
      if (old) {
        if (dir) {
          animate(old, [{ opacity: 1, transform: "none" }, { opacity: 0, transform: `translateX(${-40 * dir}px)` }], { duration: 200, easing: "ease-in" }).then(() => old.remove());
          animate(el, [{ opacity: 0, transform: `translateX(${40 * dir}px)` }, { opacity: 1, transform: "none" }], { duration: 300, delay: 60, fill: "backwards" });
        } else old.remove();
      }
      next.textContent = v.next || t("su.next"); next.disabled = false;
      status.textContent = "";
      form.dataset.step = name;
      if (dir || !keepScroll) setTimeout(() => (v.focus || (name === "done" ? next : el.querySelector(".su-opt.on") || next)).focus({ preventScroll: true }), dir ? 320 : 0);
    }
    function go(d) { if (busy) return; idx = Math.max(0, Math.min(steps.length - 1, idx + d)); render(d); }
    async function forward() {
      if (busy) return;
      const name = steps[idx];
      if (name === "done") { animate(root, [{ opacity: 1 }, { opacity: 0, transform: "scale(1.03)" }], { duration: 320 }).then(() => { root.remove(); done(S); }); return; }
      const err = cur.valid();
      if (err) { status.textContent = err; animate(form, [{ transform: "translateX(0)" }, { transform: "translateX(-8px)" }, { transform: "translateX(8px)" }, { transform: "none" }], { duration: 280 }); return; }
      complete();
    }
    async function complete() {
      busy = true; next.disabled = true; next.innerHTML = icon("loader-circle", 15, 2.2) + t("su.working"); next.classList.add("busy");
      const body = { user: S.user, password: S.password, fullname: S.fullname, hostname: S.hostname, timezone: S.timezone, lang: S.lang, keymap: S.keymap };
      if (S.wifi?.ssid) body.wifi = { ssid: S.wifi.ssid.trim(), ...(S.wifi.password ? { password: S.wifi.password } : {}), ...(S.wifi.hidden ? { hidden: true } : {}) };
      try {
        const r = await api("POST", "/setup/complete", body, { timeout: 60000 });
        S.warnings = Array.isArray(r?.warnings) ? r.warnings : [];
        busy = false; next.classList.remove("busy"); go(1);
      } catch (e) {
        busy = false; next.disabled = false; next.classList.remove("busy"); next.textContent = S.wifi ? t("su.wifi.connect") : t("su.wifi.skipGo");
        if (e.status === 409) { status.textContent = t("su.already"); next.textContent = t("su.toLogin"); next.onclick = ev => { ev.preventDefault(); root.remove(); done(null); }; return; }
        status.textContent = e.message || t("su.fail");
      }
    }

    setUI(uiOf(S.lang)).then(() => {
      render();
      animate(form, [{ opacity: 0, transform: "translateY(16px) scale(.98)" }, { opacity: 1, transform: "none" }], { duration: 420 });
    });
  });
}
