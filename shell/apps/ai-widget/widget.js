// AI Asistan widget'ı: masaüstünde MakerOS ve genel konularda Türkçe sohbet eder,
// MakerOS eylemlerini (uygulama açma, Claude Code / ajan başlatma, ayarlar, kamera vb.)
// ve Linux komutlarını yürütür.
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
// Tam sayfa (Web arayüzündeki "AI Asistan" uygulaması): ?full=1
const FULL = new URLSearchParams(location.search).has("full");
document.documentElement.classList.toggle("full", FULL);
const STORAGE_KEY = "mk.ai.history";
const out = $("out"), input = $("in"), go = $("go"), st = $("st"), prog = $("prog"), stopBtn = $("stop"), clearBtn = $("new");
document.querySelector(".logo").innerHTML = icon("sparkles", 14, 2.2);
go.innerHTML = icon("arrow-up", 16, 2.4);
stopBtn.innerHTML = icon("square", 10, 2.6) + "Durdur";
clearBtn.innerHTML = icon("trash-2", 12, 2) + "Temizle";
$("close").innerHTML = icon("x", 13, 2.2);

const CHIPS = [
  ["MakerOS nedir?", "MakerOS nedir ve neler yapabilirim?"],
  ["Claude Code'u aç", "Claude Code'u aç"],
  ["Terminali aç", "Terminali aç"],
  ["CPU sıcaklığı", "CPU sıcaklığını göster."],
  ["RAM", "RAM kullanımını kontrol et."],
  ["Disk", "Disk alanını kontrol et."],
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
const size = () => FULL ? null : parent.postMessage({ mk: "widget-size", h: Math.ceil(document.querySelector(".card").getBoundingClientRect().height) }, location.origin);
new ResizeObserver(size).observe(document.querySelector(".card"));
const tell = m => parent.postMessage(m, location.origin);
// Masaüstünde 1 sn basılı tutma → kabukta widget düzenleme modu (sürükle-bırak).
if (!FULL) {
  let timer = 0, x0 = 0, y0 = 0;
  const stop = () => { clearTimeout(timer); timer = 0; };
  addEventListener("pointerdown", e => {
    if (e.button !== 0 || e.target.closest("input, textarea, select, [contenteditable]")) return;
    x0 = e.clientX; y0 = e.clientY; stop();
    timer = setTimeout(() => { timer = 0; tell({ mk: "widget-edit" }); }, 1000);
  });
  addEventListener("pointermove", e => { if (timer && Math.hypot(e.clientX - x0, e.clientY - y0) > 10) stop(); });
  for (const ev of ["pointerup", "pointercancel", "pointerleave"]) addEventListener(ev, stop);
}
// Masaüstünde sağ tık → kabuğun widget menüsü (düzenle / taşı / kaldır); metin kutusunda yerel menü kalır.
if (!FULL) addEventListener("contextmenu", e => {
  if (e.target.closest("input, textarea, [contenteditable]") || String(getSelection())) return;
  e.preventDefault();
  tell({ mk: "widget-menu", x: e.clientX, y: e.clientY });
});

function status(cls, text) { st.className = "st " + cls; st.lastElementChild.textContent = text; }

function saveHistory() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(history.slice(-16)));
  } catch { /* depolama dolu */ }
}

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

function clearChatAndContext() {
  stopAll();
  history = [];
  try { localStorage.removeItem(STORAGE_KEY); } catch { /* yok */ }
  out.replaceChildren();
  out.hidden = true;
  $("chips").hidden = false; $("hero").hidden = false;
  fetch(AI + "/v1/makeros/temizle", { method: "POST", credentials: "same-origin", headers: { "X-Maker": "1" } }).catch(() => {});
  status("ok", "Temizlendi");
  input.focus();
}

stopBtn.onclick = stopAll;
clearBtn.onclick = clearChatAndContext;

