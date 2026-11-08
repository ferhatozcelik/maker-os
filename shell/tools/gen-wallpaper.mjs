// Özgün tema duvar kâğıtları (kodla üretilir, dış görsel yok → lisans temiz).
//   docker run --rm -v "$PWD/shell":/s -w /s node:24-alpine node tools/gen-wallpaper.mjs
// Çıktı: wallpapers/<tema>-{light,dark}.svg ve <tema>-{light,dark}-blur.png (kilit ekranı, Launchpad, "lite"
// efekt katmanı). Bulanık sürüm önceden küçük PNG'ye çizilir (resvg): Pi'de canlı Gaussian bulanıklık hesaplanmaz.
//   docker run --rm -v "$PWD/shell":/s -w /s node:24-alpine sh -c "npm i --no-save @resvg/resvg-js@2.6.2 >/dev/null && node tools/gen-wallpaper.mjs"
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const dir = join(dirname(fileURLToPath(import.meta.url)), "../wallpapers");
const W = 1920, H = 1080;

// Tohumlu rastgele sayı (her derlemede aynı görüntü)
function rng(seed) { let s = seed >>> 0; return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296); }
const lin = (id, a, b, x2 = 0, y2 = 1) => `<linearGradient id="${id}" x1="0" y1="0" x2="${x2}" y2="${y2}"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient>`;
const rad = (id, c, cx, cy, r, o = 1) => `<radialGradient id="${id}" cx="${cx}" cy="${cy}" r="${r}" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="${c}" stop-opacity="${o}"/><stop offset="1" stop-color="${c}" stop-opacity="0"/></radialGradient>`;
const f1 = n => +n.toFixed(1);

