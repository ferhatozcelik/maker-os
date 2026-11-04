// İlk açılış sihirbazı: Hoş geldiniz (dil) → Bölge ve klavye → Wi-Fi (çevrimiçiyse atlanır) → Hesap →
// Bilgisayar adı → Gizlilik → Bitti. Pi HDMI'da (WPE) ve MakerOS erişim noktasına bağlı telefonda çalışır.
// API (docs/v2/api.md → "İlk açılış kurulumu"):
//   GET /setup → {hostname, defaults:{lang,keymap,timezone}, languages:[{id,name}], keymaps:[{id,name}], timezones[],
//                 online, wifi_needed, hotspot, min_password}
//   POST /setup/wifi/scan → /net/wifi/scan gövdesi (kimliksiz)
//   POST /setup/complete {user,password,fullname?,hostname,timezone,lang,keymap,wifi?} → {user,admin,job?,warnings?} + çerez; 409 = kurulu
// Metinler i18n/*.js içinde "su.*" anahtarlarıyla; dil seçilince sihirbaz anında o dile geçer.
import { h, $, store, fold, animate } from "../js/lib.js";
import { icon, wifiIcon } from "../js/icons.js";
import tr from "../i18n/tr.js";
import { api } from "../js/api.js";

const dicts = { tr };
let ui = "tr";
const t = (k, v) => { let s = dicts[ui]?.[k] ?? tr[k] ?? k; if (v) s = s.replace(/\{(\w+)\}/g, (_, x) => v[x] ?? ""); return s; };
const loc = () => (ui === "tr" ? "tr-TR" : "en-GB");
const uiOf = id => (String(id).toLowerCase().startsWith("tr") ? "tr" : "en");

// ------------------------------------------------------------ veri normalleştirme (dize veya {id,name})
const norm = x => (typeof x === "string" ? { id: x } : { ...x, id: x.id ?? x.code ?? x.name });
function langName(id) {
  const code = String(id).split(/[_.@-]/)[0];
  try { const own = new Intl.DisplayNames([code], { type: "language" }).of(code); if (own && own !== code) return own[0].toLocaleUpperCase(code) + own.slice(1); } catch { /* yok */ }
  return id;
}
const KEYMAPS = { tr: "Türkçe Q", "tr-f": "Türkçe F", trf: "Türkçe F", "tr_f": "Türkçe F", us: "ABD (US)", uk: "Birleşik Krallık (UK)", gb: "Birleşik Krallık (UK)",
  de: "Deutsch", fr: "Français", es: "Español", it: "Italiano", ru: "Русский", az: "Azərbaycan", dvorak: "Dvorak" };
const kmName = k => k.name || KEYMAPS[k.id] || String(k.id).toUpperCase();
const tzCity = z => String(z).split("/").pop().replace(/_/g, " ");
function tzOffset(z) {
  try { return new Intl.DateTimeFormat("en-US", { timeZone: z, timeZoneName: "shortOffset" }).formatToParts(new Date()).find(p => p.type === "timeZoneName")?.value.replace("GMT", "UTC") || ""; }
  catch { return ""; }
}
const tzTime = z => { try { return new Date().toLocaleTimeString(loc(), { timeZone: z, hour: "2-digit", minute: "2-digit" }); } catch { return ""; } };