async function poll() {
  // CLI ajanı seçiliyse yerel modelin durumu gösterilmez (alt satırda ajan · model yazar).
  if ((pref("aiProvider") || "maker") !== "maker") { if (!busy && activeTasks === 0) status("ok", "Hazır"); return; }
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

// Kayıtlı sohbet geçmişini yükle
(() => {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || "[]");
    if (Array.isArray(saved) && saved.length) {
      history = saved.filter(m => m && (m.role === "user" || m.role === "assistant") && typeof m.content === "string");
      if (history.length) {
        $("chips").hidden = true; $("hero").hidden = true;
        out.hidden = false;
        for (const m of history) {
          const el = document.createElement("div");
          if (m.role === "user") {
            el.className = "q";
            el.textContent = m.content;
          } else {
            el.className = "a";
            el.innerHTML = md(m.content);
          }
          out.append(el);
        }
        out.scrollTop = out.scrollHeight;
      }
    }
  } catch { /* bozuk geçmiş */ }
})();

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
    tell({ mk: "open", app: "makeros.terminal", path: `s=${d.term.id}` });
    return act === "install"
      ? `**${label}** kurulu değildi; kurulumu Terminal'de başlatıldı.`
      : `**${label}** Terminal'de açıldı.`;
  }
  tell({ mk: "open", app: "makeros.agents" });
  return `**${label}** için AI Ajanları uygulaması açıldı.`;
}

