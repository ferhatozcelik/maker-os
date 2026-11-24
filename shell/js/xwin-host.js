// Yerel (X11) pencereler ↔ pencere yöneticisi. Pi'de bir program pencere açınca (tkinter, PySide6,
// pygame, OpenCV imshow) burada gerçek bir <mk-window> açılır; kapanınca kapanır. Menüler, açılır
// listeler ve ipuçları (X'te "override-redirect") pencere çerçevesi olmadan, sahip pencerenin üstünde
// aynı göreli konumda çizilir. Denetim akışı: WS /api/v1/xwin (bkz. makerd/src/xwin/stream.rs).
import { h } from "./lib.js";
import { XView } from "./xwin.js";

const TB = 38; // --mk-titlebar-height: içerik başlık şeridinin altında başlar (mk-window.xwin)
const PY = /^(tk|python|pyside|pygame|qt|matplotlib|opencv|cv2)/i;

export function startXHost(wm, layer) {
  const wins = new Map();  // X kimliği → wm penceresi
  const info = new Map();  // X kimliği → son bilinen bilgi (kökteki konum, boyut, sahip)
  const pops = new Map();  // X kimliği → { el, view }
  let ws = null, wait = 500;
  const send = m => { if (ws?.readyState === 1) ws.send(JSON.stringify(m)); };

  const appFor = w => {
    const cls = (w.cls || "x11").trim(), icon = PY.test(cls) ? "/shell/icons/python.svg" : "/shell/icons/app.svg";
    return {
      id: "x11." + cls.toLowerCase().replace(/[^a-z0-9._-]+/g, "-"), name: w.title || cls, kind: "web",
      url: `/shell/apps/xwin/?id=${w.id}`, w: w.w, h: w.h + TB, icon, baseIcon: icon,
      rawCtrl: true, single: true, x11: w.id, minW: 120, minH: 60, categories: [],
      // Kırmızı düğme programa kapatma isteği gönderir ("kaydedilsin mi?" sorabilir); pencere,
      // program gerçekten kapatınca kapanır. Yanıt vermeyen programda ikinci tıklama kapatır.
      onClose: win => {
        if (win.xGone) return true;
        if (win.closeAsked && Date.now() - win.closeAsked < 5000) return true;
        win.closeAsked = Date.now();
        send({ t: "close", id: w.id });
        return false;
      },
    };
  };

  // Sahip pencerenin içerik alanının ekrandaki sol üst köşesi.
  const contentOrigin = win => { const r = wm.rectOf(win); return { x: r.x, y: r.y + (win.state === "full" ? 0 : TB) }; };

  function open(w) {
    info.set(w.id, w);
    if (w.popup) return openPopup(w);
    if (wins.has(w.id)) return;
    const app = appFor(w), a = wm.area(), par = w.parent && wins.get(w.parent), pi = par && info.get(w.parent);
    let rect;
    if (par && pi) { // iletişim kutusu: programın yerleştirdiği gibi, sahibine göre
      const o = contentOrigin(par);
      rect = { x: o.x + (w.x - pi.x), y: o.y - TB + (w.y - pi.y), w: w.w, h: w.h + TB };
    } else rect = { x: Math.round(a.x + (a.w - w.w) / 2), y: Math.round(a.y + Math.max(10, (a.h - w.h - TB) / 2.6)), w: w.w, h: w.h + TB };
    const win = wm.open(app, { rect, force: true });
    win.title = w.title || app.name; win.x11 = w.id;
    wins.set(w.id, win);
    wm.render(win); wm.emit("change");
  }

  function close(id) {
    info.delete(id);
    const p = pops.get(id);
    if (p) { p.view.close(); p.el.remove(); pops.delete(id); }
    const win = wins.get(id);
    if (win) { wins.delete(id); win.xGone = true; wm.close(win); }
  }

  function openPopup(w) {
    if (pops.has(w.id)) return place(w.id);
    const el = h("canvas.xpop");
    layer.append(el);
    const view = new XView(el, w.id, { keyboard: false, onGone: () => close(w.id) });
    pops.set(w.id, { el, view });
    place(w.id);
  }

  function place(id) {
    const p = pops.get(id), w = info.get(id);
    if (!p || !w) return;
    const par = wins.get(w.parent), pi = info.get(w.parent);
    let x = w.x, y = w.y;
    if (par && pi) { const o = contentOrigin(par); x = o.x + (w.x - pi.x); y = o.y + (w.y - pi.y); }
    p.el.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
    p.el.hidden = !!(par && (par.min || par.hidden));
  }
  const placeAll = () => { for (const id of pops.keys()) place(id); };

  function msg(m) {
    switch (m.t) {
      case "list": {
        const ids = new Set(m.wins.map(w => w.id));
        for (const id of [...wins.keys(), ...pops.keys()]) if (!ids.has(id)) close(id);
        // Önce sahip pencereler (iletişim kutuları ve menüler onlara göre yerleşir).
        for (const w of [...m.wins].sort((a, b) => (a.popup - b.popup) || (!!a.parent - !!b.parent))) open(w);
        break;
      }
      case "open": open(m.win); break;
      case "close": close(m.id); break;
      case "title": {
        const w = info.get(m.id); if (w) w.title = m.title;
        const win = wins.get(m.id);
        if (win) { win.title = m.title || win.app.name; win.app.name = win.title; wm.render(win); wm.emit("change"); }
        break;
      }
      case "geom": {
        info.set(m.win.id, m.win);
        if (m.win.popup) place(m.win.id); else placeAll();
        break;
      }
      case "activate": { const win = wins.get(m.id); if (win) wm.focus(win); break; }
      case "minimize": { const win = wins.get(m.id); if (win) wm.minimize(win); break; }
      case "reset": for (const id of [...wins.keys(), ...pops.keys()]) close(id); break;
    }
  }

  function connect() {
    ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/v1/xwin`);
    ws.onopen = () => { wait = 500; };
    ws.onmessage = e => { try { msg(JSON.parse(e.data)); } catch (x) { console.warn("xwin", x); } };
    ws.onclose = () => { ws = null; setTimeout(connect, wait); wait = Math.min(wait * 2, 15000); };
  }
  connect();

  wm.on("focus", win => { if (win?.x11) send({ t: "focus", id: win.x11 }); });
  wm.on("change", placeAll);
  // Açık menü varken kabukta başka bir yere tıklanınca menü kapanır (X'teki "dışarı tıklama").
  addEventListener("pointerdown", e => { if (pops.size && !e.target.closest?.(".xpop")) send({ t: "dismiss" }); }, true);
  return { wins, pops };
}