// ------------------------------------------------------------ doğrulama
const RESERVED = new Set(["root", "bin", "daemon", "sys", "adm", "nobody", "makerd", "sshd", "messagebus", "systemd-network", "systemd-resolve", "avahi", "polkitd"]);
const COMMON = ["password", "parola", "sifre", "şifre", "123456", "12345678", "123456789", "qwerty", "qwertyui", "maker", "makeros", "raspberry", "admin", "letmein", "iloveyou", "111111", "000000", "abc123"];
export const userOk = u => /^[a-z_][a-z0-9_-]{0,31}$/.test(u) && !RESERVED.has(u);
export const hostOk = n => /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/.test(n);
export const slug = s => fold(s).replace(/[^a-z0-9]+/g, "").slice(0, 32);
// 0 çok zayıf … 4 güçlü (yalnız yönlendirme; en az 8 karakter zorunlu)
export function strength(pw, user = "", min = 8) {
  if (!pw) return 0;
  const low = pw.toLowerCase();
  if (pw.length < min) return 0;
  if (pw.length < 8 || COMMON.some(c => low.includes(c) && c.length >= pw.length - 2) || (user && low.includes(user.toLowerCase()))) return 1;
  const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^A-Za-z0-9]/].filter(r => r.test(pw)).length;
  let s = 1 + (pw.length >= 12) + (classes >= 3) + (pw.length >= 16 || (pw.length >= 12 && classes >= 3));
  if (/^(.)\1+$/.test(pw) || /^(?:0123|1234|2345|abcd|qwer)/i.test(pw)) s = 1;
  return Math.min(4, s);
}

