// GPIO: 40 pinlik başlık (Pi 4 / 400 / 5 aynı). Canlı durum (1 sn); bir pine tıklayınca ayrıntı paneli açılır:
// durum, yön, değer, iç direnç, bağlantı tahmini, son 60 sn geçmişi, işlemler (1/0, yanıp sönme, serbest bırak,
// bağlantı kontrolü) ve hazır Python örneği. "Bağlantıları tara" boştaki pinlerde bir şey bağlı mı diye bakar
// (makerd: iç pull-up/pull-down ile okuma). Kullanımdaki pinlere (I2C, UART, başka program) dokunulmaz.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { icon } from "/shell/js/icons.js";
import { get, post } from "/shell/js/api.js";
import { fail, toast } from "/shell/js/appui.js";

app.init({ title: "GPIO" });
app.chrome({ title: false });
$(".h").textContent = t("gp.title");
$(".n-help").textContent = t("gp.help");
$(".legend").append(...[["#2fbf71", t("gp.high")], ["var(--bg-hover)", t("gp.low")], ["#e8590c", "3V3"], ["#d6336c", "5V"], ["#212529", "GND"]].map(([c, l]) => h("span", h("i", { style: `background:${c}` }), l)),
  h("span", h("i.cdot"), t("gp.sum.conn")));
const board = $(".board"), side = $(".detail"), sum = $(".sum");
const scanBtn = $(".b-scan");
scanBtn.append(h("span", { html: icon("scan-eye", 14, 2) }), h("span.lb", t("gp.scan")));
scanBtn.onclick = () => scan();

let timer = 0, B = null, sel = null, blink = null;
const hist = new Map(); // gpio → son 60 değer (1 sn aralıkla)
const CONNECTED = new Set(["high", "low", "signal"]);

function pinEl(p) {
  const cls = p.gpio < 0 ? (p.name === "GND" ? ".gnd" : p.name === "5V" ? ".p5v" : ".p3v")
    : ".gp" + (p.value ? ".hi" : "") + (p.dir === "out" ? ".out" : "") + (p.used ? ".used" : "") + (CONNECTED.has(p.conn) ? ".conn" : "");
  const title = p.gpio < 0 ? p.name : `${p.name} · ${p.dir === "out" ? t("gp.out") : t("gp.in")} · ${p.value ? "1" : "0"}${p.used ? " · " + t("gp.used", { by: p.consumer || "?" }) : ""}${p.held ? " · " + t("gp.held") : ""}`;
  return h("button.pin" + cls + (sel === p.phys ? ".sel" : ""), { title, "aria-label": title, onclick: () => pick(p.phys) }, p.phys);
}
const label = (p, side) => h("div.lbl." + side + (sel === p.phys ? ".sel" : ""), { onclick: () => pick(p.phys) },
  side === "l" ? [h("span.alt", p.alt || ""), h("span.nm", p.name)] : [h("span.nm", p.name), h("span.alt", p.alt || "")]);

function render(b) {
  B = b;
  for (const p of b.pins) if (p.gpio >= 0) { const a = hist.get(p.gpio) || []; a.push(p.value); if (a.length > 60) a.shift(); hist.set(p.gpio, a); }
  $(".b-model").textContent = b.model + (b.sim ? " · " + t("gp.sim") : "");
  const rows = [];
  for (let i = 0; i < 40; i += 2) {
    const a = b.pins[i], c = b.pins[i + 1];
    rows.push(label(a, "l"), pinEl(a), pinEl(c), label(c, "r"));
  }
  board.replaceChildren(...rows);
  const gp = b.pins.filter(p => p.gpio > 1);
  const n = f => gp.filter(f).length;
  sum.replaceChildren(
    chip(t("gp.sum.free"), n(p => !p.used && !p.held && !CONNECTED.has(p.conn)), ""),
    chip(t("gp.sum.conn"), n(p => CONNECTED.has(p.conn)), ".c"),
    chip(t("gp.sum.used"), n(p => p.used), ".u"),
    chip(t("gp.sum.out"), n(p => p.dir === "out"), ".o"));
  renderDetail();
}
const chip = (l, n, cls) => h("span.schip" + cls, h("b", String(n)), l);

