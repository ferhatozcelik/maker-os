// Pardus Maker Merkezi - tek sayfalik arayuz (Ayarlar, Python, Agent'lar, Terminal, GPIO,
// Kamera, USB, Ekran). Telefonda ust cubuk + cekmece menu ile calisir.
const $ = (s, r = document) => r.querySelector(s);
const esc = t => String(t ?? "").replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
let local = false;  // sunucu /api/status ile bildirir (istemci Pi'nin kendisi mi)

const ICONS = {
  wifi: '<path d="M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0M2 9a15 15 0 0 1 20 0"/><circle cx="12" cy="19.5" r=".8"/>',
  palette: '<circle cx="12" cy="12" r="9"/><circle cx="8" cy="10" r="1"/><circle cx="12" cy="7.5" r="1"/><circle cx="16" cy="10" r="1"/><path d="M12 21a2.5 2.5 0 0 1 0-5h1.5"/>',
  chip: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/>',
  remote: '<rect x="2" y="4" width="20" height="13" rx="2"/><path d="M8 21h8M12 17v4"/>',
  lang: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  update: '<path d="M21 12a9 9 0 1 1-3-6.7L21 8"/><path d="M21 3v5h-5"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.5"/>',
  python: '<path d="M8 3h6a3 3 0 0 1 3 3v4H9a3 3 0 0 0-3 3v2H5a2 2 0 0 1-2-2V9a3 3 0 0 1 3-3h8"/><path d="M16 21h-6a3 3 0 0 1-3-3v-4h8a3 3 0 0 0 3-3V9h1a2 2 0 0 1 2 2v4a3 3 0 0 1-3 3h-8"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  agent: '<rect x="4" y="7" width="16" height="12" rx="3"/><path d="M12 3v4M9 12v1M15 12v1M9 16h6"/>',
  pins: '<path d="M7 4v16M12 4v16M17 4v16"/><circle cx="7" cy="8" r="1.4"/><circle cx="12" cy="14" r="1.4"/><circle cx="17" cy="10" r="1.4"/>',
  screen: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M7 20h10"/><path d="M9 10l2 2 4-4"/>',
  terminal: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3M12 15h5"/>',
  camera: '<path d="M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="3.5"/>',
  usb: '<path d="M12 21V5M12 5l-2.5 3M12 5l2.5 3M12 16l-5-3V9M12 14l5-3V8"/><circle cx="12" cy="20" r="1.4"/><rect x="5.5" y="7.5" width="3" height="2"/><circle cx="17" cy="7" r="1.2"/>',
};

const sections = () => [
  { group: "Ayarlar" },
  { id: "genel", title: "Genel", icon: "info", color: "#8e8e93", render: renderGenel },
  { id: "wifi", title: "Wi-Fi ve Ağ", icon: "wifi", color: "#0a84ff", render: renderWifi },
  { id: "etkinlik", title: "Görev Yöneticisi", icon: "chip", color: "#30b0c7", render: renderActivity, layout: "wide" },
  { id: "fan", title: "Fan ve Soğutma", icon: "chip", color: "#34c759", render: renderFan },
  { id: "gorunum", title: "Görünüm", icon: "palette", color: "#bf5af2", render: renderGorunum },
  { id: "dil", title: "Dil ve Klavye", icon: "lang", color: "#32ade6", render: renderDil },
  { id: "saat", title: "Tarih ve Saat", icon: "clock", color: "#ff9500", render: renderSaat },
  { id: "donanim", title: "Ekran ve Donanım", icon: "chip", color: "#30b0c7", render: renderDonanim },
  { id: "uzak", title: "Uzak Erişim", icon: "remote", color: "#5e5ce6", render: renderUzak },
  { id: "kullanici", title: "Kullanıcı", icon: "user", color: "#ff9f0a", render: renderKullanici },
  { id: "guncelleme", title: "Güncellemeler", icon: "update", color: "#34c759", render: renderGuncelleme },
  { group: "Geliştirme" },
  { id: "eklentiler", title: "Eklentiler", icon: "folder", color: "#7c3aed", render: renderPlugins },
  { id: "python", title: "Python", icon: "python", color: "#3776ab", render: renderPython },
  { id: "projeler", title: "Projeler", icon: "folder", color: "#f2a900", render: renderProjeler },
  { id: "agent", title: "Agent'lar", icon: "agent", color: "#c8102e", render: renderAgents },
  { id: "terminal", title: "Terminal", icon: "terminal", color: "#2c2c2e", render: renderTerminal, layout: "fill" },
  { group: "Donanım" },
  { id: "kamera", title: "Kamera", icon: "camera", color: "#ff375f", render: renderKamera, layout: "wide" },
  { id: "usb", title: "USB Aygıtları", icon: "usb", color: "#64748b", render: renderUsb },
  { id: "gpio", title: "GPIO Pinleri", icon: "pins", color: "#2f9e44", render: renderGpio },
  ...(local ? [] : [{ group: "Uzaktan" }, { id: "ekran", title: "Ekran", icon: "screen", color: "#1c1c1e", render: null }]),
];
let SECTIONS = [];
const ALIASES = { ayarlar: "genel", agents: "agent", camera: "kamera", ekranlar: "donanim" };
const touch = matchMedia("(pointer: coarse)").matches;

// ---------------------------------------------------------------- yardimcilar
function toast(t, ms = 3500) {
  const el = $("#toast"); el.textContent = t; el.style.display = "block";
  clearTimeout(toast.t); toast.t = setTimeout(() => el.style.display = "none", ms);
}
function showError(error, context = "") {
  const text = `${context ? context + "\n\n" : ""}${error.message || error}\n\nZaman: ${new Date().toLocaleString("tr-TR")}`;
  $("#errorDetails").textContent = text;
  $("#errorPanel").hidden = false;
}
$("#errorClose").onclick = () => $("#errorPanel").hidden = true;
$("#errorCopy").onclick = async () => {
  try { await navigator.clipboard.writeText($("#errorDetails").textContent); toast("Hata ayrıntıları kopyalandı"); }
  catch { const r = document.createRange(); r.selectNodeContents($("#errorDetails")); const s = getSelection(); s.removeAllRanges(); s.addRange(r); toast("Metin seçildi; Kopyala ile kopyalayabilirsiniz."); }
};
async function api(path, body, timeout = 65000) {
  const opt = body === undefined ? {} : {
    method: "POST", body: JSON.stringify(body), headers: { "Content-Type": "application/json", "X-Pios": "1" } };
  const controller = new AbortController(), deadline = setTimeout(() => controller.abort(), timeout);
  try {
    const r = await fetch(path, { ...opt, signal: controller.signal, cache: "no-store", credentials: "same-origin" });
    if (r.status === 401 && path !== "/api/login") {
      await requireLogin();
      location.reload();
      return {};
    }
    let j;
    try { j = await r.json(); } catch { throw new Error(`Sunucudan geçerli JSON yanıtı alınamadı.\nİstek: ${path}\nHTTP: ${r.status}`); }
    if (!r.ok) throw new Error([j.message || j.error || "Sunucu hatası", j.details,
      `İstek: ${path} · HTTP ${r.status}`, j.reference ? `Hata kodu: ${j.reference}` : ""].filter(Boolean).join("\n"));
    return j;
  } catch (e) {
    if (e.name === "AbortError") throw new Error(`İstek ${Math.round(timeout / 1000)} saniyede yanıtlanmadı.\nİstek: ${path}\nAğ değiştiyse telefonunuzu hedef ağa bağlayıp cihazın .local adresini açın.`);
    if (e instanceof TypeError) throw new Error(`Cihaza ulaşılamadı.\nİstek: ${path}\n${e.message}\nWi-Fi bağlantınızı ve cihazın yeni IP adresini kontrol edin.`);
    throw e;
  } finally { clearTimeout(deadline); }
}

let loginWait = null;
function requireLogin() {
  if (loginWait) return loginWait;
  $("#app").hidden = true;
  $("#login").hidden = false;
  const err = $("#loginError"), form = $("#loginForm"), btn = $("#loginBtn");
  err.hidden = true; err.textContent = "";
  form.password.value = "";
  form.password.focus();
  loginWait = new Promise(resolve => {
    form.onsubmit = async event => {
      event.preventDefault();
      btn.disabled = true; err.hidden = true;
      try {
        const r = await fetch("/api/login", {
          method: "POST", credentials: "same-origin", cache: "no-store",
          headers: { "Content-Type": "application/json", "X-Pios": "1" },
          body: JSON.stringify({ username: form.username.value.trim(), password: form.password.value }),
        });
        const j = await r.json().catch(() => ({}));
        if (!r.ok || !j.ok) throw new Error(j.message || "Giriş başarısız.");
        $("#login").hidden = true;
        $("#app").hidden = false;
        form.onsubmit = null;
        loginWait = null;
        resolve();
      } catch (e) {
        err.textContent = e.message || "Giriş başarısız.";
        err.hidden = false;
        form.password.focus();
        form.password.select();
      } finally { btn.disabled = false; }
    };
  });
  return loginWait;
}
async function act(path, body, okMsg) {
  try { const r = await api(path, body); toast(r.message || okMsg || "Tamam"); return r; }
  catch (e) { showError(e); return null; }
}
function icon(name, color) { return `<span class="ico" style="background:${color}"><svg viewBox="0 0 24 24">${ICONS[name]}</svg></span>`; }
const sw = (id, on) => `<label class="switch"><input type="checkbox" id="${id}" ${on ? "checked" : ""}><span></span></label>`;
const row = (t, d, right = "", cls = "") => `<div class="row ${cls}"><div class="grow"><div class="t">${t}</div>${d ? `<div class="d">${d}</div>` : ""}</div>${right}</div>`;
function fmtSize(b) { return b >= 1e12 ? (b / 1e12).toFixed(1) + " TB" : b >= 1e9 ? (b / 1e9).toFixed(1) + " GB" : Math.round(b / 1e6) + " MB"; }

// Parola/form diyalogu (prompt() yerine; telefonda da rahat kullanilir).
// fields: [{ id, label, type: "text"|"password"|"check", value, placeholder }]
function ask(title, fields, okText = "Tamam", note = "") {
  const d = $("#dlg");
  d.innerHTML = `<form method="dialog"><h3>${esc(title)}</h3>${note ? `<p class="note">${note}</p>` : ""}
    ${fields.map(f => f.type === "check"
      ? `<label class="chk"><input type="checkbox" name="${f.id}" ${f.value ? "checked" : ""}> ${esc(f.label)}</label>`
      : `<label class="fld"><span>${esc(f.label)}</span><input name="${f.id}" type="${f.type || "text"}" value="${esc(f.value || "")}"
          placeholder="${esc(f.placeholder || "")}" autocomplete="off" autocapitalize="none" autocorrect="off" spellcheck="false"></label>`).join("")}
    ${fields.some(f => f.type === "password") ? '<label class="chk"><input type="checkbox" data-show> Parolayı göster</label>' : ""}
    <div class="dlg-foot"><button type="button" class="btn" data-x>Vazgeç</button><button class="btn primary" value="ok">${esc(okText)}</button></div></form>`;
  const form = d.querySelector("form");
  const pws = [...d.querySelectorAll('input[type="password"]')];
  const show = d.querySelector("[data-show]");
  if (show) show.onchange = () => pws.forEach(i => i.type = show.checked ? "text" : "password");
  d.querySelector("[data-x]").onclick = () => d.close("");
  return new Promise(res => {
    d.onclose = () => {
      if (d.returnValue !== "ok") return res(null);
      const o = {}; fields.forEach(f => { const i = form.elements[f.id]; o[f.id] = f.type === "check" ? i.checked : i.value; });
      res(o);
    };
    d.returnValue = ""; d.showModal();
    const first = form.querySelector("input:not([type=checkbox])"); first && first.focus();
  });
}

function fmtUp(s) { const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), m = Math.floor(s % 3600 / 60); return (d ? d + " gün " : "") + h + " sa " + m + " dk"; }

