// Satır içi SVG simgeler (Lucide alt kümesi → js/icon-set.js, üretilir: tools/gen-icons.mjs; lisans vendor/lucide/LICENSE — ISC)
import ICONS from "./icon-set.js";

export function icon(name, size = 16, sw = 1.8) {
  return `<svg class="ic" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] || ""}</svg>`;
}
// Wi-Fi gücü → simge adı
export const wifiIcon = (sig, on = true) => !on ? "wifi-off" : sig == null || sig > 66 ? "wifi" : sig > 40 ? "wifi-high" : sig > 15 ? "wifi-low" : "wifi-zero";
