#!/usr/bin/env node
// PWA simgeleri: icons/logo.svg → icons/pwa-*.png (başsız Chromium; yalnız Docker, makeros/shell-smoke imajı).
//   docker run --rm --user $(id -u):$(id -g) -e HOME=/tmp -v "$PWD/shell":/shell --entrypoint node makeros/shell-smoke:dev /shell/tools/gen-pwa-icons.mjs
// any: saydam köşeli logo · maskable/apple: logonun degradesi tüm zemini doldurur, işaret güvenli alanda (köşeler kırpılır).
import { spawn } from "node:child_process";
import { readFileSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
const dir = join(dirname(fileURLToPath(import.meta.url)), "../icons");
const svg = readFileSync(join(dir, "logo.svg"), "utf8");
const OUT = [
  ["pwa-192.png", 192, 1, null], ["pwa-512.png", 512, 1, null],
  ["pwa-maskable-512.png", 512, 1.2, "linear-gradient(160deg,#40444b,#16171a)"], ["apple-touch-icon.png", 180, 1.2, "linear-gradient(160deg,#40444b,#16171a)"],
];
const port = 9800 + Math.floor(Math.random() * 100);
const ch = spawn(process.env.CHROME || "chromium", ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "c-"))}`,
  "--no-sandbox", "--hide-scrollbars", "--force-color-profile=srgb", "about:blank"], { stdio: "ignore" });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let l; for (let i = 0; i < 50 && !l; i++) { await sleep(200); l = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json()).catch(() => null); }
const ws = new WebSocket(l.find(t => t.type === "page").webSocketDebuggerUrl); await new Promise(r => ws.onopen = r);
let id = 0; const pend = new Map();
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
const cdp = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await cdp("Page.enable");
await cdp("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } });
for (const [name, size, scale, bg] of OUT) {
  await cdp("Emulation.setDeviceMetricsOverride", { width: size, height: size, deviceScaleFactor: 1, mobile: false });
  const s = Math.round(size * scale), pad = Math.round((size - s) / 2);
  const html = `<!doctype html><html><body style="margin:0;width:${size}px;height:${size}px;overflow:hidden;position:relative;background:${bg || "transparent"}">` +
    `<img src="data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}" style="position:absolute;left:${pad}px;top:${pad}px;width:${s}px;height:${s}px"></body></html>`;
  await cdp("Page.navigate", { url: "data:text/html;base64," + Buffer.from(html).toString("base64") });
  await sleep(400);
  const r = await cdp("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: size, height: size, scale: 1 } });
  writeFileSync(join(dir, name), Buffer.from(r.result.data, "base64"));
  console.log("🖼 ", name, size + "px");
}
ch.kill(); process.exit(0);