// Arka plan isi: log penceresi (sheet) ile izlenir.
async function runJob(path, body, onDone) {
  let r;
  try { r = await api(path, body); } catch (e) { showError(e); return; }
  const sheet = $("#sheet"), log = $("#sheetLog");
  sheet.hidden = false; log.textContent = ""; $("#sheetTitle").textContent = "…"; $("#sheetState").textContent = "çalışıyor…";
  $("#sheetClose").onclick = () => { sheet.hidden = true; };
  let since = 0, failures = 0;
  for (;;) {
    if (sheet.hidden) return;
    let j;
    try { j = await api(`/api/jobs/${r.job}?since=${since}`); failures = 0; }
    catch (e) {
      $("#sheetState").textContent = "İşlem durumu alınamıyor; yeniden deneniyor…";
      if (++failures >= 6) { showError(e, "İşlem izleme durdu. İşlem cihazda devam ediyor olabilir."); return; }
      await new Promise(s => setTimeout(s, 2500)); continue;
    }
    $("#sheetTitle").textContent = j.title;
    if (j.lines.length) { log.textContent = (log.textContent + j.lines.join("\n") + "\n").slice(-150000); log.scrollTop = log.scrollHeight; }
    since = j.next;
    if (j.status !== "running") {
      $("#sheetState").innerHTML = j.status === "ok" ? '<span class="badge ok">Tamamlandı</span>' : '<span class="badge acc">Hata</span>';
      onDone && onDone(j.status === "ok");
      return;
    }
    await new Promise(s => setTimeout(s, 800));
  }
}

// ---------------------------------------------------------------- gezinme
let current = null, timer = null, cleanup = null, routeVersion = 0;
function pollView(el, task, delay = 2000) {
  let stopped = false, handle;
  const previousCleanup = cleanup;
  const tick = async () => {
    if (stopped || !el.isConnected) return;
    try { if (!document.hidden) await task(); }
    catch (e) { if (!stopped && el.isConnected) showError(e, "Otomatik yenileme başarısız; yeniden denenecek."); }
    if (!stopped && el.isConnected) handle = setTimeout(tick, delay);
  };
  cleanup = () => { stopped = true; clearTimeout(handle); if (previousCleanup) previousCleanup(); };
  handle = setTimeout(tick, delay);
}
function buildNav() {
  $("#nav").innerHTML = SECTIONS.map(s => s.group ? `<div class="group">${s.group}</div>`
    : `<a href="#${s.id}" data-id="${s.id}">${icon(s.icon, s.color)}<span>${s.title}</span></a>`).join("");
  $("#nav").onclick = e => e.target.closest("a") && $("#app").classList.remove("menu");
}
function route() {
  const version = ++routeVersion;
  let id = location.hash.slice(1) || "genel";
  id = ALIASES[id] || id;
  const sec = SECTIONS.find(s => s.id === id) || SECTIONS[1];
  clearInterval(timer); timer = null;
  if (cleanup) { const f = cleanup; cleanup = null; f(); }
  $("#app").classList.remove("menu");
  document.querySelectorAll("#nav a").forEach(a => a.classList.toggle("on", a.dataset.id === sec.id));
  if (sec.id === "ekran") return openScreen();
  closeScreen();
  current = sec.id;
  $("#topTitle").textContent = sec.title;
  const c = $("#content"); c.className = sec.layout || ""; c.scrollTop = 0;
  c.innerHTML = `<div><h1>${sec.title}</h1><p class="lead">Yükleniyor…</p></div>`;
  sec.render(c.firstChild).catch(e => { if (version !== routeVersion) return;
    c.firstChild.innerHTML = `<h1>${sec.title}</h1><p class="lead">Sayfa yüklenemedi.</p><pre class="error-inline">${esc(e.message)}</pre><button class="btn" id="pageRetry">Yeniden dene</button>`;
    $("#pageRetry").onclick = route; showError(e, sec.title); });
}

// ---------------------------------------------------------------- Genel
async function renderGenel(el) {
  const draw = async () => {
    const s = await api("/api/status");
    if (!el.isConnected) return;
    $("#devName").textContent = s.hostname; $("#devModel").textContent = s.model;
    const ip = s.ips.map(i => i.addr.split("/")[0]).join(", ") || "IP yok";
    el.innerHTML = `<h1>Genel</h1><p class="lead">${esc(s.model)} · ${esc(s.os)}</p>
      <div class="tiles">
        <div class="tile"><div class="k">Sıcaklık</div><div class="n">${s.temp_c == null ? "—" : s.temp_c + " °C"}</div></div>
        <div class="tile"><div class="k">Bellek</div><div class="n">${((s.mem_total_mb - s.mem_avail_mb) / 1024).toFixed(1)} / ${(s.mem_total_mb / 1024).toFixed(1)} GB</div></div>
        <div class="tile"><div class="k">Disk boş</div><div class="n">${s.disk_free_gb} GB</div></div>
        <div class="tile"><div class="k">İşlemci yükü</div><div class="n">${esc(s.load[0])}</div></div>
      </div>
      <h2>Bu cihaz</h2><div class="list">
        ${row("Ad", "", `<span class="v">${esc(s.hostname)}.local</span>`)}
        ${row("IP adresi", "", `<span class="v">${esc(ip)}</span>`)}
        ${row("Çalışma süresi", "", `<span class="v">${fmtUp(s.uptime_s)}</span>`)}
        ${row("Linux çekirdeği", "", `<span class="v">${esc(s.kernel)}</span>`)}
        ${row("Sürüm", "", `<span class="v">${esc(s.version)}</span>`)}
      </div>
      <h2>Güç</h2><div class="list">
        ${row("Masaüstünü yeniden başlat", "Masaüstü donarsa oturumu yeniler", '<button class="btn" id="dm">Yeniden başlat</button>')}
        ${row("Pi'yi yeniden başlat", "", '<button class="btn" id="rb">Yeniden başlat</button>')}
        ${row("Pi'yi kapat", "Yeniden açmak için güç kablosunu çıkarıp takın", '<button class="btn danger" id="po">Kapat</button>')}
      </div>`;
    $("#dm", el).onclick = () => act("/api/desktop/restart", {}, "Masaüstü yeniden başlatıldı");
    $("#rb", el).onclick = () => confirm("Pi yeniden başlatılsın mı?") && act("/api/power", { action: "reboot" }, "Yeniden başlatılıyor…");
    $("#po", el).onclick = () => confirm("Pi kapatılsın mı?") && act("/api/power", { action: "poweroff" }, "Kapatılıyor…");
  };
  await draw(); if (el.isConnected) pollView(el, draw, 5000);
}

// ---------------------------------------------------------------- Wi-Fi ve ag
async function renderWifi(el) {
  const [s, n] = await Promise.all([api("/api/status"), api("/api/network")]);
  if (!el.isConnected) return;
  const e = n.ethernet, net = n.internet;
  const wifi = s.connections.find(c => c.type === "802-11-wireless" && c.name !== "pardus-hotspot");
  el.innerHTML = `<h1>Wi-Fi ve Ağ</h1><p class="lead">Ağları bulun, bağlantıyı değiştirin ve bağlantı sorunlarını inceleyin.</p>
    <div class="tiles">
      <div class="tile"><div class="k">İnternet</div><div class="n">${net.ok ? (net.dns ? "Bağlı" : "DNS sorunu") : "Erişim yok"}</div><div class="s">ICMP ve DNS kontrolü</div></div>
      <div class="tile"><div class="k">Wi-Fi</div><div class="n net-name">${wifi ? esc(wifi.name) : s.hotspot ? "Erişim noktası" : "Bağlı değil"}</div></div>
      <div class="tile"><div class="k">Ethernet</div><div class="n">${e && e.carrier ? (e.speed ? e.speed + " Mb/s" : "Bağlı") : "Kablo yok"}</div></div>
    </div>
    <section class="network-notice" aria-live="polite">
      <b>Ağ değişirken bağlantı kısa süre kesilebilir.</b>
      <p>Telefonunuzu hedef ağa bağlayın ve <a href="http://${esc(s.hostname)}.local/">http://${esc(s.hostname)}.local/</a> adresini açın. Bağlantı kurulamazsa önceki ağ veya Pardus-Maker erişim noktası geri açılır.</p>
      ${!n.nm ? '<p class="net-alert">NetworkManager çalışmıyor. Yerel terminalde <code>systemctl status NetworkManager</code> ile hizmeti kontrol edin.</p>' : ""}
      ${n.warning ? `<p class="net-alert">${esc(n.warning)}</p>` : ""}
      <div class="net-op"><div class="hrow"><b>İşlem durumu</b><span class="sp"></span><span id="netState" class="badge">Hazır</span></div><pre id="netMessage">Tarama başlatabilirsiniz.</pre></div>
    </section>
    <h2 class="hrow">Wi-Fi ağları<span class="sp"></span><button class="btn primary" id="scan">Tara</button><button class="btn" id="manual">Elle bağlan</button></h2>
    ${s.hotspot ? '<p class="hint">Erişim noktası açık. Tarama için duraklatma gerekir; telefon bağlantısı geçici kesilir ve tarama sonunda erişim noktası geri açılır.</p><button class="btn" id="apScan">Erişim noktasını duraklat ve tara</button>' : ""}
    <div class="list network-list" id="nets"><div class="empty">Henüz tarama yapılmadı. Ağ adını biliyorsanız elle bağlanabilirsiniz.</div></div>
    <h2>Kayıtlı ağlar</h2><div class="list">${n.saved.length ? n.saved.map((name, i) => row(esc(name), "Kaydedilmiş parola ile bağlan veya parolayı yeniden gir",
      `<div class="btns"><button class="btn" data-saved="${i}">Bağlan</button><button class="btn" data-edit="${i}">Parola değiştir</button><button class="btn danger" data-forget="${i}">Unut</button></div>`)).join("") : '<div class="empty">Kayıtlı ağ yok</div>'}</div>
    <h2>Etkin bağlantılar</h2><div class="list">${s.connections.map(c => row(esc(c.name), esc(c.device))).join("")}
      ${row("Pardus-Maker erişim noktası", "Ağ yokken yerel bağlantı · 10.42.0.1", sw("hs", s.hotspot))}
      ${e ? row("Ethernet", esc(e.ip || "Adres yok") + (e.gateway ? " · Ağ geçidi: " + esc(e.gateway) : "")) : ""}</div>
    <h2 class="hrow">Ağ tanılama<span class="sp"></span><button class="btn" id="diag">Ayrıntıları göster</button><button class="btn" id="netRefresh">Yenile</button></h2>
    <pre id="netDiag" class="diagnostics" hidden></pre>
    <p class="hint">Anlık aktarım hızı ve alınan/gönderilen toplam veri için <a href="#etkinlik">Görev Yöneticisi</a>.</p>`;
  let lastUpdated = 0, seenRunning = false, refreshing = false;
  const controls = busy => el.querySelectorAll("#scan, #manual, #apScan, [data-net], [data-saved], [data-edit], [data-forget], #hs").forEach(b => b.disabled = busy);
  const begin = async (path, body) => {
    controls(true); $("#netState", el).textContent = "Başlatılıyor…";
    const r = await act(path, body, "İşlem başlatıldı");
    if (!el.isConnected) return;
    if (r) { seenRunning = true; $("#netMessage", el).textContent = "İşlem sürüyor. Bağlantı kesilirse hedef ağa veya Pardus-Maker ağına yeniden bağlanın."; }
    else controls(false);
  };
  const connect = async (network, edit = false) => {
    let values = { password: "", identity: "" };
    if (network.enterprise || edit || (network.secure && !network.saved)) {
      values = await ask(`“${network.ssid}” ağına bağlan`, [
        ...(network.enterprise ? [{ id: "identity", label: "Kurumsal kullanıcı adı" }] : []),
        { id: "password", label: "Wi-Fi parolası", type: "password" }], "Bağlan");
    }
    if (values) await begin("/api/wifi/connect", { ssid: network.ssid, ...values, hidden: !!network.hidden });
  };
  const drawNetworks = nets => {
    $("#nets", el).innerHTML = nets.length ? nets.map((x, i) => row(esc(x.ssid),
      `Sinyal %${x.signal} · ${x.enterprise ? "Kurumsal" : x.secure ? "Şifreli" : "Açık"}${x.saved ? " · Kayıtlı" : ""}`,
      x.active ? '<span class="badge ok">Bağlı</span>' : `<button class="btn" data-net="${i}">Bağlan</button>`)).join("") : '<div class="empty">Tarama tamamlandı; ağ bulunamadı. Modeme yaklaşın veya ağ adını elle girin.</div>';
    el.querySelectorAll("[data-net]").forEach(b => b.onclick = () => connect(nets[b.dataset.net]));
  };
  const update = async () => {
    try {
      const op = await api("/api/network/operation", undefined, 8000);
      if (!el.isConnected) return;
      controls(op.status === "running");
      el.querySelectorAll("[data-net]").forEach(b => b.disabled = op.status === "running");
      $("#netState", el).textContent = ({ idle: "Hazır", running: "Çalışıyor…", ok: "Tamamlandı", error: "Başarısız" })[op.status];
      $("#netMessage", el).textContent = op.message || "Tarama başlatabilirsiniz.";
      if (op.status === "running") seenRunning = true;
      if (op.updated !== lastUpdated) {
        if (op.kind === "scan" && op.status === "ok") drawNetworks(op.networks);
        if (op.status === "error") showError(new Error(op.message), "Ağ işlemi");
        lastUpdated = op.updated;
      }
      if (seenRunning && op.status === "ok" && op.kind !== "scan" && !refreshing) { refreshing = true; route(); }
    } catch (error) {
      if (!el.isConnected) return;
      $("#netState", el).textContent = "Cihaza ulaşılamıyor";
      $("#netMessage", el).textContent = "Ağ geçişi sırasında bağlantı kesilmiş olabilir. Telefonunuzu hedef ağa veya Pardus-Maker ağına bağlayın. Bu sayfa yeniden deneyecek.\n\n" + error.message;
    }
  };
  $("#scan", el).onclick = () => s.hotspot ? toast("Erişim noktasını duraklat ve tara düğmesini kullanın.") : begin("/api/wifi/scan", {});
  if ($("#apScan", el)) $("#apScan", el).onclick = () => confirm("Tarama sırasında Pardus-Maker bağlantısı kesilecek, ardından geri açılacak. Devam edilsin mi?") && begin("/api/wifi/scan", { interrupt: true });
  $("#manual", el).onclick = async () => {
    const values = await ask("Ağ bilgilerini girin", [{ id: "ssid", label: "Ağ adı (SSID)" },
      { id: "password", label: "Parola (açık ağda boş bırakın)", type: "password" },
      { id: "identity", label: "Kurumsal kullanıcı adı (isteğe bağlı)" }, { id: "hidden", label: "Gizli ağ", type: "check" }], "Bağlan");
    if (values) begin("/api/wifi/connect", values);
  };
  el.querySelectorAll("[data-saved]").forEach(b => b.onclick = () => connect({ ssid: n.saved[b.dataset.saved], saved: true }));
  el.querySelectorAll("[data-edit]").forEach(b => b.onclick = () => connect({ ssid: n.saved[b.dataset.edit], saved: true }, true));
  el.querySelectorAll("[data-forget]").forEach(b => b.onclick = () => confirm("Kayıtlı ağ unutulsun mu?") && begin("/api/wifi/forget", { name: n.saved[b.dataset.forget] }));
  $("#hs", el).onchange = ev => begin("/api/hotspot", { on: ev.target.checked });
  $("#diag", el).onclick = async () => {
    try { const d = await api("/api/network/diagnostics"); $("#netDiag", el).hidden = false; $("#netDiag", el).textContent = d.details + "\n\n" + d.hint; }
    catch (e) { showError(e, "Ağ tanılama"); }
  };
  $("#netRefresh", el).onclick = route;
  await update();
  if (!el.isConnected) return;
  pollView(el, update, 2500);
  if (!s.hotspot && n.nm && !seenRunning && !lastUpdated) begin("/api/wifi/scan", {});
}

