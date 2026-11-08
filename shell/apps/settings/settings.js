// Sistem Ayarları: kenar çubuğu + sayfalar. Derin bağlantı: #wifi, #keyboard ...
import { app, t } from "/shell/js/app-kit.js";
import { h, $, bytes, duration, store, fold } from "/shell/js/lib.js";
import { icon, wifiIcon } from "/shell/js/icons.js";
import { api, get, post, put } from "/shell/js/api.js";
import { SETTINGS_PAGES, APPS, loadApps } from "/shell/js/apps.js";
import { pref, setPref, flushPrefs, isDark } from "/shell/js/prefs.js";
import { THEMES, wallURL, registerPluginThemes, themeName, themeDesc } from "/shell/js/themes.js";
import { SHORTCUTS, macLabel, pcLabel } from "/shell/js/keys.js";
import { lang, locale } from "/shell/js/i18n.js";
import { confirmDlg, toast } from "/shell/js/appui.js";
import { chooseWallpaper } from "/shell/js/wallpaper-upload.js";

app.init({ title: t("app.settings") });
app.chrome({ sidebar: 220, title: false });
const COLORS = { wifi: "#1a7cf5", network: "#1a7cf5", general: "#8a8a90", datetime: "#3a86ff", language: "#3a86ff", keyboard: "#8a8a90", appearance: "#2b2b30", startup: "#2fa84f", user: "#9b55e0", power: "#f08a1c" };
const split = $(".split"), side = $(".sidebar"), content = $(".content");
let cur = null, cleanup = [];

const row = (title, sub, right, o = {}) => h("div.row" + (o.click ? ".click" : ""), { onclick: o.click },
  o.ic ? h("span", { html: o.ic }) : "", h("div.grow", h("div.t", title), sub ? h("div.s", sub) : ""), right ?? "");
const val = v => h("span.v", v ?? "—");
const group = (...rows) => h("div.group", ...rows.filter(Boolean));
const btn = (label, onclick, cls = "") => h("button.btn" + cls, { onclick }, label);
const sw = (checked, onchange) => h("input.switch", { type: "checkbox", checked, onchange: e => onchange(e.target.checked) });
const seg = (opts, value, onpick) => h("div.seg", ...opts.map(([v, l]) => h("button" + (v === value ? ".on" : ""), { onclick: e => { e.currentTarget.parentNode.querySelectorAll("button").forEach(b => b.classList.toggle("on", b === e.currentTarget)); onpick(v); } }, l)));
const note = (text, kind = "") => h("p.hint" + (kind ? "." + kind : ""), text);
function flash(el, text, ok = true) { el.textContent = text; el.className = "hint " + (ok ? "" : "err"); }
const tell = m => app.embedded ? parent.postMessage(m, location.origin) : null;

// ------------------------------------------------------------ yazılım güncellemesi
// GitHub Releases'teki imzalı RAUC paketi → boştaki A/B bölümüne kurulum → deneme açılışı (makerd internal/ota).
function updates() {
  const box = h("div.group"), notes = h("div.up-notes"), msg = h("p.hint");
  const bar = h("div.up-bar", h("i")), wrap = h("div", box, notes, msg);
  let st = null, admin = false, busy = false;
  const fmtDate = d => new Date(d).toLocaleDateString(locale, { dateStyle: "medium" });
  const running = () => st && ["downloading", "verifying", "installing", "rebooting"].includes(st.phase);
  const render = () => {
    if (!st) return box.replaceChildren(h("div.empty", t("loading")));
    const av = st.available;
    const sub = running() ? st.message || t("set.up.working")
      : st.error ? st.error
      : av ? t("set.up.available", { v: av.version, d: fmtDate(av.published) })
      : st.checked ? t("set.up.latest") : t("set.up.never");
    const check = btn(t("set.up.check"), () => check_(true), ".sm");
    check.disabled = busy || running();
    const apply = av && admin && st.can_apply ? btn(t("set.up.apply", { v: av.version }), apply_, ".sm.primary") : "";
    if (apply) apply.disabled = busy || running();
    box.replaceChildren(
      row(`MakerOS ${st.current}`, sub, h("span", { style: { display: "flex", gap: "6px" } }, check, apply), { ic: icon(av ? "download" : "check", 18, 2) }),
      running() ? h("div.row", h("div.grow", bar)) : "");
    bar.firstChild.style.width = (st.progress || 0) + "%";
    notes.replaceChildren(...(av && !running() ? [h("div.up-h", av.name || "MakerOS " + av.version, " · ", h("a", { href: av.url, target: "_blank", rel: "noopener" }, t("set.up.notes"))),
      h("pre.up-txt", (av.notes || "").trim() || "—")] : []));
    msg.textContent = av && !admin ? t("set.up.adminOnly") : av && !st.can_apply ? t("set.up.cannot") : st.phase === "rebooting" ? t("set.up.rebooting") : "";
  };
  const load = async () => { st = await get("/system/update"); render(); return st; };
  const check_ = async manual => {
    busy = true; render();
    try { st = await post("/system/update/check"); } catch (e) { if (manual) flash(msg, e.message, false); await load().catch(() => {}); }
    busy = false; render();
  };
  async function apply_() {
    const av = st.available;
    if (!await confirmDlg(t("set.up.confirmT", { v: av.version }), t("set.up.confirm"), t("set.up.applyShort"))) return;
    busy = true; render();
    try { await post("/system/update/apply"); await load(); } catch (e) { flash(msg, e.message, false); }
    busy = false; render();
  }
  // İlerleme: iş olayları (jobs) gelince durum yeniden okunur.
  cleanup.push(app.events.subscribe("jobs", () => { if (st?.job || running()) load().catch(() => {}); }));
  const iv = setInterval(() => { if (running()) load().catch(() => {}); }, 2000); cleanup.push(() => clearInterval(iv));
  Promise.all([load(), get("/session").then(s => { admin = !!s.admin; })]).then(() => { render(); if (!st.checked) check_(false); })
    .catch(e => box.replaceChildren(h("div.empty.err", e.message)));
  render();
  return wrap;
}