// Doğal dildeki doğrudan MakerOS uygulama/sistem eylemlerini yakalar.
// Sorular ("nedir", "nasıl", "ne işe yarar", "?") eylem olarak yakalanmaz; sohbet modeline gider.
function matchAction(raw) {
  const q = fold(raw).trim();
  const rawTrim = raw.trim();

  // Soru cümleleri doğrudan sohbet modeline yönlendirilir
  if (rawTrim.includes("?") || /\b(nedir|nasil|ne ise|ne yapar|anlat|hakkinda|bilgi|farki|neden|niye|kac|kimsin|merhaba|selam|nasilsin)\b/.test(q)) {
    return null;
  }

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
        if (d.term?.id) tell({ mk: "open", app: "makeros.terminal", path: `s=${d.term.id}` });
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
  const words = q.split(/\s+/).filter(Boolean);
  for (const a of agents) {
    if (a.keys.some(k => q === k || (words.length <= 5 && q.includes(k) && (actionVerb || q.endsWith(k))))) {
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
    { app: "makeros.python", name: "Python Yöneticisi", keys: ["python yoneticisi", "projeler", "projeleri"] },
    { app: "makeros.agents", name: "AI Ajanları", keys: ["ai ajan", "ajanlar", "ajanlari", "kodlama ajan"] },
    { app: "makeros.camera", name: "Kamera", keys: ["kamera", "kamerayi", "camera"] },
    { app: "makeros.gpio", name: "GPIO", keys: ["gpio", "pinler", "pinleri"] },
    { app: "makeros.plugins", name: "Eklentiler", keys: ["eklentiler", "eklentileri", "magaza", "magazayi", "plugins"] },
    { app: "makeros.activity", name: "Etkinlik Monitörü", keys: ["etkinlik monitor", "gorev yoneticisi", "calisan uygulamalar"] },
    { app: "chromium", name: "Tarayıcı", keys: ["tarayici", "tarayiciyi", "chromium", "chrome", "browser", "interneti"] },
    { app: "makeros.settings", name: "Ayarlar › Wi-Fi", path: "wifi", keys: ["wifi ayar", "wi-fi ayar", "kablosuz ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Ağ", path: "network", keys: ["ag ayar", "network ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Görünüm", path: "appearance", keys: ["gorunum ayar", "tema ayar", "duvar kagidi ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Otomatik Başlatma", path: "startup", keys: ["otomatik baslat", "baslangic ayar", "cron ayar", "zamanlayici ayar"] },
    { app: "makeros.settings", name: "Ayarlar › Genel", path: "general", keys: ["yazilim guncelle", "guncelleme ayar", "sistem bilgi"] },
    { app: "makeros.settings", name: "Ayarlar › Kullanıcı", path: "user", keys: ["kullanici ayar", "parola degistir", "sifre degistir"] },
    { app: "makeros.settings", name: "Ayarlar › Güç", path: "power", keys: ["guc ayar"] },
    { app: "makeros.settings", name: "Ayarlar", keys: ["ayarlar", "ayarlari", "sistem ayarlari", "settings"] },
  ];

  const openVerb = /\b(ac|acabilir|acar|baslat|calistir|goster|open|start|launch)\b/.test(q);
  for (const item of apps) {
    if (item.keys.some(k => q === k || (words.length <= 5 && q.includes(k) && openVerb))) {
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
      tell({ mk: "open", app: "makeros.terminal", path: `s=${d.term.id}` });
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
  for (let i = 0; i < list.length; i++) {
    const c = list[i];
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
    // Full Bypass modunda yalnız ilk (ana) komut doğrudan çalışır; alternatif komutlar düğmeyle çalıştırılır.
    if (mode() === "bypass" && i === 0) setTimeout(() => runCmd(c, row, true, c.command), 0);
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
  $("chips").hidden = true; out.hidden = false; $("hero").hidden = true;
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
    saveHistory();
    busy = false; updateWorkingUI(); out.scrollTop = out.scrollHeight;
    return;
  }

  if (provider() !== "maker") return askAgent(q, aEl);
  history.push({ role: "user", content: q });
  saveHistory();
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
    saveHistory();
    if (timings?.predicted_per_second) $("meta").textContent = `MakerOS AI · ${timings.predicted_per_second.toFixed(1)} token/sn`;
    if (activeTasks === 0) status("ok", "Hazır");
  } catch (e) {
    aEl.classList.remove("cur");
    if (e.name === "AbortError") {
      if (!text) aEl.innerHTML = `<span style="color:var(--text-2)">Durduruldu.</span>`;
      status("ok", "Durduruldu");
    } else {
      history.pop();
      saveHistory();
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
$("close").onclick = () => setPref("aiWidget", false);

// =============================================================== sağlayıcı / model
// "Maker AI" (yerel makeros-ai, varsayılan) ya da kartta kurulu bir CLI ajanı (Claude Code, Codex, Gemini CLI,
// OpenCode, Ollama). Ajan kullanıcının kendi hesabıyla, ~/Projeler'de etkileşimsiz çalışır (makerd /asistan/ajan).
// Seçim tercihlerde: aiProvider, aiModel.<ajan>.
const provSel = $("prov"), modelSel = $("model");
let agents = [];
const provider = () => { const p = pref("aiProvider") || "maker"; return p === "maker" || agents.some(a => a.id === p) ? p : "maker"; };
const modelOf = id => pref("aiModel." + id) ?? "";
const provName = id => id === "maker" ? "Maker AI" : agents.find(a => a.id === id)?.name || id;
function renderModels() {
  const p = provider(), a = agents.find(x => x.id === p);
  provSel.value = p;
  modelSel.hidden = !a;
  if (!a) { $("meta").textContent = "MakerOS AI"; return; }
  const cur = modelOf(p) || (a.needs_model ? a.models[0] || "" : "");
  const list = [...new Set([...a.models, cur].filter(m => m !== undefined))];
  modelSel.replaceChildren(...list.map(m => new Option(m || "Varsayılan model", m)), new Option("Diğer model…", "__other"));
  if (a.needs_model && !a.models.length) modelSel.prepend(new Option("Model yok — ollama pull ile indirin", ""));
  modelSel.value = cur;
  $("meta").textContent = `${a.name}${cur ? " · " + cur : ""}`;
}
async function loadAgents() {
  try {
    const r = await fetch("/api/v1/asistan/ajanlar", { credentials: "same-origin" });
    agents = r.ok ? await r.json() : [];
  } catch { agents = []; }
  provSel.replaceChildren(new Option("Maker AI (yerel)", "maker"), ...agents.map(a => new Option(a.name, a.id)));
  provSel.title = agents.length ? "Asistan: Maker AI (yerel) veya kurulu CLI ajanı" : "Ajan kurmak için: AI Ajanları uygulaması";
  renderModels();
}
provSel.onchange = () => { setPref("aiProvider", provSel.value); renderModels(); poll(); };
modelSel.onchange = () => {
  const p = provider();
  if (modelSel.value === "__other") {
    const m = (prompt("Model adı (ör. claude-sonnet-4-5, gpt-5, gemini-2.5-pro, llama3.2:1b):", modelOf(p)) || "").trim();
    if (m && !/^[A-Za-z0-9._:/-]{1,100}$/.test(m)) { alert("Geçersiz model adı."); renderModels(); return; }
    setPref("aiModel." + p, m);
  } else setPref("aiModel." + p, modelSel.value);
  renderModels();
};
prefs.on("change", k => { if (k === "aiProvider" || String(k).startsWith("aiModel.")) renderModels(); });
loadAgents();

/** Seçili CLI ajanına sorar; yanıt geldikçe (akış) gösterilir. Önceki birkaç mesaj bağlam olarak eklenir. */
async function askAgent(q, aEl) {
  const p = provider(), a = agents.find(x => x.id === p), model = modelOf(p) || (a?.needs_model ? a.models[0] || "" : "");
  const ctx = history.slice(-6).map(m => `${m.role === "user" ? "Kullanıcı" : "Asistan"}: ${m.content}`).join("\n\n");
  const prompt = (ctx ? `Önceki konuşma (bağlam):\n${ctx}\n\n---\n` : "") +
    `MakerOS (Raspberry Pi tabanlı Linux) üzerinde, kullanıcının ~/Projeler klasöründesin. Türkçe yanıt ver.\n\n${q}`;
  history.push({ role: "user", content: q });
  saveHistory();
  let text = "";
  status("load", `${provName(p)} çalışıyor…`);
  ctrl = new AbortController();
  try {
    const r = await fetch("/api/v1/asistan/ajan", { method: "POST", credentials: "same-origin", signal: ctrl.signal,
      headers: { "Content-Type": "application/json", "X-Maker": "1" }, body: JSON.stringify({ agent: p, model, prompt }) });
    if (!r.ok) { const e = await r.json().catch(() => ({})); throw new Error(e.error?.message || `hata ${r.status}`); }
    const rd = r.body.getReader(), dec = new TextDecoder();
    for (;;) {
      const { value, done } = await rd.read();
      if (done) break;
      text += dec.decode(value, { stream: true });
      // Uçbirim renk/denetim kodları (ör. ollama'nın dönen imleci) gösterilmez.
      const clean = text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/[\u2800-\u28ff]/g, "");
      if (clean.trim()) { status("load", "Yazıyor…"); aEl.innerHTML = md(clean.trim()); out.scrollTop = out.scrollHeight; }
    }
    text = text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/[\u2800-\u28ff]/g, "").trim();
    aEl.classList.remove("cur");
    if (!text) aEl.innerHTML = `<span style="color:var(--text-2)">Ajan yanıt vermedi. Giriş yapılmamış olabilir: AI Ajanları uygulamasından ajanı bir kez terminalde açıp oturum açın.</span>`;
    history.push({ role: "assistant", content: text });
    saveHistory();
    if (activeTasks === 0) status("ok", "Hazır");
  } catch (e) {
    aEl.classList.remove("cur");
    if (e.name === "AbortError") { if (!text) aEl.innerHTML = `<span style="color:var(--text-2)">Durduruldu.</span>`; status("ok", "Durduruldu"); }
    else {
      history.pop(); saveHistory();
      const er = document.createElement("div"); er.className = "err"; er.textContent = e.message || "Ajana ulaşılamadı."; aEl.after(er);
      status("off", "Hata");
    }
  } finally {
    busy = false; ctrl = null; updateWorkingUI(); out.scrollTop = out.scrollHeight;
  }
}
