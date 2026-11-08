// AI Asistan widget'ı: masaüstünde yerel Türkçe Linux asistanına (makeros-ai) soru sorar.
// İstekler makerd üzerinden gider (POST /api/v1/ai/v1/chat/completions, oturum gerekir); yanıt akışla gelir.
// Önerilen komutların risk düzeyi (SAFE/WARNING/DANGEROUS) makeros-ai'nin kurallı sınıflandırıcısından gelir.
// İzin modu (tercih aiMode): oner = yalnız öneri · sor = her komutu kullanıcı başlatır · otomatik = SAFE komutlar
// sormadan çalışır. WARNING/DANGEROUS her modda onay ister (DANGEROUS: komut yeniden yazılır). Karar ve sınırlar
// sunucuda yeniden denetlenir (POST /api/v1/asistan/calistir); komutlar kullanıcının kendi yetkisiyle çalışır.
import { icon } from "/shell/js/icons.js";
import { apply, prefs, pref, setPref } from "/shell/js/prefs.js";

apply();
prefs.on("change", () => apply());
addEventListener("storage", () => apply());

const $ = id => document.getElementById(id);
const AI = "/api/v1/ai";
const out = $("out"), input = $("in"), go = $("go"), st = $("st");
document.querySelector(".logo").innerHTML = icon("sparkles", 14, 2.2);
go.innerHTML = icon("arrow-up", 16, 2.4);
$("new").innerHTML = icon("rotate-ccw", 13, 2);
$("close").innerHTML = icon("x", 13, 2.2);

const CHIPS = [["CPU sıcaklığı", "CPU sıcaklığını göster."], ["RAM", "RAM kullanımını kontrol et."], ["Disk", "Disk alanını kontrol et."],
  ["IP adresi", "IP adresimi öğren."], ["Son hatalar", "Son sistem hatalarını göster."], ["Servisler", "Çalışan servisleri listele."]];
$("chips").append(...CHIPS.map(([l, q]) => { const b = document.createElement("button"); b.type = "button"; b.textContent = l; b.onclick = () => ask(q); return b; }));

let history = [], busy = false, ctrl = null;

// Kabuk widget alanının yüksekliğini içeriğe göre ayarlar.
const size = () => parent.postMessage({ mk: "widget-size", h: Math.ceil(document.querySelector(".card").getBoundingClientRect().height) + 4 }, location.origin);
new ResizeObserver(size).observe(document.querySelector(".card"));

function status(cls, text) { st.className = "st " + cls; st.lastElementChild.textContent = text; }
async function poll() {
  try {
    const r = await fetch(AI + "/v1/makeros/durum", { credentials: "same-origin" });
    if (!r.ok) throw new Error();
    const d = await r.json();
    status("ok", d.llama?.loaded ? "Hazır" : "Uykuda");
    $("meta").textContent = d.llama?.loaded ? `Yerel · internetsiz · RAM ${Math.round(d.llama.rss_mb)} MB` : "Yerel · internetsiz · Qwen2.5-0.5B";
  } catch { status("off", "Kapalı"); }
}
poll();
setInterval(() => { if (!busy && document.visibilityState === "visible") poll(); }, 20000);

