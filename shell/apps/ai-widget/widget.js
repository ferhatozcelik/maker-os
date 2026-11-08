// AI Asistan widget'ı: masaüstünde MakerOS eylemlerini (uygulama açma, Claude Code / ajan başlatma,
// ayarlar, kamera vb.) ve Linux komutlarını yürütür.
// İzin modu (tercih aiMode):
// - bypass = Full Bypass (varsayılan): komutları ve eylemleri onay sormadan doğrudan çalıştırır.
// - sor    = İzin iste: çalıştırmadan önce kullanıcıdan onay alır.
import { icon } from "/shell/js/icons.js";
import { apply, prefs, pref, setPref } from "/shell/js/prefs.js";

apply();
prefs.on("change", () => apply());
addEventListener("storage", () => apply());

const $ = id => document.getElementById(id);
const AI = "/api/v1/ai";
const out = $("out"), input = $("in"), go = $("go"), st = $("st"), prog = $("prog"), stopBtn = $("stop");
document.querySelector(".logo").innerHTML = icon("sparkles", 14, 2.2);
go.innerHTML = icon("arrow-up", 16, 2.4);
stopBtn.innerHTML = icon("square", 10, 2.6) + "Durdur";
$("new").innerHTML = icon("rotate-ccw", 13, 2);
$("close").innerHTML = icon("x", 13, 2.2);

const CHIPS = [
  ["Claude Code'u aç", "Claude Code'u aç"],
  ["Terminali aç", "Terminali aç"],
  ["CPU sıcaklığı", "CPU sıcaklığını göster."],
  ["RAM", "RAM kullanımını kontrol et."],
  ["Disk", "Disk alanını kontrol et."],
  ["IP adresi", "IP adresimi öğren."],
];
$("chips").append(...CHIPS.map(([l, q]) => {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = l;
  b.onclick = () => ask(q);
  return b;
}));

let history = [], busy = false, ctrl = null, activeTasks = 0;

// Kabuk widget alanının yüksekliğini içeriğe göre ayarlar.
const size = () => parent.postMessage({ mk: "widget-size", h: Math.ceil(document.querySelector(".card").getBoundingClientRect().height) + 4 }, location.origin);
new ResizeObserver(size).observe(document.querySelector(".card"));
const tell = m => parent.postMessage(m, location.origin);

function status(cls, text) { st.className = "st " + cls; st.lastElementChild.textContent = text; }

function updateWorkingUI() {
  const working = busy || activeTasks > 0;
  prog.hidden = !working;
  if (working) {
    go.classList.add("stop");
    go.innerHTML = icon("square", 12, 2.6);
    go.title = "Durdur";
  } else {
    go.classList.remove("stop");
    go.innerHTML = icon("arrow-up", 16, 2.4);
    go.title = "Gönder";
  }
}

function beginTask(label = "Çalışıyor…") {
  activeTasks++;
  status("load", label);
  updateWorkingUI();
}

function endTask(doneLabel = "Hazır") {
  activeTasks = Math.max(0, activeTasks - 1);
  if (!busy && activeTasks === 0) status("ok", doneLabel);
  updateWorkingUI();
}

function stopAll() {
  if (ctrl) {
    ctrl.abort();
    ctrl = null;
  }
  busy = false;
  activeTasks = 0;
  document.querySelectorAll(".cur").forEach(el => el.classList.remove("cur"));
  status("ok", "Durduruldu");
  updateWorkingUI();
}

stopBtn.onclick = stopAll;

async function poll() {
  try {
    const r = await fetch(AI + "/v1/makeros/durum", { credentials: "same-origin" });
    if (!r.ok) throw new Error();
    const d = await r.json();
    if (!busy && activeTasks === 0) status("ok", d.llama?.loaded ? "Hazır" : "Aktif");
    $("meta").textContent = d.llama?.loaded ? `MakerOS AI · RAM ${Math.round(d.llama.rss_mb)} MB` : "MakerOS AI";
  } catch { if (!busy && activeTasks === 0) status("ok", "Aktif"); $("meta").textContent = "MakerOS AI"; }
}
poll();
setInterval(() => { if (!busy && activeTasks === 0 && document.visibilityState === "visible") poll(); }, 20000);