// ---------------------------------------------------------------- Maker (grafit)
function maker(dark) {
  const c = dark
    ? { bg: ["#2c2f34", "#141518"], glow: "#4a4f57", dot: "#ffffff", line: "#ffffff", acc: "#ff7a1a", ink: "#e9eaec" }
    : { bg: ["#fff7ef", "#f2dcc8"], glow: "#ffffff", dot: "#1b1d21", line: "#1b1d21", acc: "#f06a00", ink: "#2a2d32" };
  // Sıcak turuncu (sağ üst) ve mavi (sol alt) ışıltı: bulanık sürümde (kilit ekranı) gri yerine renkli bir zemin.
  const defs = lin("bg", ...c.bg, .4, 1) + rad("g", c.glow, 360, 180, 900, dark ? .55 : .9) +
    rad("o", "#ff8a3d", 1560, 140, 980, dark ? .32 : .5) + rad("b", "#4f8fff", 180, 1020, 900, dark ? .26 : .32) +
    `<pattern id="dots" width="32" height="32" patternUnits="userSpaceOnUse"><circle cx="16" cy="16" r="1.3" fill="${c.dot}" opacity="${dark ? .14 : .16}"/></pattern>` +
    `<pattern id="grid" width="160" height="160" patternUnits="userSpaceOnUse"><path d="M160 0H0V160" fill="none" stroke="${c.line}" stroke-width="1" opacity="${dark ? .08 : .1}"/></pattern>`;
  // İzometrik küp tel çizimi + ölçü çizgileri (teknik çizim)
  const cx = 1380, cy = 560, s = 190, k = Math.cos(Math.PI / 6) * s;
  const P = { t: [cx, cy - s], l: [cx - k, cy - s / 2], r: [cx + k, cy - s / 2], c: [cx, cy], bl: [cx - k, cy + s / 2], br: [cx + k, cy + s / 2], b: [cx, cy + s] };
  const L = (a, b) => `M${f1(P[a][0])} ${f1(P[a][1])}L${f1(P[b][0])} ${f1(P[b][1])}`;
  const cube = L("t", "l") + L("t", "r") + L("l", "c") + L("r", "c") + L("c", "b") + L("l", "bl") + L("r", "br") + L("bl", "b") + L("br", "b");
  const hidden = `M${f1(cx)} ${f1(cy - s)}V${f1(cy)}`;
  const dim = `M${f1(cx + k + 40)} ${f1(cy - s / 2)}V${f1(cy + s / 2)}M${f1(cx + k + 28)} ${f1(cy - s / 2)}h24M${f1(cx + k + 28)} ${f1(cy + s / 2)}h24`;
  const iso = `<path d="${cube}" stroke="${c.ink}" stroke-width="2.5" fill="none" opacity="${dark ? .32 : .35}" stroke-linejoin="round"/>` +
    `<path d="${hidden}" stroke="${c.ink}" stroke-width="2" stroke-dasharray="10 8" opacity="${dark ? .2 : .22}"/>` +
    `<path d="${dim}" stroke="${c.acc}" stroke-width="2.5" opacity=".85"/>`;
  // Alt kenarda cetvel (kesim altlığı), sağ üstte orijin artısı ve ince yay
  let ticks = "";
  for (let x = 0; x <= W; x += 16) { const m = x % 160 === 0, h = m ? 26 : x % 80 === 0 ? 16 : 9; ticks += `M${x} ${H - 40}v${-h}`; }
  const body = `<rect width="${W}" height="${H}" fill="url(#bg)"/><rect width="${W}" height="${H}" fill="url(#g)"/>` +
    `<rect width="${W}" height="${H}" fill="url(#o)"/><rect width="${W}" height="${H}" fill="url(#b)"/>` +
    `<rect width="${W}" height="${H}" fill="url(#grid)"/><rect width="${W}" height="${H}" fill="url(#dots)"/>` + iso +
    `<path d="${ticks}" stroke="${c.ink}" stroke-width="1.5" opacity="${dark ? .22 : .3}"/>` +
    `<path d="M0 ${H - 40}H${W}" stroke="${c.ink}" stroke-width="1.5" opacity="${dark ? .22 : .3}"/>` +
    `<g stroke="${c.acc}" fill="none" stroke-width="2.5" opacity=".9"><circle cx="560" cy="260" r="16"/><path d="M560 214v92M514 260h92"/></g>` +
    `<path d="M300 ${H - 40}v-170M300 ${H - 210}h420" stroke="${c.acc}" stroke-width="2.5" opacity=".5" fill="none"/>` +
    `<circle cx="300" cy="${H - 210}" r="6" fill="${c.acc}" opacity=".8"/><circle cx="720" cy="${H - 210}" r="6" fill="${c.acc}" opacity=".8"/>`;
  return { defs, body };
}

// ---------------------------------------------------------------- Konya HackerSpace (mürekkep + lime, KHS yıldızı)
const KHS_STAR = "M50 5 63.18 18.18 81.82 18.18 81.82 36.82 95 50 81.82 63.18 81.82 81.82 63.18 81.82 50 95 36.82 81.82 18.18 81.82 18.18 63.18 5 50 18.18 36.82 18.18 18.18 36.82 18.18Z";
function khs(dark) {
  const c = dark
    ? { bg: ["#0f1322", "#070912"], lime: "#c8f53c", ink: "#0c0f1a", dim: "#c8f53c", txt: "#c8f53c" }
    : { bg: ["#f3f6ea", "#dfe6cf"], lime: "#7aa300", ink: "#0c0f1a", dim: "#0c0f1a", txt: "#3d5200" };
  const mark = (x, y, s, sw, op) => `<g transform="translate(${x} ${y}) scale(${s})" fill="none" stroke="${c.lime}" stroke-width="${sw}" stroke-linejoin="miter" opacity="${op}"><path d="${KHS_STAR}"/><path d="M33.8 36.5 47.3 50 33.8 63.5"/><path d="M52.12 62.73H68.03"/></g>`;
  const lines = ["guest@khs:~$ ./join.sh", "» Konya HackerSpace", "» create awesome stuff", "» donanım · yazılım · maker", "guest@khs:~$ _"];
  const text = lines.map((l, i) => `<text x="150" y="${360 + i * 52}" font-family="DejaVu Sans Mono, JetBrains Mono, monospace" font-size="30" fill="${c.txt}" opacity="${i === 1 ? .95 : .55}"${i === 1 ? ' font-weight="700"' : ""}>${l.replace(/&/g, "&amp;").replace(/</g, "&lt;")}</text>`).join("");
  const defs = lin("bg", ...c.bg, .4, 1) + rad("gl", c.lime, 1380, 520, 640, dark ? .12 : .1) +
    `<pattern id="grid" width="48" height="48" patternUnits="userSpaceOnUse"><path d="M48 0H0V48" fill="none" stroke="${c.dim}" stroke-width="1" opacity="${dark ? .06 : .07}"/></pattern>` +
    `<pattern id="scan" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="4" height="1" fill="${c.dim}" opacity="${dark ? .03 : .025}"/></pattern>`;
  const body = `<rect width="${W}" height="${H}" fill="url(#bg)"/><rect width="${W}" height="${H}" fill="url(#grid)"/><rect width="${W}" height="${H}" fill="url(#gl)"/>` +
    mark(1060, 200, 6.4, 3.2, dark ? .95 : .9) + mark(1640, 80, 1.6, 4, .25) + mark(120, 860, 1.2, 4, .2) + text +
    `<rect width="${W}" height="${H}" fill="url(#scan)"/>`;
  return { defs, body };
}

