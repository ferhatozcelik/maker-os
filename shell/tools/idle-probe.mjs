#!/usr/bin/env node
// Boşta çalışma denetimi (yalnız Docker): sayfa açıldıktan sonra 6 sn boyunca
// layout/style/paint/script sayaçları ve çalışan animasyonlar. Boşta hepsi ~0 olmalı.
//   node tools/idle-probe.mjs --url http://makerd:8080/ [--login]
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const URL0 = arg("--url", "http://makerd:8080/"), LOGIN = process.argv.includes("--login");
const port = 9300 + Math.floor(Math.random() * 500);
const ch = spawn(process.env.CHROME || "chromium", ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "c-"))}`, "--no-sandbox", "--window-size=1280,720", "about:blank"], { stdio: "ignore" });
const sleep = ms => new Promise(r => setTimeout(r, ms));
let l; for (let i = 0; i < 50 && !l; i++) { await sleep(200); l = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json()).catch(() => null); }
const ws = new WebSocket(l.find(t => t.type === "page").webSocketDebuggerUrl); await new Promise(r => ws.onopen = r);
let id = 0; const pend = new Map(), anims = new Map();
ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  if (m.method === "Animation.animationStarted") { const a = m.params.animation; anims.set(a.id, `${a.type} ${a.name || ""} dur=${a.source?.duration} iter=${a.source?.iterations} node=${a.source?.backendNodeId}`); } };
const cdp = (method, params = {}) => new Promise(r => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async e => (await cdp("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.result?.value;
await cdp("Page.enable"); await cdp("Performance.enable"); await cdp("Animation.enable");
await cdp("Page.navigate", { url: URL0 }); await sleep(2500);
if (LOGIN) { await js(`fetch("/api/v1/auth/login",{method:"POST",headers:{"X-Maker":"1","Content-Type":"application/json"},body:JSON.stringify({user:"maker",password:"maker"})})`); await cdp("Page.reload"); await sleep(4000); }
const m = async () => Object.fromEntries((await cdp("Performance.getMetrics")).result.metrics.map(x => [x.name, x.value]));
anims.clear();
// rAF sayacı + uzun süreli zamanlayıcılar
await js(`window.__raf=0; (function f(){ window.__raf++; requestAnimationFrame(f); })(); true`);
const a = await m(); await sleep(6000); const b = await m();
const d = k => +(b[k] - a[k]).toFixed(3);
const out = { url: URL0, login: LOGIN, layouts: d("LayoutCount"), styles: d("RecalcStyleCount"), script_s: d("ScriptDuration"), task_s: d("TaskDuration"), layout_s: d("LayoutDuration"), style_s: d("RecalcStyleDuration"), animations: [...anims.values()] };
// Hangi elemanlar animasyonlu?
out.animated_nodes = await js(`[...document.getAnimations({subtree:true})].filter(a=>a.playState==="running").slice(0,10).map(a=>{const t=a.effect?.target; return (a.animationName||a.constructor.name)+" "+(t?(t.tagName+"."+[...t.classList].join(".")):"")+" iter="+a.effect?.getTiming().iterations})`);
out.shadow_anims = await js(`(()=>{const r=[];const walk=n=>{for(const e of n.querySelectorAll("*")){if(e.shadowRoot){for(const a of e.shadowRoot.getAnimations?.()||[]) if(a.playState==="running") r.push((a.animationName||"")+" in "+e.tagName); walk(e.shadowRoot);}}};walk(document);return r.slice(0,10)})()`);
console.log(JSON.stringify(out, null, 1));
ch.kill(); process.exit(0);