async function renderFan(el) {
  const s = await api("/api/fan");
  if (!el.isConnected) return;
  el.innerHTML = `<h1>Fan ve Soğutma</h1><p class="lead">Sıcaklığa göre otomatik fan yönetimi</p>
    <div class="tiles"><div class="tile"><div class="k">İşlemci sıcaklığı</div><div class="n">${s.temp_c == null ? "—" : s.temp_c + " °C"}</div></div>
    <div class="tile"><div class="k">Çalışan fan sürücüsü</div><div class="n">${s.active.length ? s.active.some(f => Number(f.state) > 0) ? "Fan açık" : "Fan bekliyor" : "Algılanmadı"}</div><div class="s">Mevcut çekirdek durumu</div></div></div>
    <p class="network-notice">${esc(s.note)} Fan gücünü GPIO’dan vermeyin; uygun fan kontrol devresi veya kontrol girişli fan kullanın.</p>
    <form id="fanForm" class="list form-list">
      <label class="row"><span class="grow">Fan modu</span><select id="fanMode"><option value="unmanaged">Yazılım yönetimi kapalı</option><option value="auto">Sıcaklığa göre otomatik</option><option value="always">Sürekli açık</option></select></label>
      <label class="row"><span class="grow">Kontrol pini (BCM GPIO)</span><input id="fanPin" type="number" min="4" max="27" value="${s.pin}" required></label>
      <label class="row fan-temp"><span class="grow">Bu sıcaklıkta aç (°C)</span><input id="fanOn" type="number" min="36" max="85" value="${s.on_c}" required></label>
      <label class="row fan-temp"><span class="grow">Bu sıcaklıkta kapat (°C)</span><input id="fanOff" type="number" min="35" max="84" value="${s.off_c}" required></label>
      <div class="row"><span class="grow hint">Açma ve kapama eşikleri fanın sık sık açılıp kapanmasını önler. Kaydetme sonrası yeniden başlatın.</span><button class="btn primary" type="submit">Kaydet</button></div>
    </form><p class="hint">Yönetimi kapatmak fanın fiziksel olarak durduğunu garanti etmez. Yeni ayar yeniden başlatmada geçerli olur.</p><p id="fanResult" role="status"></p>`;
  $("#fanMode", el).value = s.mode;
  const modeChanged = () => el.querySelectorAll(".fan-temp").forEach(r => r.hidden = $("#fanMode", el).value !== "auto");
  $("#fanMode", el).onchange = modeChanged; modeChanged();
  $("#fanForm", el).onsubmit = async ev => {
    ev.preventDefault();
    const r = await act("/api/fan", { mode: $("#fanMode", el).value, pin: Number($("#fanPin", el).value), on_c: Number($("#fanOn", el).value), off_c: Number($("#fanOff", el).value) });
    if (r) $("#fanResult", el).textContent = r.message;
  };
}

async function renderActivity(el) {
  el.innerHTML = `<h1>Görev Yöneticisi</h1><p class="lead">İşlemci, bellek ve ağ kullanımı · iki saniyede bir yenilenir</p>
    <div class="hrow activity-tools"><label>Sıralama <select id="processSort"><option value="cpu">CPU kullanımı</option><option value="memory">Bellek kullanımı</option><option value="pid">İşlem numarası</option></select></label>
    <label>İşlem ara <input id="processFilter" type="text" placeholder="Ad veya kullanıcı"></label><button class="btn" id="activityPause">Duraklat</button></div>
    <p class="hint" id="activityState" role="status">İlk örnek alınıyor…</p><div id="activitySummary" class="tiles"></div>
    <h2>Ağ kullanımı</h2><div id="activityNetwork" class="list"></div>
    <p class="hint">Hızlar anlık veri aktarımıdır. Toplamlar ağ arayüzü sayaçlarıdır; yeniden başlatma veya arayüz sıfırlamasında sıfırlanabilir. Bağlantı hızı aktarım hızından farklıdır.</p>
    <h2>Çalışan işlemler <span id="processCount"></span></h2><p class="hint">CPU yüzdesi işlem başına bir çekirdeğe göredir; çok çekirdekli işlemler %100’ü aşabilir.</p>
    <div class="process-wrap"><table class="process-table"><thead><tr><th>İşlem</th><th>PID</th><th>Kullanıcı</th><th>CPU</th><th>Bellek</th><th>Durum</th></tr></thead><tbody id="processRows"></tbody></table></div>`;
  let paused = false, latest = null;
  const bytes = b => b < 1000 ? Math.round(b) + " B" : b < 1e6 ? (b / 1000).toFixed(1) + " KB" : fmtSize(b);
  const draw = () => {
    if (!latest || !latest.memory) return;
    const s = latest, m = s.memory;
    $("#activitySummary", el).innerHTML = `<div class="tile"><div class="k">Toplam CPU · ${s.cores} çekirdek</div><div class="n">%${s.cpu_percent}</div><progress max="100" value="${s.cpu_percent}"></progress></div>
      <div class="tile"><div class="k">Bellek</div><div class="n">%${m.percent}</div><div class="s">${bytes(m.used)} / ${bytes(m.total)}</div><progress max="100" value="${m.percent}"></progress></div>
      <div class="tile"><div class="k">Sıcaklık</div><div class="n">${s.temp_c == null ? "—" : s.temp_c + " °C"}</div></div><div class="tile"><div class="k">Çalışma süresi</div><div class="n small">${fmtUp(s.uptime)}</div><div class="s">Takas: ${bytes(m.swap_used)} / ${bytes(m.swap_total)}</div></div>`;
    $("#activityNetwork", el).innerHTML = s.interfaces.length ? s.interfaces.map(n => row(esc(n.name) + ` <span class="badge ${n.connected ? "ok" : ""}">${n.connected ? "Bağlı" : esc(n.state)}</span>`,
      `Alınan toplam ${bytes(n.rx)} · Gönderilen toplam ${bytes(n.tx)}${n.link_mbps ? " · Bağlantı " + n.link_mbps + " Mb/s" : ""} · Hatalar ↓${n.rx_errors} ↑${n.tx_errors}`,
      `<div class="net-rates"><span>↓ ${bytes(n.rx_bps)}/sn</span><span>↑ ${bytes(n.tx_bps)}/sn</span></div>`)).join("") : '<div class="empty">Ağ arayüzü yok</div>';
    const sort = $("#processSort", el).value, filter = $("#processFilter", el).value.toLocaleLowerCase("tr");
    const list = s.processes.filter(p => `${p.name} ${p.user} ${p.pid}`.toLocaleLowerCase("tr").includes(filter)).sort((a, b) => sort === "pid" ? a.pid - b.pid : b[sort] - a[sort]);
    $("#processCount", el).textContent = `(${list.length})`;
    const states = { R: "Çalışıyor", S: "Bekliyor", D: "G/Ç bekliyor", T: "Duraklatıldı", Z: "Sonlandı", I: "Boşta" };
    $("#processRows", el).innerHTML = list.map(p => `<tr><td>${esc(p.name)}</td><td>${p.pid}</td><td>${esc(p.user)}</td><td>%${p.cpu.toFixed(1)}</td><td>${bytes(p.memory)} <small>(%${p.memory_percent})</small></td><td>${esc(states[p.state] || p.state)}</td></tr>`).join("");
  };
  const update = async () => {
    if (paused) return;
    try {
      const s = await api("/api/activity", undefined, 8000);
      if (!el.isConnected) return;
      latest = s; draw();
      $("#activityState", el).textContent = s.error || (s.ready ? "Son ölçüm: " + new Date(s.sampled * 1000).toLocaleTimeString("tr-TR") : "İlk örnek alınıyor; hızlar ikinci ölçümde görünecek.");
    } catch (e) { if (el.isConnected) $("#activityState", el).textContent = e.message; }
  };
  $("#processSort", el).onchange = draw; $("#processFilter", el).oninput = draw;
  $("#activityPause", el).onclick = ev => { paused = !paused; ev.target.textContent = paused ? "Devam et" : "Duraklat"; $("#activityState", el).textContent = paused ? "Görünüm duraklatıldı." : "Yenileniyor…"; if (!paused) update(); };
  await update(); if (el.isConnected) pollView(el, update);
}

