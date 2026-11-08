// Kontrol Merkezi (popover). Tembel yüklenir. Wi-Fi radyosu: PUT /net/wifi {enabled} (yönetici).
import { h } from "../lib.js";
import { icon, wifiIcon } from "../icons.js";
import { t } from "../i18n.js";
import { popover, rectOf, closePopup } from "./popup.js";
import { pref, setPref, isDark, fxTier } from "../prefs.js";
import { put } from "../api.js";
import { S } from "../shell.js";

// Wi-Fi radyosu açık mı? Sözleşme önerisi: /net → wifi_enabled; yoksa arayüz durumundan çıkarılır.
export const wifiEnabled = n => {
  if (typeof n?.wifi_enabled === "boolean") return n.wifi_enabled;
  const w = n?.ifaces?.find(i => i.type === "wifi");
  return !!w && !["unavailable", "disabled", "off", "unmanaged"].includes(w.state);
};

export function toggleControl(btn, owner) {
  if (btn.classList.contains("open")) return closePopup();
  btn.classList.add("open");
  const reopen = () => { closePopup(); toggleControl(btn, owner); };
  const n = S.net, w = n?.ifaces?.find(i => i.type === "wifi" && i.state === "connected"), on = wifiEnabled(n);
  const tile = (ic, title, sub, isOn, onIcon, onText, busy) => h("div.cc-tile",
    h("button.cc-ic" + (isOn ? ".on" : "") + (busy ? ".busy" : ""), { onclick: onIcon, "aria-pressed": String(!!isOn), "aria-label": title, html: icon(ic, 16, 2) }),
    h("button.cc-tx", { onclick: onText || onIcon }, h("b", title), h("small", sub)));
  const wifiSub = !on ? t("off") : w ? w.ssid : t("status.wifiOff");
  const wifi = tile(on ? wifiIcon(w?.signal, !!w || on) : "wifi-off", t("cc.wifi"), wifiSub, on, async e => {
    if (!S.user?.admin) { closePopup(); return S.openSettings("wifi"); }
    const b = e.currentTarget; b.classList.add("busy"); b.disabled = true;
    try {
      const r = await put("/net/wifi", { enabled: !on }); // 200 + güncel /net gövdesi
      S.net = r?.ifaces ? r : { ...n, wifi_enabled: !on }; S.emit("net", S.net);
      reopen();
    } catch (x) { b.classList.remove("busy"); b.disabled = false; S.toast(x.message, "circle-alert"); }
  }, () => { closePopup(); S.openSettings("wifi"); });
  const dark = tile(isDark() ? "moon" : "sun", t("cc.dark"), isDark() ? t("on") : t("off"), isDark(), () => { setPref("theme", isDark() ? "light" : "dark"); reopen(); });
  const body = h("div.cc",
    h("div.cc-grid", wifi, dark),
    h("button.cc-link", { onclick: () => { closePopup(); S.webMode(); } }, t("cc.web")),
    h("button.cc-link", { onclick: () => { closePopup(); S.openSettings("network"); } }, t("cc.netSettings")));
  popover(body, rectOf(btn), { owner, onClose: () => btn.classList.remove("open"), cls: "cc-pop" });
}