// ---------------------------------------------------------------- Donanım (PCB)
function hardware(dark) {
  const c = dark
    ? { bg: ["#0d3f2d", "#05211a"], cu: "#d39a4a", silk: "#f4f1e6", hole: "#05211a", mask: "#0f4a35" }
    : { bg: ["#e3f1e8", "#c3dccd"], cu: "#b87a2c", silk: "#14382a", hole: "#c3dccd", mask: "#d3e8da" };
  const r = rng(42), G = 40;
  let tr = "", vias = "";
  for (let i = 0; i < 17; i++) {
    let x = Math.floor(r() * 4) * G - G, y = (3 + Math.floor(r() * 22)) * G;
    let d = `M${x} ${y}`, steps = 4 + Math.floor(r() * 6);
    for (let s = 0; s < steps; s++) {
      const run = (2 + Math.floor(r() * 8)) * G; x += run; d += `H${x}`;
      if (r() < .7) { const dy = (r() < .5 ? -1 : 1) * (1 + Math.floor(r() * 3)) * G; x += Math.abs(dy); y += dy; d += `L${x} ${y}`; }
      if (x > W + 80) break;
    }
    tr += `<path d="${d}"/>`;
    if (x < W) vias += `<circle cx="${x}" cy="${y}" r="9"/>`;
  }
  // IC ayak izleri (silkscreen + pad dizileri)
  const ic = (x, y, w, h, n) => {
    let p = `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="6" fill="none" stroke="${c.silk}" stroke-width="3" opacity=".35"/><circle cx="${x + 18}" cy="${y + 18}" r="5" fill="${c.silk}" opacity=".35"/>`;
    const step = (w - 40) / (n - 1);
    for (let k = 0; k < n; k++) { const px = f1(x + 20 + k * step); p += `<rect x="${px - 7}" y="${y - 26}" width="14" height="22" rx="3" fill="${c.cu}"/><rect x="${px - 7}" y="${y + h + 4}" width="14" height="22" rx="3" fill="${c.cu}"/>`; }
    return p;
  };
  const hole = (x, y) => `<circle cx="${x}" cy="${y}" r="30" fill="none" stroke="${c.cu}" stroke-width="12"/><circle cx="${x}" cy="${y}" r="16" fill="${c.hole}"/>`;
  const defs = lin("bg", ...c.bg, .3, 1) + rad("sh", "#ffffff", 600, 200, 1100, dark ? .07 : .35);
  const body = `<rect width="${W}" height="${H}" fill="url(#bg)"/><rect width="${W}" height="${H}" fill="url(#sh)"/>` +
    `<g fill="none" stroke="${c.cu}" stroke-width="7" stroke-linecap="round" stroke-linejoin="round" opacity="${dark ? .55 : .6}">${tr}</g>` +
    `<g fill="${c.cu}" opacity=".8">${vias}</g>` +
    ic(1180, 360, 360, 220, 9) + ic(420, 680, 220, 140, 5) +
    hole(80, 80) + hole(W - 80, 80) + hole(80, H - 80) + hole(W - 80, H - 80);
  return { defs, body };
}