// ---------------------------------------------------------------- Gorunum
async function renderGorunum(el) {
  const st = await api("/api/settings");
  if (!el.isConnected) return;
  el.innerHTML = `<h1>Görünüm</h1><p class="lead">Pardus 100. Yıl teması · açık ve koyu</p>
    <div class="list">${row("Tema", "Masaüstü, pencereler, simgeler ve dock",
      `<div class="seg" id="seg"><button data-m="light" class="${st.theme === "light" ? "on" : ""}">Açık</button><button data-m="dark" class="${st.theme === "dark" ? "on" : ""}">Koyu</button></div>`)}</div>`;
  $("#seg", el).querySelectorAll("button").forEach(b => b.onclick = async () => {
    if (await act("/api/settings/theme", { mode: b.dataset.m }, "Tema uygulandı")) {
      $("#seg", el).querySelectorAll("button").forEach(x => x.classList.toggle("on", x === b));
      document.documentElement.dataset.theme = b.dataset.m;
    }
  });
}

// ---------------------------------------------------------------- Dil ve klavye
async function renderDil(el) {
  const l = await api("/api/language");
  if (!el.isConnected) return;
  el.innerHTML = `<h1>Dil ve Klavye</h1><p class="lead">Masaüstü, menüler ve uygulamaların dili ile klavye düzeni</p>
    <h2>Sistem dili</h2><div class="list">${row("Dil", "Masaüstü yeniden başlatılınca uygulanır" + (l.installed.length < Object.keys(l.locales).length ? "; ilk kez seçilen dil birkaç saniye hazırlanır" : ""),
      `<select id="loc">${Object.entries(l.locales).map(([k, t]) => `<option value="${k}" ${k === l.locale ? "selected" : ""}>${esc(t)}</option>`).join("")}</select>`)}
      ${row("Masaüstünü yeniden başlat", "Dil değişikliğini şimdi uygular (açık pencereler kapanır)", '<button class="btn" id="dm">Yeniden başlat</button>')}</div>
    <h2>Klavye</h2><div class="list">
      ${row("Klavye düzeni", "Hemen uygulanır (Pi'nin masaüstü ve uzak ekran)",
        `<select id="kb">${Object.entries(l.keyboards).map(([k, t]) => `<option value="${k}" ${k === l.keyboard ? "selected" : ""}>${esc(t)}</option>`).join("")}</select>`)}
      ${row("İkinci düzen: İngilizce (ABD)", "Alt+Shift ile düzenler arasında geçiş", sw("kbs", l.switch))}
    </div>
    <p class="hint">Pardus Maker Merkezi'nin arayüzü Türkçedir. Klavye düzeni Pi'ye takılı klavye içindir; telefon/bilgisayardaki klavye uzak ekranda kendi düzeniyle yazar.</p>`;
  $("#loc", el).onchange = async e => {
    e.target.disabled = true; toast("Dil hazırlanıyor…", 15000);
    if (await act("/api/language", { locale: e.target.value }) && confirm("Masaüstü şimdi yeniden başlatılsın mı? Açık pencereler kapanır."))
      act("/api/desktop/restart", {}, "Masaüstü yeniden başlatıldı");
    e.target.disabled = false;
  };
  $("#dm", el).onclick = () => act("/api/desktop/restart", {}, "Masaüstü yeniden başlatıldı");
  const kb = () => act("/api/keyboard", { keyboard: $("#kb", el).value, switch: $("#kbs", el).checked });
  $("#kb", el).onchange = kb; $("#kbs", el).onchange = kb;
}

// ---------------------------------------------------------------- Tarih ve saat
async function renderSaat(el) {
  const t = await api("/api/time");
  if (!el.isConnected) return;
  // Pi saati ile tarayici saati arasindaki fark; saat ekranda canli ilerler.
  const offset = t.epoch * 1000 - Date.now();
  const drift = Math.round(offset / 1000);
  const fmt = new Intl.DateTimeFormat("tr-TR", { timeZone: t.timezone, dateStyle: "full", timeStyle: "medium" });
  const pad = n => String(n).padStart(2, "0");
  const groups = {};
  t.timezones.forEach(z => (groups[z.includes("/") ? z.split("/")[0] : "Diğer"] ||= []).push(z));
  el.innerHTML = `<h1>Tarih ve Saat</h1><p class="lead">Pi'de pilli donanım saati yoktur: internet varken saat otomatik (NTP) düzeltilir, yoksa buradan ayarlayın.</p>
    <div class="tiles"><div class="tile" style="grid-column:1/-1"><div class="k">Pi'nin saati · ${esc(t.timezone)}</div>
      <div class="n" id="clk">—</div>
      <div class="s">${t.ntp ? (t.synced ? '<span class="dot-s ok"></span>Saat sunucusuyla eşitlendi' : '<span class="dot-s"></span>Otomatik, henüz eşitlenmedi (internet gerekir)')
                             : '<span class="dot-s bad"></span>Otomatik saat kapalı'}
        ${Math.abs(drift) > 60 ? ` · <b>bu cihazdan ${Math.abs(drift) >= 3600 ? Math.round(Math.abs(drift) / 3600) + " saat" : Math.round(Math.abs(drift) / 60) + " dk"} ${drift > 0 ? "ileri" : "geri"}</b>` : ""}</div></div></div>
    <h2>Saat dilimi</h2><div class="list">${row("Saat dilimi", "Türkiye: Europe/Istanbul (UTC+3)",
      `<select id="tz">${Object.keys(groups).sort().map(g => `<optgroup label="${esc(g)}">${groups[g].map(z =>
        `<option ${z === t.timezone ? "selected" : ""}>${esc(z)}</option>`).join("")}</optgroup>`).join("")}</select>`)}</div>
    <h2>Otomatik saat</h2><div class="list">
      ${row("İnternetten otomatik ayarla (NTP)", t.systemd ? "Ağ bağlantısı gelince saat sunucusundan alınır" : "Bu test ortamında saat ana makineden gelir; Pi imajında NTP kullanılır", sw("ntp", t.ntp))}
      ${row("Şimdi eşitle", "Saat sunucusuna yeniden bağlanır", '<button class="btn" id="sync">Eşitle</button>')}
    </div>
    <h2>Elle ayarla</h2><div class="list">
      ${row("Bu cihazın saatini kullan", "İnternet yokken (ör. Pardus-Maker erişim noktasında) telefon/bilgisayarın saatini Pi'ye aktarır",
        '<button class="btn primary" id="usemine">Uygula</button>')}
      ${row("Tarih ve saat", "Otomatik saat kapatılır",
        `<div class="btns"><input type="date" id="md"><input type="time" id="mt" step="1"><button class="btn" id="mset">Ayarla</button></div>`)}
    </div>
    <p class="hint">${t.systemd ? 'Okul/kurum ağları NTP\'yi (UDP 123) engelleyebilir; saat eşitlenmiyorsa "Bu cihazın saatini kullan"ı deneyin. Yanlış saat HTTPS sitelerinde ve <code>apt</code> güncellemelerinde sertifika hatalarına yol açar.' : "Docker testinde systemd-timesyncd yoktur; otomatik saat açık tutulur ve ana makine saati kullanılır. Gerçek Pi'de NTP çalışır."}</p>`;
  const tick = () => { const c = $("#clk", el); if (c) c.textContent = fmt.format(new Date(Date.now() + offset)); };
  tick(); timer = setInterval(tick, 1000);
  const now = new Date(Date.now() + offset);
  $("#md", el).value = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  $("#mt", el).value = `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
  $("#tz", el).onchange = async e => { if (await act("/api/time/timezone", { timezone: e.target.value })) route(); };
  $("#ntp", el).onchange = async e => { if (await act("/api/time/ntp", { enabled: e.target.checked })) setTimeout(route, 1500); };
  $("#sync", el).onclick = async () => { if (await act("/api/time/sync", {})) setTimeout(route, 4000); };
  $("#usemine", el).onclick = async () => { if (await act("/api/time/set", { epoch: Date.now() / 1000 })) route(); };
  $("#mset", el).onclick = async () => {
    // Girilen saat tarayicinin yerel diliminde yorumlanir.
    const d = new Date(`${$("#md", el).value}T${$("#mt", el).value || "00:00:00"}`);
    if (isNaN(d)) return toast("Geçerli bir tarih ve saat girin");
    if (await act("/api/time/set", { epoch: d.getTime() / 1000 })) route();
  };
}

// ---------------------------------------------------------------- Ekran ve donanim
async function renderDonanim(el) {
  const [{ hardware: h }, dsp] = await Promise.all([api("/api/settings"), api("/api/displays")]);
  if (!el.isConnected) return;
  const ifs = [["i2c", "I2C", "Sensörler, OLED ekranlar (GPIO2/3)"], ["spi", "SPI", "Ekranlar, ADC'ler (GPIO7-11)"],
               ["serial", "Seri port (UART)", "GPIO14/15 · seri konsol"]];
  const resName = r => r === "auto" ? "Otomatik (monitöre göre)" : r;
  el.innerHTML = `<h1>Ekran ve Donanım</h1><p class="lead">Ekran, ses ve GPIO arayüzleri. Yapılandırma değişiklikleri yeniden başlatınca uygulanır.</p>
    <h2>Ekranlar</h2><div class="list">${dsp.displays.length ? dsp.displays.map(d => row(
      `${esc(d.kind)}${d.port ? ` <span class="badge">${esc(d.port)}</span>` : ""}`,
      d.connected ? [d.monitor || "Monitör bağlı", dsp.current[d.connector] && "şu an " + dsp.current[d.connector],
                     d.modes[0] && "en iyi " + d.modes[0]].filter(Boolean).map(esc).join(" · ") : "Bağlı değil",
      d.connected ? '<span class="badge ok">bağlı</span>' : '<span class="badge">boş</span>')).join("")
      : '<div class="empty">Ekran çıkışı bulunamadı (Raspberry Pi üzerinde çalıştırın)</div>'}</div>
    <p class="hint">Pi 4/400'de iki mikro HDMI çıkışı vardır; tek monitörü USB-C güç girişinin yanındaki <b>HDMI 0</b>'a takın, ikinci monitör HDMI 1'de genişletilmiş masaüstü olur.
      Resmi Raspberry Pi dokunmatik ekranı (DSI) otomatik algılanır. Monitör sonradan takılırsa görüntü gelmezse "Masaüstünü yeniden başlat"ı kullanın.</p>
    <h2>Çözünürlük</h2><div class="list">${row("HDMI 0 çözünürlüğü", "Monitör yokken uzak ekran da bu boyutta çalışır",
      `<select id="res">${h.resolutions.map(r => `<option value="${r}" ${r === h.resolution ? "selected" : ""}>${resName(r)}</option>`).join("")}</select>`)}
      ${row("Masaüstünü yeniden başlat", "Yeni takılan monitörü algılatır", '<button class="btn" id="dm">Yeniden başlat</button>')}</div>
    <h2>Ses</h2><div class="list">${dsp.audio.length ? dsp.audio.map(a => row(esc(a.name),
      [a.playback && "çıkış", a.capture && "giriş (mikrofon)"].filter(Boolean).join(" · ") || "—", `<span class="v">kart ${a.card}</span>`)).join("")
      : '<div class="empty">Ses aygıtı bulunamadı</div>'}</div>
    <p class="hint">HDMI sesi monitör hoparlöründen, 3.5 mm çıkış kulaklıktan verilir; USB mikrofon ve ses kartları tak-çalıştır çalışır. Ses çıkışı masaüstündeki ses simgesinden seçilir.</p>
    <h2>GPIO arayüzleri</h2><div class="list">${ifs.map(([k, t, d]) => row(t, d, sw("if-" + k, h[k]))).join("")}
      ${row("Kamera", "Kamera modeli, canlı görüntü ve fotoğraf", '<a class="btn" href="#kamera">Kamera</a>')}</div>
    <div class="btns" style="margin-top:12px"><button class="btn primary" id="rb">Şimdi yeniden başlat</button></div>`;
  $("#res", el).onchange = e => act("/api/settings/resolution", { resolution: e.target.value });
  $("#dm", el).onclick = () => act("/api/desktop/restart", {}, "Masaüstü yeniden başlatıldı");
  ifs.forEach(([k]) => $("#if-" + k, el).onchange = e => act("/api/settings/interface", { key: k, enabled: e.target.checked }));
  $("#rb", el).onclick = () => confirm("Pi yeniden başlatılsın mı?") && act("/api/power", { action: "reboot" }, "Yeniden başlatılıyor…");
}

// ---------------------------------------------------------------- Uzak erisim
async function renderUzak(el) {
  const [st, s] = await Promise.all([api("/api/settings"), api("/api/status")]);
  if (!el.isConnected) return;
  const ip = (s.ips[0] || {}).addr?.split("/")[0] || "<IP>";
  el.innerHTML = `<h1>Uzak Erişim</h1><p class="lead">Aynı ağdaki bir bilgisayar veya tabletten bu cihazı yönetin.</p>
    <div class="list">
      ${row("Uzak ekran", "Masaüstünü tarayıcıdan görüntüle ve kontrol et (VNC)", sw("rem", st.remote))}
      ${row("Web adresi", "Tarayıcıya yazın; kullanıcı adı ve parola ile girin", `<span class="v">http://${esc(s.hostname)}.local</span>`)}
      ${row("IP ile", "", `<span class="v">http://${esc(ip)}</span>`)}
      ${row("VNC istemcisi", "RealVNC, Remmina vb.", `<span class="v">${esc(ip)}:5900</span>`)}
      ${row("SSH", "", `<span class="v">ssh pardus@${esc(s.hostname)}.local</span>`)}
    </div><p class="hint">Bağlantı şifrelenmez (HTTP/VNC); yalnızca güvendiğiniz yerel ağda kullanın.</p>`;
  $("#rem", el).onchange = e => act("/api/settings/remote", { enabled: e.target.checked }, e.target.checked ? "Uzak ekran açıldı" : "Uzak ekran kapatıldı");
}

// ---------------------------------------------------------------- Kullanici
async function renderKullanici(el) {
  const s = await api("/api/status");
  if (!el.isConnected) return;
  el.innerHTML = `<h1>Kullanıcı</h1><p class="lead">Kullanıcı: <b>pardus</b> · otomatik oturum açık</p>
    <h2>Parola</h2><div class="list">
      ${row("Yeni parola", "Giriş, SSH, sudo, web ve uzak ekran (uzak ekran ilk 8 karakteri kullanır)",
        '<input type="password" id="pw" placeholder="en az 6 karakter" style="width:180px"> <button class="btn primary" id="pwb">Değiştir</button>')}
    </div>
    <h2>Cihaz adı</h2><div class="list">
      ${row("Ağdaki ad", "Örnek: lab-pi-3 → lab-pi-3.local",
        `<input type="text" id="hn" value="${esc(s.hostname)}" style="width:160px"> <button class="btn" id="hnb">Kaydet</button>`)}
    </div>
    <h2>Oturum</h2><div class="list">
      ${row("Web oturumu", "Bu tarayıcıdaki Merkez oturumunu kapatır", '<button class="btn" id="logout">Çıkış yap</button>')}
    </div>`;
  $("#pwb", el).onclick = async () => { if (await act("/api/settings/password", { password: $("#pw", el).value })) $("#pw", el).value = ""; };
  $("#hnb", el).onclick = async () => { if (await act("/api/settings/hostname", { name: $("#hn", el).value.trim() })) $("#devName").textContent = $("#hn", el).value.trim(); };
  $("#logout", el).onclick = async () => {
    try { await api("/api/logout", {}); } catch (_) {}
    location.reload();
  };
}

// ---------------------------------------------------------------- Guncellemeler
async function renderGuncelleme(el) {
  el.innerHTML = `<h1>Güncellemeler</h1><p class="lead">Pardus paket deposu, güvenlik güncellemeleri ve VS Code.</p>
    <div class="list">
      ${row("Güncellemeleri denetle", "Paket listesini yeniler, güncellenebilecekleri gösterir", '<button class="btn" id="chk">Denetle</button>')}
      ${row("Tümünü güncelle", "İnternet hızına göre birkaç dakika sürebilir", '<button class="btn primary" id="ins">Güncelle</button>')}
    </div>
    <p class="hint">Python sürümleri <a href="#python">Python</a>, yapay zeka ajanları <a href="#agent">Agent'lar</a> bölümünden güncellenir.</p>`;
  $("#chk", el).onclick = () => runJob("/api/updates/check", {});
  $("#ins", el).onclick = () => runJob("/api/updates/install", {});
}

