// MakerOS service worker (kapsam "/": makerd /sw.js olarak sunar).
// Strateji: ağ önce. Kabuk dosyaları başarılı her yanıtta önbelleğe yazılır; Pi'ye ulaşılamazsa
// önbellekten açılır (en azından "bağlantı yok" ekranı ve son görülen arayüz). API, WebSocket,
// akışlar (kamera/terminal) ve dosya indirmeleri hiç önbelleklenmez.
const CACHE = "makeros-shell-v5";
const SKIP = /^\/(api|plugin|port)\//;

self.addEventListener("install", e => { self.skipWaiting(); e.waitUntil(caches.open(CACHE).then(c => c.addAll(["/web", "/shell/icons/pwa-192.png"]).catch(() => {}))); });
self.addEventListener("activate", e => {
  e.waitUntil((async () => {
    for (const k of await caches.keys()) if (k !== CACHE) await caches.delete(k);
    await self.clients.claim();
  })());
});

const offline = () => new Response(`<!doctype html><html lang="tr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>MakerOS</title><body style="margin:0;height:100dvh;display:grid;place-items:center;background:#16171a;color:#e6e6ea;font:16px system-ui;text-align:center;padding:24px">
<div><img src="/shell/icons/pwa-192.png" width="72" height="72" alt=""><h2 style="margin:16px 0 6px">Pi'ye ulaşılamıyor</h2>
<p style="color:#9a9aa3;margin:0 0 18px">Pi açık mı, aynı ağda mısınız?</p><button onclick="location.reload()" style="font:inherit;padding:9px 18px;border:0;border-radius:9px;background:#ff7a1a;color:#111">Yeniden dene</button></div>`,
  { status: 503, headers: { "Content-Type": "text/html; charset=utf-8" } });

self.addEventListener("fetch", e => {
  const req = e.request, url = new URL(req.url);
  if (req.method !== "GET" || url.origin !== location.origin || SKIP.test(url.pathname) || req.headers.get("range")) return;
  e.respondWith((async () => {
    try {
      const res = await fetch(req);
      if (res.ok && res.type === "basic" && !res.headers.get("content-disposition")) {
        const copy = res.clone();
        caches.open(CACHE).then(c => c.put(req, copy)).catch(() => {});
      }
      return res;
    } catch {
      const hit = await caches.match(req, { ignoreSearch: req.mode === "navigate" });
      if (hit) return hit;
      return req.mode === "navigate" ? offline() : Response.error();
    }
  })());
});
