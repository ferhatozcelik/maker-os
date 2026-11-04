#!/usr/bin/env node
// Uçtan uca Uzak Ekran testi (yalnız Docker): başsız Chromium -> kabuk "Uzak Ekran"
// (noVNC) -> makerd /api/v1/vnc -> wayvnc -> labwc (sanal masaüstü kabı).
// Klavye olayları VNC üzerinden foot terminaline yazılır; test kabı dosyanın
// masaüstü kabında oluştuğunu ayrıca doğrular (Makefile: vnc-e2e).
//   node tools/vnc-e2e.mjs --url http://makerd:8080 --out /out
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const URL0 = arg("--url", "http://makerd:8080"), OUT = arg("--out", "/out"), MARK = arg("--mark", "/tmp/vnc-e2e-ok");
mkdirSync(OUT, { recursive: true });
const port = 9300 + Math.floor(Math.random() * 500);
const chrome = spawn(process.env.CHROME || "chromium", ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "c-"))}`,
  "--no-first-run", "--window-size=1280,800", "--hide-scrollbars", "--disable-dev-shm-usage", "--no-sandbox", "about:blank"], { stdio: "ignore" });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let list; for (let i = 0; i < 50 && !list; i++) { await sleep(200); list = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json()).catch(() => null); }
const ws = new WebSocket(list.find(t => t.type === "page").webSocketDebuggerUrl);
await new Promise(r => ws.onopen = r);
let id = 0; const pend = new Map(), errors = [];
ws.onmessage = e => { const m = JSON.parse(e.data);
  if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  if (m.method === "Runtime.exceptionThrown") errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text); };
const cdp = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async expr => { const r = await cdp("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "js"); return r.result?.result?.value; };
const shot = async name => { const r = await cdp("Page.captureScreenshot", { format: "png" }); writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64")); console.log("📷", join(OUT, name + ".png")); };
let fail = 0; const ok = (c, msg) => { console.log(c ? "✔" : "✘", msg); if (!c) fail++; };
const nav = async u => { await cdp("Page.navigate", { url: u }); await sleep(800); };

await cdp("Runtime.enable"); await cdp("Page.enable");
await nav(URL0 + "/");
const login = await js(`fetch("/api/v1/auth/login", { method: "POST", headers: { "X-Maker": "1", "Content-Type": "application/json" }, body: JSON.stringify({ user: "maker", password: "maker" }) }).then(r => r.status)`);
ok(login === 200, `giriş (HTTP ${login})`);

const t0 = Date.now();
await nav(URL0 + "/shell/apps/screen/");
let connected = false;
for (let i = 0; i < 60 && !connected; i++) { await sleep(250); connected = await js(`document.querySelector(".bar .st")?.classList.contains("ok")`); }
const tConn = Date.now() - t0;
ok(connected, `noVNC bağlandı (${tConn} ms, sayfa açılışından)`);

// Çerçeve gerçekten çiziliyor mu: tuvalde siyah olmayan piksel oranı.
await sleep(1200);
const px = await js(`(() => { const c = document.querySelector(".screen canvas"); if (!c) return null;
  const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data; let n = 0, s = 0;
  for (let i = 0; i < d.length; i += 4 * 97) { s++; if (d[i] + d[i+1] + d[i+2] > 30) n++; } return { w: c.width, h: c.height, lit: n / s }; })()`);
ok(px && px.w === 1280 && px.h === 720, `uzak masaüstü çözünürlüğü ${px?.w}x${px?.h}`);
ok(px && px.lit > 0.05, `ekran içeriği görünüyor (aydınlık örnek oranı ${(px?.lit * 100).toFixed(1)}%)`);
await shot("vnc-1-connected");

// Klavye: foot terminaline VNC üzerinden komut yaz.
await js(`(() => { const c = document.querySelector(".screen canvas"); const r = c.getBoundingClientRect();
  const o = { bubbles: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, button: 0 };
  c.dispatchEvent(new MouseEvent("mousedown", o)); c.dispatchEvent(new MouseEvent("mouseup", o)); })()`);
await sleep(300);
const cmd = `touch ${MARK} && echo VNC_E2E_TAMAM`;
const lat = await js(`(async () => { const sleep = ms => new Promise(r => setTimeout(r, ms));
  // RFB nesnesine modül dışından erişim yok: klavye olaylarını tuvale gönder.
  const c = document.querySelector(".screen canvas"); c.focus();
  const t0 = performance.now();
  for (const ch of ${JSON.stringify(cmd)}) {
    const o = { key: ch, code: "", bubbles: true };
    c.dispatchEvent(new KeyboardEvent("keydown", o)); c.dispatchEvent(new KeyboardEvent("keyup", o)); await sleep(15);
  }
  c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", bubbles: true }));
  c.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", code: "Enter", bubbles: true }));
  return performance.now() - t0; })()`);
ok(lat > 0, `${cmd.length} karakter VNC ile gönderildi (${Math.round(lat)} ms)`);
await sleep(1500);
await shot("vnc-2-typed");

// Telefon görünümü
await cdp("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
await sleep(800);
await shot("vnc-3-phone");
await cdp("Emulation.clearDeviceMetricsOverride");

ok(errors.length === 0, `konsol hatası yok${errors.length ? ": " + errors.join(" | ") : ""}`);
writeFileSync(join(OUT, "vnc-e2e.json"), JSON.stringify({ connected, connect_ms: tConn, frame: px, typed_ms: Math.round(lat), errors, fail }, null, 2));
chrome.kill();
process.exit(fail ? 1 : 0);
