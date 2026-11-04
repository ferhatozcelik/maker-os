// Etkinlik Monitörü: events WS "system" + "processes" konuları, canvas çizgi grafikleri.
import { app, t } from "/shell/js/app-kit.js";
import { h, $, bytes, duration } from "/shell/js/lib.js";
import { get, post } from "/shell/js/api.js";
import { prefs } from "/shell/js/prefs.js";

app.init({ title: t("app.activity") });
app.chrome({ tbRight: 290 });
const N = 60, hist = { cpu: [], mem: [], temp: [] };
let procs = [], sortKey = "cpu_pct", sortDir = -1, selPid = null, sys = null;
const q = $("#q"), kill = $("#kill"), rows = $("#rows"), status = $("#status");
q.placeholder = t("launch.search"); kill.textContent = t("act.kill");

const accent = () => getComputedStyle(document.documentElement).getPropertyValue("--accent").trim();
const card = (k, label, color) => {
  const c = h("canvas"), n = h("span.n"), sub = h("span");
  $("#graphs").append(h("div.card", h("div.k", h("span", label), sub), n, c));
  return { k, c, n, sub, color };
};
const G = [card("cpu", t("act.cpu")), card("mem", t("act.mem"), "#9b55e0"), card("temp", t("act.temp"), "#f08a1c")];

function spark({ c, color, k }, max) {
  const dpr = devicePixelRatio || 1, w = c.clientWidth, hh = c.clientHeight;
  if (!w) return;
  if (c.width !== w * dpr) { c.width = w * dpr; c.height = hh * dpr; }
  const x = c.getContext("2d"), d = hist[k], col = color || accent();
  x.setTransform(dpr, 0, 0, dpr, 0, 0); x.clearRect(0, 0, w, hh);
  if (d.length < 2) return;
  const px = i => w - (d.length - 1 - i) * (w / (N - 1)), py = v => hh - 2 - (Math.min(v, max) / max) * (hh - 6);
  x.beginPath(); d.forEach((v, i) => i ? x.lineTo(px(i), py(v)) : x.moveTo(px(i), py(v)));
  x.strokeStyle = col; x.lineWidth = 1.6; x.lineJoin = "round"; x.stroke();
  x.lineTo(px(d.length - 1), hh); x.lineTo(px(0), hh); x.closePath();
  const g = x.createLinearGradient(0, 0, 0, hh); g.addColorStop(0, col + "55"); g.addColorStop(1, col + "00");
  x.fillStyle = g; x.fill();
}
const push = (k, v) => { hist[k].push(v); if (hist[k].length > N) hist[k].shift(); };

let shown = app.visible;
function onSystem(s) {
  sys = s;
  if (!shown) return; // gizliyken veri biriktirme/çizim yok
  const cpu = s.cpu.usage_pct ?? Math.min(100, 100 * s.cpu.load[0] / (s.cpu.cores || 1));
  push("cpu", cpu); push("mem", 100 * s.mem.used / s.mem.total); push("temp", s.cpu.temp_c ?? 0);
  G[0].n.textContent = `%${cpu.toFixed(0)}`; G[0].sub.textContent = `${t("act.load")} ${s.cpu.load.map(v => v.toFixed(2)).join(" ")}`;
  G[1].n.textContent = bytes(s.mem.used); G[1].sub.textContent = `/ ${bytes(s.mem.total)}`;
  G[2].n.textContent = s.cpu.temp_c != null ? `${s.cpu.temp_c.toFixed(1)} °C` : "—"; G[2].sub.textContent = `${s.cpu.freq_mhz} MHz${s.throttled ? " · ⚠" : ""}`;
  spark(G[0], 100); spark(G[1], 100); spark(G[2], 90);
  status.textContent = `${s.hostname} · ${t("set.gen.uptime")} ${duration(s.uptime_s)} · ${procs.length} ${t("act.procs")}`;
}

const COLS = [["name", "act.name"], ["pid", "PID", 1], ["user", "act.user", 0, 1], ["cpu_pct", "% CPU", 1], ["rss", "act.rss", 1], ["slice", "act.slice", 0, 1]];
function head() {
  $("#head").replaceChildren(...COLS.map(([k, l, num, hide]) => h("th" + (num ? ".num" : "") + (hide ? ".hide-s" : "") + (k === sortKey ? ".sort" : ""),
    { onclick: () => { sortDir = k === sortKey ? -sortDir : (num ? -1 : 1); sortKey = k; head(); render(); } }, (l.includes(".") ? t(l) : l) + (k === sortKey ? (sortDir < 0 ? " ▾" : " ▴") : ""))));
}
function render() {
  const f = q.value.toLocaleLowerCase("tr");
  const list = procs.filter(p => !f || p.name.toLocaleLowerCase("tr").includes(f) || String(p.pid) === f)
    .sort((a, b) => (typeof a[sortKey] === "string" ? a[sortKey].localeCompare(b[sortKey], "tr") : a[sortKey] - b[sortKey]) * sortDir);
  rows.replaceChildren(...list.map(p => h("tr" + (p.pid === selPid ? ".sel" : ""), { onclick: () => { selPid = selPid === p.pid ? null : p.pid; kill.disabled = !selPid; render(); } },
    h("td", p.name), h("td.num", p.pid), h("td.hide-s", p.user), h("td.num", p.cpu_pct.toFixed(1)), h("td.num", bytes(p.rss)), h("td.hide-s", h("span.slice", (p.slice || "").replace(".slice", ""))))));
}
q.oninput = render;
kill.onclick = async e => {
  const p = procs.find(x => x.pid === selPid); if (!p) return;
  if (!confirm(t("act.confirm", { name: p.name, pid: p.pid }))) return;
  try { await post(`/processes/${p.pid}/signal`, { signal: e.altKey ? "KILL" : "TERM" }); selPid = null; kill.disabled = true; }
  catch (x) { alert(x.message); }
};
head();
try { procs = await get("/processes"); render(); onSystem(await get("/system")); } catch (e) { status.textContent = e.message; }
app.events.subscribe("system", onSystem);
// "processes" konusu yalnız pencere görünürken abone (makerd süreç taramasını durdurur)
const onProcs = p => { procs = p; if (selPid && !p.some(x => x.pid === selPid)) { selPid = null; kill.disabled = true; } render(); };
let unsubProcs = shown ? app.events.subscribe("processes", onProcs) : null;
app.onVisibility(v => {
  shown = v;
  if (v && !unsubProcs) { unsubProcs = app.events.subscribe("processes", onProcs); get("/processes").then(onProcs).catch(() => {}); if (sys) onSystem(sys); }
  else if (!v && unsubProcs) { unsubProcs(); unsubProcs = null; }
});
const redraw = () => { if (shown) G.forEach((g, i) => spark(g, [100, 100, 90][i])); };
prefs.on("change", redraw);
let rt = 0; addEventListener("resize", () => { cancelAnimationFrame(rt); rt = requestAnimationFrame(redraw); });
