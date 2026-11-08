#!/usr/bin/env node
// Başsız Chromium ile ekran görüntüsü (yalnız Docker; makeros/shell-smoke imajı).
//   node tools/shot.mjs --url URL --out dosya.png [--w 1440 --h 900 --scale 1 --mobile] [--login] [--js "ifade"] [--wait ms]
//     [--clicks "x,y;x,y"] [--type "metin"] [--after "ifade"]  (--type: tıklamalardan sonra gerçek tuş vuruşları; "\n" = Enter)  (--js'ten sonra gerçek fare tıklamaları, CDP Input; dokunmatikte --mobile ile dokunuş)
import { spawn } from "node:child_process";
import { writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const a = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const URL0 = a("--url"), OUT = a("--out", "/out/shot.png"), W = +a("--w", 1440), H = +a("--h", 900), S = +a("--scale", 1);
const CLICKS = (a("--clicks", "") || "").split(";").filter(Boolean).map(p => p.split(",").map(Number));
const MOBILE = process.argv.includes("--mobile"), LOGIN = process.argv.includes("--login"), JS = a("--js", ""), WAIT = +a("--wait", 1500);
const port = 9300 + Math.floor(Math.random() * 500);
const ch = spawn(process.env.CHROME || "chromium", ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "c-"))}`,
  "--no-sandbox", "--hide-scrollbars", `--window-size=${W},${H}`, "--force-color-profile=srgb", "about:blank"], { stdio: "ignore" });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let l; for (let i = 0; i < 50 && !l; i++) { await sleep(200); l = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json()).catch(() => null); }
const ws = new WebSocket(l.find(t => t.type === "page").webSocketDebuggerUrl); await new Promise(r => ws.onopen = r);
let id = 0; const pend = new Map();
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  if (m.method === "Runtime.exceptionThrown") console.error("JS hata:", m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") console.error("konsol:", m.params.args.map(x => x.value ?? x.description).join(" "));
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error") console.error("log:", m.params.entry.text, m.params.entry.url || ""); };
const cdp = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async e => (await cdp("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.result?.value;
await cdp("Page.enable"); await cdp("Runtime.enable"); await cdp("Log.enable");
await cdp("Emulation.setDeviceMetricsOverride", { width: W, height: H, deviceScaleFactor: S, mobile: MOBILE });
if (MOBILE) await cdp("Emulation.setTouchEmulationEnabled", { enabled: true });
if (LOGIN) {
  await cdp("Page.navigate", { url: new URL("/", URL0).href }); await sleep(800);
  await js(`(async()=>{let r=await fetch("/api/v1/auth/login",{method:"POST",headers:{"X-Maker":"1","Content-Type":"application/json"},body:JSON.stringify({user:"makeros",password:"makeros"})});if(!r.ok)await fetch("/api/v1/auth/login",{method:"POST",headers:{"X-Maker":"1","Content-Type":"application/json"},body:JSON.stringify({user:"maker",password:"maker"})});})()`);
}
await cdp("Page.navigate", { url: URL0 }); await sleep(WAIT);
if (JS) { const v = await js(`(async()=>{${JS}})()`); if (v !== undefined) console.log("js:", JSON.stringify(v)); await sleep(WAIT); }
for (const [x, y] of CLICKS) {
  if (MOBILE) {
    await cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y }] });
    await cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  } else {
    await cdp("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await sleep(50);
    await cdp("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 });
    await cdp("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 });
  }
  await sleep(WAIT);
}
const TYPE = a("--type", "");
for (const ch of TYPE.replace(/\\n/g, "\n")) {
  const enter = ch === "\n", key = enter ? "Enter" : ch;
  const base = enter ? { key, code: "Enter", windowsVirtualKeyCode: 13 } : { key, text: ch, unmodifiedText: ch };
  await cdp("Input.dispatchKeyEvent", { type: "keyDown", ...base, ...(enter ? { text: "\r" } : {}) });
  await cdp("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  await sleep(40);
}
if (TYPE) await sleep(WAIT);
const AFTER = a("--after", "");
if (AFTER) console.log("after:", JSON.stringify(await js(`(async()=>{${AFTER}})()`)));
const r = await cdp("Page.captureScreenshot", { format: "png" });
writeFileSync(OUT, Buffer.from(r.result.data, "base64")); console.log("📷", OUT);
ch.kill(); process.exit(0);