// ------------------------------------------------------------ sayfalar
const PAGES = {
  async wifi(el) {
    const list = h("div.group", h("div.empty", t("loading")));
    const status = h("p.hint");
    let nets = [], job = null, sel = null, busySsid = null, formErr = "";
    // Bağlanırken: düğme dönen simge + "Bağlanılıyor…", ağ satırında durum; sonuç bildirim (toast) + satır içi hata.
    const connect = async (n, extra = {}) => {
      busySsid = n.ssid; formErr = ""; render();
      try {
        const r = await post("/net/wifi/connect", { ssid: n.ssid, ...extra });
        job = r?.job; flash(status, t("set.wifi.connecting", { ssid: n.ssid }));
        if (!job) { busySsid = null; toast(t("set.wifi.ok") + ": " + n.ssid); sel = null; scan(); return; }
        // WS kaçırırsa: GET /jobs/{id} ile yokla
        const poll = setInterval(async () => { if (!job) return clearInterval(poll); try { onJob(await get(`/jobs/${encodeURIComponent(job)}`)); } catch { /* yok */ } }, 2500);
        cleanup.push(() => clearInterval(poll));
      } catch (e) { busySsid = null; formErr = e.message; flash(status, e.message, false); toast(e.message); render(); }
    };
    const form = n => {
      const ent = n.security === "enterprise";
      const id = h("input", { placeholder: t("set.wifi.identity"), autocapitalize: "none" }), pw = h("input", { type: "password", placeholder: t("login.pass") });
      const go = () => connect(n, ent ? { eap: { identity: id.value, password: pw.value, method: "peap" } } : { password: pw.value });
      pw.onkeydown = e => { if (e.key === "Enter") go(); };
      setTimeout(() => (ent ? id : pw).focus());
      const busy = busySsid === n.ssid;
      const b = h("button.btn.primary" + (busy ? ".busy" : ""), { onclick: go, disabled: busy },
        ...(busy ? [h("span", { html: icon("loader-circle", 14), style: { display: "inline-flex", marginRight: "6px" } }), t("set.wifi.connectingShort")] : [t("connect")]));
      return h("div.row", h("div.grow", { style: { display: "grid", gap: "6px" } }, ent ? id : "", pw,
        formErr ? h("p.hint.err", { style: { margin: 0 } }, formErr) : ""), b);
    };
    const render = () => {
      if (!nets.length) return list.replaceChildren(h("div.empty", t("set.wifi.none")));
      list.replaceChildren(...nets.flatMap(n => {
        const r = row(n.ssid, [busySsid === n.ssid ? t("set.wifi.connectingShort") : n.active ? t("set.wifi.connected") : n.known ? t("set.wifi.known") : "", n.security === "open" ? t("set.wifi.open") : n.security.toUpperCase()].filter(Boolean).join(" · "),
          h("span", { style: { display: "flex", gap: "8px", alignItems: "center" } },
            n.known ? h("button.btn.sm", { onclick: e => { e.stopPropagation(); post("/net/wifi/forget", { ssid: n.ssid }).then(scan).catch(x => flash(status, x.message, false)); } }, t("forget")) : "",
            n.security !== "open" ? h("span.muted", { html: icon("lock", 13) }) : "", h("span", { html: icon(wifiIcon(n.signal), 17) })),
          { click: n.active ? null : () => { if (n.known || n.security === "open") connect(n); else { sel = sel === n.ssid ? null : n.ssid; render(); } }, ic: n.active ? icon("check", 16, 2.4) : `<span style="display:inline-block;width:16px"></span>` });
        return sel === n.ssid ? [r, form(n)] : [r];
      }));
    };
    const scan = async () => {
      rescan.disabled = true; rescan.innerHTML = icon("loader-circle", 14) ; rescan.classList.add("busy");
      try { nets = (await post("/net/wifi/scan")).sort((a, b) => b.active - a.active || b.known - a.known || b.signal - a.signal); render(); }
      catch (e) { list.replaceChildren(h("div.empty.err", e.message)); }
      finally { rescan.disabled = false; rescan.classList.remove("busy"); rescan.textContent = t("set.wifi.scan"); }
    };
    const rescan = btn(t("set.wifi.scan"), () => scan(), ".sm");
    const hidden = () => {
      const s = h("input", { placeholder: "SSID" }), p = h("input", { type: "password", placeholder: t("login.pass") });
      other.replaceWith(group(h("div.row", h("div.grow", { style: { display: "grid", gap: "6px" } }, s, p), btn(t("connect"), () => connect({ ssid: s.value }, { password: p.value || undefined, hidden: true }), ".primary"))));
      s.focus();
    };
    const other = h("button.link", { onclick: hidden }, t("set.wifi.other"));
    el.append(h("div.page-h", h("h2", { style: { margin: 0, flex: 1 } }, t("set.wifi.networks")), rescan), status, list, other);
    const onJob = j => {
      if (!j || j.id !== job) return;
      if (j.state === "running") flash(status, `${j.log || ""} %${j.progress}`);
      else {
        const ok = j.state === "done", ssid = busySsid, m = j.log || (ok ? t("set.wifi.ok") : t("error"));
        flash(status, m, ok); job = null; busySsid = null;
        toast(ok ? `${t("set.wifi.ok")}: ${ssid}` : m);
        if (ok) { sel = null; formErr = ""; } else formErr = m; // hata: şifre kutusu açık kalır, altında neden yazar
        render(); scan();
      }
    };
    cleanup.push(app.events.subscribe("jobs", onJob));
    scan();
  },

  async network(el) {
    const n = await get("/net");
    el.append(group(row(t("set.net.online"), null, h("span.badge" + (n.online ? ".ok" : ".bad"), n.online ? t("set.net.yes") : t("set.net.no")))));
    if (!n.ifaces?.length) {
      el.append(group(row(t("set.net.ip"), null, val("—"))));
    }
    for (const i of n.ifaces || []) {
      const up = i.state === "connected";
      el.append(h("h2", `${i.type === "wifi" ? "Wi-Fi" : "Ethernet"} · ${i.name}`), group(
        row(t("set.net.state"), null, h("span.badge" + (up ? ".ok" : ""), up ? t("set.wifi.connected") : i.state)),
        i.ssid ? row("SSID", null, val(i.ssid)) : null,
        row(t("set.net.ip"), null, val(i.ipv4?.length ? i.ipv4.join(", ") : "—")),
        row("MAC", null, val(i.mac)),
        i.speed_mbps ? row(t("set.net.speed"), null, val(i.speed_mbps + " Mb/sn")) : null));
    }
    el.append(h("h2", t("set.net.hotspot")), group(row(n.hotspot?.ssid || "—", t("set.net.hotspotHint"), h("span.badge" + (n.hotspot?.active ? ".ok" : ""), n.hotspot?.active ? t("on") : t("off")))));
  },

  async general(el) {
    const s = await get("/system");
    const hn = h("input", { value: s.hostname, style: { width: "160px" }, autocapitalize: "none", spellcheck: "false" }), msg = h("p.hint");
    const save = async () => { try { await put("/system/hostname", { hostname: hn.value.trim() }); flash(msg, t("set.saved")); } catch (e) { flash(msg, e.message, false); } };
    const pct = (u, tot) => h("div", { style: { display: "grid", gap: "4px", minWidth: "160px" } }, h("span.v", `${bytes(u)} / ${bytes(tot)}`),
      h("div", { style: { height: "5px", borderRadius: "3px", background: "var(--bg-active)", overflow: "hidden" } }, h("div", { style: { height: "100%", width: (100 * u / tot).toFixed(1) + "%", background: "var(--accent)" } })));
    el.append(
      h("div.about", h("img", { src: "/shell/icons/logo.svg", width: 72, height: 72, alt: "" }), h("div", h("h1", `${s.os.name} ${s.os.version}`), h("div.muted", s.model))),
      group(row(t("set.gen.name"), `${s.hostname}.local`, h("span", { style: { display: "flex", gap: "6px" } }, hn, btn(t("save"), save, ".sm")))), msg,
      h("h2", t("set.up.title")), updates(),
      group(row(t("set.gen.model"), null, val(s.model)), row(t("set.gen.os"), null, val(`${s.os.name} ${s.os.version}`)), row(t("set.gen.kernel"), null, val(s.kernel)),
        row(t("set.gen.uptime"), null, val(duration(s.uptime_s)))),
      group(row(t("set.gen.cpu"), `${s.cpu.cores} ${t("set.gen.cores")} · ${s.cpu.freq_mhz} MHz`, val(`${s.cpu.temp_c} °C`)),
        row(t("set.gen.mem"), s.mem.zram_used ? `zram ${bytes(s.mem.zram_used)}` : null, pct(s.mem.used, s.mem.total)), row(t("set.gen.disk"), null, pct(s.disk.used, s.disk.total)),
        row(t("set.gen.power"), null, h("span.badge" + (s.throttled ? ".bad" : ".ok"), s.throttled ? t("set.gen.throttled") : t("set.gen.okPower")))),
      h("p.hint", t("set.gen.legal")));
  },

  async datetime(el) {
    const [tm, zones] = await Promise.all([get("/system/time"), get("/system/timezones").catch(() => [])]);
    const now = h("div.n"), msg = h("p.hint");
    const tick = () => { now.textContent = new Date().toLocaleString(locale, { timeZone: tm.timezone, dateStyle: "full", timeStyle: "medium" }); };
    tick(); const iv = setInterval(tick, 1000); cleanup.push(() => clearInterval(iv));
    const tz = h("select", ...[...new Set([tm.timezone, ...zones])].sort().map(z => h("option", { selected: z === tm.timezone }, z)));
    const save = async b => { try { Object.assign(tm, await put("/system/time", b) || b); tick(); flash(msg, t("set.saved")); } catch (e) { flash(msg, e.message, false); } };
    tz.onchange = () => save({ timezone: tz.value });
    el.append(h("div.tiles", h("div.tile", h("div.k", t("set.dt.now")), now)),
      group(row(t("set.dt.ntp"), t("set.dt.ntpHint"), sw(tm.ntp, v => save({ ntp: v }))), row(t("set.dt.tz"), null, tz)), msg);
  },

  async language(el) {
    const l = await get("/system/locale"), msg = h("p.hint");
    const save = async b => { try { await put("/system/locale", b); flash(msg, t("set.saved")); } catch (e) { flash(msg, e.message, false); } };
    const sel = (opts, v, k) => h("select", { onchange: e => save({ [k]: e.target.value }) }, ...opts.map(([o, n]) => h("option", { value: o, selected: o === v }, n)));
    el.append(
      h("h2", t("set.lang.ui")), group(row(t("set.lang.uiLang"), t("set.lang.reload"), seg([["tr", "Türkçe"], ["en", "English"]], lang, async v => { setPref("lang", v); await flushPrefs(); (app.embedded ? top : window).location.reload(); }))),
      h("h2", t("set.lang.system")), group(
        row(t("set.lang.locale"), null, sel([["tr_TR.UTF-8", "Türkçe (Türkiye)"], ["en_US.UTF-8", "English (US)"], ["en_GB.UTF-8", "English (UK)"], ["de_DE.UTF-8", "Deutsch"]], l.lang, "lang")),
        row(t("set.lang.keymap"), null, sel([["tr", "Türkçe Q"], ["trf", "Türkçe F"], ["us", "ABD"], ["gb", "Birleşik Krallık"], ["de", "Almanca"]], l.keymap, "keymap"))), msg);
  },

  keyboard(el) {
    el.append(group(row(t("set.kb.ctrl"), t("set.kb.ctrlHint"), seg([["auto", t("set.auto")], ["on", t("on")], ["off", t("off")]], pref("ctrlCmd"), v => setPref("ctrlCmd", v)))),
      h("h2", t("set.kb.table")),
      h("div.group", h("table.kt", h("thead", h("tr", h("th", t("set.kb.action")), h("th", "⌘"), h("th", t("set.kb.pc")))),
        h("tbody", ...SHORTCUTS.map(s => h("tr", h("td", t("k." + s.id)), h("td", h("kbd", macLabel(s.combos[0]))), h("td", ...s.combos.map(c => h("kbd", pcLabel(c))))))))),
      note(t("set.kb.note")));
  },

  async appearance(el) {
    registerPluginThemes(await get("/plugins").catch(() => [])); // eklenti temaları (ör. Konya HackerSpace)
    // Tema paketi: duvar kâğıdı + vurgu rengi. Seçim anında tüm pencerelere yayılır (prefs).
    const packs = h("div.packs", ...THEMES.map(p => h("button.pack" + (pref("look") === p.id ? ".on" : ""), { "data-id": p.id,
      onclick: e => { packs.querySelectorAll(".pack").forEach(b => b.classList.toggle("on", b === e.currentTarget)); setPref("look", p.id); setPref("accent", p.accent); setPref("wallpaper", "auto"); if (p.mode) setPref("theme", p.mode); syncAcc(); wsel.value = "auto"; } },
      h("span.thumb", { style: `background-image:url("${wallURL(p.id, isDark())}")` }), h("span.pn", themeName(p, t, lang)), h("span.pd", themeDesc(p, t, lang)))));
    const refreshThumbs = () => packs.querySelectorAll(".pack").forEach(b => { b.querySelector(".thumb").style.backgroundImage = `url("${wallURL(b.dataset.id, isDark())}")`; });
    const custom = () => String(pref("wallpaper") || "").startsWith("custom:");
    const wsel = h("select", { onchange: e => setPref("wallpaper", e.target.value) }, h("option", { value: "auto" }, t("set.ap.wall.auto")), ...THEMES.map(p => h("option", { value: p.id }, themeName(p, t, lang))));
    const syncWsel = () => {
      wsel.querySelector("option.cu")?.remove();
      if (custom()) wsel.append(h("option.cu", { value: pref("wallpaper") }, t("set.ap.wall.custom")));
      wsel.value = pref("wallpaper") || "auto";
    };
    syncWsel();
    // Kendi resmin: masaüstü ve kilit ekranı aynı duvar kâğıdını kullanır.
    const upBtn = (label, fn) => {
      const b = h("button.btn.sm", { onclick: async () => {
        b.disabled = true; const old = b.textContent; b.textContent = t("set.ap.uploading");
        try { if (await fn()) toast(t("set.ap.saved")); } catch (e) { toast(e.message); }
        b.disabled = false; b.textContent = old;
      } }, label);
      return b;
    };
    const wallUp = upBtn(t("set.ap.wall.upload"), async () => { if (!await chooseWallpaper("desktop")) return false; setPref("wallpaper", "custom:" + Date.now()); syncWsel(); return true; });
    const themes = [["light", t("set.ap.light")], ["dark", t("set.ap.dark")], ["auto", t("set.auto")]];
    const th = h("div.themes", ...themes.map(([v, l]) => h("button.theme" + (pref("theme") === v ? ".on" : ""), { onclick: e => { th.querySelectorAll(".theme").forEach(b => b.classList.toggle("on", b === e.currentTarget)); setPref("theme", v); setTimeout(refreshThumbs, 50); } },
      h("span.prev.prev-" + v), h("span", l))));
    const ACC = ["orange", "yellow", "lime", "green", "teal", "cyan", "blue", "purple", "pink", "red", "graphite"];
    const syncAcc = () => acc.querySelectorAll(".acc").forEach(b => b.classList.toggle("on", b.dataset.a === pref("accent")));
    const acc = h("div.accents", ...ACC.map(a => h("button.acc" + (pref("accent") === a ? ".on" : ""), { "data-a": a, title: t("set.ap.c." + a), "aria-label": t("set.ap.c." + a),
      onclick: e => { acc.querySelectorAll(".acc").forEach(b => b.classList.toggle("on", b === e.currentTarget)); setPref("accent", a); } })));
    el.append(
      h("h2.first", t("set.ap.packs")), h("div.group.pad", packs),
      group(
        row(t("set.ap.mode"), null, th),
        row(t("set.ap.wall"), t("set.ap.wallHint"), h("span", { style: { display: "flex", gap: "8px", alignItems: "center" } }, wsel, wallUp)),
      ),
      group(row(t("set.ap.accent"), null, acc)),
      h("h2", t("set.ap.sysui")),
      group(
        row(t("set.ap.uiMenubar"), t("set.ap.uiMenubarHint"), sw(pref("uiMenubar") !== false, v => setPref("uiMenubar", v))),
        row(t("set.ap.uiDock"), t("set.ap.uiDockHint"), sw(pref("uiDock") !== false, v => setPref("uiDock", v))),
        row(t("set.ap.aiWidget"), t("set.ap.aiWidgetHint"), sw(pref("aiWidget") !== false, v => setPref("aiWidget", v))),
        row(t("set.ap.uiThemeWidget"), t("set.ap.uiThemeWidgetHint"), sw(pref("uiThemeWidget") !== false, v => setPref("uiThemeWidget", v))),
        row(t("set.ap.uiStatusTemp"), t("set.ap.uiStatusTempHint"), sw(pref("uiStatusTemp") !== false, v => setPref("uiStatusTemp", v))),
        row(t("set.ap.uiToasts"), t("set.ap.uiToastsHint"), sw(pref("uiToasts") !== false, v => setPref("uiToasts", v))),
      ),
      group(
        row(t("set.ap.aiMode"), t("set.ap.aiModeHint"), seg([["bypass", t("ai.mode.bypass")], ["sor", t("ai.mode.sor")]], pref("aiMode") === "sor" ? "sor" : "bypass", v => setPref("aiMode", v))),
      ),
    );
  },

  async startup(el) {
    await loadApps().catch(() => {});
    const appOpts = [["", t("set.st.appNone")], ...APPS.filter(a => a.id !== "makeros.settings").map(a => [a.id, a.name])];
    const appSel = h("select", { onchange: e => { setPref("startupApp", e.target.value); openNowBtn.disabled = !e.target.value; } },
      ...appOpts.map(([id, name]) => h("option", { value: id, selected: (pref("startupApp") || "") === id }, name)));
    const openNowBtn = btn(t("set.st.openNow"), () => {
      const id = appSel.value;
      if (id) tell({ mk: "open", app: id });
    }, ".sm");
    openNowBtn.disabled = !pref("startupApp");

    const projBox = h("div.group", h("div.empty", t("loading")));
    const tasksBox = h("div.group", h("div.empty", t("loading")));
    const statusMsg = h("p.hint");
    let projects = [], tasks = [];

    const renderProjects = () => {
      if (!projects.length) {
        projBox.replaceChildren(h("div.row", h("div.grow", h("div.s", t("set.st.projEmpty"))),
          btn(t("app.python"), () => tell({ mk: "open", app: "makeros.python" }), ".sm")));
        return;
      }
      projBox.replaceChildren(...projects.map(p => {
        const badge = h("span.badge" + (p.running ? ".ok" : ""), p.running ? t("py.running") : t("py.stopped"));
        const runBtn = btn(p.running ? t("py.stop") : t("py.run"), async () => {
          runBtn.disabled = true;
          try {
            await post(`/projects/${encodeURIComponent(p.name)}/${p.running ? "stop" : "start"}`);
            await refresh();
          } catch (e) { toast(e.message); }
          runBtn.disabled = false;
        }, ".sm");
        const autoSw = sw(!!p.autostart, async v => {
          try {
            await put(`/projects/${encodeURIComponent(p.name)}/autostart`, { autostart: v });
            p.autostart = v;
            toast(v ? t("py.autoOnToast") : t("py.autoOffToast"));
          } catch (e) { toast(e.message); await refresh(); }
        });
        return row(p.name, `~/Projeler/${p.name}`, h("span", { style: { display: "flex", gap: "8px", alignItems: "center" } }, badge, runBtn, autoSw), { ic: icon("code", 16, 2) });
      }));
    };

    const scheduleLabel = s => {
      if (s === "@reboot") return t("set.st.whenBoot");
      if (s === "* * * * *") return t("set.st.when1m");
      if (s === "*/5 * * * *") return t("set.st.when5m");
      if (s === "*/15 * * * *") return t("set.st.when15m");
      if (s === "*/30 * * * *") return t("set.st.when30m");
      if (s === "0 * * * *") return t("set.st.when1h");
      const m = s.match(/^(\d{1,2})\s+(\d{1,2})\s+\*\s+\*\s+\*$/);
      if (m) return `${t("set.st.whenDaily")} (${String(m[2]).padStart(2, "0")}:${String(m[1]).padStart(2, "0")})`;
      return `Cron: ${s}`;
    };

    const renderTasks = () => {
      if (!tasks.length) {
        tasksBox.replaceChildren(h("div.row", h("div.grow", h("div.s", t("set.st.noTasks")))));
        return;
      }
      tasksBox.replaceChildren(...tasks.map(tk => {
        const kindBadge = h("span.badge.acc", tk.kind === "project" ? t("set.st.kindProject") : t("set.st.kindScript"));
        const schedBadge = h("span.badge", scheduleLabel(tk.schedule));
        const subParts = [
          tk.kind === "project" ? `~/Projeler/${tk.target}` : tk.target,
          tk.last_run ? t("set.st.lastRun", { time: tk.last_run, code: tk.last_code ?? 0 }) : "",
        ].filter(Boolean).join(" · ");
        const runNow = btn(t("set.st.runNow"), async () => {
          runNow.disabled = true; runNow.textContent = t("set.st.running");
          try {
            await post(`/autostart/${encodeURIComponent(tk.id)}/run`);
            toast(t("set.st.ran"));
            await refresh();
          } catch (e) { toast(e.message); }
          runNow.disabled = false; runNow.textContent = t("set.st.runNow");
        }, ".sm");
        const delBtn = h("button.btn.sm.danger", { onclick: async () => {
          try { await api("DELETE", `/autostart/${encodeURIComponent(tk.id)}`); await refresh(); } catch (e) { toast(e.message); }
        }, html: icon("trash-2", 13, 2), title: t("delete") });
        const enSw = sw(tk.enabled !== false, async v => {
          try { await post(`/autostart/${encodeURIComponent(tk.id)}/toggle`, { enabled: v }); tk.enabled = v; }
          catch (e) { toast(e.message); await refresh(); }
        });
        return row(
          h("span", { style: { display: "inline-flex", gap: "6px", alignItems: "center", flexWrap: "wrap" } }, tk.name, kindBadge, schedBadge),
          subParts,
          h("span", { style: { display: "flex", gap: "6px", alignItems: "center" } }, runNow, enSw, delBtn),
          { ic: icon(tk.schedule === "@reboot" ? "power" : "clock", 16, 2) },
        );
      }));
    };

    // Yeni görev ekleme formu (Python Projesi veya Script/Komut + @reboot veya Cron)
    let taskKind = "project";
    const nameIn = h("input", { placeholder: t("set.st.taskNamePh"), style: { width: "220px" } });
    const projSel = h("select", { style: { width: "220px" } });
    const scriptIn = h("input", { placeholder: t("set.st.targetScriptPh"), style: { width: "260px" }, spellcheck: "false" });
    const targetWrap = h("span", projSel);
    const targetLabel = h("div.t", t("set.st.targetProject"));

    const syncKindUI = () => {
      if (taskKind === "project" && projects.length) {
        targetLabel.textContent = t("set.st.targetProject");
        targetWrap.replaceChildren(projSel);
      } else {
        taskKind = "script";
        targetLabel.textContent = t("set.st.targetScript");
        targetWrap.replaceChildren(scriptIn);
      }
    };

    const whenPresets = [
      ["@reboot", t("set.st.whenBoot")],
      ["*/5 * * * *", t("set.st.when5m")],
      ["*/15 * * * *", t("set.st.when15m")],
      ["*/30 * * * *", t("set.st.when30m")],
      ["0 * * * *", t("set.st.when1h")],
      ["* * * * *", t("set.st.when1m")],
      ["daily", t("set.st.whenDaily")],
      ["custom", t("set.st.whenCustom")],
    ];
    const timeIn = h("input", { type: "time", value: "08:00", style: { width: "110px" } });
    const cronIn = h("input", { placeholder: "*/10 * * * *", value: "0 8 * * *", style: { width: "140px" }, spellcheck: "false" });
    const whenExtra = h("span");
    const whenSel = h("select", { onchange: () => {
      whenExtra.replaceChildren(whenSel.value === "daily" ? timeIn : whenSel.value === "custom" ? cronIn : "");
    } }, ...whenPresets.map(([v, l]) => h("option", { value: v }, l)));

    const addTask = async () => {
      const target = taskKind === "project" ? projSel.value.trim() : scriptIn.value.trim();
      if (!target) return flash(statusMsg, t("set.st.targetScriptPh"), false);
      let schedule = whenSel.value;
      if (schedule === "daily") {
        const [hh, mm] = (timeIn.value || "08:00").split(":").map(x => parseInt(x, 10) || 0);
        schedule = `${mm} ${hh} * * *`;
      } else if (schedule === "custom") {
        schedule = cronIn.value.trim();
      }
      try {
        await post("/autostart", {
          name: nameIn.value.trim() || target,
          kind: taskKind,
          target,
          schedule,
          enabled: true,
        });
        nameIn.value = "";
        scriptIn.value = "";
        flash(statusMsg, t("set.saved"));
        await refresh();
      } catch (e) {
        flash(statusMsg, e.message, false);
      }
    };

    const kindSeg = seg([["project", t("set.st.kindProject")], ["script", t("set.st.kindScript")]], taskKind, v => {
      taskKind = v;
      syncKindUI();
    });

    const refresh = async () => {
      try {
        const d = await get("/autostart");
        projects = d?.projects || [];
        tasks = d?.tasks || [];
        projSel.replaceChildren(...projects.map(p => h("option", { value: p.name }, p.name)));
        if (!projects.length && taskKind === "project") {
          taskKind = "script";
          kindSeg.querySelectorAll("button").forEach((b, i) => b.classList.toggle("on", i === 1));
        }
        syncKindUI();
        renderProjects();
        renderTasks();
      } catch (e) {
        flash(statusMsg, e.message, false);
      }
    };

    el.append(
      h("h2.first", t("set.st.appTitle")),
      group(
        row(t("set.st.app"), t("set.st.appHint"), h("span", { style: { display: "flex", gap: "8px", alignItems: "center" } }, appSel, openNowBtn), { ic: icon("app-window", 18, 2) }),
        row(t("set.st.appFull"), t("set.st.appFullHint"), sw(!!pref("startupFullscreen"), v => setPref("startupFullscreen", v))),
      ),
      h("h2", t("set.st.projTitle")),
      note(t("set.st.projHint")),
      projBox,
      h("h2", t("set.st.cronTitle")),
      note(t("set.st.cronHint")),
      h("h2", t("set.st.newTask")),
      group(
        row(t("set.st.kind"), null, kindSeg),
        h("div.row", h("div.grow", targetLabel), targetWrap),
        row(t("set.st.taskName"), null, nameIn),
        row(t("set.st.when"), null, h("span", { style: { display: "flex", gap: "8px", alignItems: "center", flexWrap: "wrap" } }, whenSel, whenExtra)),
        h("div.row", h("div.grow"), btn(t("set.st.add"), addTask, ".primary")),
      ),
      statusMsg,
      h("h2", t("set.st.tasks")),
      tasksBox,
    );
    await refresh();
  },

  async user(el) {
    const s = await get("/session");
    const o = h("input", { type: "password", autocomplete: "current-password" }), n1 = h("input", { type: "password", autocomplete: "new-password" }), n2 = h("input", { type: "password", autocomplete: "new-password" });
    const msg = h("p.hint");
    const change = async () => {
      if (n1.value !== n2.value) return flash(msg, t("set.user.mismatch"), false);
      try { await put("/system/password", { old: o.value, new: n1.value }); o.value = n1.value = n2.value = ""; flash(msg, t("set.user.changed")); } catch (e) { flash(msg, e.message, false); }
    };
    el.append(group(row(s.user, s.admin ? t("set.user.admin") : t("set.user.std"), h("button.btn.sm", { onclick: () => tell({ mk: "shortcut", id: "lock" }) }, t("menu.lock")), { ic: icon("user", 28, 1.5) })),
      h("h2", t("set.user.pw")), group(row(t("set.user.old"), null, o), row(t("set.user.new"), null, n1), row(t("set.user.again"), null, n2),
        h("div.row", h("div.grow"), btn(t("set.user.change"), change, ".primary"))), msg);
  },

  power(el) {
    const act = a => app.embedded ? tell({ mk: "power", action: a }) : (confirm(t(a === "reboot" ? "power.confirmReboot" : "power.confirmOff")) && post("/system/power", { action: a }).then(() => alert(t("power.sent"))));
    el.append(group(row(t("power.reboot"), t("set.pw.rebootHint"), btn(t("power.reboot"), () => act("reboot"))),
      row(t("power.off"), t("set.pw.offHint"), btn(t("power.off"), () => act("poweroff"), ".danger"))), note(t("set.pw.note")));
  },
};

