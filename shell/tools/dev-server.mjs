#!/usr/bin/env node
// MakerOS kabuğu için geliştirme sunucusu (bağımlılıksız).
// shell/ dizinini sunar ve docs/api.md uç noktalarını sahte veriyle uygular:
// oturum, sistem, ağ, uygulamalar, süreçler, bildirimler, events WS ve sahte terminal.
//   node shell/tools/dev-server.mjs [--port 8090]      kullanıcı: maker / maker
//   node shell/tools/dev-server.mjs --fresh            ilk açılış: kullanıcı yok, GET /setup + POST /setup/complete
//   POST /api/v1/dev/reset (yalnız geliştirme)         --fresh durumuna geri döner (duman testi için)
// Gerçek arka uç: makerd --mock --web ./shell
import { createServer } from "node:http";
import { createHash, randomBytes } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const PORT = Number(process.argv[process.argv.indexOf("--port") + 1]) || Number(process.env.PORT) || 8090;
const MIME = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8", ".svg": "image/svg+xml", ".json": "application/json", ".woff2": "font/woff2", ".png": "image/png", ".jpg": "image/jpeg", ".txt": "text/plain; charset=utf-8", ".md": "text/plain; charset=utf-8" };

// ------------------------------------------------------------ durum
const FRESH = process.argv.includes("--fresh");
let users = FRESH ? {} : { maker: { password: "maker", admin: true } };
let setupDone = !FRESH, wifiOn = true;
const prefsByUser = new Map();
const sessions = new Map(), loginTries = new Map();
const t0 = Date.now();
let hostname = "makeros", tz = "Europe/Istanbul", ntp = true, locale = { lang: "tr_TR.UTF-8", keymap: "tr" };
let load = [0.42, 0.38, 0.31], usage = 12, temp = 47.5, memUsed = 1.1e9;
const knownWifi = new Set(["Atölye", "eduroam"]);
let activeSsid = FRESH ? "" : "Atölye";
// Yazılım güncellemesi benzetimi: 1.0.1 yayında; kurulum ~12 sn sürer (indirme → doğrulama → kurulum → yeniden başlatma).
const update = { checked: "", started: 0 };
const updateState = () => {
  const st = { current: "1.0.0", repo: "ferhatozcelik/maker-os", checked: update.checked || undefined, phase: "idle", progress: 0, can_apply: true,
    available: { version: "1.0.1", tag: "v1.0.1", name: "MakerOS 1.0.1", notes: "- Yerel AI Asistanı ve sohbet modu\n- Otomatik Başlatma ve Cron zamanlayıcı\n- Yazılım güncelleme iyileştirmeleri", published: "2026-11-08T10:00:00Z", url: "https://github.com/ferhatozcelik/maker-os/releases", size: 420e6 } };
  if (update.started) {
    const p = Math.min(100, Math.round((Date.now() - update.started) / 120));
    Object.assign(st, { progress: p, phase: p < 60 ? "downloading" : p < 62 ? "verifying" : p < 100 ? "installing" : "rebooting",
      message: p < 60 ? `İndiriliyor: ${Math.round(p * 7)} / 420 MB` : p < 100 ? "Kuruluyor" : "Kuruldu. Yeni sürümle yeniden başlatılıyor…", job: "up1" });
  }
  return st;
};
const notifications = [
  { id: "n1", app: "makeros.system", title: "MakerOS'a hoş geldiniz", body: "Kısayollar için Ayarlar → Klavye bölümüne bakın.", time: new Date(t0 - 6e5).toISOString(), read: false },
  { id: "n2", app: "makeros.update", title: "Sistem güncel", body: "MakerOS 1.0.0 çalışıyor.", time: new Date(t0 - 36e5).toISOString(), read: true },
];
const procNames = ["makerd", "cog", "WPEWebProcess", "WPENetworkProcess", "cage", "systemd", "NetworkManager", "systemd-journald", "bash", "python3", "node", "wpa_supplicant", "systemd-resolved", "rauc", "dbus-daemon", "zram-mon", "kworker/0:1", "avahi-daemon"];
const procs = procNames.map((name, i) => ({ pid: [412, 655, 702, 703, 640, 1, 380, 210, 1288, 1402, 1530, 395, 301, 520, 290, 233, 18, 344][i], name,
  user: ["bash", "python3", "node"].includes(name) ? "maker" : name.startsWith("WPE") || name === "cog" || name === "cage" ? "maker" : "root",
  cpu_pct: 0, rss: Math.round((5 + Math.random() * 120) * 1048576), slice: name.startsWith("WPE") || name === "cog" || name === "cage" ? "shell.slice" : ["bash", "python3", "node"].includes(name) ? "apps.slice" : "system.slice" }));
