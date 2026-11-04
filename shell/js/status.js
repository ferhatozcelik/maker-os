// Tema durum göstergesi (eklenti "status"): ör. Konya HackerSpace kapısı açık mı.
// Etkin temanın eklentisi gerekiyorsa başlatılır (autostart); görünür değilken yoklama durur.
import { get, post } from "./api.js";
import { pref, prefs } from "./prefs.js";
import { themeById } from "./themes.js";

export function watchThemeStatus(onUpdate) {
  let timer = 0, started = new Set();
  async function tick() {
    clearTimeout(timer);
    const st = themeById(pref("look")).status;
    if (!st) { onUpdate(null); return; }
    try {
      if (st.autostart && !st.running && !started.has(st.plugin)) {
        started.add(st.plugin);
        await post(`/plugins/${st.plugin}/start`).catch(() => {});
        await new Promise(r => setTimeout(r, 2500)); // ilk başlatmada uv ortamı kurulur
      }
      const r = await fetch(st.url, { cache: "no-store" });
      const d = r.ok ? await r.json() : null;
      onUpdate({ label: st.label || "", open: d?.open ?? null, state: d?.state || "unknown", since: d?.since || null, plugin: st.plugin, app: themeById(pref("look")).apps?.[0]?.id });
    } catch { onUpdate({ label: st.label || "", open: null, state: "unknown", plugin: st.plugin }); }
    timer = setTimeout(tick, Math.max(15, st.every || 60) * 1000);
  }
  prefs.on("change", k => { if (k === "look") tick(); });
  document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") tick(); else clearTimeout(timer); });
  tick();
}
