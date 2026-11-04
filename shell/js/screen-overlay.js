// Ekran Modu katmanı: Pi'nin ekranı aynı sayfanın içinde, tüm görünümü kaplayarak açılır (adres değişmez).
// Masaüstü kabuğu (/) ve Web arayüzü (/web) ortak kullanır. "‹ MakerOS" (uygulamadan {mk:"close"}),
// Esc tuşu yok (Pi'ye gider) — tarayıcının Geri tuşu da katmanı kapatır (history.pushState).
let frame = null;

export function isScreenOpen() { return !!frame; }

export function openScreen() {
  if (frame) return frame;
  frame = document.createElement("iframe");
  frame.src = "/shell/apps/screen/";
  frame.title = "Ekran Modu";
  frame.allow = "clipboard-read; clipboard-write; fullscreen";
  frame.className = "mk-screen-overlay";
  frame.style.cssText = "position:fixed;inset:0;width:100%;height:100%;border:0;z-index:2147483000;background:#000";
  document.body.append(frame);
  document.documentElement.classList.add("mk-screen-open");
  try { history.pushState({ mkScreen: 1 }, "", location.href); } catch { /* yok */ }
  frame.addEventListener("load", () => { try { frame.contentWindow.focus(); } catch { /* yok */ } });
  return frame;
}

export function closeScreen({ fromHistory = false } = {}) {
  if (!frame) return;
  frame.src = "about:blank"; // bağlantı hemen kapansın (Pi'de görüntü kodlama durur)
  frame.remove(); frame = null;
  document.documentElement.classList.remove("mk-screen-open");
  if (!fromHistory && history.state?.mkScreen) { try { history.back(); } catch { /* yok */ } }
}

addEventListener("popstate", () => { if (frame) closeScreen({ fromHistory: true }); });
addEventListener("message", e => {
  if (!frame || e.source !== frame.contentWindow || e.origin !== location.origin) return;
  if (e.data?.mk === "close") { e.stopImmediatePropagation(); closeScreen(); }
}, true);