// --------------------------------------------------------------- ayrıntı paneli
const FUNCS = [[/I2C/, "i2c", "i2cdetect -y 1"], [/UART/, "uart", "ls -l /dev/serial0"], [/SPI/, "spi", "ls /dev/spidev*"], [/PWM/, "pwm", ""], [/PCM/, "pcm", ""], [/GPCLK/, "clk", ""], [/EEPROM/, "eeprom", ""]];
const funcOf = p => FUNCS.find(([re]) => re.test(p.alt || "")) || [null, "none", ""];
const kv = (k, v) => h("div.kv", h("span.k", k), h("span.v", v));
const pill = (txt, cls = "") => h("span.pill" + cls, txt);

function pick(phys) {
  sel = sel === phys ? null : phys;
  stopBlink();
  render(B);
  if (sel && matchMedia("(max-width: 899px)").matches) side.scrollIntoView({ behavior: "smooth", block: "start" });
}

function renderDetail() {
  const p = sel && B?.pins.find(x => x.phys === sel);
  if (!p) { side.replaceChildren(h("div.d-empty", h("span", { html: icon("circuit-board", 30, 1.6) }), h("p", t("gp.pick")))); return; }
  const head = (cls, sub) => h("div.d-hd", h("span.big.pin" + cls, p.phys), h("div", h("div.d-nm", p.name), h("div.d-sub", sub)),
    h("button.btn.icon.sm.d-x", { html: icon("x", 14, 2), "aria-label": "×", onclick: () => pick(p.phys) }));
  if (p.gpio < 0) {
    const k = p.name === "GND" ? "gnd" : p.name === "5V" ? "5v" : "3v3";
    side.replaceChildren(head(k === "gnd" ? ".gnd" : k === "5v" ? ".p5v" : ".p3v", t("gp.phys", { n: p.phys })), h("p.d-txt", t("gp.pw." + k)), h("p.d-warn", t("gp.warn")));
    return;
  }
  const [, fk, cmd] = funcOf(p);
  const reserved = p.gpio <= 1;
  const state = reserved ? t("gp.st.reserved") : p.held ? t("gp.st.held") : p.used ? t("gp.st.used", { by: p.consumer || "?" }) : t("gp.st.free");
  const conn = p.conn || "unknown";
  const hs = hist.get(p.gpio) || [];
  const changes = hs.reduce((n, v, i) => n + (i && v !== hs[i - 1] ? 1 : 0), 0);
  const els = [
    head(".gp" + (p.value ? ".hi" : "") + (p.dir === "out" ? ".out" : ""), `${t("gp.phys", { n: p.phys })} · ${t("gp.bcm", { n: p.gpio })}`),
    h("div.kvs",
      kv(t("gp.k.state"), pill(state, reserved || p.used ? ".u" : p.held ? ".o" : "")),
      kv(t("gp.k.dir"), p.dir === "out" ? t("gp.out") : t("gp.in")),
      kv(t("gp.k.value"), h("span.val" + (p.value ? ".hi" : ""), p.value ? "1 · HIGH" : "0 · LOW")),
      p.bias ? kv(t("gp.k.bias"), t("gp.bias." + p.bias)) : "",
      kv(t("gp.k.func"), p.alt || "—")),
    h("div.d-sec", t("gp.k.conn")),
    h("div.conn." + conn, h("div.c-t", h("i.cd"), t("gp.c." + conn)), h("div.c-d", t("gp.cd." + conn))),
    h("div.d-sec", h("span", t("gp.hist")), h("span.d-mut", t("gp.changes", { n: changes }))),
    h("div.spark", ...Array.from({ length: 60 }, (_, i) => { const v = hs[hs.length - 60 + i]; return h("i" + (v == null ? ".na" : v ? ".hi" : "")); })),
    h("p.d-txt", t("gp.f." + fk)),
  ];
  if (!reserved && !p.used) {
    els.push(h("div.d-sec", t("gp.actions")), h("div.acts",
      h("button.btn.sm" + (p.held && p.value ? ".primary" : ""), { onclick: () => setv(p, 1) }, t("gp.set1")),
      h("button.btn.sm" + (p.held && !p.value ? ".primary" : ""), { onclick: () => setv(p, 0) }, t("gp.set0")),
      h("button.btn.sm", { onclick: () => (blink ? stopBlink() : startBlink(p)) }, blink ? t("gp.blinkStop") : t("gp.blink")),
      p.held ? h("button.btn.sm", { onclick: () => release(p) }, t("gp.release")) : "",
      h("button.btn.sm", { disabled: p.held, title: p.held ? t("gp.probeHeld") : "", onclick: () => probe(p) }, h("span", { html: icon("scan-eye", 13, 2) }), t("gp.probe"))));
  }
  if (cmd) els.push(h("div.d-sec", t("gp.cmd")), codeBox(cmd));
  if (!reserved && fk !== "i2c" && fk !== "uart" && fk !== "eeprom") {
    const out = p.dir === "out" || p.held;
    const code = out
      ? `from gpiozero import LED\nfrom time import sleep\n\nled = LED(${p.gpio})  # fiziksel pin ${p.phys}\nwhile True:\n    led.toggle()\n    sleep(0.5)`
      : `from gpiozero import Button\nfrom signal import pause\n\nbtn = Button(${p.gpio})  # fiziksel pin ${p.phys}, diğer ucu GND\nbtn.when_pressed = lambda: print("basıldı")\nbtn.when_released = lambda: print("bırakıldı")\npause()`;
    els.push(h("div.d-sec", h("span", t("gp.code")), h("button.btn.sm.ghost", { onclick: () => app.open("makeros.python", "projects") }, t("gp.newPy"))), codeBox(code));
  }
  els.push(h("p.d-warn", t("gp.warn")));
  side.replaceChildren(...els);
}

