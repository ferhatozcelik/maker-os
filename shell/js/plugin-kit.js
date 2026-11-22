// MakerOS eklenti kiti: eklenti arayüzlerinin kabukla aynı tema ve dilde görünmesi ve canlı eşitlenmesi.
//
// Eklenti sayfasında (/plugin/<id>/ aynı köken olduğu için kabuk dosyaları doğrudan kullanılır):
//   <link rel=stylesheet href="/shell/css/tokens.css">   renk/yazı/köşe belirteçleri (--accent, --text, --bg-window…)
//   <script src="/shell/js/plugin-kit.js"></script>      bu dosya: window.MK
//
// MK.lang ("tr" | "en"), MK.locale, MK.dark, MK.accent (#rrggbb), MK.accentName
// MK.i18n({ tr: {...}, en: {...} })   sözlük kaydı; MK.t("anahtar", {ad: "x"}) → metin ({ad} yer tutucuları)
// MK.t({ tr: "…", en: "…" })          satır içi çift dilli metin
// MK.translate(kök?)                  data-t / data-t-placeholder / data-t-title özniteliklerini çevirir
// MK.on(fn)                           fn(MK, değişenler: Set<"lang"|"theme"|"accent">) — kabukta tercih değişince
//                                    Dil değişince varsayılan: MK.translate(); sayfa kendini yeniden çizmek için dinler.
// MK.theme()                          {accent, dark, lang, font, radius, colors:{...}} (ör. gömülü üçüncü taraf arayüze aktarmak için)
//
// Kabuk dışında (doğrudan açılan eklenti) varsayılanlar: tr, açık tema, turuncu vurgu.
(function () {
  "use strict";
  const ACCENTS = { orange: "#f07a1a", blue: "#1a7cf5", purple: "#9b55e0", pink: "#e64d97", red: "#e0354b", yellow: "#e8b416",
    green: "#3cb44f", teal: "#14a38b", cyan: "#12b3d1", lime: "#c8f53c", graphite: "#8a8a90" };
  const LANGS = ["tr", "en"];
  const root = document.documentElement;
  const darkMQ = matchMedia("(prefers-color-scheme: dark)");
  const get = (k, d) => { try { const v = localStorage.getItem("mk." + k); return v == null ? d : JSON.parse(v); } catch { return d; } };
  const dicts = {}, listeners = new Set();

  const MK = {
    lang: "tr", locale: "tr-TR", dark: false, accent: ACCENTS.orange, accentName: "orange",
    i18n(d) { for (const l of Object.keys(d)) dicts[l] = Object.assign(dicts[l] || {}, d[l]); this.translate(); return this; },
    t(key, vars) {
      let s = typeof key === "object" && key ? (key[this.lang] ?? key.tr ?? "") : ((dicts[this.lang] || {})[key] ?? (dicts.tr || {})[key] ?? key);
      if (vars) s = String(s).replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? "");
      return s;
    },
    translate(el = document) {
      if (!el.querySelectorAll) return;
      for (const n of el.querySelectorAll("[data-t]")) n.textContent = this.t(n.dataset.t);
      for (const n of el.querySelectorAll("[data-t-html]")) n.innerHTML = this.t(n.dataset.tHtml);
      for (const n of el.querySelectorAll("[data-t-placeholder]")) n.placeholder = this.t(n.dataset.tPlaceholder);
      for (const n of el.querySelectorAll("[data-t-title]")) { n.title = this.t(n.dataset.tTitle); n.setAttribute("aria-label", n.title); }
    },
    on(fn) { listeners.add(fn); return () => listeners.delete(fn); },
    theme() {
      const cs = getComputedStyle(root), v = n => cs.getPropertyValue(n).trim();
      const names = ["bg-window", "bg-content", "bg-elevated", "bg-sidebar", "bg-control", "text", "text-2", "text-3", "border", "separator", "success", "danger", "warning", "on-accent"];
      return { accent: this.accent, dark: this.dark, lang: this.lang, font: v("--font-sans") || "system-ui, sans-serif",
        radius: parseInt(v("--radius-12")) || 12, colors: Object.fromEntries(names.map(n => [n, v("--" + n)]).filter(([, x]) => x)) };
    },
  };

  // prefs.apply ile aynı: kök öznitelikleri → tokens.css değişkenleri.
  function read() {
    const lang = get("lang", "tr"), theme = get("theme", "auto"), acc = get("accent", "orange");
    MK.lang = LANGS.includes(lang) ? lang : "tr";
    MK.locale = MK.lang === "tr" ? "tr-TR" : "en-GB";
    MK.dark = theme === "dark" || (theme === "auto" && darkMQ.matches);
    MK.accentName = ACCENTS[acc] ? acc : "orange";
    MK.accent = ACCENTS[MK.accentName];
    theme === "auto" ? root.removeAttribute("data-theme") : root.setAttribute("data-theme", theme);
    root.dataset.accent = MK.accentName;
    root.dataset.dark = MK.dark ? "1" : "0";
    root.lang = MK.lang;
    root.style.colorScheme = MK.dark ? "dark" : "light";
  }
  function emit(changed) {
    if (changed.has("lang")) MK.translate();
    for (const f of listeners) { try { f(MK, changed); } catch (e) { console.error(e); } }
  }
  function refresh(keys) {
    const before = { lang: MK.lang, dark: MK.dark, accent: MK.accent };
    read();
    const changed = new Set(keys || []);
    if (before.lang !== MK.lang) changed.add("lang");
    if (before.dark !== MK.dark || before.accent !== MK.accent) changed.add("theme");
    if (changed.size) emit(changed);
  }

  read();
  // Kabuk tercihleri localStorage'a yazar; "storage" olayı aynı kökenli tüm pencerelere (eklenti iframe'leri dahil) gelir.
  addEventListener("storage", e => {
    if (!e.key || !e.key.startsWith("mk.")) return;
    const k = e.key.slice(3);
    if (k === "lang" || k === "theme" || k === "accent") refresh([k === "accent" ? "theme" : k]);
  });
  darkMQ.addEventListener?.("change", () => refresh());
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => MK.translate());
  // Masaüstü widget'ı olarak gömülüyse sağ tık kabuğun widget menüsünü açar (düzenle / taşı / kaldır).
  // Metin kutuları ve seçili metinde tarayıcının kendi menüsü kalır; pencerede açılan eklentiye dokunulmaz.
  addEventListener("contextmenu", e => {
    let host = null;
    try { host = window.frameElement && window.frameElement.closest && window.frameElement.closest("#widgets"); } catch (_) { /* farklı köken */ }
    if (!host || e.target.closest("input, textarea, [contenteditable]") || String(getSelection())) return;
    e.preventDefault();
    parent.postMessage({ mk: "widget-menu", x: e.clientX, y: e.clientY }, location.origin);
  });
  window.MK = MK;
})();
