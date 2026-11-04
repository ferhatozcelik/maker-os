// Dil: localStorage "mk.lang" (tr|en), yoksa tr. Kabuk ve uygulamalar paylaşır.
import tr from "../i18n/tr.js";
import { store } from "./lib.js";

export const LANGS = ["tr", "en"];
export const lang = (() => { const l = store.get("lang", "tr"); return LANGS.includes(l) ? l : "tr"; })();
// Yalnız seçili dil yüklenir (tr her zaman yedek)
const dict = lang === "tr" ? tr : (await import(`../i18n/${lang}.js`)).default;
document.documentElement.lang = lang;

export function t(key, vars) {
  let s = dict[key] ?? tr[key] ?? key;
  if (vars) s = s.replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? "");
  return s;
}
export const locale = lang === "tr" ? "tr-TR" : "en-GB";
