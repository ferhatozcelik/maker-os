#!/usr/bin/env node
// Başsız Chrome ile kabuk duman testi (bağımlılıksız, CDP + Node yerleşik WebSocket).
//   node shell/tools/dev-server.mjs &   node shell/tools/smoke.mjs [--url http://localhost:8090] [--out dizin]
//   node shell/tools/dev-server.mjs --fresh &   node shell/tools/smoke.mjs --fresh    (önce kurulum sihirbazı)
// Önerilen: yalnız Docker ile → shell/Makefile (make smoke | smoke-fresh | smoke-makerd)
// Oturum açar, uygulamaları açar, kısayolları dener, ekran görüntüsü alır ve konsol hatalarını raporlar.
// M2: kurulum sihirbazı (--fresh), tercih eşitlemesi (GET/PUT /prefs), tembel yükleme, Wi-Fi anahtarı, perf HUD.
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const URL0 = arg("--url", "http://localhost:8090"), OUT = arg("--out", join(tmpdir(), "mk-smoke")), FRESH = process.argv.includes("--fresh");
let USER = "maker", PASS = "maker";
const CHROME = process.env.CHROME || (process.platform === "darwin" ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "chromium");
mkdirSync(OUT, { recursive: true });
const port = 9300 + Math.floor(Math.random() * 500);
const proc = spawn(CHROME, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "mk-chr-"))}`,
  "--no-first-run", "--window-size=1440,900", "--hide-scrollbars", "--disable-dev-shm-usage",
  ...(process.env.CHROME_NO_SANDBOX ? ["--no-sandbox"] : []), "about:blank"], { stdio: "ignore" });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let ver; for (let i = 0; i < 50 && !ver; i++) { await sleep(200); ver = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json()).catch(() => null); }
const page = ver.find(t => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise(r => ws.onopen = r);
let id = 0; const pend = new Map(), errors = [];
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  if (m.method === "Runtime.exceptionThrown") errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") errors.push(m.params.args.map(a => a.value ?? a.description).join(" "));
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error" && !/401 \(Unauthorized\)/.test(m.params.entry.text) && !/\/api\/v1\/setup$/.test(m.params.entry.url || "")) errors.push(m.params.entry.text + " " + (m.params.entry.url || ""));
};
const cdp = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async expr => { const r = await cdp("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "js"); return r.result?.result?.value; };
const shot = async name => { const r = await cdp("Page.captureScreenshot", { format: "png" }); writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64")); console.log("📷", join(OUT, name + ".png")); };
// Yeniden yükleme/gezinmeden sonra sabit bekleme yerine koşul beklenir (yük altında açılış uzayabilir).
const waitFor = async (expr, ms = 12000) => { for (let t = 0; t < ms; t += 200) { if (await js(expr).catch(() => false)) return true; await sleep(200); } return false; };
const SHELL_READY = `!!document.querySelector('.lk-card') || !!document.querySelector('#dock .di[data-app="makeros.settings"]')`;
const MOD = { alt: 1, ctrl: 2, meta: 4, shift: 8 };
async function key(k, mods = [], code) {
  const m = mods.reduce((a, x) => a | MOD[x], 0);
  const c = code || (k.length === 1 ? (/[a-z]/i.test(k) ? "Key" + k.toUpperCase() : /\d/.test(k) ? "Digit" + k : "") : k);
  const kc = { ArrowUp: 38, Tab: 9, Escape: 27, Enter: 13, F3: 114, F4: 115, " ": 32 }[k] || k.toUpperCase().charCodeAt(0);
  const text = k === "Enter" ? "\r" : k.length === 1 && !(m & 6) ? k : undefined;
  await cdp("Input.dispatchKeyEvent", { type: "keyDown", key: k, code: c === " " ? "Space" : c, modifiers: m, windowsVirtualKeyCode: kc, text });
  await cdp("Input.dispatchKeyEvent", { type: "keyUp", key: k, code: c === " " ? "Space" : c, modifiers: m, windowsVirtualKeyCode: kc });
}
async function drag(x1, y1, x2, y2) {
  await cdp("Input.dispatchMouseEvent", { type: "mouseMoved", x: x1, y: y1 });
  await cdp("Input.dispatchMouseEvent", { type: "mousePressed", x: x1, y: y1, button: "left", clickCount: 1 });
  for (let i = 1; i <= 8; i++) { await cdp("Input.dispatchMouseEvent", { type: "mouseMoved", x: x1 + (x2 - x1) * i / 8, y: y1 + (y2 - y1) * i / 8, button: "left", buttons: 1 }); await sleep(20); }
  await cdp("Input.dispatchMouseEvent", { type: "mouseReleased", x: x2, y: y2, button: "left", clickCount: 1 });
}
const click = async (x, y) => { await cdp("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 }); await cdp("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 }); };
const ok = (c, msg) => { console.log(c ? "✔" : "✘", msg); if (!c) process.exitCode = 1; };

await cdp("Runtime.enable"); await cdp("Log.enable"); await cdp("Page.enable");
const typeIn = async (sel, text) => { await js(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); e.focus(); e.select?.(); })()`); await cdp("Input.insertText", { text }); };
const clickSel = sel => js(`document.querySelector(${JSON.stringify(sel)}).click()`);
const step = () => js(`document.querySelector('.su-card')?.dataset.step`);
const next = async () => { await clickSel(".su-next"); await sleep(520); };

