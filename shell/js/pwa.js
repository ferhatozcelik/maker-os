// PWA: service worker kaydı (yalnız güvenli bağlamda: HTTPS veya localhost; Pi'ye düz http ile
// gelindiğinde tarayıcı izin vermez, manifest yine de "Ana ekrana ekle"yi tam ekran açar).
// Pi'nin kendi ekranında (cog/WPE, yerel bilet) kayıt yapılmaz.
export function registerSW() {
  if (!("serviceWorker" in navigator) || !isSecureContext || /WPE/.test(navigator.userAgent)) return;
  addEventListener("load", () => navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => {}), { once: true });
}
