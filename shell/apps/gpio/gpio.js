// GPIO: 40 pinlik başlık (Pi 4 / 400 / 5 aynı). Canlı durum; boş pine tıklayınca çıkış yapılıp değiştirilir,
// sağ tık (veya uzun bas) pini serbest bırakır. Kullanımdaki pinlere (I2C, UART, başka program) dokunulmaz.
import { app, t } from "/shell/js/app-kit.js";
import { h, $ } from "/shell/js/lib.js";
import { get, post } from "/shell/js/api.js";
import { fail } from "/shell/js/appui.js";

app.init({ title: "GPIO" });
app.chrome({ title: false });
$(".h").textContent = t("gp.title");
$(".n-help").textContent = t("gp.help");
$(".legend").append(...[["#2fbf71", t("gp.high")], ["var(--bg-hover)", t("gp.low")], ["#e8590c", "3V3"], ["#d6336c", "5V"], ["#212529", "GND"]].map(([c, l]) => h("span", h("i", { style: `background:${c}` }), l)));
const board = $(".board");
let timer = 0;

function pinEl(p) {
  const cls = p.gpio < 0 ? (p.name === "GND" ? ".gnd" : p.name === "5V" ? ".p5v" : ".p3v") : ".gp" + (p.value ? ".hi" : "") + (p.dir === "out" ? ".out" : "") + (p.used ? ".used" : "");
  const title = p.gpio < 0 ? p.name : `${p.name} · ${p.dir === "out" ? t("gp.out") : t("gp.in")} · ${p.value ? "1" : "0"}${p.used ? " · " + t("gp.used", { by: p.consumer || "?" }) : ""}${p.held ? " · " + t("gp.held") : ""}`;
  return h("button.pin" + cls, { title, "aria-label": title, onclick: () => toggle(p), oncontextmenu: e => { e.preventDefault(); release(p); } }, p.phys);
}
const label = (p, side) => h("div.lbl." + side, side === "l" ? [h("span.alt", p.alt || ""), h("span.nm", p.name)] : [h("span.nm", p.name), h("span.alt", p.alt || "")]);
function render(b) {
  $(".b-model").textContent = b.model + (b.sim ? " · " + t("gp.sim") : "");
  const rows = [];
  for (let i = 0; i < 40; i += 2) {
    const a = b.pins[i], c = b.pins[i + 1];
    rows.push(label(a, "l"), pinEl(a), pinEl(c), label(c, "r"));
  }
  board.replaceChildren(...rows);
}
async function refresh() {
  clearTimeout(timer);
  try { render(await get("/gpio")); } catch (e) { board.replaceChildren(h("div.empty.err", e.message)); }
  if (app.visible) timer = setTimeout(refresh, 1000);
}
async function toggle(p) {
  if (p.gpio < 0 || p.used) return;
  try { render(await post("/gpio/" + p.gpio, { value: p.dir === "out" && p.value ? 0 : 1 })); } catch (e) { fail(e); }
}
async function release(p) {
  if (p.gpio < 0) return;
  try { render(await post("/gpio/" + p.gpio, { release: true })); } catch (e) { fail(e); }
}
app.onVisibility(v => (v ? refresh() : clearTimeout(timer)));
refresh();