// ------------------------------------------------------------ M2: ilk açılış sihirbazı (--fresh)
if (FRESH) {
  // telefon (erişim noktası → captive portal "/")
  await cdp("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
  await cdp("Page.navigate", { url: URL0 + "/?ui=desktop" }); await sleep(1800);
  ok(await step() === "welcome", "telefon: sihirbaz açıldı (welcome)");
  await shot("m2-01-phone-welcome");
  await next(); ok(await step() === "region", "telefon: bölge adımı"); await shot("m2-02-phone-region");
  await next();
  if (await step() === "wifi") {
    await sleep(1200); // POST /setup/wifi/scan
    await js(`document.querySelector('.su-nets .su-opt')?.click()`); await sleep(200);
    await shot("m2-03-phone-wifi");
    await js(`[...document.querySelectorAll('.su-opt')].at(-1).click()`); await sleep(100); // atla
    await next();
  }
  await shot("m2-04-phone-account");
  // sihirbaz tamamlanmadığı için sunucu durumu değişmedi; masaüstü akışı aynı kurulumsuz cihazda
  await cdp("Emulation.clearDeviceMetricsOverride");
  // masaüstü / HDMI: tam akış
  await cdp("Page.navigate", { url: URL0 + "/?ui=desktop&fx=lite" }); await sleep(1800);
  ok(await step() === "welcome", "sihirbaz: hoş geldiniz");
  await shot("m2-05-welcome");
  await js(`[...document.querySelectorAll('.su-opt')].find(b => b.dataset.v?.startsWith("en"))?.click()`); await sleep(400);
  ok(await js(`document.querySelector('.su-page h1').textContent`) === "Welcome to MakerOS", "sihirbaz: dil anında İngilizce");
  await js(`[...document.querySelectorAll('.su-opt')].find(b => b.dataset.v?.startsWith("tr"))?.click()`); await sleep(400);
  ok(/hoş geldiniz/.test(await js(`document.querySelector('.su-page h1').textContent`)), "sihirbaz: Türkçeye dönüş");
  await next();
  await typeIn(".su-search", "berlin"); await sleep(200);
  await clickSel(".su-tz .su-opt"); await sleep(200);
  await shot("m2-06-region");
  await next();
  let wifiSsid = null;
  if (await step() === "wifi") {
    await sleep(1500); // POST /setup/wifi/scan
    ok(await js(`document.querySelectorAll('.su-nets .su-opt').length > 2`), "Wi-Fi: POST /setup/wifi/scan listesi");
    wifiSsid = await js(`(() => { const b = [...document.querySelectorAll('.su-nets .su-opt')].find(b => b.querySelector('.su-sig svg') && b.textContent && b.querySelectorAll('.su-sig svg').length === 2); b?.click(); return b?.querySelector('b').textContent || null; })()`);
    await sleep(200);
    if (wifiSsid) {
      await next(); ok(await step() === "wifi" && /8/.test(await js(`document.querySelector('.su-err').textContent`)), "Wi-Fi: kısa parola reddedildi");
      await typeIn(".su-pw input", "atolye-2026"); await sleep(100);
    }
    await shot("m2-07-wifi");
    await next();
  } else console.log("ℹ wifi_needed=false → Wi-Fi adımı atlandı");
  ok(await step() === "account", "sihirbaz: hesap");
  await typeIn('.su-field input[autocomplete=name]', "Ada Yılmaz"); await sleep(100);
  ok(await js(`document.querySelector('.su-field input[autocomplete=username]').value`) === "ada", "kullanıcı adı ad soyaddan türetildi (ada)");
  await typeIn('.su-field input[autocomplete=new-password]', "maker"); await sleep(100);
  await next(); ok(await step() === "account", "hesap: zayıf/kısa parola reddedildi");
  USER = "ada"; PASS = "Kurulum-2026!";
  await typeIn('.su-field input[autocomplete=new-password]', PASS);
  await js(`document.querySelectorAll('.su-field input[autocomplete=new-password]')[1].focus()`); await cdp("Input.insertText", { text: PASS }); await sleep(150);
  ok(+(await js(`document.querySelector('.su-meter').dataset.s`)) >= 3, "parola gücü göstergesi (iyi/güçlü)");
  await shot("m2-08-account");
  await next(); ok(await step() === "computer", "sihirbaz: bilgisayar adı");
  await typeIn(".su-field input", "atolye-pi"); await sleep(100);
  await shot("m2-09-computer");
  await next(); ok(await step() === "privacy", "sihirbaz: gizlilik");
  await shot("m2-10-privacy");
  await clickSel(".su-next"); await sleep(1800);
  ok(await step() === "done", "sihirbaz: kurulum tamamlandı");
  await shot("m2-11-done");
  const ses = await js(`fetch("/api/v1/session").then(r => r.json())`);
  ok(ses.user === "ada" && ses.setup_done === true, `oturum çerezi: ${JSON.stringify(ses)}`);
  const sys = await js(`fetch("/api/v1/system").then(r => r.json())`);
  ok(sys.hostname === "atolye-pi", `bilgisayar adı uygulandı (${sys.hostname})`);
  ok(typeof sys.cpu?.usage_pct === "number", "/system cpu.usage_pct mevcut");
  ok((await js(`fetch("/api/v1/setup").then(r => r.status)`)) === 409, "GET /setup → 409 (kurulum bitti)");
  await clickSel(".su-next"); await sleep(2500);
  ok(await js(`!document.querySelector('#setup') && document.querySelector('#lock').hidden && !!document.querySelector('.dock-inner')`), "sihirbaz → masaüstü (giriş istemeden)");
  await shot("m2-12-first-desktop");
}
const run = async (theme, fx) => {
  await cdp("Page.navigate", { url: `${URL0}/?ui=desktop&fx=${fx}` }); await sleep(1200);
  await js(`localStorage.setItem("mk.theme", '"${theme}"')`);
  return theme;
};
await run("light", "full");
await cdp("Page.reload"); await sleep(600); await waitFor(SHELL_READY); await sleep(300);
if (!FRESH) {
  await shot("01-login");
  ok(await js(`!!document.querySelector('.lk-card')`), "giriş ekranı (lock.js tembel yüklendi)");
  // Not: form ile giriş başsız Chrome'da "parolayı kaydet" balonunu açıp fare girdisini kilitliyor → fetch + yeniden yükleme
  await js(`fetch("/api/v1/auth/login", { method: "POST", headers: { "X-Maker": "1", "Content-Type": "application/json" }, body: JSON.stringify({ user: "${USER}", password: "${PASS}" }) })`);
}
await js(`localStorage.removeItem("mk.wm.session")`);
await cdp("Page.reload"); await sleep(600); await waitFor(`!!document.querySelector('#dock .di[data-app="makeros.settings"]')`); await sleep(400);
ok(await js(`document.querySelector('#lock').hidden`), "oturum açıldı");
await js(`localStorage.removeItem("mk.wm.session")`);
// tembel yükleme: masaüstü açılışında katman/kilit/mobil/sihirbaz modülleri yüklenmemeli
const res0 = await js(`performance.getEntriesByType("resource").map(e => [new URL(e.name).pathname, e.decodedBodySize])`);
const loaded = new Set(res0.map(r => r[0]));
for (const m of ["/shell/js/ui/launchpad.js", "/shell/js/ui/overview.js", "/shell/js/ui/cc.js", "/shell/js/ui/lock.js", "/shell/js/ui/mobile.js", "/shell/setup/setup.js", "/shell/js/perf.js", "/shell/css/lock.css", "/shell/css/mobile.css"])
  ok(!loaded.has(m), "tembel: açılışta yüklenmedi " + m);
const PREFETCH = /\/(find|switcher|layer)\.js$|overlays\.css$/;
const own = res0.filter(([p]) => /^\/shell\/(js|css|apps|setup)\/.*\.(js|css)$/.test(p) && !PREFETCH.test(p));
console.log(`ℹ açılış (vendor/i18n hariç, boşta ön yükleme hariç): ${own.length} dosya, ${(own.reduce((a, r) => a + r[1], 0) / 1024).toFixed(1)} KB`);
await shot("02-desktop");
// tercih eşitlemesi: PUT /prefs → yerel önbellek silinse de sunucudan geri gelir
await js(`import("/shell/js/prefs.js").then(m => m.setPref("accent", "purple"))`); await sleep(1300);
ok((await js(`fetch("/api/v1/prefs").then(r => r.json())`)).accent === "purple", "PUT /prefs (debounce) → sunucuda accent=purple");
await js(`localStorage.removeItem("mk.accent")`); await cdp("Page.reload"); await sleep(600); await waitFor(SHELL_READY); await sleep(400);
ok(await js(`document.documentElement.dataset.accent`) === "purple", "tercih yeniden yüklemede sunucudan geldi");
await js(`import("/shell/js/prefs.js").then(m => m.setPref("accent", "blue"))`); await sleep(1000);
// raftan uygulama aç
await js(`document.querySelector('.di[data-app="makeros.settings"]').click()`); await sleep(1500);
ok(await js(`__mk.wm.list().length === 1`), "Ayarlar penceresi açıldı");
await shot("03-settings");
await js(`__mk.launch("makeros.activity")`); await sleep(2500);
await js(`__mk.launch("makeros.terminal")`); await sleep(2500);
await shot("04-three-windows");
// sürükle
const r0 = await js(`JSON.stringify(__mk.wm.focused.rect)`); const rr = JSON.parse(r0);
await drag(rr.x + rr.w / 2, rr.y + 15, rr.x + rr.w / 2 - 200, rr.y + 80); await sleep(400);
const r1 = JSON.parse(await js(`JSON.stringify(__mk.wm.focused.rect)`));
ok(Math.abs(r1.x - (rr.x - 200)) < 3 && Math.abs(r1.y - (rr.y + 65)) < 3, `sürükleme ${rr.x},${rr.y} → ${r1.x},${r1.y}`);
// boyutlandır (sağ alt köşe)
await drag(r1.x + r1.w + 1, r1.y + r1.h + 1, r1.x + r1.w + 101, r1.y + r1.h + 51); await sleep(400);
const r2 = JSON.parse(await js(`JSON.stringify(__mk.wm.focused.rect)`));
ok(Math.abs(r2.w - r1.w - 100) < 4 && Math.abs(r2.h - r1.h - 50) < 4, `boyutlandırma ${r1.w}x${r1.h} → ${r2.w}x${r2.h}`);
// terminal echo
await sleep(500);
const termText = await js(`(() => { const f = __mk.wm.focused.view.frame.contentDocument; return f.querySelector('.xterm-rows')?.textContent || "" })()`);
ok(/[$#] *$/m.test(termText.trim()) || /\$/.test(termText), "terminal istemi görünüyor");
await js(`(() => { const t = __mk.wm.focused.view.frame.contentWindow; t.document.querySelector('.xterm-helper-textarea').focus(); })()`);
await cdp("Input.insertText", { text: "neofetch" }); await key("Enter"); await sleep(600);
await shot("05-terminal");
// yakınlaştır + küçült
await js(`__mk.wm.zoom(__mk.wm.focused)`); await sleep(500);
ok(await js(`__mk.wm.focused.state === "max"`), "yakınlaştırma");
await js(`__mk.wm.zoom(__mk.wm.focused)`); await sleep(400);
await key("m", ["meta"]); await sleep(600);
ok(await js(`__mk.wm.list().some(w => w.min)`), "⌘M küçültme");
// Bul
await key(" ", ["ctrl"]); await sleep(400);
ok(await js(`!!document.querySelector('.find')`), "Ctrl+Space → Bul");
await cdp("Input.insertText", { text: "12*(3+4)" }); await sleep(300);
ok(await js(`document.querySelector('.find-row .find-l')?.textContent.includes("84")`), "hesap makinesi 12*(3+4)=84");
await js(`document.querySelector('.find-in').value = ''`); await cdp("Input.insertText", { text: "wifi" }); await sleep(300);
await shot("06-find");
await key("Escape"); await sleep(300);
// Genel bakış
await key("F3"); await sleep(700);
ok(await js(`__mk.wm.overviewOn`), "F3 → genel bakış");
ok(await js(`performance.getEntriesByType("resource").some(e => e.name.endsWith("/ui/overview.js"))`), "tembel: overview.js ilk kullanımda yüklendi");
await shot("07-overview");
await key("Escape"); await sleep(600);
// uygulama değiştirici
await cdp("Input.dispatchKeyEvent", { type: "keyDown", key: "Meta", code: "MetaLeft", modifiers: 4 });
await cdp("Input.dispatchKeyEvent", { type: "keyDown", key: "Tab", code: "Tab", modifiers: 4, windowsVirtualKeyCode: 9 }); await sleep(300);
await shot("08-switcher");
await cdp("Input.dispatchKeyEvent", { type: "keyUp", key: "Meta", code: "MetaLeft", modifiers: 0 }); await sleep(400);
ok(await js(`!document.querySelector('.sw')`), "⌘Tab değiştirici kapandı");
// Uygulamalar ızgarası + bildirim merkezi + raf menüsü
await key("F4"); await sleep(500); await shot("08b-apps-grid"); await key("Escape"); await sleep(300);
await js(`document.querySelector('.mb-clock').click()`); await sleep(400); await shot("08c-notifications"); await js(`document.querySelector('.mb-clock').click()`);
await js(`document.querySelector('.di[data-app="makeros.terminal"]').dispatchEvent(new MouseEvent('contextmenu', {bubbles:true}))`); await sleep(300); await shot("08d-dock-menu");
await key("Escape"); await sleep(200);
// Kontrol Merkezi + menü
await js(`document.querySelector('.mb-right button[title="Kontrol Merkezi"]').click()`); await sleep(300);
await shot("09-control-center");
await js(`document.querySelector('.cc-tile .cc-ic').click()`); await sleep(900);
ok((await js(`fetch("/api/v1/net").then(r => r.json())`)).wifi_enabled === false, "Kontrol Merkezi: Wi-Fi kapatıldı (PUT /net/wifi)");
await shot("09b-cc-wifi-off");
await js(`document.querySelector('.cc-tile .cc-ic').click()`); await sleep(900);
ok((await js(`fetch("/api/v1/net").then(r => r.json())`)).wifi_enabled === true, "Kontrol Merkezi: Wi-Fi yeniden açıldı");
await key("Escape"); await js(`document.body.dispatchEvent(new PointerEvent('pointerdown', {bubbles:true}))`);
await js(`document.querySelector('.mb-logo').dispatchEvent(new PointerEvent('pointerdown', {bubbles:true}))`); await sleep(300);
await shot("10-logo-menu");
await js(`document.querySelector('#desktop').dispatchEvent(new PointerEvent('pointerdown', {bubbles:true}))`);
// Ayarlar sayfaları
for (const p of ["keyboard", "appearance", "general", "network", "datetime", "language", "user", "power", "wifi"]) {
  await js(`__mk.openSettings("${p}")`); await sleep(700);
  const err = await js(`(() => { const d = __mk.wm.byApp("makeros.settings")[0].view.frame.contentDocument; return d.querySelector('.empty.err')?.textContent || "" })()`);
  ok(!err, `Ayarlar #${p}${err ? ": " + err : ""}`);
  if (p === "keyboard" || p === "appearance") await shot("11-settings-" + p);
}
await js(`__mk.wm.focus(__mk.wm.byApp("makeros.activity")[0])`); await sleep(2500);
await shot("12-activity");
await js(`__mk.wm.minimize(__mk.wm.byApp("makeros.activity")[0])`); await sleep(600);
ok(await js(`__mk.wm.byApp("makeros.activity")[0].view._vis === false`), "küçültülen Etkinlik penceresine visibility:false gönderildi");
await js(`__mk.wm.focus(__mk.wm.byApp("makeros.activity")[0])`); await sleep(800);
// koyu tema + lite
await js(`localStorage.setItem("mk.theme", '"dark"'); localStorage.setItem("mk.fx", '"lite"'); dispatchEvent(new StorageEvent("storage", {key: "mk.theme"}))`); await sleep(800);
await shot("13-dark-lite");
// İngilizce arayüz
// dil artık sunucu tercihi (GET/PUT /prefs): setPref + hemen gönder
await js(`import("/shell/js/prefs.js").then(async m => { m.setPref("lang", "en"); await m.flushPrefs(); })`); await cdp("Page.reload"); await sleep(800); await waitFor(`document.querySelector('.mb-item[data-k=file]')?.textContent === "File"`); await sleep(300);
ok(await js(`document.querySelector('.mb-item[data-k=file]')?.textContent === "File"`), "İngilizce arayüz");
await js(`import("/shell/js/prefs.js").then(async m => { m.setPref("lang", "tr"); await m.flushPrefs(); })`);
// perf HUD
await cdp("Page.navigate", { url: URL0 + "/?ui=desktop&perf=1&fx=lite" }); await sleep(2600);
ok(/fps/.test(await js(`document.querySelector('#perf')?.textContent || ""`)), "?perf=1 → fps/uzun görev göstergesi");
await shot("13b-perf-hud");
// mobil
await cdp("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
await cdp("Page.navigate", { url: URL0 + "/?ui=desktop" }); await sleep(2500);
await shot("14-mobile-home");
await js(`__mk.launch("makeros.settings")`); await sleep(1500);
await shot("15-mobile-settings");
await js(`localStorage.setItem("mk.theme", '"light"'); localStorage.setItem("mk.fx", '"auto"')`);
await cdp("Emulation.clearDeviceMetricsOverride");
// bağımsız uygulama (telefon tarayıcısı)
await cdp("Page.navigate", { url: URL0 + "/shell/apps/activity/" }); await sleep(2000);
await shot("16-standalone-activity");

console.log(errors.length ? "Konsol hataları:\n  " + [...new Set(errors)].join("\n  ") : "✔ konsol hatası yok");
if (errors.length) process.exitCode = 1;
ws.close(); proc.kill();
