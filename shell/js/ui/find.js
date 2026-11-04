// Bul: bulanık arama (uygulamalar, Ayarlar sayfaları) + güvenli hesap makinesi. Tembel yüklenir.
import { h, fold, animate } from "../lib.js";
import { icon } from "../icons.js";
import { t, locale } from "../i18n.js";
import { APPS, SETTINGS_PAGES, appById } from "../apps.js";
import { show, closeOverlay, overlayOpen } from "./layer.js";
import { S } from "../shell.js";

// Bulanık eşleşme: alt dizi + kelime başı bonusu
export function score(q, text) {
  q = fold(q).trim(); text = fold(text);
  if (!q) return 1;
  const i = text.indexOf(q);
  if (i >= 0) return 100 - i + (i === 0 || text[i - 1] === " " ? 50 : 0);
  let ti = 0, s = 0, run = 0;
  for (const c of q) {
    const k = text.indexOf(c, ti); if (k < 0) return 0;
    run = k === ti ? run + 1 : 0; s += 1 + run * 2 + (k === 0 || text[k - 1] === " " ? 3 : 0); ti = k + 1;
  }
  return s;
}

// ------------------------------------------------------------ Bul
// Güvenli hesap makinesi (eval yok): + - * / ^ % ( ) ondalık nokta/virgül
export function calc(src) {
  const s = src.replace(/×/g, "*").replace(/÷/g, "/").replace(/(\d),(\d)/g, "$1.$2").replace(/\s+/g, "");
  if (!/^[\d.+\-*/^%()]+$/.test(s) || !/[+\-*/^%]/.test(s.replace(/^-/, "")) || !/\d/.test(s)) return null;
  let i = 0;
  const peek = () => s[i], num = () => { const m = s.slice(i).match(/^\d*\.?\d+/); if (!m) throw 0; i += m[0].length; return +m[0]; };
  const atom = () => {
    if (peek() === "-") { i++; return -atom(); }
    if (peek() === "(") { i++; const v = expr(); if (s[i++] !== ")") throw 0; return v; }
    let v = num(); if (peek() === "%") { i++; v /= 100; } return v;
  };
  const pow = () => { const b = atom(); if (peek() === "^") { i++; return b ** pow(); } return b; };
  const term = () => { let v = pow(); while (peek() === "*" || peek() === "/") v = s[i++] === "*" ? v * pow() : v / pow(); return v; };
  const expr = () => { let v = term(); while (peek() === "+" || peek() === "-") v = s[i++] === "+" ? v + term() : v - term(); return v; };
  try { const v = expr(); return i === s.length && Number.isFinite(v) ? v : null; } catch { return null; }
}

export function find() {
  if (overlayOpen("find")) return closeOverlay();
  const input = h("input.find-in", { placeholder: t("find.placeholder"), autocomplete: "off", spellcheck: "false", "aria-label": t("find.placeholder") });
  const res = h("div.find-res"), hint = h("div.find-hint", t("find.hint"));
  let rows = [], sel = 0;
  const render = () => {
    const q = input.value.trim(); rows = [];
    const v = q && calc(q);
    if (v != null) {
      const out = new Intl.NumberFormat(locale, { maximumFractionDigits: 10 }).format(v);
      rows.push({ group: t("find.calc"), ic: "calculator", label: `= ${out}`, sub: q, run: () => { navigator.clipboard?.writeText(String(v)).catch(() => {}); S.toast(t("find.copied"), "check"); } });
    }
    if (q) {
      rows.push(...APPS.map(a => ({ s: score(q, a.keys), a })).filter(x => x.s > 0).sort((x, y) => y.s - x.s).slice(0, 6)
        .map(({ a }) => ({ group: t("find.apps"), img: a.icon, label: a.name, run: () => S.launch(a) })));
      const set = appById("makeros.settings");
      rows.push(...SETTINGS_PAGES.map(([id, ic, k]) => ({ s: score(q, t("set." + id) + " " + k), id, ic })).filter(x => x.s > 0).sort((x, y) => y.s - x.s).slice(0, 4)
        .map(({ id, ic }) => ({ group: t("find.settings"), ic, label: t("set." + id), sub: set?.name, run: () => S.openSettings(id) })));
    }
    sel = Math.min(sel, Math.max(0, rows.length - 1));
    let g = null;
    res.replaceChildren(...rows.flatMap((r, i) => {
      const out = [];
      if (r.group !== g) { g = r.group; out.push(h("div.find-g", g)); }
      const b = h("button.find-row" + (i === sel ? ".sel" : ""), { onclick: () => { closeOverlay(); r.run(); }, onpointermove: () => { if (sel !== i) { sel = i; mark(); } } },
        r.img ? h("img", { src: r.img, alt: "" }) : h("span.find-ic", { html: icon(r.ic, 16) }), h("span.find-l", r.label), r.sub ? h("small", r.sub) : "");
      out.push(b); return out;
    }));
    if (q && !rows.length) res.append(h("div.find-none", t("find.none")));
    panel.classList.toggle("has", !!q);
  };
  const mark = () => res.querySelectorAll(".find-row").forEach((b, i) => { b.classList.toggle("sel", i === sel); if (i === sel) b.scrollIntoView({ block: "nearest" }); });
  input.oninput = () => { sel = 0; render(); };
  input.onkeydown = e => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); if (rows.length) { sel = (sel + (e.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length; mark(); } }
    else if (e.key === "Enter") { e.preventDefault(); const r = rows[sel]; if (r) { closeOverlay(); r.run(); } }
    else if (e.key === "Escape") { e.preventDefault(); closeOverlay(); }
  };
  const panel = h("div.find.mat-hud", h("div.find-bar", h("span", { html: icon("search", 20) }), input), res, hint);
  const w = show("find", h("div.find-wrap", { onpointerdown: e => { if (e.target === e.currentTarget) closeOverlay(); } }, panel));
  animate(panel, [{ opacity: 0, transform: "scale(.96)" }, { opacity: 1, transform: "none" }], { duration: 220 });
  render(); input.focus();
  return w;
}