// Çok küçük, güvenli markdown: önce HTML kaçışı, sonra kod blokları, satır içi kod ve kalın.
const esc = s => s.replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
function md(s) {
  const parts = esc(s).split(/```[a-z]*\n?/i);
  return parts.map((p, i) => i % 2 ? `<pre><code>${p.replace(/\n$/, "")}</code></pre>`
    : p.replace(/`([^`\n]+)`/g, "<code>$1</code>").replace(/\*\*([^*\n]+)\*\*/g, "<b>$1</b>").split(/\n{2,}/).map(x => `<p>${x.replace(/\n/g, "<br>")}</p>`).join("")).join("");
}

const modeSel = $("mode");
const mode = () => pref("aiMode") === "sor" ? "sor" : "bypass";
modeSel.value = mode();
modeSel.onchange = () => setPref("aiMode", modeSel.value);
prefs.on("change", k => { if (k === "aiMode") modeSel.value = mode(); });
addEventListener("storage", () => { modeSel.value = mode(); });

// Türkçe duyarlı küçük harf normalleştirmesi
const fold = s => String(s || "").toLocaleLowerCase("tr").normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/ı/g, "i");

// Terminalde kodlama ajanı başlatır (kurulu değilse kurulumunu başlatır).
async function launchAgent(id, label, signal) {
  const list = await fetch("/api/v1/agents", { credentials: "same-origin", signal }).then(r => r.json()).catch(() => []);
  const ag = Array.isArray(list) ? list.find(x => x.id === id) : null;
  const act = ag && !ag.installed ? "install" : "run";
  const r = await fetch(`/api/v1/agents/${id}/${act}`, {
    method: "POST",
    credentials: "same-origin",
    signal,
    headers: { "Content-Type": "application/json", "X-Maker": "1" },
  });
  const d = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(d.error?.message || `${label} başlatılamadı`);
  if (d.term?.id) {
    tell({ mk: "open", app: "makeros.terminal", path: `${d.term.id}:${d.term.ticket}` });
    return act === "install"
      ? `**${label}** kurulu değildi; kurulumu Terminal'de başlatıldı.`
      : `**${label}** Terminal'de açıldı.`;
  }
  tell({ mk: "open", app: "makeros.agents" });
  return `**${label}** için AI Ajanları uygulaması açıldı.`;
}