// ---------------------------------------------------------------- Python eklentileri
async function renderPlugins(el) {
  el.innerHTML = `<h1>Eklentiler</h1><p class="lead">Kendi Python projelerinizi uygulama gibi ekleyin ve çalıştırın.</p>
    <div class="network-notice"><b>Bir şablonla başlayın.</b><p>Python kodunuzu ve bağımlılıklarınızı projeye ekleyin; ZIP dosyasını içe aktarın. Eklentiler pardus kullanıcısı olarak çalışır. İlk açılışta paketlerin indirilmesi gerekebilir.</p></div>
    <div class="plugin-tools"><a class="btn" href="/api/plugins/template.zip" download="pardus-python-eklenti.zip">Python şablonunu indir</a>
      <label class="btn primary" for="pluginZip">ZIP içe aktar</label><input id="pluginZip" type="file" accept=".zip,application/zip" hidden><button class="btn" id="pluginsRefresh">Yenile</button></div>
    <p id="pluginImportState" role="status" class="hint">ZIP kökünde plugin.json, pyproject.toml ve başlangıç .py dosyası bulunmalı. En fazla 15 MB; .venv eklemeyin.</p>
    <p id="pluginWarning" class="error-inline" hidden></p><div id="pluginList" class="plugin-grid"></div>
    <section id="pluginLogs" hidden><h2 class="hrow"><span id="pluginLogTitle">Çalışma günlüğü</span><span class="sp"></span><button class="btn" id="pluginLogClose">Kapat</button></h2><pre id="pluginLogText" class="diagnostics plugin-log"></pre></section>`;
  let selected = null, busy = false;
  const log = async () => {
    if (!selected) return;
    const data = await api('/api/plugins/logs?id=' + encodeURIComponent(selected));
    if (!el.isConnected) return;
    $('#pluginLogTitle', el).textContent = selected + ' · Çalışma günlüğü';
    $('#pluginLogText', el).textContent = data.text || 'Henüz günlük kaydı yok.';
    $('#pluginLogs', el).hidden = false;
  };
  const update = async () => {
    if (busy) return;
    const data = await api('/api/plugins');
    if (!el.isConnected) return;
    $('#pluginWarning', el).hidden = !data.warning;
    $('#pluginWarning', el).textContent = data.warning || '';
    $('#pluginList', el).innerHTML = data.plugins.length ? data.plugins.map(p => {
      const running = ['active', 'activating', 'deactivating'].includes(p.state);
      const state = ({ active: 'Çalışıyor', activating: 'Başlatılıyor', deactivating: 'Durduruluyor', failed: 'Hata ile durdu', inactive: 'Durduruldu' })[p.state] || p.state;
      return `<article class="plugin-card"><div class="hrow"><span class="plugin-letter" aria-hidden="true">${esc(p.name[0])}</span><div class="grow"><b>${esc(p.name)}</b><div class="hint">${esc(p.id)} · ${esc(p.version)}</div></div><span class="badge ${running ? 'ok' : p.state === 'failed' ? 'acc' : ''}">${esc(state)}</span></div>
        <p>${esc(p.description)}</p>${p.error ? `<pre class="error-inline">${esc(p.error)}</pre>` : ''}
        <p class="hint">${p.pid !== '0' ? 'PID ' + esc(p.pid) : ''}${/^\d+$/.test(p.memory) ? ' · Bellek ' + fmtSize(Number(p.memory)) : ''}${p.state === 'failed' ? ' · Çıkış kodu ' + esc(p.exit_code) : ''}</p>
        <div class="btns"><button class="btn ${running ? '' : 'primary'}" data-plugin="${esc(p.id)}" data-action="${running ? 'stop' : 'start'}" ${p.error && !running ? 'disabled' : ''}>${running ? 'Durdur' : 'Çalıştır'}</button>
        ${running && p.web_port ? `<a class="btn" target="_blank" rel="noopener" href="${location.protocol}//${location.hostname}:${Number(p.web_port)}/">Uygulamayı aç</a>` : ''}
        <button class="btn" data-log="${esc(p.id)}">Günlük</button><button class="btn danger" data-plugin="${esc(p.id)}" data-action="remove" ${running ? 'disabled' : ''}>Kaldır</button></div>
        ${p.path ? `<details><summary>Proje klasörü</summary><code>${esc(p.path)}</code></details>` : ''}</article>`;
    }).join('') : '<div class="list"><div class="empty">Henüz eklenti yok. Şablonu indirin, Python kodunuzu ekleyin ve ZIP olarak içe aktarın.</div></div>';
    el.querySelectorAll('[data-plugin]').forEach(button => button.onclick = async () => {
      if (busy) return;
      if (button.dataset.action === 'remove' && !confirm('Eklenti listeden kaldırılıp arşiv klasörüne taşınsın mı?')) return;
      busy = true; button.disabled = true;
      await act('/api/plugins/action', { id: button.dataset.plugin, action: button.dataset.action });
      busy = false;
      if (el.isConnected) await update();
    });
    el.querySelectorAll('[data-log]').forEach(button => button.onclick = async () => {
      selected = button.dataset.log;
      try { await log(); } catch (error) { showError(error, 'Eklenti günlüğü'); }
    });
    await log();
  };
  $('#pluginLogClose', el).onclick = () => { selected = null; $('#pluginLogs', el).hidden = true; };
  $('#pluginsRefresh', el).onclick = () => update().catch(error => showError(error));
  $('#pluginZip', el).onchange = async event => {
    const file = event.target.files[0];
    if (!file || busy) return;
    if (file.size > 15 * 1024 * 1024) { showError(new Error('ZIP en fazla 15 MB olabilir. .venv klasörünü eklemeyin.')); event.target.value = ''; return; }
    busy = true; $('#pluginImportState', el).textContent = 'Eklenti içe aktarılıyor…';
    try {
      const archive = await new Promise((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result).split(',')[1]); reader.onerror = () => reject(new Error('ZIP dosyası okunamadı.')); reader.readAsDataURL(file); });
      const r = await api('/api/plugins/import', { archive }, 120000);
      $('#pluginImportState', el).textContent = r.message;
    } catch (error) { $('#pluginImportState', el).textContent = 'İçe aktarma başarısız.'; showError(error, 'Eklenti içe aktarma'); }
    finally { busy = false; event.target.value = ''; if (el.isConnected) update().catch(error => showError(error)); }
  };
  await update(); if (el.isConnected) pollView(el, update, 5000);
}