// ---------------------------------------------------------------- Siber Güvenlik (ağ grafiği + altıgen)
function cyber(dark) {
  const c = dark
    ? { bg: ["#070a13", "#0c1328"], hex: "#38d6ff", edge: "#38d6ff", node: "#8bf0ff", alt: "#a46bff" }
    : { bg: ["#eef3fa", "#d3deee"], hex: "#1d5aa8", edge: "#1d5aa8", node: "#0b3b7a", alt: "#6a3fd1" };
  const r = rng(99), pts = [];
  for (let i = 0; i < 34; i++) pts.push([f1(120 + r() * (W - 240)), f1(110 + r() * (H - 220))]);
  let edges = "";
  pts.forEach((p, i) => {
    const near = pts.map((q, j) => [j, (q[0] - p[0]) ** 2 + (q[1] - p[1]) ** 2]).filter(([j]) => j !== i).sort((a, b) => a[1] - b[1]).slice(0, 2);
    for (const [j] of near) if (j > i) edges += `M${p[0]} ${p[1]}L${pts[j][0]} ${pts[j][1]}`;
  });
  const nodes = pts.map(([x, y], i) => `<circle cx="${x}" cy="${y}" r="${i % 7 === 0 ? 9 : 5}" fill="${i % 7 === 0 ? c.alt : c.node}" opacity="${i % 7 === 0 ? .9 : .7}"/>`).join("");
  const hw = 56, hh = 97;
  const defs = lin("bg", ...c.bg, .5, 1) + rad("gl", c.edge, 1380, 420, 760, dark ? .14 : .12) +
    `<pattern id="hex" width="${hw * 3}" height="${hh}" patternUnits="userSpaceOnUse"><path d="M0 ${hh / 2}l${hw / 2} -${hh / 2}h${hw}l${hw / 2} ${hh / 2}l-${hw / 2} ${hh / 2}h-${hw}zM${hw * 2} ${hh / 2}h${hw}" fill="none" stroke="${c.hex}" stroke-width="1.5" opacity="${dark ? .07 : .1}"/></pattern>`;
  const body = `<rect width="${W}" height="${H}" fill="url(#bg)"/><rect width="${W}" height="${H}" fill="url(#hex)"/><rect width="${W}" height="${H}" fill="url(#gl)"/>` +
    `<path d="${edges}" stroke="${c.edge}" stroke-width="1.6" opacity="${dark ? .3 : .35}"/>${nodes}` +
    // büyük kilit simgesi (yalın çizgi)
    `<g fill="none" stroke="${c.edge}" stroke-width="14" stroke-linecap="round" opacity="${dark ? .5 : .45}"><rect x="1300" y="470" width="200" height="160" rx="26"/><path d="M1345 470v-50a55 55 0 0 1 110 0v50"/></g>` +
    `<circle cx="1400" cy="540" r="16" fill="${c.edge}" opacity="${dark ? .6 : .5}"/><path d="M1400 556v34" stroke="${c.edge}" stroke-width="12" stroke-linecap="round" opacity="${dark ? .6 : .5}"/>`;
  return { defs, body };
}