// Çok küçük, güvenli markdown: önce HTML kaçışı, sonra kod blokları, satır içi kod ve kalın.
const esc = s => s.replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
function md(s) {
  const parts = esc(s).split(/```[a-z]*\n?/i);
  return parts.map((p, i) => i % 2 ? `<pre><code>${p.replace(/\n$/, "")}</code></pre>`
    : p.replace(/`([^`\n]+)`/g, "<code>$1</code>").replace(/\*\*([^*\n]+)\*\*/g, "<b>$1</b>").split(/\n{2,}/).map(x => `<p>${x.replace(/\n/g, "<br>")}</p>`).join("")).join("");
}

const modeSel = $("mode");
const mode = () => pref("aiMode") || "oner";
modeSel.value = mode();
modeSel.onchange = () => setPref("aiMode", modeSel.value);
prefs.on("change", k => { if (k === "aiMode") modeSel.value = mode(); });
addEventListener("storage", () => { modeSel.value = mode(); });

async function runCmd(c, row, onay = false, yazilan) {
  row.querySelector(".conf")?.remove();
  row.querySelector(".res")?.remove();
  const res = document.createElement("div"); res.className = "res";
  res.innerHTML = `<div class="rh">çalışıyor…</div>`;
  row.append(res);
  try {
    const r = await fetch("/api/v1/asistan/calistir", { method: "POST", credentials: "same-origin", headers: { "Content-Type": "application/json", "X-Maker": "1" }, body: JSON.stringify({ command: c.command, onay, yazilan }) });
    const d = await r.json().catch(() => ({}));
    if (!r.ok) { res.innerHTML = ""; const e = document.createElement("div"); e.className = "rh"; e.textContent = d.error?.message || `hata ${r.status}`; res.append(e); return; }
    if (d.durum === "onay_gerekli") { res.remove(); confirmBox(c, row, d); return; }
    const head = document.createElement("div"); head.className = "rh";
    const b = document.createElement("b"); b.className = d.cikis === 0 ? "ok" : "bad"; b.textContent = d.cikis == null ? "çıkış ?" : `çıkış ${d.cikis}`;
    const ms = document.createElement("span"); ms.textContent = `${(d.ms / 1000).toFixed(1)} sn${d.not ? " · " + d.not : ""}`;
    const ex = document.createElement("button"); ex.textContent = "Açıkla"; ex.title = "Çıktıyı asistana açıklat";
    ex.onclick = () => ask(`Şu komutu çalıştırdım: \`${c.command}\`\nÇıktı:\n\`\`\`\n${(d.cikti || "(boş)").slice(0, 1200)}\n\`\`\`\nÇıktıyı kısaca Türkçe açıkla; sorun varsa belirt.`);
    head.append(b, ms, ex);
    const pre = document.createElement("pre"); pre.textContent = d.cikti || "(çıktı yok)";
    res.replaceChildren(head, pre);
    out.scrollTop = out.scrollHeight;
  } catch (e) { res.textContent = e.message || "çalıştırılamadı"; }
}

// Onay kutusu: SAFE/WARNING tek tık; DANGEROUS komutun aynen yazılmasını ister.
function confirmBox(c, row, d) {
  row.querySelector(".conf")?.remove();
  const box = document.createElement("div"); box.className = "conf";
  const msg = document.createElement("div");
  const why = (d?.reasons || c.reasons || []).join(" · ");
  const btns = document.createElement("div"); btns.className = "row";
  const ok = document.createElement("button"); ok.className = "ok"; ok.textContent = "Çalıştır";
  const no = document.createElement("button"); no.textContent = "Vazgeç"; no.onclick = () => box.remove();
  if (c.risk === "DANGEROUS") {
    msg.textContent = `TEHLİKELİ: ${why || "geri alınamayabilir"}. Çalıştırmak için komutu aynen yazın:`;
    const inp = document.createElement("input"); inp.placeholder = c.command; inp.spellcheck = false;
    ok.classList.add("danger"); ok.disabled = true;
    inp.oninput = () => { ok.disabled = inp.value.trim() !== c.command.trim(); };
    ok.onclick = () => runCmd(c, row, true, inp.value);
    box.append(msg, inp, btns);
  } else {
    msg.textContent = c.risk === "WARNING" ? `Dikkat: ${why || "sistemi değiştirebilir"}. Çalıştırılsın mı?` : "Çalıştırılsın mı?";
    ok.onclick = () => runCmd(c, row, true);
    box.append(msg, btns);
  }
  btns.append(ok, no);
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
    const acts = document.createElement("span"); acts.className = "acts"; acts.append(cp);
    if (mode() !== "oner") {
      const run = document.createElement("button"); run.className = "ib run"; run.innerHTML = icon("play", 11, 2.4) + "Çalıştır";
      run.title = "Komutu çalıştır (kendi kullanıcınızla, 30 sn sınırı)";
      // Kullanıcının tıklaması SAFE için yeterli onaydır; WARNING/DANGEROUS ayrıca onay kutusu açar.
      run.onclick = () => c.risk === "SAFE" ? runCmd(c, row, true) : confirmBox(c, row);
      acts.prepend(run);
    }
    row.append(rk, code, acts);
    if (c.reasons?.length && c.risk !== "SAFE") { const w = document.createElement("div"); w.className = "why"; w.textContent = c.reasons.join(" · "); row.append(w); }
    box.append(row);
    // Otomatik mod: salt okunur komutlar sormadan çalışır.
    if (mode() === "otomatik" && c.risk === "SAFE") setTimeout(() => runCmd(c, row, false), 0);
  }
  const n = document.createElement("div"); n.className = "note";
  n.textContent = { oner: "İzin modu: yalnız öneri. Komutlar çalıştırılmaz; modu aşağıdan değiştirebilirsiniz.", sor: "İzin modu: izin iste. Komutu siz başlatırsınız.", otomatik: "İzin modu: otomatik. Salt okunur komutlar çalıştırıldı; diğerleri onay ister." }[mode()];
  box.append(n);
  return box;
}

async function ask(q) {
  q = (q || "").trim();
  if (!q || busy) return;
  busy = true; go.disabled = true; input.value = "";
  $("chips").hidden = true; out.hidden = false;
  const qEl = document.createElement("div"); qEl.className = "q"; qEl.textContent = q;
  const aEl = document.createElement("div"); aEl.className = "a cur";
  out.append(qEl, aEl); out.scrollTop = out.scrollHeight;
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
    if (timings?.predicted_per_second) $("meta").textContent = `Yerel · ${timings.predicted_per_second.toFixed(1)} token/sn`;
    status("ok", "Hazır");
  } catch (e) {
    aEl.classList.remove("cur");
    history.pop();
    if (e.name !== "AbortError") { const er = document.createElement("div"); er.className = "err"; er.textContent = e.message || "Asistana ulaşılamadı."; aEl.after(er); }
    poll();
  } finally {
    busy = false; go.disabled = false; ctrl = null; out.scrollTop = out.scrollHeight;
  }
}

$("f").onsubmit = e => { e.preventDefault(); ask(input.value); };
$("new").onclick = () => { ctrl?.abort(); history = []; out.replaceChildren(); out.hidden = true; $("chips").hidden = false; input.focus(); };
$("close").onclick = () => setPref("aiWidget", false);