const apps = [
  ["settings", "Ayarlar", ["system"]], ["terminal", "Terminal", ["system", "dev"]], ["activity", "Etkinlik Monitörü", ["system"]],
  ["files", "Dosyalar", ["system"]], ["python", "Python", ["dev"]], ["camera", "Kamera", ["hardware"]], ["gpio", "GPIO", ["hardware"]], ["agents", "AI Ajanları", ["dev"]], ["plugins", "Eklentiler", ["system"]], ["browser", "Tarayıcı", ["network"]],
].map(([n, name, categories]) => ({ id: "makeros." + n, name, icon: `/shell/icons/${n}.svg`, kind: "web", url: `/shell/apps/${n}/`, categories }));

const wobble = (v, d, lo, hi) => Math.min(hi, Math.max(lo, v + (Math.random() - 0.5) * d));
setInterval(() => {
  usage = wobble(usage, 14, 3, 96); temp = wobble(temp, 1.2, 41, 72); memUsed = wobble(memUsed, 6e7, 8e8, 2.6e9);
  load = [wobble(load[0], .3, .05, 3.8), wobble(load[1], .08, .05, 3), wobble(load[2], .03, .05, 2.5)];
  for (const p of procs) p.cpu_pct = Math.max(0, +(Math.random() ** 3 * (p.name === "cog" || p.name.startsWith("WPE") ? 30 : 12)).toFixed(1));
}, 1000);
const system = () => ({ hostname, os: { name: "MakerOS", version: "1.0.0" }, kernel: "6.18.4-v8+ #1 SMP PREEMPT", model: "Raspberry Pi 4 Model B Rev 1.5",
  uptime_s: Math.round((Date.now() - t0) / 1000) + 8040,
  cpu: { cores: 4, freq_mhz: Math.round(1200 + usage * 6), temp_c: +temp.toFixed(1), load: load.map(v => +v.toFixed(2)), usage_pct: +usage.toFixed(1) },
  mem: { total: 4031e6, used: Math.round(memUsed), available: Math.round(4031e6 - memUsed), zram_used: 12e6 }, disk: { total: 31.2e9, used: 6.4e9 }, throttled: false });
const net = () => ({ online: !!(wifiOn && activeSsid), wifi_enabled: wifiOn, ifaces: [
  { name: "eth0", type: "ethernet", state: "disconnected", ipv4: [], mac: "dc:a6:32:01:02:03" },
  { name: "wlan0", type: "wifi", state: !wifiOn ? "unavailable" : activeSsid ? "connected" : "disconnected", ipv4: wifiOn && activeSsid ? ["192.168.1.42/24"] : [], mac: "dc:a6:32:01:02:04", ssid: wifiOn && activeSsid || undefined },
], hotspot: { active: !setupDone, ssid: "MakerOS" } });
// makerd ile aynı biçim (docs/api.md → "İlk açılış kurulumu")
const setupInfo = () => ({ setup_done: false, hostname: "makeros-0204", defaults: { lang: "tr_TR.UTF-8", keymap: "trq", timezone: "Europe/Istanbul" },
  languages: [["tr_TR.UTF-8", "Türkçe"], ["en_US.UTF-8", "English (US)"], ["de_DE.UTF-8", "Deutsch"]].map(([id, name]) => ({ id, name })),
  keymaps: [["trq", "Türkçe Q"], ["trf", "Türkçe F"], ["us", "İngilizce (ABD)"], ["gb", "İngilizce (BK)"], ["de", "Almanca"], ["fr", "Fransızca"]].map(([id, name]) => ({ id, name })),
  timezones: ["Europe/Istanbul", "Europe/Berlin", "Europe/London", "Europe/Paris", "Europe/Amsterdam", "Europe/Kyiv", "Asia/Baku", "Asia/Tokyo", "America/New_York", "America/Los_Angeles", "UTC"],
  online: !!activeSsid, wifi_needed: !activeSsid, hotspot: !setupDone, min_password: 6 });