// ---------------------------------------------------------------- Python
async function renderPython(el) {
  const [v, ai] = await Promise.all([api("/api/python/versions"), api("/api/python/ai")]);
  if (!el.isConnected) return;
  el.innerHTML = `<h1>Python</h1><p class="lead">Varsayılan: <b>Python ${esc(v.default)}</b> · terminalde <code>python3</code></p>
    <h2>Sürümler</h2><div class="list">${v.versions.map(r => row(
      `Python ${esc(r.installed || r.latest)}${r.default ? ' <span class="badge acc">varsayılan</span>' : ""}${r.installed ? ' <span class="badge ok">kurulu</span>' : ""}`,
      r.update ? `Yeni yama: ${esc(r.latest)}` : (r.installed ? "" : "Kurulabilir"),
      `<div class="btns">${r.installed
        ? `${r.update ? `<button class="btn" data-a="install" data-m="${r.minor}">Güncelle</button>` : ""}
           ${r.default ? "" : `<button class="btn" data-a="default" data-m="${r.minor}">Varsayılan yap</button>
           <button class="btn danger" data-a="uninstall" data-m="${r.minor}">Kaldır</button>`}`
        : `<button class="btn primary" data-a="install" data-m="${r.minor}">Kur</button>`}</div>`)).join("")}</div>
    <h2>Hazır yapay zeka ortamı</h2><div class="list">${ai.present ? `
      ${row(`AI ortamı · Python ${esc(ai.python)}`, `${esc(ai.path)} · terminalde <code>ai-env</code> ile etkinleştirin`,
        '<div class="btns"><button class="btn" id="aiT">Terminal</button><button class="btn primary" id="aiJ">Jupyter Lab</button></div>')}
      ${row("Kurulu paketler", ai.packages.map(p => `${esc(p.name)} ${esc(p.version)}`).join(" · "))}`
      : '<div class="empty">AI ortamı bulunamadı</div>'}</div>
    <p class="hint">Her proje kendi sanal ortamını kullanır; paket eklemek için <a href="#projeler">Projeler</a>.</p>`;
  el.querySelectorAll("button[data-a]").forEach(b => b.onclick = async () => {
    const m = b.dataset.m, a = b.dataset.a;
    if (a === "default") { if (await act("/api/python/default", { minor: m })) route(); return; }
    if (a === "uninstall" && !confirm(`Python ${m} kaldırılsın mı? Bu sürümü kullanan projeler çalışmaz.`)) return;
    runJob(`/api/python/${a}`, { minor: m }, () => route());
  });
  if (ai.present) { $("#aiT", el).onclick = () => act("/api/python/ai/open", { how: "terminal" }, "Terminal açıldı");
                    $("#aiJ", el).onclick = () => act("/api/python/ai/open", { how: "jupyter" }, "Jupyter Lab başlatılıyor…"); }
}

// ---------------------------------------------------------------- Projeler
async function renderProjeler(el) {
  const [p, v] = await Promise.all([api("/api/python/projects"), api("/api/python/versions")]);
  if (!el.isConnected) return;
  const inst = v.versions.filter(r => r.installed);
  el.innerHTML = `<h1>Projeler</h1><p class="lead">${esc(p.dir)} · her proje kendi sanal ortamıyla (uv)</p>
    <h2>Yeni proje</h2><div class="list">
      <div class="row" style="flex-wrap:wrap">
        <input type="text" id="pn" placeholder="proje-adi" style="flex:1;min-width:140px">
        <select id="pt"><optgroup label="Genel">${Object.entries(p.templates).filter(([k]) => !k.startsWith("mp-")).map(([k, t]) => `<option value="${k}">${esc(t)}</option>`).join("")}</optgroup>
          <optgroup label="MediaPipe · kamera ile yapay zeka">${Object.entries(p.templates).filter(([k]) => k.startsWith("mp-")).map(([k, t]) => `<option value="${k}">${esc(t.replace("MediaPipe · ", ""))}</option>`).join("")}</optgroup></select>
        <select id="pv">${inst.map(r => `<option value="${r.minor}" ${r.default ? "selected" : ""}>Python ${r.minor}</option>`).join("")}</select>
        <button class="btn primary" id="pc">Oluştur</button>
      </div></div>
    <p class="hint" id="ptip" hidden>MediaPipe örnekleri Python 3.12 ile kurulur, Pi kamerayı veya USB kamerayı otomatik kullanır.
      Projeyi Terminal'de açıp <code>uv run main.py --profil tasarruf</code> yazın; sonucu Pi'nin ekranında veya telefondan <b>http://&lt;pi&gt;.local:8000</b> adresinde izleyin.
      Resim için <code>uv run main.py --kaynak foto.jpg</code>: PNG ve JSON çıktıları <code>sonuclar/</code> klasörüne kaydedilir. Kamera aynı anda tek uygulamada açılabilir: Kamera sayfasını kapatın.</p>
    <h2>Projelerim</h2><div class="list">${p.projects.length ? p.projects.map(x => row(esc(x.name),
      (x.python ? "Python " + esc(x.python) : "sanal ortam yok"),
      `<div class="btns"><button class="btn" data-o="code" data-n="${esc(x.name)}">VS Code</button>
        <button class="btn" data-o="terminal" data-n="${esc(x.name)}">Terminal</button>
        <button class="btn" data-o="add" data-n="${esc(x.name)}">Paket ekle</button>
        <button class="btn danger" data-o="del" data-n="${esc(x.name)}">Sil</button></div>`)).join("")
      : '<div class="empty">Henüz proje yok</div>'}</div>`;
  $("#pt", el).onchange = e => { const mp = e.target.value.startsWith("mp-"); $("#pv", el).disabled = mp;
    $("#ptip", el).hidden = !mp; };
  $("#pc", el).onclick = () => {
    const name = $("#pn", el).value.trim();
    if (!name) return toast("Proje adı girin");
    runJob("/api/python/projects/create", { name, template: $("#pt", el).value, python: $("#pv", el).value }, () => route());
  };
  el.querySelectorAll("button[data-o]").forEach(b => b.onclick = async () => {
    const n = b.dataset.n, o = b.dataset.o;
    if (o === "add") { const pkg = prompt("Eklenecek paket(ler) (örn. requests numpy):"); if (pkg) runJob("/api/python/projects/add", { name: n, package: pkg }); }
    else if (o === "del") { if (confirm(`"${n}" silinsin mi? (Projeler/.silinenler klasörüne taşınır)`) && await act("/api/python/projects/delete", { name: n }, "Taşındı")) route(); }
    else act("/api/python/projects/open", { name: n, how: o }, o === "code" ? "VS Code açılıyor…" : "Terminal açıldı");
  });
}

// ---------------------------------------------------------------- Agent'lar
const AGENT_COLORS = { claude: "#d97757", codex: "#10a37f", gemini: "#4285f4", opencode: "#3b3b46", antigravity: "#7b61ff", ollama: "#1c1c1e" };
async function renderAgents(el) {
  const [{ agents, node }, proj] = await Promise.all([api("/api/agents"), api("/api/python/projects")]);
  if (!el.isConnected) return;
  el.innerHTML = `<h1>Agent'lar</h1><p class="lead">Terminalde çalışan yapay zeka kodlama ajanları ve yerel modeller · Node.js ${esc(node || "yok")}</p>
    <div class="list">${agents.map(a => `<div class="row" style="align-items:flex-start">
      <div class="agent-logo" style="background:${AGENT_COLORS[a.id] || "#888"}">${esc(a.name[0])}</div>
      <div class="grow"><div class="t">${esc(a.name)} <span class="badge">${esc(a.vendor)}</span>${a.installed ? ` <span class="badge ok">${esc(a.version || "kurulu")}</span>` : ""}</div>
        <div class="d">${esc(a.summary)}</div>
        <div class="d"><b>Ne zaman:</b> ${esc(a.tip)}</div>
        <div class="d"><b>Gerekli:</b> ${esc(a.account)} · <a href="${esc(a.docs)}" target="_blank" rel="noopener">belgeler</a></div></div>
      <div class="btns">${a.installed
        ? `<button class="btn primary" data-l="${a.id}">Başlat</button><button class="btn" data-a="update" data-id="${a.id}">Güncelle</button><button class="btn danger" data-a="remove" data-id="${a.id}">Kaldır</button>`
        : `<button class="btn primary" data-a="install" data-id="${a.id}">Kur</button>`}</div></div>`).join("")}</div>
    <h2>Başlatma klasörü</h2><div class="list">${row("Proje", "Ajan bu klasörde açılır",
      `<select id="ap"><option value="">Ev klasörü</option>${proj.projects.map(p => `<option>${esc(p.name)}</option>`).join("")}</select>`)}</div>
    <p class="hint">Ajanlar Pi'nin masaüstünde bir terminal penceresinde açılır; ilk açılışta hesabınızla giriş yapmanız istenir.</p>`;
  el.querySelectorAll("button[data-a]").forEach(b => b.onclick = () => {
    if (b.dataset.a === "remove" && !confirm("Kaldırılsın mı?")) return;
    runJob("/api/agents/action", { id: b.dataset.id, action: b.dataset.a }, () => route());
  });
  el.querySelectorAll("button[data-l]").forEach(b => b.onclick = () =>
    act("/api/agents/launch", { id: b.dataset.l, project: $("#ap", el).value }, "Terminalde açıldı"));
}

// ---------------------------------------------------------------- GPIO
let selPin = null;
async function renderGpio(el) {
  const draw = async () => {
    const g = await api("/api/gpio");
    if (!el.isConnected) return;
    const pinHtml = p => {
      const lvl = p.mode ? `<span class="lvl ${p.value ? "h" : "l"}">${p.mode === "out" ? "Ç" : "G"} ${p.value == null ? "?" : p.value ? "1" : "0"}</span>` : "";
      return `<div class="pin ${p.usable ? "usable" : ""} ${selPin === p.bcm && p.usable ? "sel" : ""}" data-bcm="${p.bcm ?? ""}">
        <span class="dot k-${p.kind}">${p.phys}</span><span class="nm">${esc(p.name)}</span>${lvl}</div>`;
    };
    const sel = g.pins.find(p => p.usable && p.bcm === selPin);
    el.innerHTML = `<h1>GPIO Pinleri</h1><p class="lead">${g.chip ? `40 pinli başlık · ${esc(g.chip)}` : "GPIO donanımı bulunamadı (Raspberry Pi üzerinde çalıştırın)"}</p>
      <div class="gpio-grid" style="display:grid;grid-template-columns:minmax(0,1.3fr) minmax(0,1fr);gap:14px;align-items:start">
        <div class="header40">${g.pins.map(pinHtml).join("")}</div>
        <div>
          <div class="list">${sel ? `
            ${row(esc(sel.name), `Fiziksel pin ${sel.phys} · BCM ${sel.bcm}`)}
            ${row("Çıkış", "LED, röle, buzzer", '<div class="btns"><button class="btn" data-m="out" data-v="1">Yüksek (1)</button><button class="btn" data-m="out" data-v="0">Düşük (0)</button></div>')}
            ${row("Giriş", "Buton, sensör", '<div class="btns"><button class="btn" data-m="in" data-b="pull-up">Pull-up</button><button class="btn" data-m="in" data-b="pull-down">Pull-down</button></div>')}
            ${row("Bırak", "Pini serbest bırak", '<button class="btn" data-m="release">Bırak</button>')}`
            : '<div class="empty">Bir GPIO pinine tıklayın</div>'}</div>
          <h2>I2C aygıtları</h2><div class="list">${row("Veri yolu 1", '<span id="i2c">—</span>', '<button class="btn" id="scan">Tara</button>')}</div>
          <h2>Python ile</h2><div class="list"><div class="row"><code style="font-size:12px;white-space:pre">from gpiozero import LED
led = LED(${sel ? sel.bcm : 17})
led.on()</code></div></div>
          <p class="hint">Renkler: <span class="lvl" style="background:#2f9e44;color:#fff">GPIO</span> <span class="lvl" style="background:#1c7ed6;color:#fff">I2C</span>
            <span class="lvl" style="background:#9c36b5;color:#fff">SPI</span> <span class="lvl" style="background:#0c8599;color:#fff">UART</span>
            <span class="lvl" style="background:#e0243b;color:#fff">5V</span> <span class="lvl" style="background:#f08c00;color:#fff">3V3</span>.
            Pinler 3.3 V'tur; 5 V uygulamayın.</p>
        </div></div>`;
    el.querySelectorAll(".pin.usable").forEach(d => d.onclick = () => { selPin = +d.dataset.bcm; draw(); });
    el.querySelectorAll("button[data-m]").forEach(b => b.onclick = async () => {
      await act("/api/gpio/set", { bcm: selPin, mode: b.dataset.m, value: b.dataset.v === "1", bias: b.dataset.b }, "Uygulandı");
      draw();
    });
    $("#scan", el).onclick = async () => {
      try { const r = await api("/api/gpio/i2c"); $("#i2c", el).textContent = r.devices.length ? r.devices.map(a => "0x" + a).join(", ") : "aygıt yok"; }
      catch (e) { $("#i2c", el).textContent = e.message; }
    };
  };
  await draw(); if (el.isConnected) pollView(el, draw, 2000);
}