// ------------------------------------------------------------ sihirbaz
export function runSetup(info) {
  return new Promise(done => {
    const L = info.languages || info.langs, D = info.defaults || {}, MIN = Math.max(1, info.min_password || 8);
    const langs = (L?.length ? L : ["tr_TR.UTF-8", "en_US.UTF-8"]).map(norm);
    const keymaps = (info.keymaps?.length ? info.keymaps : ["trq", "us"]).map(norm);
    const zones = (info.timezones?.length ? info.timezones : ["Europe/Istanbul", "UTC"]).map(z => norm(z).id);
    const here = (() => { try { return Intl.DateTimeFormat().resolvedOptions().timeZone; } catch { return ""; } })();
    const pickLang = langs.find(l => uiOf(l.id) === store.get("lang")) || langs.find(l => l.id === D.lang) || langs[0];
    const S = {
      lang: pickLang.id,
      timezone: zones.includes(here) ? here : zones.includes(D.timezone) ? D.timezone : zones.includes("Europe/Istanbul") ? "Europe/Istanbul" : zones[0],
      keymap: null, wifi: null, fullname: "", user: "", password: "", password2: "", hostname: info.hostname || "makeros", userEdited: false,
    };
    const kmFor = l => {
      if (l === D.lang && D.keymap && keymaps.some(k => k.id === D.keymap)) return D.keymap;
      const c = String(l).split(/[_.@-]/)[0], cand = c === "en" ? ["us", "gb", "uk"] : [c + "q", c];
      return (keymaps.find(k => cand.includes(k.id)) || keymaps.find(k => k.id.startsWith(c)) || keymaps[0]).id;
    };
    S.keymap = kmFor(S.lang);
    let networks = (info.wifi_networks || []).filter(n => n.ssid), scanned = !!info.wifi_networks;
    const scan = async () => { try { const r = await api("POST", "/setup/wifi/scan", {}, { timeout: 30000 }); if (Array.isArray(r)) networks = r.filter(n => n.ssid); } catch { /* yok */ } scanned = true; };
    const kiosk = /WPE|cog/i.test(navigator.userAgent);
    const steps = ["welcome", "region", ...(info.wifi_needed ? ["wifi"] : []), "account", "computer", "privacy", "done"];
    let idx = 0, busy = false;

    const root = h("div#setup", { role: "dialog", "aria-modal": "true" });
    const dots = h("div.su-dots", { "aria-hidden": "true" });
    const stage = h("div.su-stage");
    const back = h("button.btn.su-back", { type: "button", onclick: () => go(-1) });
    const next = h("button.btn.primary.su-next", { type: "submit" });
    const status = h("p.su-err", { role: "alert" });
    const form = h("form.su-card", { autocomplete: "off", novalidate: true, onsubmit: e => { e.preventDefault(); forward(); } },
      dots, stage, status, h("div.su-foot", back, h("span.su-step-n"), next));
    root.append(h("div.su-bg"), form);
    document.body.append(root);
    root.addEventListener("keydown", e => { if (e.key === "Escape" && idx > 0 && !busy && steps[idx] !== "done") { e.preventDefault(); go(-1); } });

    // Her adım: {icon, title, sub, body(): Node, valid(): string|"" (hata metni), next: etiket}
    const VIEWS = {
      welcome: () => {
        const hello = h("div.su-hello", "Merhaba");
        const greets = ["Merhaba", "Hello", "Hallo", "Bonjour", "Hola", "Ciao", "Salam", "Olá"];
        let gi = 0; const iv = setInterval(() => { if (!hello.isConnected) return clearInterval(iv); gi = (gi + 1) % greets.length;
          animate(hello, [{ opacity: 1 }, { opacity: 0, transform: "translateY(-6px)" }], { duration: 260, fill: "forwards" }).then(() => {
            hello.textContent = greets[gi]; animate(hello, [{ opacity: 0, transform: "translateY(6px)" }, { opacity: 1, transform: "none" }], { duration: 320, fill: "forwards" }); }); }, 2600);
        const list = h("div.su-list.su-langs", { role: "radiogroup", "aria-label": t("su.lang.label") }, ...langs.map(l => opt(l.name || langName(l.id), l.id === S.lang, async () => {
          S.lang = l.id; S.keymap = kmFor(l.id); await setUI(uiOf(l.id)); render(0, true);
        }, null, l.id)));
        return { hero: h("div.su-hero", h("img", { src: "/shell/icons/logo.svg", alt: "", width: 76, height: 76 }), hello), title: t("su.welcome.title"), sub: t("su.welcome.sub"), body: list };
      },
      region: () => {
        const q = h("input.su-search", { type: "search", placeholder: t("su.region.search"), "aria-label": t("su.region.search"), autocomplete: "off", spellcheck: "false" });
        const list = h("div.su-list.su-tz", { role: "radiogroup", "aria-label": t("su.region.tz") });
        const fill = () => {
          const f = fold(q.value.trim());
          const rows = zones.filter(z => !f || fold(z.replace(/_/g, " ")).includes(f)).slice(0, 80);
          list.replaceChildren(...rows.map(z => opt(tzCity(z), z === S.timezone, () => { S.timezone = z; fill(); clock(); }, `${z.split("/")[0]} · ${tzOffset(z)}`)));
          if (!rows.length) list.append(h("p.su-none", t("su.region.none")));
        };
        q.oninput = fill; fill();
        const now = h("span.su-now"), clock = () => { now.textContent = t("su.region.now", { time: tzTime(S.timezone), zone: tzCity(S.timezone) }); };
        clock();
        const km = h("select.su-select", { "aria-label": t("su.region.keymap"), onchange: e => { S.keymap = e.target.value; } },
          ...keymaps.map(k => h("option", { value: k.id, selected: k.id === S.keymap }, kmName(k))));
        return { icon: "globe", title: t("su.region.title"), sub: t("su.region.sub"),
          body: h("div.su-col", h("label.su-lbl", t("su.region.tz")), q, list, now, h("label.su-lbl", t("su.region.keymap")), km),
          focus: q, valid: () => (zones.includes(S.timezone) ? "" : t("su.region.pick")) };
      },
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
        return { icon: "wifi", title: t("su.wifi.title"), sub: t("su.wifi.sub"), body: h("div.su-col", list, hidRow, pwRow, h("div.su-row-end", rescan), note),
          valid: () => {
            if (!S.wifi) return "";
            if (!S.wifi.ssid?.trim()) return t("su.wifi.needSsid");
            const n = sel(); const needPw = S.wifi.hidden ? !!S.wifi.password : n && n.security !== "open";
            if (needPw && (S.wifi.password || "").length < 8 && n?.security !== "enterprise") return t("su.wifi.short");
            return "";
          } };
      },
      account: () => {
        const full = field(t("su.acc.full"), "text", S.fullname, v => { S.fullname = v; if (!S.userEdited) { S.user = slug(v.split(/\s+/)[0] || v); user.input.value = S.user; } upd(); }, { autocomplete: "name", autocapitalize: "words" });
        const user = field(t("su.acc.user"), "text", S.user, v => { S.user = v.toLowerCase(); S.userEdited = !!v; user.input.value = S.user; upd(); }, { autocomplete: "username", autocapitalize: "none", spellcheck: "false" });
        const pw = field(t("su.acc.pass"), "password", S.password, v => { S.password = v; upd(); }, { autocomplete: "new-password" });
        const pw2 = field(t("su.acc.again"), "password", S.password2, v => { S.password2 = v; upd(); }, { autocomplete: "new-password" });
        const eye = h("button.su-eye", { type: "button", "aria-label": t("su.acc.show"), title: t("su.acc.show"), html: icon("eye", 16), onclick: () => {
          const on = pw.input.type === "password"; pw.input.type = pw2.input.type = on ? "text" : "password"; eye.innerHTML = icon(on ? "eye-off" : "eye", 16); } });
        pw.wrap.append(eye);
        const meter = h("div.su-meter", h("i"), h("i"), h("i"), h("i")), mtext = h("span.su-mtext");
        const uhint = h("small.su-hint"), mhint = h("small.su-hint");
        const upd = () => {
          const s = strength(S.password, S.user, MIN);
          meter.dataset.s = S.password ? s : "";
          mtext.textContent = S.password ? t("su.acc.s" + s) : t("su.acc.min", { n: MIN });
          uhint.textContent = S.user && !userOk(S.user) ? t("su.acc.badUser") : S.user ? t("su.acc.home", { user: S.user }) : "";
          uhint.classList.toggle("bad", !!S.user && !userOk(S.user));
          mhint.textContent = S.password2 && S.password2 !== S.password ? t("su.acc.mismatch") : "";
          mhint.classList.toggle("bad", !!mhint.textContent);
        };
        upd();
        return { icon: "user", title: t("su.acc.title"), sub: t("su.acc.sub"),
          body: h("div.su-col", full.wrap, user.wrap, uhint, pw.wrap, h("div.su-strength", meter, mtext), pw2.wrap, mhint),
          focus: full.input,
          valid: () => !userOk(S.user) ? t("su.acc.badUser") : S.password.length < MIN ? t("su.acc.min", { n: MIN })
            : S.password.toLowerCase() === S.user ? t("su.acc.same") : S.password !== S.password2 ? t("su.acc.mismatch") : "" };
      },
      computer: () => {
        const name = field(t("su.pc.name"), "text", S.hostname, v => { S.hostname = v.toLowerCase().replace(/\s+/g, "-"); name.input.value = S.hostname; upd(); },
          { autocapitalize: "none", spellcheck: "false", maxlength: 63 });
        name.wrap.append(h("span.su-suffix", ".local"));
        const hint = h("small.su-hint"), upd = () => {
          hint.textContent = hostOk(S.hostname) ? t("su.pc.url", { host: S.hostname }) : t("su.pc.bad");
          hint.classList.toggle("bad", !hostOk(S.hostname));
        };
        upd();
        return { icon: "monitor", title: t("su.pc.title"), sub: t("su.pc.sub"), body: h("div.su-col", name.wrap, hint), focus: name.input,
          valid: () => (hostOk(S.hostname) ? "" : t("su.pc.bad")) };
      },
      privacy: () => ({ icon: "shield", title: t("su.priv.title"), sub: t("su.priv.sub"), next: t("su.priv.go"),
        body: h("ul.su-points", ...[["eye-off", "1"], ["hard-drive", "2"], ["network", "3"], ["user", "4"]].map(([ic, n]) =>
          h("li", h("span.su-pi", { html: icon(ic, 18) }), h("div", h("b", t("su.priv.t" + n)), h("p", t("su.priv.d" + n)))))) }),
      done: () => {
        const name = S.fullname.trim().split(/\s+/)[0] || S.user;
        const rows = [[t("su.pc.title"), `${S.hostname}.local`], [t("su.acc.user"), S.user], [t("su.region.tz"), tzCity(S.timezone)], ...(S.wifi ? [["Wi-Fi", S.wifi.ssid]] : [])];
        return { hero: h("div.su-hero.su-ok", h("span.su-check", { html: icon("check", 40, 2.6) })), title: t("su.done.title", { name }), sub: t("su.done.sub"), next: t("su.done.go"),
          body: h("div.su-col", h("div.su-summary", ...rows.map(([k, v]) => h("div", h("span", k), h("b", v)))),
            ...(S.warnings || []).map(w => h("p.su-note.su-warn", { html: icon("circle-alert", 14) }, h("span", w))),
            S.wifi && !kiosk ? h("p.su-note", { html: icon("info", 14) + `<span>${t("su.done.wifi", { ssid: S.wifi.ssid, host: S.hostname })}</span>` }) : "") };
      },
    };

    function opt(label, on, pick, sub, value, extra) {
      return h("button.su-opt" + (on ? ".on" : ""), { type: "button", role: "radio", "aria-checked": String(on), "data-v": value, onclick: pick },
        h("span.su-ol", h("b", label), sub ? h("small", sub) : ""), extra || "", h("span.su-tick", { html: on ? icon("check", 14, 2.6) : "" }));
    }
    function field(label, type, value, onInput, attrs = {}) {
      const input = h("input", { type, value, ...attrs });
      input.oninput = () => { onInput(input.value); status.textContent = ""; };
      return { input, wrap: h("label.su-field", h("span", label), input) };
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
      const n = steps.length - 1; // "done" sayılmaz
      dots.replaceChildren(...steps.slice(0, n).map((_, i) => h("i" + (i < idx ? ".past" : i === idx ? ".cur" : ""))));
      dots.hidden = name === "done";
      back.textContent = t("su.back"); back.hidden = idx === 0 || name === "done";
      next.textContent = v.next || t("su.next"); next.disabled = false;
      form.querySelector(".su-step-n").textContent = name === "done" ? "" : t("su.stepOf", { n: idx + 1, total: n });
      status.textContent = "";
      form.dataset.step = name;
      if (dir || !keepScroll) setTimeout(() => (v.focus || (name === "done" || name === "privacy" ? next : el.querySelector(".su-opt.on") || next)).focus({ preventScroll: true }), dir ? 320 : 0);
    }
    function go(d) { if (busy) return; idx = Math.max(0, Math.min(steps.length - 1, idx + d)); render(d); }
    async function forward() {
      if (busy) return;
      const name = steps[idx];
      if (name === "done") { animate(root, [{ opacity: 1 }, { opacity: 0, transform: "scale(1.03)" }], { duration: 320 }).then(() => { root.remove(); done(S); }); return; }
      const err = cur.valid();
      if (err) { status.textContent = err; animate(form, [{ transform: "translateX(0)" }, { transform: "translateX(-8px)" }, { transform: "translateX(8px)" }, { transform: "none" }], { duration: 280 }); return; }
      if (name === "privacy") return complete();
      go(1);
    }
    async function complete() {
      busy = true; next.disabled = back.disabled = true; next.innerHTML = icon("loader-circle", 15, 2.2) + t("su.working"); next.classList.add("busy");
      const body = { user: S.user, password: S.password, fullname: S.fullname.trim() || undefined, hostname: S.hostname, timezone: S.timezone, lang: S.lang, keymap: S.keymap };
      if (S.wifi?.ssid) body.wifi = { ssid: S.wifi.ssid.trim(), ...(S.wifi.password ? { password: S.wifi.password } : {}), ...(S.wifi.hidden ? { hidden: true } : {}) };
      try {
        const r = await api("POST", "/setup/complete", body, { timeout: 60000 });
        S.warnings = Array.isArray(r?.warnings) ? r.warnings : [];
        busy = false; back.disabled = false; next.classList.remove("busy"); go(1);
      } catch (e) {
        busy = false; next.disabled = back.disabled = false; next.classList.remove("busy"); next.textContent = t("su.priv.go");
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