const procList = () => [...procs].sort((a, b) => b.cpu_pct - a.cpu_pct);
const wifiScan = () => [["Atölye", 82, "wpa2"], ["eduroam", 64, "enterprise"], ["Kafe_Misafir", 48, "open"], ["Ev-5G", 71, "wpa3"], ["TP-LINK_42", 30, "wpa2"], ["Kütüphane", 22, "wpa2"]]
  .map(([ssid, signal, security]) => ({ ssid, signal: Math.round(wobble(signal, 8, 5, 100)), security, known: knownWifi.has(ssid), active: ssid === activeSsid }));

// ------------------------------------------------------------ yardımcılar
const cookie = req => Object.fromEntries((req.headers.cookie || "").split(/;\s*/).filter(Boolean).map(c => c.split("=").map(decodeURIComponent)));
const sess = req => sessions.get(cookie(req).mk_session);
function send(res, code, body, headers = {}) {
  if (body === undefined) { res.writeHead(code, headers); return res.end(); }
  res.writeHead(code, { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store", ...headers });
  res.end(JSON.stringify(body));
}
const err = (res, code, c, message) => send(res, code, { error: { code: c, message } });
const body = req => new Promise(r => { let d = ""; req.on("data", c => d += c); req.on("end", () => { try { r(d ? JSON.parse(d) : {}); } catch { r(null); } }); });
const delay = ms => new Promise(r => setTimeout(r, ms));

// ------------------------------------------------------------ REST
async function api(req, res, path) {
  const m = req.method;
  if (["POST", "PUT", "DELETE"].includes(m) && req.headers["x-maker"] !== "1") return err(res, 403, "forbidden", "X-Maker başlığı eksik");
  await delay(60 + Math.random() * 120); // gerçekçi gecikme
  if (path === "/auth/login" && m === "POST") {
    const ip = req.socket.remoteAddress, now = Date.now(), tries = (loginTries.get(ip) || []).filter(x => now - x < 60000);
    if (tries.length >= 5) return err(res, 429, "rate_limited", "Çok fazla deneme; bir dakika sonra yeniden deneyin");
    const b = await body(req); const u = b && users[b.user];
    if (!u || u.password !== b?.password) loginTries.set(ip, [...tries, now]);
    if (!u || u.password !== b.password) return err(res, 401, "unauthorized", "Kullanıcı adı veya parola hatalı");
    const sid = randomBytes(18).toString("hex"); sessions.set(sid, { user: b.user, admin: u.admin });
    return send(res, 200, { user: b.user, admin: u.admin }, { "Set-Cookie": `mk_session=${sid}; HttpOnly; SameSite=Strict; Path=/` });
  }
  if (path === "/auth/local") {
    if (!setupDone) return send(res, 302, undefined, { Location: "/" }); // kurulumdan önce oturum yok
    const sid = randomBytes(18).toString("hex"); sessions.set(sid, { user: Object.keys(users)[0], admin: true });
    return send(res, 302, undefined, { Location: "/", "Set-Cookie": `mk_session=${sid}; HttpOnly; SameSite=Strict; Path=/` });
  }
  // ilk açılış (kimlik doğrulamasız; kurulum bitince 409)
  if (path === "/setup" && m === "GET") return setupDone ? err(res, 409, "conflict", "Kurulum zaten tamamlandı") : send(res, 200, setupInfo());
  if (path === "/setup/wifi/scan" && m === "POST") { if (setupDone) return err(res, 409, "conflict", "Kurulum zaten tamamlandı"); await delay(700); return send(res, 200, wifiScan()); }
  if (path === "/setup/complete" && m === "POST") {
    if (setupDone) return err(res, 409, "conflict", "Kurulum zaten tamamlandı");
    const b = await body(req);
    if (!b || !/^[a-z_][a-z0-9_-]{0,31}$/.test(b.user || "")) return err(res, 400, "invalid", "Geçersiz kullanıcı adı");
    if ((b.password || "").length < 6) return err(res, 400, "invalid", "Parola en az 6 karakter olmalı");
    if (!/^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/.test(b.hostname || "")) return err(res, 400, "invalid", "Geçersiz cihaz adı");
    if (!setupInfo().timezones.includes(b.timezone)) return err(res, 400, "invalid", "Geçersiz saat dilimi");
    await delay(900);
    users = { [b.user]: { password: b.password, admin: true, fullname: b.fullname || "" } };
    hostname = b.hostname; tz = b.timezone; locale = { lang: b.lang || locale.lang, keymap: b.keymap || locale.keymap };
    if (b.wifi?.ssid) { activeSsid = b.wifi.ssid; knownWifi.add(b.wifi.ssid); }
    setupDone = true; publish("net", net());
    const sid = randomBytes(18).toString("hex"); sessions.set(sid, { user: b.user, admin: true });
    console.log(`kurulum tamamlandı: ${b.user}@${hostname} (${tz}, ${locale.lang}, ${locale.keymap}${b.wifi ? ", wifi " + b.wifi.ssid : ""})`);
    return send(res, 200, { user: b.user, admin: true, ...(b.wifi?.ssid ? { job: "wifi-" + Date.now() } : {}) }, { "Set-Cookie": `mk_session=${sid}; HttpOnly; SameSite=Strict; Path=/` });
  }
  if (path === "/dev/reset" && m === "POST") { // yalnız dev-server: --fresh durumuna dön
    users = {}; setupDone = false; activeSsid = ""; sessions.clear(); prefsByUser.clear(); hostname = "makeros"; wifiOn = true;
    return send(res, 204);
  }
  const s = sess(req);
  if (!s) return err(res, 401, "unauthorized", "Oturum açmanız gerekiyor");
  const route = `${m} ${path}`;
  switch (route) {
    case "POST /auth/logout": sessions.delete(cookie(req).mk_session); return send(res, 204, undefined, { "Set-Cookie": "mk_session=; Max-Age=0; Path=/" });
    case "GET /session": return send(res, 200, { user: s.user, admin: s.admin, setup_done: setupDone });
    case "GET /prefs": return send(res, 200, prefsByUser.get(s.user) || {});
    case "PUT /prefs": {
      const raw = await new Promise(r => { let d = ""; req.on("data", c => d += c); req.on("end", () => r(d)); });
      if (raw.length > 16384) return err(res, 413, "invalid", "Tercihler 16 KB'tan büyük olamaz");
      let b; try { b = JSON.parse(raw); } catch { return err(res, 400, "invalid", "Geçersiz JSON"); }
      if (!b || typeof b !== "object" || Array.isArray(b)) return err(res, 400, "invalid", "Nesne bekleniyor");
      prefsByUser.set(s.user, b); return send(res, 200, b);
    }
    case "PUT /net/wifi": {
      if (!s.admin) return err(res, 403, "forbidden", "Yönetici yetkisi gerekiyor");
      const b = await body(req); if (typeof b?.enabled !== "boolean") return err(res, 400, "invalid", "enabled (bool) gerekli");
      wifiOn = b.enabled; publish("net", net()); return send(res, 200, net());
    }
    case "GET /system": return send(res, 200, system());
    case "PUT /system/hostname": { const b = await body(req); if (!b || !/^[a-z0-9-]{1,63}$/i.test(b.hostname || "")) return err(res, 400, "invalid", "Geçersiz cihaz adı"); hostname = b.hostname; return send(res, 204); }
    case "GET /system/time": return send(res, 200, { now: new Date().toISOString(), timezone: tz, ntp });
    case "PUT /system/time": { const b = await body(req); if (b.timezone) tz = b.timezone; if ("ntp" in b) ntp = !!b.ntp; return send(res, 200, { now: new Date().toISOString(), timezone: tz, ntp }); }
    case "GET /system/timezones": return send(res, 200, ["Europe/Istanbul", "Europe/Berlin", "Europe/London", "Europe/Paris", "Europe/Amsterdam", "Asia/Baku", "Asia/Tokyo", "America/New_York", "America/Los_Angeles", "UTC"]);
    case "GET /system/locale": return send(res, 200, locale);
    case "PUT /system/locale": { const b = await body(req); locale = { ...locale, ...b }; return send(res, 200, locale); }
    case "PUT /system/password": { const b = await body(req); if (users[s.user].password !== b.old) return err(res, 400, "invalid", "Mevcut parola hatalı"); if ((b.new || "").length < 4) return err(res, 400, "invalid", "Yeni parola en az 4 karakter olmalı"); users[s.user].password = b.new; return send(res, 204); }
    case "POST /system/power": { const b = await body(req); if (!["reboot", "poweroff"].includes(b.action)) return err(res, 400, "invalid", "Geçersiz işlem"); return send(res, 202, { job: "power-" + Date.now() }); }
    case "GET /net": return send(res, 200, net());
    case "POST /net/wifi/scan": await delay(700); return send(res, 200, wifiScan());
    case "POST /net/wifi/connect": {
      const b = await body(req); const id = "job-" + Date.now();
      const sec = wifiScan().find(n => n.ssid === b.ssid)?.security;
      const fail = sec && sec !== "open" && !knownWifi.has(b.ssid) && (b.password || "").length < 8;
      let p = 0; const iv = setInterval(() => {
        p += 25;
        if (p >= 100 || (fail && p >= 50)) {
          clearInterval(iv);
          if (!fail) { activeSsid = b.ssid; knownWifi.add(b.ssid); publish("net", net()); }
          publish("jobs", { id, state: fail ? "failed" : "done", progress: 100, log: fail ? "Kimlik doğrulama başarısız: parola hatalı" : `${b.ssid} ağına bağlanıldı` });
        } else publish("jobs", { id, state: "running", progress: p, log: ["Ağ aranıyor…", "Kimlik doğrulanıyor…", "IP adresi alınıyor…"][p / 25 - 1] });
      }, 600);
      return send(res, 202, { job: id });
    }
    case "POST /net/wifi/forget": { const b = await body(req); knownWifi.delete(b.ssid); if (activeSsid === b.ssid) activeSsid = ""; publish("net", net()); return send(res, 204); }
    case "GET /apps": return send(res, 200, apps);
    // Yeni uçların (2.1) asıl uygulaması makerd'dedir; dev-server boş liste döner.
    case "GET /plugins": case "GET /term": case "GET /projects": return send(res, 200, []);
    case "GET /processes": return send(res, 200, procList());
    case "GET /notifications": return send(res, 200, notifications);
    case "GET /system/update": return send(res, 200, updateState());
    case "POST /system/update/check": update.checked = new Date().toISOString(); return send(res, 200, updateState());
    case "POST /system/update/apply": update.started = Date.now(); return send(res, 202, { job: "up1" });
    case "POST /term": { const b = await body(req); if (terms.size >= 6) return err(res, 409, "conflict", "Terminal oturum sınırı (6)");
      const id = "t" + randomBytes(4).toString("hex"), ticket = randomBytes(12).toString("hex"); terms.set(id, { ticket, cols: b.cols, rows: b.rows }); return send(res, 200, { id, ticket }); }
  }
  let mm;
  if (m === "POST" && (mm = path.match(/^\/term\/([^/]+)\/ticket$/))) { const t = terms.get(mm[1]); if (!t) return err(res, 404, "not_found", "Oturum yok"); t.ticket = randomBytes(12).toString("hex"); return send(res, 200, { id: mm[1], ticket: t.ticket }); }
  if (m === "GET" && (mm = path.match(/^\/jobs\/([^/]+)$/))) return jobs.has(mm[1]) ? send(res, 200, jobs.get(mm[1])) : err(res, 404, "not_found", "İş yok");
  if (m === "POST" && (mm = path.match(/^\/apps\/([^/]+)\/launch$/))) return apps.some(a => a.id === mm[1]) ? send(res, 202, { job: "launch" }) : err(res, 404, "not_found", "Uygulama bulunamadı");
  if (m === "POST" && (mm = path.match(/^\/processes\/(\d+)\/signal$/))) {
    const i = procs.findIndex(p => p.pid === +mm[1]); if (i < 0) return err(res, 404, "not_found", "Süreç bulunamadı");
    if (procs[i].user !== s.user) return err(res, 403, "forbidden", "Yalnızca kendi süreçlerinizi sonlandırabilirsiniz");
    procs.splice(i, 1); return send(res, 204);
  }
  if (m === "POST" && (mm = path.match(/^\/notifications\/([^/]+)\/read$/))) { const n = notifications.find(n => n.id === mm[1]); if (!n) return err(res, 404, "not_found", "Bildirim yok"); n.read = true; return send(res, 204); }
  if (m === "DELETE" && path === "/notifications") { notifications.length = 0; publish("notifications", { cleared: true }); return send(res, 204); }
  return err(res, 404, "not_found", "Uç nokta bulunamadı: " + route);
}

// ------------------------------------------------------------ statik
async function serveStatic(req, res, url) {
  const ROOTS = { "/": "index.html", "/web": "web/index.html", "/manifest.webmanifest": "manifest.webmanifest", "/sw.js": "sw.js" };
  let rel = ROOTS[url.pathname] || (url.pathname.startsWith("/shell/") ? url.pathname.slice(7) : null);
  if (rel === null) return err(res, 404, "not_found", "Yok");
  rel = normalize(decodeURIComponent(rel)).replace(/^(\.\.[/\\])+/, "");
  let file = join(ROOT, rel);
  try { if ((await stat(file)).isDirectory()) file = join(file, "index.html"); const data = await readFile(file);
    const imm = file.endsWith(".woff2"); // makerd ile aynı: vendor/ de yeniden doğrulanır
    res.writeHead(200, { "Content-Type": MIME[extname(file)] || "application/octet-stream", "Cache-Control": imm ? "max-age=31536000, immutable" : "no-cache" });
    res.end(data);
  } catch { res.writeHead(404, { "Content-Type": "text/plain" }); res.end("404"); }
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  try {
    if (url.pathname.startsWith("/api/v1/")) return await api(req, res, url.pathname.slice(7));
    return await serveStatic(req, res, url);
  } catch (e) { console.error(e); err(res, 500, "internal", String(e)); }
});

// ------------------------------------------------------------ WebSocket (RFC 6455, en küçük)
function wsAccept(req, socket) {
  const key = req.headers["sec-websocket-key"];
  const acc = createHash("sha1").update(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64");
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${acc}\r\n\r\n`);
  const ws = { socket, open: true, onmessage: () => {}, onclose: () => {} };
  ws.send = (data, binary = false) => {
    if (!ws.open) return;
    const payload = Buffer.isBuffer(data) ? data : Buffer.from(data);
    const len = payload.length, head = len < 126 ? Buffer.from([binary ? 0x82 : 0x81, len])
      : len < 65536 ? Buffer.from([binary ? 0x82 : 0x81, 126, len >> 8, len & 255]) : (() => { const b = Buffer.alloc(10); b[0] = binary ? 0x82 : 0x81; b[1] = 127; b.writeBigUInt64BE(BigInt(len), 2); return b; })();
    socket.write(Buffer.concat([head, payload]));
  };
  let buf = Buffer.alloc(0);
  socket.on("data", chunk => {
    buf = Buffer.concat([buf, chunk]);
    while (buf.length >= 2) {
      const op = buf[0] & 15, masked = buf[1] & 128; let len = buf[1] & 127, off = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); off = 4; }
      else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); off = 10; }
      const mask = masked ? buf.subarray(off, off + 4) : null; if (masked) off += 4;
      if (buf.length < off + len) return;
      const data = Buffer.from(buf.subarray(off, off + len)); buf = buf.subarray(off + len);
      if (mask) for (let i = 0; i < data.length; i++) data[i] ^= mask[i & 3];
      if (op === 8) { ws.open = false; socket.end(Buffer.from([0x88, 0])); ws.onclose(); return; }
      if (op === 9) { socket.write(Buffer.concat([Buffer.from([0x8a, data.length]), data])); continue; }
      if (op === 1 || op === 2) ws.onmessage(data, op === 2);
    }
  });
  const close = () => { if (ws.open) { ws.open = false; ws.onclose(); } };
  socket.on("close", close); socket.on("error", close);
  return ws;
}

const subscribers = new Set();
const jobs = new Map();
function publish(topic, data) { if (topic === "jobs") jobs.set(data.id, data); const msg = JSON.stringify({ topic, data }); for (const c of subscribers) if (c.topics.has(topic)) c.ws.send(msg); }
setInterval(() => { publish("system", system()); publish("processes", procList()); }, 2000);
setInterval(() => {
  const n = { id: "n" + Date.now(), app: "makeros.system", title: "Sıcaklık bildirimi", body: `İşlemci sıcaklığı ${temp.toFixed(0)} °C. Fan bağlantısını kontrol edin.`, time: new Date().toISOString(), read: false };
  notifications.unshift(n); notifications.length = Math.min(notifications.length, 20); publish("notifications", n);
}, 90000);

const terms = new Map();
const PROMPT = () => `\x1b[1;32mmaker@${hostname}\x1b[0m:\x1b[1;34m~\x1b[0m$ `;
function fakeShell(ws, t) {
  let line = "";
  const out = s => ws.send(Buffer.from(s.replace(/\n/g, "\r\n")), true);
  if (!t.started) { t.started = true; out(`\x1b[1;31mMakerOS\x1b[0m geliştirme terminali (sahte kabuk). \x1b[2m'help' yazın.\x1b[0m\n\n`); }
  out(PROMPT());
  const run = cmd => {
    const [c, ...a] = cmd.trim().split(/\s+/);
    const r = {
      "": "", help: "Komutlar: help, ls, pwd, whoami, uname, date, echo, clear, neofetch, python3, exit",
      ls: "\x1b[1;34mMasaüstü  Projeler  Belgeler\x1b[0m  notlar.txt  blink.py", pwd: "/home/maker", whoami: "maker",
      uname: a[0] === "-a" ? "Linux " + hostname + " 6.18.4-v8+ #1 SMP PREEMPT aarch64 GNU/Linux" : "Linux", date: new Date().toString(), echo: a.join(" "),
      clear: "\x1b[2J\x1b[H", python3: "Python 3.13.5 (sahte) — gerçek yorumlayıcı makerd ile gelir.",
      neofetch: `\x1b[1;31m  ███╗   ███╗\x1b[0m  maker@${hostname}\n\x1b[1;31m  ████╗ ████║\x1b[0m  OS: MakerOS 1.0.0 aarch64\n\x1b[1;31m  ██╔████╔██║\x1b[0m  Host: Raspberry Pi 4 Model B\n\x1b[1;31m  ██║╚██╔╝██║\x1b[0m  Kernel: 6.18.4-v8+\n\x1b[1;31m  ██║ ╚═╝ ██║\x1b[0m  CPU: BCM2711 (4) @ 1.8GHz\n\x1b[1;31m  ╚═╝     ╚═╝\x1b[0m  Bellek: ${(memUsed / 1e9).toFixed(1)} GB / 4.0 GB`,
    }[c];
    if (c === "exit") { out("çıkış\n"); setTimeout(() => ws.socket.end(), 50); return; }
    out((r ?? `${c}: komut bulunamadı`) + (r === "\x1b[2J\x1b[H" || r === "" ? "" : "\n"));
  };
  ws.onmessage = (data, bin) => {
    if (!bin) { try { const j = JSON.parse(data); if (j.type === "resize") { t.cols = j.cols; t.rows = j.rows; } } catch {} return; }
    for (const ch of data.toString("utf8")) {
      if (ch === "\r") { out("\n"); run(line); line = ""; if (ws.open) out(PROMPT()); }
      else if (ch === "\x7f") { if (line) { line = [...line].slice(0, -1).join(""); out("\b \b"); } }
      else if (ch === "\x03") { out("^C\n" + PROMPT()); line = ""; }
      else if (ch === "\x0c") { out("\x1b[2J\x1b[H" + PROMPT() + line); }
      else if (ch === "\x1b" || ch < " ") { /* kaçış dizileri yok sayılır */ }
      else { line += ch; out(ch); }
    }
  };
}

server.on("upgrade", (req, socket) => {
  const url = new URL(req.url, "http://x");
  if (!sess(req)) { socket.end("HTTP/1.1 401 Unauthorized\r\n\r\n"); return; }
  if (url.pathname === "/api/v1/events") {
    const ws = wsAccept(req, socket), c = { ws, topics: new Set() }; subscribers.add(c);
    ws.onmessage = d => { try { const j = JSON.parse(d); for (const tp of j.topics || []) {
      if (j.op === "sub") { c.topics.add(tp); if (tp === "system") ws.send(JSON.stringify({ topic: tp, data: system() })); if (tp === "net") ws.send(JSON.stringify({ topic: tp, data: net() })); if (tp === "processes") ws.send(JSON.stringify({ topic: tp, data: procList() })); }
      else c.topics.delete(tp); } } catch {} };
    ws.onclose = () => subscribers.delete(c); return;
  }
  const m = url.pathname.match(/^\/api\/v1\/term\/([^/]+)$/), t = m && terms.get(m[1]);
  if (!t || (t.ticket !== url.searchParams.get("ticket"))) { socket.end("HTTP/1.1 403 Forbidden\r\n\r\n"); return; }
  t.ticket = null; // tek kullanım
  fakeShell(wsAccept(req, socket), t);
});

server.listen(PORT, () => console.log(`MakerOS kabuk geliştirme sunucusu: http://localhost:${PORT}/  ${FRESH ? "(ilk açılış: kurulum sihirbazı)" : "(maker / maker)"}`));