// ---------------------------------------------------------------- 3D Baskı (katman çizgileri + baskı tablası)
function print3d(dark) {
  const c = dark
    ? { bg: ["#22262d", "#111317"], grid: "#ffffff", fa: "#ff8a3d", fb: "#ffd166", plate: "#2e333b" }
    : { bg: ["#f1f2f4", "#d6dae0"], grid: "#20242a", fa: "#e8651a", fb: "#f2a900", plate: "#c9ced6" };
  // Perspektif tabla ızgarası
  const vx = W / 2, vy = 380, base = H + 40;
  let grid = "";
  for (let i = -14; i <= 14; i++) grid += `M${vx + i * 14} ${vy + 260}L${vx + i * 170} ${base}`;
  for (let k = 0; k < 9; k++) { const t = (k / 8) ** 1.8, y = f1(vy + 260 + t * (base - vy - 260)), half = 14 * 14 + t * (170 * 14 - 14 * 14); grid += `M${f1(vx - half)} ${y}H${f1(vx + half)}`; }
  // Dilimlenmiş vazo: her katman bir elips çizgisi
  let layers = "";
  const n = 46, top = 300, bottom = 760;
  for (let i = 0; i < n; i++) {
    const t = i / (n - 1), y = f1(bottom - t * (bottom - top));
    const rx = f1(150 + 70 * Math.sin(t * Math.PI * 1.15 + .3) + 40 * (1 - t));
    layers += `<ellipse cx="${vx}" cy="${y}" rx="${rx}" ry="${f1(rx * .16)}" stroke-opacity="${f1(.35 + .6 * t)}"/>`;
  }
  const defs = lin("bg", ...c.bg) + lin("fil", c.fb, c.fa) + rad("gl", c.fa, vx, 560, 520, dark ? .16 : .12);
  const body = `<rect width="${W}" height="${H}" fill="url(#bg)"/><rect width="${W}" height="${H}" fill="url(#gl)"/>` +
    `<path d="${grid}" stroke="${c.grid}" stroke-width="1.5" opacity="${dark ? .1 : .14}"/>` +
    `<g fill="none" stroke="url(#fil)" stroke-width="5">${layers}</g>` +
    // nozul
    `<path d="M${vx - 46} 150h92v70l-30 40h-32l-30-40z" fill="${c.grid}" opacity="${dark ? .18 : .2}"/><path d="M${vx - 10} 260h20l-10 22z" fill="${c.fa}"/>`;
  return { defs, body };
}

// KHS teması bir eklentidir (makerd/assets/plugin-defaults/khs) ve kendi duvar kâğıtlarını taşır; burada üretilmez.
const THEMES = { maker, hardware, cyber, print3d };
let Resvg = null;
try { ({ Resvg } = await import("@resvg/resvg-js")); } catch { console.warn("@resvg/resvg-js yok: bulanık PNG'ler üretilmedi"); }
for (const [id, fn] of Object.entries(THEMES)) for (const dark of [false, true]) {
  const { defs, body } = fn(dark);
  const name = `${id}-${dark ? "dark" : "light"}`;
  writeFileSync(join(dir, `${name}.svg`), `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid slice"><defs>${defs}</defs>${body}</svg>\n`);
  if (!Resvg) continue;
  // Bulanık: tam boyutta çizip bulanıklaştır, 480 px genişliğe küçült (CSS büyütünce zaten yumuşak).
  const f = `<filter id="blur" x="-5%" y="-5%" width="110%" height="110%"><feGaussianBlur stdDeviation="${dark ? 40 : 46}"/></filter>`;
  // Kenarlar bulanıklıkta saydamlaşıp kararmasın: zemin düz renk + bulanık katman %16 büyütülür (kenarlar dışarıda kalır).
  const k = 1.16, tx = -(W * (k - 1)) / 2, ty = -(H * (k - 1)) / 2;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}"><defs>${defs}${f}</defs><rect width="${W}" height="${H}" fill="url(#bg)"/>` +
    `<g transform="translate(${tx} ${ty}) scale(${k})"><g filter="url(#blur)">${body}</g></g></svg>`;
  writeFileSync(join(dir, `${name}-blur.png`), new Resvg(svg, { fitTo: { mode: "width", value: 480 } }).render().asPng());
}
console.log("duvar kâğıtları yazıldı:", Object.keys(THEMES).join(", "));