// ---------------------------------------------------------------- USB
async function renderUsb(el) {
  const draw = async () => {
    const d = await api("/api/devices");
    if (!el.isConnected) return;
    if (current !== "usb") return;
    el.innerHTML = `<h1>USB Aygıtları</h1><p class="lead">Takılı aygıtlar otomatik yenilenir · USB bellekler, seri portlar ve diğer aygıtlar</p>
      <h2>USB bellek ve diskler</h2><div class="list">${d.storage.length ? d.storage.map(disk => `
        ${row(`${esc(disk.model)} <span class="badge">${fmtSize(disk.size)}</span>`, disk.system ? "Sistem bu diskten çalışıyor" : "",
          disk.system ? "" : `<button class="btn" data-ej="${esc(disk.path)}">Güvenle çıkar</button>`)}
        ${disk.parts.map(p => row(`<span class="sub">${esc(p.label || p.path)}</span>`,
          `<span class="sub">${esc(p.fstype || "biçimlendirilmemiş")} · ${fmtSize(p.size)}${p.mountpoint ? " · " + esc(p.mountpoint) : ""}</span>`,
          disk.system || !p.fstype ? "" : p.mountpoint
            ? (local ? `<button class="btn" data-open="${esc(p.mountpoint)}">Dosyalar'da aç</button>` : '<span class="badge ok">bağlı</span>')
            : `<button class="btn primary" data-mt="${esc(p.path)}">Bağla</button>`)).join("")}`).join("")
        : '<div class="empty">USB bellek takılı değil</div>'}</div>
      <p class="hint">Bağlanan bellekler <code>/media/pardus/</code> altında görünür (FAT32, exFAT, NTFS, ext4). Çıkarmadan önce "Güvenle çıkar"a basın.</p>
      <h2>Seri portlar</h2><div class="list">${d.serial.length ? d.serial.map(sp => row(esc(sp.product), esc(sp.vendor || ""),
        `<code>${esc(sp.dev)}</code>`)).join("") : '<div class="empty">Seri aygıt yok (Arduino, ESP32, USB-seri dönüştürücü takınca görünür)</div>'}</div>
      <h2>Tüm USB aygıtları</h2><div class="list">${d.usb.filter(u => !u.hub).length ? d.usb.filter(u => !u.hub).map(u => row(
        `${esc(u.product)}${u.kinds.map(k => ` <span class="badge">${esc(k)}</span>`).join("")}`,
        [u.vendor, u.speed, "ID " + u.id].filter(Boolean).map(esc).join(" · "), `<span class="v">port ${esc(u.port)}</span>`)).join("")
        : '<div class="empty">USB aygıtı takılı değil</div>'}</div>
      <p class="hint">Pi 4: mavi portlar USB 3.0, siyah portlar USB 2.0'dır. Harici diskler fazla akım çekerse güç kaynağı yetmeyebilir; beslemeli USB hub kullanın.</p>`;
    el.querySelectorAll("button[data-mt]").forEach(b => b.onclick = async () => { b.disabled = true; await act("/api/devices/mount", { path: b.dataset.mt }); draw(); });
    el.querySelectorAll("button[data-ej]").forEach(b => b.onclick = async () => { b.disabled = true; b.textContent = "Çıkarılıyor…"; await act("/api/devices/eject", { path: b.dataset.ej }); draw(); });
    el.querySelectorAll("button[data-open]").forEach(b => b.onclick = () => act("/api/devices/open", { path: b.dataset.open }, "Dosyalar açıldı"));
  };
  await draw(); if (el.isConnected) pollView(el, draw, 3000);
}

// ---------------------------------------------------------------- Kamera
let camSel = null;
async function renderKamera(el) {
  const st = await api("/api/camera");
  if (!el.isConnected) return;
  const cams = st.cameras;
  if (!cams.find(c => c.id === camSel)) camSel = (cams[0] || {}).id || null;
  const modelOpts = [["auto", "Otomatik algıla"], ...Object.entries(st.models), ["off", "Kapalı"]];
  el.innerHTML = `<h1>Kamera</h1><p class="lead">Raspberry Pi kamera modülü (CSI) ve USB web kameraları · canlı görüntü ve fotoğraf</p>
    ${cams.length ? `<div class="cam-view" id="cv"><div class="cam-stage"><img id="live" alt="Canlı görüntü"><div class="rec" id="rec" hidden>CANLI</div>
      <div class="cam-msg" id="cmsg">Kameraya bağlanılıyor…</div></div>
      <div class="cam-bar"><div><select id="csel">${cams.map(c => `<option value="${esc(c.id)}" ${c.id === camSel ? "selected" : ""}>${esc(c.name)}</option>`).join("")}</select></div>
        <button class="shutter" id="shot" title="Fotoğraf çek" aria-label="Fotoğraf çek"><span>Çek</span></button>
        <div><button class="btn" id="cfull" title="Tam ekran">⤢</button></div></div></div>`
    : `<div class="list"><div class="empty">Kamera bulunamadı.<br><br>Pi kamera: kabloyu Pi kapalıyken <b>CAMERA</b> yazan yuvaya, mavi/şeritli yüz Ethernet portuna bakacak şekilde takın.
        Üçüncü taraf bir kameraysa aşağıdan modelini seçip yeniden başlatın. USB kamera: takın, birkaç saniye içinde burada görünür.
        ${st.libcamera ? "" : "<br><br>Uyarı: libcamera uygulamaları kurulu değil."}<br><br><button class="btn" id="cre">Yeniden ara</button></div></div>`}
    <div class="list photo-naming"><label class="row"><span class="grow">Fotoğraf adı <small class="d">Tarih ve benzersiz kod otomatik eklenir.</small></span><input type="text" id="photoName" maxlength="60" placeholder="Örn. deney-1" aria-label="Fotoğraf adı"></label></div>
    <h2>Fotoğraflar <span class="v" style="font-weight:400">· ${esc(st.dir)}</span></h2>
    <div id="gal"></div>
    <details class="camera-settings"><summary>Kamera ayarları</summary><div class="list">
      ${row("Canlı görüntü çözünürlüğü", "Yüksek çözünürlük ağda daha fazla bant genişliği ister. Fotoğraflar Pi kamerada tam çözünürlükte çekilir.",
        `<select id="csize">${st.sizes.map(z => `<option ${z === st.settings.size ? "selected" : ""}>${z}</option>`).join("")}</select>`)}
      ${row("180° döndür", "Kamera ters takılıysa", sw("crot", st.settings.rotate))}
      ${row("Pi kamera modeli", "Resmi kameralar otomatik algılanır; Arducam vb. için modeli seçin (yeniden başlatma gerekir)",
        `<select id="cmodel">${modelOpts.map(([k, t]) => `<option value="${k}" ${k === st.model ? "selected" : ""}>${esc(t)}</option>`).join("")}</select>`)}
    </div></details>
    <h2>Python ile</h2><div class="list"><pre class="code">#  /usr/bin/python3 foto.py   (picamera2 sistem Python'unda kurulu)
from picamera2 import Picamera2
cam = Picamera2()
cam.start_and_capture_file("foto.jpg")</pre></div>
    <p class="hint">Terminalden: <code>libcamera-hello --list-cameras</code>, <code>libcamera-still -o foto.jpg</code>, <code>libcamera-vid -t 10000 -o video.h264</code>.
      Kamera aynı anda tek uygulama tarafından kullanılabilir; Python betiğiniz çalışırken bu sayfadaki canlı görüntü kapanır.</p>`;

  const drawGallery = list => {
    $("#gal", el).innerHTML = list.length ? `<div class="gallery">${list.map(p =>
      `<button data-n="${esc(p.name)}" title="${esc(p.name)}"><img loading="lazy" src="/api/camera/photo/${encodeURIComponent(p.name)}?thumb=1&v=${p.time}" alt=""></button>`).join("")}</div>`
      : '<div class="list"><div class="empty">Henüz fotoğraf yok</div></div>';
    $("#gal", el).querySelectorAll("button[data-n]").forEach(b => b.onclick = () => openPhoto(b.dataset.n, () => refreshGallery()));
  };
  const refreshGallery = async () => { const s2 = await api("/api/camera").catch(() => null); if (s2 && current === "kamera") drawGallery(s2.photos); };
  drawGallery(st.photos);
  $("#csize", el).onchange = e => act("/api/camera/settings", { size: e.target.value, rotate: $("#crot", el).checked }, "Kaydedildi").then(() => cams.length && startLive());
  $("#crot", el).onchange = e => act("/api/camera/settings", { size: $("#csize", el).value, rotate: e.target.checked }, "Kaydedildi").then(() => cams.length && startLive());
  $("#cmodel", el).onchange = e => act("/api/camera/model", { model: e.target.value });
  if (!cams.length) {
    $("#cre", el).onclick = () => route();
    pollView(el, async () => { const s2 = await api("/api/camera"); if (el.isConnected && s2.cameras.length) route(); }, 4000);
    return;
  }

  const img = $("#live", el), msg = $("#cmsg", el);
  let retry = null;
  const showMsg = (html) => { msg.hidden = false; msg.innerHTML = html; $("#rec", el).hidden = true; };
  function startLive() {
    if (!el.isConnected || document.hidden) return;
    clearTimeout(retry);
    showMsg("Kameraya bağlanılıyor…");
    img.src = `/api/camera/stream?cam=${encodeURIComponent(camSel)}&_=${Date.now()}`;
  }
  img.onload = () => { msg.hidden = true; $("#rec", el).hidden = false; };
  img.onerror = async () => {
    if (current !== "kamera" || !img.getAttribute("src")) return;
    const s2 = await api("/api/camera").catch(() => ({}));
    if (!el.isConnected) return;
    showMsg(`Görüntü alınamadı${s2.error ? ": " + esc(s2.error) : ""}<br><button class="btn" id="cretry">Yeniden dene</button>`);
    $("#cretry", el).onclick = startLive;
    retry = setTimeout(startLive, 5000);
  };
  $("#csel", el).onchange = e => { camSel = e.target.value; startLive(); };
  $("#cfull", el).onclick = async () => {
    const cv = $("#cv", el);
    if (document.fullscreenElement) return document.exitFullscreen();
    try { await cv.requestFullscreen(); if (touch) await screen.orientation.lock("landscape"); } catch { /* desteklenmiyor */ }
  };
  $("#shot", el).onclick = async () => {
    const b = $("#shot", el); b.disabled = true;
    const cv = $("#cv", el); cv.classList.remove("flash"); void cv.offsetWidth; cv.classList.add("flash");
    toast("Fotoğraf çekiliyor…", 6000);
    try { const r = await api("/api/camera/photo", { cam: camSel, name: $("#photoName", el).value.trim() }); toast(r.message); refreshGallery(); }
    catch (e) { showError(e, "Fotoğraf çekimi"); }
    b.disabled = false;
  };
  startLive();
  const visibility = () => { if (document.hidden) { clearTimeout(retry); img.removeAttribute("src"); } else startLive(); };
  document.addEventListener("visibilitychange", visibility);
  cleanup = () => { clearTimeout(retry); document.removeEventListener("visibilitychange", visibility); img.onerror = null; img.removeAttribute("src"); };
}

