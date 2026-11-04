// Küçük performans göstergesi (?perf=1): kare hızı, uzun görevler (>50 ms), pencere sayısı, JS yığını.
// WPE'de "longtask" gözlemcisi yoksa ana iş parçacığı gecikmesi rAF aralığından tahmin edilir.
export function perfHUD() {
  const el = document.createElement("div");
  el.id = "perf";
  el.style.cssText = "position:fixed;left:8px;bottom:8px;z-index:99999;padding:4px 8px;border-radius:6px;background:rgba(0,0,0,.72);color:#9f9;" +
    "font:11px/1.35 var(--font-mono,monospace);pointer-events:none;white-space:pre;contain:strict;width:210px;height:54px;box-sizing:border-box";
  document.body.append(el);
  let frames = 0, last = performance.now(), prev = last, worst = 0, longN = 0, longMs = 0, native = false;
  try {
    new PerformanceObserver(l => { for (const e of l.getEntries()) { longN++; longMs = Math.max(longMs, e.duration); } }).observe({ type: "longtask", buffered: true });
    native = true;
  } catch { /* desteklenmiyor */ }
  const tick = now => {
    frames++;
    const gap = now - prev; prev = now;
    if (gap > worst) worst = gap;
    if (!native && gap > 50) { longN++; longMs = Math.max(longMs, gap); }
    if (now - last >= 1000) {
      const fps = Math.round(frames * 1000 / (now - last)), mem = performance.memory?.usedJSHeapSize;
      el.style.color = fps >= 50 ? "#9f9" : fps >= 30 ? "#fd6" : "#f77";
      el.textContent = `${fps} fps  en kötü kare ${Math.round(worst)} ms\n` +
        `uzun görev ${longN}${longN ? ` (max ${Math.round(longMs)} ms)` : ""}${native ? "" : "*"}\n` +
        `pencere ${window.__mk?.wm?.list().length ?? 0}${mem ? `  yığın ${(mem / 1048576).toFixed(0)} MB` : ""}`;
      frames = 0; last = now; worst = 0;
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}