// ------------------------------------------------------------ gezinme
async function show(id, push = true) {
  if (!PAGES[id]) id = "wifi";
  for (const f of cleanup) try { f(); } catch { /* yok */ } cleanup = [];
  cur = id;
  side.querySelectorAll(".sb-item").forEach(b => b.classList.toggle("on", b.dataset.p === id));
  const page = h("div.page", h("div.page-h", h("button.btn.icon.back", { onclick: () => split.classList.remove("detail"), html: icon("chevron-left", 16), "aria-label": t("m.home") }), h("h1", t("set." + id))));
  content.replaceChildren(page); content.scrollTop = 0;
  split.classList.add("detail");
  if (push && location.hash.slice(1) !== id) history.replaceState(null, "", "#" + id);
  app.path(id); app.title(t("set." + id));
  try { await PAGES[id](page); }
  catch (e) { page.append(h("div.group", h("div.empty.err", e.message)), btn(t("retry"), () => show(id))); }
}
const search = h("input.sb-search", { type: "search", placeholder: t("launch.search"), oninput: () => {
  const q = fold(search.value);
  side.querySelectorAll(".sb-item").forEach(b => { b.hidden = q && !fold(b.textContent + " " + b.dataset.k).includes(q); });
} });
side.append(search, ...SETTINGS_PAGES.flatMap(([id, ic, k], i) => [
  [2, 4, 8].includes(i) ? h("div.sb-gap") : null,
  h("button.sb-item", { "data-p": id, "data-k": k, onclick: () => show(id) }, h("span.sb-ic", { style: { background: COLORS[id] }, html: icon(ic, 14, 2) }), t("set." + id)),
].filter(Boolean)));
app.on("navigate", p => show(p || "wifi"));
addEventListener("hashchange", () => { const p = location.hash.slice(1); if (p && p !== cur) show(p, false); });
const narrow = matchMedia("(max-width: 699px)").matches;
show(location.hash.slice(1) || "wifi");
if (narrow && !location.hash) split.classList.remove("detail");