function openPhoto(name, onDelete) {
  const lb = $("#lightbox"), url = `/api/camera/photo/${encodeURIComponent(name)}`;
  $("#lbImg").src = url; $("#lbName").textContent = name; $("#lbDl").href = url + "?download=1";
  lb.hidden = false;
  const close = () => { lb.hidden = true; $("#lbImg").src = ""; };
  $("#lbClose").onclick = close;
  $("#lbImg").onclick = close;
  $("#lbDel").onclick = async () => { if (confirm(name + " silinsin mi?") && await act("/api/camera/delete", { name }, "Silindi")) { close(); onDelete && onDelete(); } };
}

// ---------------------------------------------------------------- Terminal
// Oturum sayfalar arasinda gezinirken kapanmaz: terminal kutusu baska bir
// bolume gecince gizli "parking" alanina tasinir.
let term = null, fit = null, termWs = null, termBox = null, ctrlOn = false;
const enc = new TextEncoder();
function loadScript(src) {
  return new Promise((ok, fail) => { const t = document.createElement("script"); t.src = src; t.onload = ok; t.onerror = fail; document.head.appendChild(t); });
}
async function loadXterm() {
  if (window.Terminal) return;
  const l = document.createElement("link"); l.rel = "stylesheet"; l.href = "/xterm/xterm.css"; document.head.appendChild(l);
  await loadScript("/xterm/xterm.js"); await loadScript("/xterm/addon-fit.js");
}
const TERM_KEYS = [["Esc", "\x1b"], ["Tab", "\t"], ["Ctrl", null], ["↑", "\x1b[A"], ["↓", "\x1b[B"], ["←", "\x1b[D"], ["→", "\x1b[C"],
                   ["|", "|"], ["/", "/"], ["~", "~"], ["-", "-"], ["Home", "\x1b[H"], ["End", "\x1b[F"], ["^C", "\x03"]];
function termSend(data) {
  if (termWs && termWs.readyState === 1) termWs.send(enc.encode(data));
}
function termState(t, ok) { const b = $("#tstate"); if (b) { b.textContent = t; b.className = "badge" + (ok ? " ok" : ""); } }
function sendSize() {
  if (termWs && termWs.readyState === 1) termWs.send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }));
}
async function connectTerm() {
  if (termWs && termWs.readyState <= 1) termWs.close();
  termState("bağlanıyor…");
  let ticket;
  try { ({ ticket } = await api("/api/terminal/ticket")); } catch (e) { termState("hata"); term.write("\r\n" + e.message + "\r\n"); return; }
  const ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/terminal/ws?ticket=${encodeURIComponent(ticket)}`);
  ws.binaryType = "arraybuffer";
  termWs = ws;
  ws.onopen = () => { termState("bağlı", true); fit.fit(); sendSize(); term.focus(); };
  ws.onmessage = e => term.write(typeof e.data === "string" ? e.data : new Uint8Array(e.data));
  ws.onclose = () => {
    if (termWs !== ws) return;
    termState("kapandı");
    term.write("\r\n\x1b[90m[Oturum kapandı · yeniden bağlanmak için Enter'a basın]\x1b[0m\r\n");
  };
}
async function renderTerminal(el) {
  el.innerHTML = `<div class="term-head"><h1>Terminal</h1><span class="badge" id="tstate">—</span><span class="sp"></span>
    <button class="btn" id="tnew" title="Kabuğu kapatıp yenisini açar">Yeni oturum</button></div>`;
  try { await loadXterm(); }
  catch { el.innerHTML += '<p class="lead">Terminal bileşeni (xterm.js) bulunamadı. Pardus Maker imajını yeniden oluşturun.</p>'; return; }
  if (!termBox) {
    termBox = document.createElement("div"); termBox.id = "termBox";
    termBox.innerHTML = `<div id="termEl"></div><div class="keys">${TERM_KEYS.map(([t], i) => `<button data-k="${i}">${t}</button>`).join("")}</div>`;
    term = new Terminal({ cursorBlink: true, fontSize: touch ? 13 : 14, scrollback: 5000, allowProposedApi: true,
      fontFamily: '"DejaVu Sans Mono", "Noto Sans Mono", ui-monospace, Menlo, monospace',
      theme: { background: "#1e1e20", foreground: "#e6e6ea", cursor: "#ff455a", selectionBackground: "#5a5a64" } });
    fit = new FitAddon.FitAddon(); term.loadAddon(fit);
    term.open(termBox.querySelector("#termEl"));
    term.onData(d => {
      if (!termWs || termWs.readyState !== 1) { if (d === "\r") connectTerm(); return; }
      if (ctrlOn && d.length === 1) {
        const c = d.toUpperCase().charCodeAt(0);
        if (c >= 64 && c <= 95) d = String.fromCharCode(c - 64);
        ctrlOn = false; termBox.querySelector('[data-k="2"]').classList.remove("on");
      }
      termSend(d);
    });
    term.onBinary(d => termWs && termWs.readyState === 1 && termWs.send(Uint8Array.from(d, c => c.charCodeAt(0))));
    term.onResize(sendSize);
    termBox.querySelector(".keys").addEventListener("click", e => {
      const b = e.target.closest("button"); if (!b) return;
      const [, seq] = TERM_KEYS[b.dataset.k];
      if (seq === null) { ctrlOn = !ctrlOn; b.classList.toggle("on", ctrlOn); } else termSend(seq);
      term.focus();
    });
    const refit = () => { if (termBox.isConnected && termBox.offsetParent) try { fit.fit(); } catch { /* gizli */ } };
    new ResizeObserver(refit).observe(termBox);
    window.visualViewport && visualViewport.addEventListener("resize", refit);
  }
  el.appendChild(termBox);
  fit.fit();
  if (!termWs || termWs.readyState > 1) connectTerm(); else { termState("bağlı", true); term.focus(); }
  $("#tnew", el).onclick = () => { term.reset(); connectTerm(); };
  cleanup = () => $("#parking").appendChild(termBox);
}

// ---------------------------------------------------------------- Uzak ekran
// Telefonda masaustu yatay gosterilir: tam ekran + yatay kilit denenir
// (Android Chrome); kilitlenemezse (iOS) "yatay cevirin" uyarisi cikar.
let rfb = null, KeyTable = null, keysyms = null;
async function landscape() {
  try {
    if (!document.fullscreenElement) await $("#screen").requestFullscreen({ navigationUI: "hide" });
    await screen.orientation.lock("landscape");
  } catch { /* tarayici desteklemiyor: uyari kalir */ }
}
async function openScreen() {
  $("#app").hidden = true; $("#screen").hidden = false;
  if (touch) landscape();
  if (rfb) return;
  const { password, port } = await api("/api/vnc");
  const [{ default: RFB }, ks, kd] = await Promise.all([import("/novnc/core/rfb.js"),
    import("/novnc/core/input/keysym.js"), import("/novnc/core/input/keysymdef.js")]);
  KeyTable = ks.default; keysyms = kd.default;
  const proto = location.protocol === "https:" ? "wss" : "ws";
  const connect = () => {
    $("#screenState").textContent = "bağlanıyor…";
    $("#vncTarget").innerHTML = "";
    rfb = new RFB($("#vncTarget"), `${proto}://${location.hostname}:${port}/websockify`, { credentials: { password } });
    // Dusuk gecikme: olcekle, surukleyerek kaydirma kapali, tiklamada odak al.
    rfb.scaleViewport = true;
    rfb.clipViewport = false;
    rfb.dragViewport = false;
    rfb.resizeSession = false;
    rfb.focusOnClick = true;
    rfb.addEventListener("connect", () => {
      $("#screenState").textContent = "bağlı";
      try { rfb.focus(); } catch { /* yok */ }
    });
    rfb.addEventListener("disconnect", e => {
      $("#screenState").textContent = e.detail.clean ? "bağlantı kapandı" : "bağlantı koptu, yeniden deneniyor…";
      rfb = null;
      if (!e.detail.clean && location.hash === "#ekran") setTimeout(connect, 2000);
    });
    rfb.addEventListener("credentialsrequired", () => rfb.sendCredentials({ password: prompt("Uzak ekran parolası:") || "" }));
  };
  connect();
}
function closeScreen() {
  if ($("#screen").hidden) return;
  $("#screen").hidden = true; $("#app").hidden = false;
  $("#screen").classList.remove("portrait-ok");
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  try { screen.orientation.unlock(); } catch { /* yok */ }
  if (rfb) { const r = rfb; rfb = null; r.disconnect(); }
}
$("#screen").addEventListener("click", e => {
  const a = e.target.dataset && e.target.dataset.act;
  if (!a) return;
  if (a === "back") location.hash = "#genel";
  if (a === "full") document.fullscreenElement ? document.exitFullscreen() : (touch ? landscape() : $("#screen").requestFullscreen());
  if (a === "keys" && rfb) rfb.sendCtrlAltDel();
  if (a === "kbd") { const k = $("#kbd"); k.value = "_"; k.focus(); }
  if (a === "landscape") landscape();
  if (a === "portrait") $("#screen").classList.add("portrait-ok");
});
// Ekran klavyesi: gizli textarea'ya yazilanlar VNC tus olaylarina cevrilir.
// Android klavyeleri cogu tusta keyCode 229 gonderdigi icin "input" olayi
// kullanilir; silme tusu icin alanda hep bir "_" bekcisi tutulur.
const KBD_KEYS = { Backspace: "XK_BackSpace", Enter: "XK_Return", Tab: "XK_Tab", Escape: "XK_Escape", ArrowUp: "XK_Up",
                   ArrowDown: "XK_Down", ArrowLeft: "XK_Left", ArrowRight: "XK_Right", Delete: "XK_Delete", Home: "XK_Home", End: "XK_End" };
$("#kbd").addEventListener("keydown", e => {
  if (!rfb || !KBD_KEYS[e.key]) return;
  e.preventDefault(); rfb.sendKey(KeyTable[KBD_KEYS[e.key]], e.code || e.key);
});
$("#kbd").addEventListener("input", e => {
  const k = e.target, v = k.value;
  if (rfb) {
    if (!v.startsWith("_")) rfb.sendKey(KeyTable.XK_BackSpace, "Backspace");
    for (const ch of v.slice(v.startsWith("_") ? 1 : 0)) {
      if (ch === "\n") rfb.sendKey(KeyTable.XK_Return, "Enter");
      else rfb.sendKey(keysyms.lookup(ch.codePointAt(0)));
    }
  }
  k.value = "_";
});

// ---------------------------------------------------------------- baslangic
window.addEventListener("hashchange", route);
$("#menuBtn").onclick = () => $("#app").classList.toggle("menu");
$("#scrim").onclick = () => $("#app").classList.remove("menu");
$("#loginShowPw").onchange = e => {
  $("#loginForm").password.type = e.target.checked ? "text" : "password";
};
// Masaüstü ?t= jetonunu URL'den temizle (cerez zaten yazildi).
if (new URLSearchParams(location.search).has("t")) {
  history.replaceState(null, "", location.pathname + location.hash);
}

async function boot() {
  let s = null;
  for (;;) {
    const r = await fetch("/api/status", { cache: "no-store", credentials: "same-origin" });
    if (r.status === 401) { await requireLogin(); continue; }
    if (!r.ok) throw new Error("Durum alınamadı (HTTP " + r.status + ")");
    s = await r.json();
    break;
  }
  $("#login").hidden = true;
  $("#app").hidden = false;
  local = !!s.client_local;
  if (s.hostname) { $("#devName").textContent = s.hostname; $("#devModel").textContent = s.model; }
  SECTIONS = sections(); buildNav();
  api("/api/settings").then(st => document.documentElement.dataset.theme = st.theme).catch(() => {});
  // Agdan gelen ilk ziyarette dogrudan masaustu; Pi'nin kendisinde Ayarlar.
  if (!local && !location.hash) location.hash = "#ekran"; else route();
}
boot().catch(e => {
  $("#login").hidden = false;
  $("#app").hidden = true;
  $("#loginError").textContent = e.message || "Bağlantı kurulamadı.";
  $("#loginError").hidden = false;
  requireLogin().then(() => boot());
});
