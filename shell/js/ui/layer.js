// Tam ekran katman yöneticisi (çekirdek, küçük). Bul, Uygulamalar ızgarası, değiştirici ve
// Genel Bakış ayrı modüllerdir ve ilk kullanımda tembel yüklenir (bkz. main.js → ov()).
import { h, $, animate } from "../lib.js";
import { closePopup } from "./popup.js";

const layer = () => $("#overlays");
let active = null; // {kind, el, close}
export const overlayOpen = k => active && (!k || active.kind === k);
export function closeOverlay() { const a = active; active = null; a?.close(); }

export function show(kind, el, onClose, cls = "") {
  closeOverlay(); closePopup();
  const wrap = h("div.ov-layer" + cls, el);
  layer().append(wrap);
  animate(wrap, [{ opacity: 0 }, { opacity: 1 }], { duration: 180 });
  active = { kind, el: wrap, close() { onClose?.(); animate(wrap, [{ opacity: 1 }, { opacity: 0 }], { duration: 140 }).then(() => wrap.remove()); } };
  return wrap;
}