// Doğal dildeki MakerOS uygulama/sistem isteklerini yakalar.
function matchAction(raw) {
  const q = fold(raw).trim();
  const rawTrim = raw.trim();

  // "terminalde <komut> çalıştır" veya "<komut> komutunu çalıştır"
  const termMatch = rawTrim.match(/^(?:terminalde|terminal'de)\s+`?([^`]+?)`?\s+(?:calistir|çalıştır|ac|aç)$/i)
    || rawTrim.match(/^`([^`]+)`\s*(?:komutunu\s+)?(?:calistir|çalıştır|yap)$/i);
  if (termMatch) {
    const cmd = termMatch[1].trim();
    return {
      label: cmd,
      desc: `Terminal'de \`${cmd}\` komutu çalıştırılıyor.`,
      run: async signal => {
        const r = await fetch("/api/v1/asistan/calistir", {
          method: "POST",
          credentials: "same-origin",
          signal,
          headers: { "Content-Type": "application/json", "X-Maker": "1" },
          body: JSON.stringify({ command: cmd, onay: true, terminal: true }),
        });
        const d = await r.json().catch(() => ({}));
        if (!r.ok) throw new Error(d.error?.message || "Komut başlatılamadı");
        if (d.term?.id) tell({ mk: "open", app: "makeros.terminal", path: `${d.term.id}:${d.term.ticket}` });
        return `\`${cmd}\` Terminal'de başlatıldı.`;
      },
    };
  }

  // Kodlama ajanları: Claude Code, Codex, Gemini, OpenCode, Ollama
  const agents = [
    { id: "claude", name: "Claude Code", keys: ["claude code", "claude kodu", "claudecode", "claude'u", "claudeu", "claude"] },
    { id: "codex", name: "Codex CLI", keys: ["codex"] },
    { id: "gemini", name: "Gemini CLI", keys: ["gemini"] },
    { id: "opencode", name: "OpenCode", keys: ["opencode", "open code"] },
    { id: "ollama", name: "Ollama", keys: ["ollama"] },
  ];
  const actionVerb = /\b(ac|acabilir|acar|baslat|calistir|kur|yukle|open|start|run|launch|yap)\b/.test(q);
  for (const a of agents) {
    if (a.keys.some(k => q === k || (q.includes(k) && (actionVerb || q.endsWith(k))))) {
      return {
        label: `${a.name} başlat`,
        desc: `**${a.name}** Terminal'de başlatılıyor…`,
        run: signal => launchAgent(a.id, a.name, signal),
      };
    }
  }

  // Kamera ile fotoğraf çekme
  if ((q.includes("fotograf") || q.includes("foto") || q.includes("resim")) && (q.includes("cek") || q.includes("al"))) {
    return {
      label: "Fotoğraf çek",
      desc: "Kamera ile fotoğraf çekiliyor…",
      run: async signal => {
        const r = await fetch("/api/v1/camera/photo", { method: "POST", credentials: "same-origin", signal, headers: { "X-Maker": "1" } });
        const d = await r.json().catch(() => ({}));
        if (!r.ok) throw new Error(d.error?.message || "Fotoğraf çekilemedi");
        return `Fotoğraf kaydedildi: \`${d.path || "Resimler"}\``;
      },
    };
  }

  // Tema değiştirme (koyu / açık)
  if ((q.includes("koyu") || q.includes("karanlik") || q.includes("dark")) && (q.includes("tema") || q.includes("mod") || q.includes("gorunum") || q.includes("gec") || q.includes("yap") || q.includes("ac"))) {
    return {
      label: "Koyu görünüm",
      desc: "Koyu görünüme geçiliyor…",
      run: async () => { setPref("theme", "dark"); return "Görünüm **Koyu** olarak ayarlandı."; },
    };
  }
  if ((q.includes("acik") || q.includes("aydinlik") || q.includes("light")) && (q.includes("tema") || q.includes("mod") || q.includes("gorunum") || q.includes("gec") || q.includes("yap"))) {
    return {
      label: "Açık görünüm",
      desc: "Açık görünüme geçiliyor…",
      run: async () => { setPref("theme", "light"); return "Görünüm **Açık** olarak ayarlandı."; },
    };
  }

  // Ekran kilitleme / masaüstü kısayolları
  if (q.includes("ekrani kilitle") || q === "kilitle" || q.includes("oturumu kilitle")) {
    return {
      label: "Ekranı kilitle",
      desc: "Ekran kilitleniyor…",
      run: async () => { tell({ mk: "shortcut", id: "lock" }); return "Ekran kilitlendi."; },
    };
  }
  if ((q.includes("bildirim") && (q.includes("ac") || q.includes("goster"))) && !q.includes("neler") && !q.includes("son")) {
    return {
      label: "Bildirim Merkezi",
      desc: "Bildirim Merkezi açılıyor…",
      run: async () => { tell({ mk: "shortcut", id: "nc" }); return "**Bildirim Merkezi** açıldı."; },
    };
  }

  // Web adresi / site açma
  const urlMatch = rawTrim.match(/(https?:\/\/[^\s]+|\b(?:github\.com|google\.com|youtube\.com|raspberrypi\.com|claude\.ai)[^\s]*)/i);
  if (urlMatch && actionVerb) {
    const rawUrl = urlMatch[1];
    const url = /^https?:\/\//i.test(rawUrl) ? rawUrl : "https://" + rawUrl;
    return {
      label: `Tarayıcı: ${url}`,
      desc: `**${url}** Tarayıcı'da açılıyor…`,
      run: async () => { tell({ mk: "open", app: "chromium", path: url }); return `**${url}** açıldı.`; },
    };
  }

  // Uygulama açma istekleri
  const apps = [
    { app: "makeros.terminal", name: "Terminal", keys: ["terminal", "terminali", "komut satir", "kabuk", "konsol"] },
    { app: "makeros.files", name: "Dosyalar", path: "/home/makeros/İndirilenler", keys: ["indirilenler", "indirilenleri", "downloads"] },
    { app: "makeros.files", name: "Dosyalar", path: "/home/makeros/Belgeler", keys: ["belgeler", "belgeleri", "documents"] },
    { app: "makeros.files", name: "Dosyalar", path: "/home/makeros/Resimler", keys: ["resimler", "resimleri", "galeri"] },
    { app: "makeros.files", name: "Dosyalar", path: "/home/makeros/Projeler", keys: ["proje klasor", "projeler klasor"] },
    { app: "makeros.files", name: "Dosyalar", keys: ["dosyalar", "dosyalari", "dosya yoneticisi", "klasorler", "klasorleri", "files"] },
    { app: "makeros.python", name: "Python Yöneticisi", keys: ["python", "python yoneticisi", "python'u", "pythonu", "projeler", "projeleri"] },
    { app: "makeros.agents", name: "AI Ajanları", keys: ["ai ajan", "ajanlar", "ajanlari", "kodlama ajan"] },
    { app: "makeros.camera", name: "Kamera", keys: ["kamera", "kamerayi", "camera"] },
    { app: "makeros.gpio", name: "GPIO", keys: ["gpio", "pinler", "pinleri"] },
    { app: "makeros.plugins", name: "Eklentiler", keys: ["eklentiler", "eklentileri", "magaza", "magazayi", "plugins"] },
    { app: "makeros.activity", name: "Etkinlik Monitörü", keys: ["etkinlik monitor", "gorev yoneticisi", "calisan uygulamalar"] },
    { app: "chromium", name: "Tarayıcı", keys: ["tarayici", "tarayiciyi", "chromium", "chrome", "browser", "interneti"] },
    { app: "makeros.settings", name: "Ayarlar › Wi-Fi", path: "wifi", keys: ["wifi ayar", "wi-fi ayar", "kablosuz ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Ağ", path: "network", keys: ["ag ayar", "network ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Görünüm", path: "appearance", keys: ["gorunum ayar", "tema ayar", "duvar kagidi ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Otomatik Başlatma", path: "startup", keys: ["otomatik baslat", "baslangic ayar", "cron", "zamanlayici"] },
    { app: "makeros.settings", name: "Ayarlar › Genel", path: "general", keys: ["yazilim guncelle", "guncelleme ayar", "sistem bilgi"] },
    { app: "makeros.settings", name: "Ayarlar › Kullanıcı", path: "user", keys: ["kullanici ayar", "parola degistir", "sifre degistir"] },
    { app: "makeros.settings", name: "Ayarlar › Güç", path: "power", keys: ["guc ayar"] },
    { app: "makeros.settings", name: "Ayarlar", keys: ["ayarlar", "ayarlari", "sistem ayarlari", "settings"] },
  ];

  for (const item of apps) {
    if (item.keys.some(k => q === k || (q.includes(k) && (actionVerb || q.startsWith(k) || q.endsWith(k))))) {
      return {
        label: `${item.name} aç`,
        desc: `**${item.name}** açılıyor…`,
        run: async () => {
          tell({ mk: "open", app: item.app, ...(item.path ? { path: item.path } : {}) });
          return `**${item.name}** açıldı.`;
        },
      };
    }
  }

  return null;
}

async function runCmd(c, row, onay = false, yazilan) {
  row.querySelector(".conf")?.remove();
  row.querySelector(".res")?.remove();
  const res = document.createElement("div"); res.className = "res";
  res.innerHTML = `<div class="rh">çalışıyor…</div>`;
  row.append(res);
  const cmdAbort = new AbortController();
  ctrl = cmdAbort;
  beginTask("Komut çalışıyor…");
  try {
    const isBypass = mode() === "bypass";
    const r = await fetch("/api/v1/asistan/calistir", {
      method: "POST",
      credentials: "same-origin",
      signal: cmdAbort.signal,
      headers: { "Content-Type": "application/json", "X-Maker": "1" },
      body: JSON.stringify({ command: c.command, onay: isBypass || onay, yazilan }),
    });
    const d = await r.json().catch(() => ({}));
    if (!r.ok) {
      res.innerHTML = "";
      const e = document.createElement("div"); e.className = "rh"; e.textContent = d.error?.message || `hata ${r.status}`;
      res.append(e);
      return;
    }
    if (d.durum === "onay_gerekli") { res.remove(); confirmBox(c, row, d); return; }
    if (d.durum === "terminal" && d.term?.id) {
      tell({ mk: "open", app: "makeros.terminal", path: `${d.term.id}:${d.term.ticket}` });
      res.innerHTML = `<div class="rh"><b class="ok">Terminal'de açıldı</b></div>`;
      return;
    }
    const head = document.createElement("div"); head.className = "rh";
    const b = document.createElement("b"); b.className = d.cikis === 0 ? "ok" : "bad"; b.textContent = d.cikis == null ? "çıkış ?" : `çıkış ${d.cikis}`;
    const ms = document.createElement("span"); ms.textContent = `${(d.ms / 1000).toFixed(1)} sn${d.not ? " · " + d.not : ""}`;
    const ex = document.createElement("button"); ex.textContent = "Açıkla"; ex.title = "Çıktıyı asistana açıklat";
    ex.onclick = () => ask(`Şu komutu çalıştırdım: \`${c.command}\`\nÇıktı:\n\`\`\`\n${(d.cikti || "(boş)").slice(0, 1200)}\n\`\`\`\nÇıktıyı kısaca Türkçe açıkla; sorun varsa belirt.`);
    head.append(b, ms, ex);
    const pre = document.createElement("pre"); pre.textContent = d.cikti || "(çıktı yok)";
    res.replaceChildren(head, pre);
    out.scrollTop = out.scrollHeight;
  } catch (e) {
    res.textContent = e.name === "AbortError" ? "durduruldu" : (e.message || "çalıştırılamadı");
  } finally {
    if (ctrl === cmdAbort) ctrl = null;
    endTask();
  }
}

// Onay kutusu (İzin iste modunda)
function confirmBox(c, row, d) {
  row.querySelector(".conf")?.remove();
  const box = document.createElement("div"); box.className = "conf";
  const msg = document.createElement("div");
  const why = (d?.reasons || c.reasons || []).join(" · ");
  const btns = document.createElement("div"); btns.className = "row";
  const ok = document.createElement("button"); ok.className = "ok"; ok.textContent = "İzin ver ve çalıştır";
  const no = document.createElement("button"); no.textContent = "Vazgeç"; no.onclick = () => box.remove();
  msg.textContent = why ? `İzin gerekiyor (${why}). Çalıştırılsın mı?` : "Bu komut çalıştırılsın mı?";
  ok.onclick = () => runCmd(c, row, true, c.command);
  btns.append(ok, no);
  box.append(msg, btns);
  row.append(box);
  out.scrollTop = out.scrollHeight;
}

function commands(list) {
  if (!list?.length) return null;
  const box = document.createElement("div"); box.className = "cmds";
  for (const c of list) {
    const row = document.createElement("div"); row.className = "cmd";
    const rk = document.createElement("span"); rk.className = "rk " + c.risk; rk.textContent = c.risk;
    rk.title = { SAFE: "Güvenli: yalnız okur", WARNING: "Dikkat: sistemi değiştirebilir", DANGEROUS: "Tehlikeli: geri alınamayabilir" }[c.risk] || "";
    const code = document.createElement("code"); code.textContent = c.command; code.title = c.command;
    const cp = document.createElement("button"); cp.className = "ib"; cp.title = "Kopyala"; cp.innerHTML = icon("copy", 13, 2);
    cp.onclick = async () => { try { await navigator.clipboard.writeText(c.command); cp.innerHTML = icon("check", 13, 2.4); setTimeout(() => { cp.innerHTML = icon("copy", 13, 2); }, 1200); } catch { /* izin yok */ } };
    const acts = document.createElement("span"); acts.className = "acts";
    const run = document.createElement("button"); run.className = "ib run"; run.innerHTML = icon("play", 11, 2.4) + "Çalıştır";
    run.title = "Komutu çalıştır";
    run.onclick = () => mode() === "bypass" ? runCmd(c, row, true, c.command) : confirmBox(c, row);
    acts.append(run, cp);
    row.append(rk, code, acts);
    if (c.reasons?.length && c.risk !== "SAFE") { const w = document.createElement("div"); w.className = "why"; w.textContent = c.reasons.join(" · "); row.append(w); }
    box.append(row);
    // Full Bypass modunda tüm komutlar doğrudan çalışır; İzin iste modunda onay kutusu açılır.
    if (mode() === "bypass") setTimeout(() => runCmd(c, row, true, c.command), 0);
  }
  return box;
}

async function runActionCard(act, aEl) {
  const box = document.createElement("div"); box.className = "cmds";
  const row = document.createElement("div"); row.className = "cmd";
  const rk = document.createElement("span"); rk.className = "rk ACTION"; rk.textContent = "EYLEM";
  const code = document.createElement("code"); code.textContent = act.label;
  const acts = document.createElement("span"); acts.className = "acts";
  const exec = async () => {
    row.querySelector(".conf")?.remove();
    aEl.innerHTML = md(act.desc);
    const actAbort = new AbortController();
    ctrl = actAbort;
    beginTask("Çalıştırılıyor…");
    try {
      const msg = await act.run(actAbort.signal);
      aEl.innerHTML = md(msg);
      endTask("Hazır");
    } catch (e) {
      aEl.innerHTML = md(e.name === "AbortError" ? "İşlem durduruldu." : `Hata: ${e.message || "işlem yapılamadı"}`);
      endTask(e.name === "AbortError" ? "Durduruldu" : "Hata");
    } finally {
      if (ctrl === actAbort) ctrl = null;
    }
  };
  const run = document.createElement("button"); run.className = "ib run"; run.innerHTML = icon("play", 11, 2.4) + "Çalıştır";
  run.onclick = exec;
  acts.append(run);
  row.append(rk, code, acts);
  box.append(row);
  aEl.after(box);
  if (mode() === "bypass") {
    await exec();
  } else {
    aEl.innerHTML = md(`${act.desc}\nİzin iste modundasınız; başlatmak için **Çalıştır**'a tıklayın.`);
    status("ok", "Onay bekliyor");
  }
}

async function ask(q) {
  q = (q || "").trim();
  if (!q || busy) return;
  busy = true; input.value = "";
  $("chips").hidden = true; out.hidden = false;
  const qEl = document.createElement("div"); qEl.className = "q"; qEl.textContent = q;
  const aEl = document.createElement("div"); aEl.className = "a cur";
  out.append(qEl, aEl); out.scrollTop = out.scrollHeight;
  updateWorkingUI();

  // Önce doğrudan MakerOS uygulama / ajan / sistem eylemi mi kontrol et:
  const act = matchAction(q);
  if (act) {
    aEl.classList.remove("cur");
    await runActionCard(act, aEl);
    history.push({ role: "user", content: q }, { role: "assistant", content: aEl.textContent || act.label });
    busy = false; updateWorkingUI(); out.scrollTop = out.scrollHeight;
    return;
  }

  history.push({ role: "user", content: q });
  let text = "", extra = null, timings = null;
  status("load", "Düşünüyor…");
  ctrl = new AbortController();
  try {
    const r = await fetch(AI + "/v1/chat/completions", { method: "POST", credentials: "same-origin", signal: ctrl.signal,
      headers: { "Content-Type": "application/json", "X-Maker": "1" }, body: JSON.stringify({ messages: history.slice(-6), stream: true, max_tokens: 400 }) });
    if (!r.ok) { const e = await r.json().catch(() => ({})); throw new Error(e.error?.message || `hata ${r.status}`); }
    const rd = r.body.getReader(), dec = new TextDecoder();
    let buf = "";
    for (;;) {
      const { value, done } = await rd.read();
      if (done) break;
      buf += dec.decode(value, { stream: true });
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, i).trim(); buf = buf.slice(i + 1);
        if (!line.startsWith("data: ") || line === "data: [DONE]") continue;
        let d; try { d = JSON.parse(line.slice(6)); } catch { continue; }
        if (d.x_makeros) extra = d.x_makeros;
        if (d.timings) timings = d.timings;
        const c = d.choices?.[0]?.delta?.content;
        if (c) { if (!text) status("load", "Yazıyor…"); text += c; aEl.innerHTML = md(text); out.scrollTop = out.scrollHeight; }
      }
    }
    aEl.classList.remove("cur");
    const box = commands(extra?.commands); if (box) aEl.after(box);
    history.push({ role: "assistant", content: text });
    if (timings?.predicted_per_second) $("meta").textContent = `MakerOS AI · ${timings.predicted_per_second.toFixed(1)} token/sn`;
    if (activeTasks === 0) status("ok", "Hazır");
  } catch (e) {
    aEl.classList.remove("cur");
    if (e.name === "AbortError") {
      if (!text) aEl.innerHTML = `<span style="color:var(--text-2)">Durduruldu.</span>`;
      status("ok", "Durduruldu");
    } else {
      history.pop();
      const er = document.createElement("div"); er.className = "err"; er.textContent = e.message || "Asistana ulaşılamadı."; aEl.after(er);
      poll();
    }
  } finally {
    busy = false; ctrl = null; updateWorkingUI(); out.scrollTop = out.scrollHeight;
  }
}

$("f").onsubmit = e => {
  e.preventDefault();
  if (busy || activeTasks > 0) stopAll();
  else ask(input.value);
};
$("new").onclick = () => { stopAll(); history = []; out.replaceChildren(); out.hidden = true; $("chips").hidden = false; input.focus(); };
$("close").onclick = () => setPref("aiWidget", false);