function codeBox(text) {
  const b = h("button.btn.icon.sm.cp", { html: icon("copy", 13, 2), title: t("gp.copy"), onclick: async () => {
    try { await navigator.clipboard.writeText(text); toast(t("gp.copied")); } catch { /* izin yok */ }
  } });
  return h("div.code", h("pre", text), b);
}

// --------------------------------------------------------------- işlemler
async function refresh() {
  clearTimeout(timer);
  try { render(await get("/gpio")); } catch (e) { board.replaceChildren(h("div.empty.err", e.message)); }
  if (app.visible) timer = setTimeout(refresh, 1000);
}
async function setv(p, v) {
  try { render(await post("/gpio/" + p.gpio, { value: v })); } catch (e) { fail(e); }
}
async function release(p) {
  stopBlink();
  try { render(await post("/gpio/" + p.gpio, { release: true })); } catch (e) { fail(e); }
}
async function probe(p) {
  try { render(await post("/gpio/" + p.gpio, { probe: true })); } catch (e) { fail(e); }
}
async function scan() {
  scanBtn.disabled = true; scanBtn.querySelector(".lb").textContent = t("gp.scanning");
  try { render(await post("/gpio/scan")); toast(t("gp.scanned")); } catch (e) { fail(e); }
  scanBtn.disabled = false; scanBtn.querySelector(".lb").textContent = t("gp.scan");
}
function startBlink(p) {
  stopBlink();
  let v = p.value ? 0 : 1;
  blink = setInterval(() => { post("/gpio/" + p.gpio, { value: v }).then(render).catch(e => { stopBlink(); fail(e); }); v ^= 1; }, 500);
  renderDetail();
}
function stopBlink() { if (blink) { clearInterval(blink); blink = null; if (B) renderDetail(); } }

app.onVisibility(v => (v ? refresh() : (clearTimeout(timer), stopBlink())));
refresh().then(() => scan());
