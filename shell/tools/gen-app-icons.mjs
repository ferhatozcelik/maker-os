// Uygulama simgeleri: özgün süperelips zemin + Lucide (ISC) glif → icons/<ad>.svg
//   node shell/tools/gen-app-icons.mjs
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const here = dirname(fileURLToPath(import.meta.url));
const icons = (await import(join(here, "../js/icon-set.js"))).default;
// n=4.2 süperelips, 128 birimlik kutuda 10 birim kenar boşluğu
const pts = [];
for (let i = 0; i < 96; i++) {
  const a = (i / 96) * Math.PI * 2, c = Math.cos(a), s = Math.sin(a), n = 2 / 4.2;
  pts.push([64 + 54 * Math.sign(c) * Math.abs(c) ** n, 64 + 54 * Math.sign(s) * Math.abs(s) ** n].map(v => v.toFixed(1)).join(" "));
}
const shape = "M" + pts.join("L") + "Z";
const apps = {
  settings: ["#8e8e96", "#4b4b52", "settings"],
  terminal: ["#3a3a40", "#121215", "square-terminal"],
  activity: ["#2fbf71", "#127a52", "activity"],
  camera: ["#5e6bff", "#2d2fb3", "camera"],
  gpio: ["#21b5a6", "#0b6e7a", "circuit-board"],
  python: ["#f2c744", "#3b74c4", "file-code"],
  files: ["#55b6ff", "#1a6fe0", "folder"],
  plugins: ["#ff9f43", "#e8590c", "puzzle"],
  agents: ["#8b7bff", "#4c3fd6", "bot"],
  screen: ["#4b5563", "#111827", "monitor"],
  launchpad: ["#c3c8d6", "#7b8296", "layout-grid"],
  web: ["#ff8a3d", "#d9480f", "layout-grid"],
  chromium: ["#5aa9ff", "#1f5fd1", "globe"],
  app: ["#9aa0ad", "#5b6170", "app-window"],
};
for (const [name, [a, b, glyph]] of Object.entries(apps)) {
  writeFileSync(join(here, "../icons", name + ".svg"),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient>` +
    `<linearGradient id="h" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".28"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>` +
    `<path d="${shape}" fill="url(#g)"/><path d="${shape}" fill="url(#h)"/>` +
    `<g transform="translate(34 34) scale(2.5)" fill="none" stroke="#fff" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${icons[glyph]}</g></svg>\n`);
}
// Marka: grafit süperelips + devre izi "M" (lehim pedleri ve via turuncu). Özgün çizim.
writeFileSync(join(here, "../icons/logo.svg"),
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><linearGradient id="g" x1="0" y1="0" x2=".4" y2="1"><stop offset="0" stop-color="#40444b"/><stop offset="1" stop-color="#16171a"/></linearGradient>` +
  `<linearGradient id="h" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".22"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>` +
  `<path d="${shape}" fill="url(#g)"/><path d="${shape}" fill="url(#h)"/>` +
  `<path d="M38 88V44l26 26 26-26v44" fill="none" stroke="#f3f3f5" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/>` +
  `<g fill="#ff7a1a"><circle cx="38" cy="90" r="8.5"/><circle cx="90" cy="90" r="8.5"/><circle cx="64" cy="70" r="6.5"/></g>` +
  `<g fill="#1c1d21"><circle cx="38" cy="90" r="3.2"/><circle cx="90" cy="90" r="3.2"/><circle cx="64" cy="70" r="2.4"/></g></svg>\n`);
writeFileSync(join(here, "../icons/mark.svg"),
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 18.5V7l7 7 7-7v11.5"/><circle cx="5" cy="19" r="1.9" fill="currentColor" stroke="none"/><circle cx="19" cy="19" r="1.9" fill="currentColor" stroke="none"/></svg>\n`);
console.log("simgeler yazıldı");

// Konya HackerSpace simge paketi (eklenti teması): mürekkep süperelips + lime çizgi glif, ince lime kenar.
// Çıktı: makerd/internal/plugins/defaults/khs/icons/<ad>.svg (plugin.json "theme.icons")
const khsDir = join(here, "../../makerd/internal/plugins/defaults/khs/icons");
mkdirSync(khsDir, { recursive: true });
for (const [name, [, , glyph]] of Object.entries(apps)) {
  writeFileSync(join(khsDir, name + ".svg"),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1a2033"/><stop offset="1" stop-color="#0c0f1a"/></linearGradient></defs>` +
    `<path d="${shape}" fill="url(#g)"/><path d="${shape}" fill="none" stroke="#c8f53c" stroke-opacity=".35" stroke-width="2.5"/>` +
    `<g transform="translate(34 34) scale(2.5)" fill="none" stroke="#c8f53c" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${icons[glyph]}</g></svg>\n`);
}
console.log("KHS simge paketi yazıldı");
