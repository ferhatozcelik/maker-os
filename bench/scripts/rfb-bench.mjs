#!/usr/bin/env node
// Bagimliliksiz RFB istemcisi (yalniz Docker'da): VNC sunucusuna baglanir (None),
// Tight + JPEG kodlamalarini ister ve olcer:
//   - ilk tam cerceve suresi ve boyutu
//   - DURATION sn boyunca ust uste tam cerceve istendiginde FPS ve bant genisligi
// Cerceve icerigi cozulmez; yalniz bayt sayilir (sunucu kodlama maliyeti olculur).
//   node rfb-bench.mjs host port [saniye] [jpegKalite 0-9]
import net from "node:net";
const [host, port = "5900", dur = "10", q = "6"] = process.argv.slice(2);
const sock = net.connect(+port, host); sock.setNoDelay(true);
let buf = Buffer.alloc(0), waiters = [];
sock.on("data", d => { buf = Buffer.concat([buf, d]); pump(); });
sock.on("error", e => { console.error(JSON.stringify({ error: e.message })); process.exit(2); });
function pump() { while (waiters.length && buf.length >= waiters[0].n) { const w = waiters.shift(); const b = buf.subarray(0, w.n); buf = buf.subarray(w.n); w.r(b); } }
const read = n => new Promise(r => { waiters.push({ n, r }); pump(); });
let bytes = 0; const rd = async n => { bytes += n; return read(n); };
const ver = (await rd(12)).toString();
sock.write("RFB 003.008\n");
const nt = (await rd(1))[0], types = [...await rd(nt)];
if (!types.includes(1)) { console.error(JSON.stringify({ error: "None guvenlik yok", types })); process.exit(2); }
sock.write(Buffer.from([1]));
if ((await rd(4)).readUInt32BE(0) !== 0) { console.error("auth fail"); process.exit(2); }
sock.write(Buffer.from([1])); // paylasimli
const si = await rd(24); const W = si.readUInt16BE(0), H = si.readUInt16BE(2);
await rd(si.readUInt32BE(20));
// Kodlamalar: Tight(7), ZRLE(16), Raw(0), JPEG kalite (-32+q), sikistirma 2 (-256+2), DesktopSize(-223)
const enc = [7, 16, 0, -32 + +q, -256 + 2, -223];
const se = Buffer.alloc(4 + enc.length * 4); se[0] = 2; se.writeUInt16BE(enc.length, 2); enc.forEach((e, i) => se.writeInt32BE(e, 4 + i * 4)); sock.write(se);
const req = inc => { const b = Buffer.alloc(10); b[0] = 3; b[1] = inc; b.writeUInt16BE(W, 6); b.writeUInt16BE(H, 8); sock.write(b); };
// Tight dikdortgen govdesini atla (yalniz uzunluk alanlarini cozumle).
async function compactLen() { let len = 0; for (let i = 0; i < 3; i++) { const b = (await rd(1))[0]; len |= (b & (i < 2 ? 0x7f : 0xff)) << (7 * i); if (i < 2 && !(b & 0x80)) break; } return len; }
async function skipRect(x, y, w, h, e) {
  if (e === 0) return rd(w * h * 4);
  if (e === -223) return;
  if (e === 16) { const l = (await rd(4)).readUInt32BE(0); return rd(l); }
  if (e === 7) {
    const c = (await rd(1))[0], ctl = c >> 4;
    if (ctl === 8) return rd(3);                       // fill (TPIXEL 3 bayt)
    if (ctl === 9) return rd(await compactLen());      // JPEG
    let pal = 0, bpp = 3;
    if (c & 0x40) { const f = (await rd(1))[0]; if (f === 1) { pal = (await rd(1))[0] + 1; await rd(pal * 3); bpp = 0; } else if (f === 2) { bpp = 3; } }
    const rowBytes = pal ? (pal <= 2 ? Math.ceil(w / 8) : w) : w * 3, raw = rowBytes * h;
    if (raw < 12) return rd(raw);
    return rd(await compactLen());
  }
  throw new Error("beklenmeyen kodlama " + e);
}
async function frame() {
  const hd = await rd(4); if (hd[0] !== 0) { if (hd[0] === 2) return frame(); throw new Error("mesaj " + hd[0]); }
  const n = hd.readUInt16BE(2);
  for (let i = 0; i < n; i++) { const r = await rd(12); await skipRect(r.readUInt16BE(0), r.readUInt16BE(2), r.readUInt16BE(4), r.readUInt16BE(6), r.readInt32BE(8)); }
}
let t = performance.now(); bytes = 0; req(0); await frame();
const first = { ms: +(performance.now() - t).toFixed(1), bytes };
let frames = 0; bytes = 0; t = performance.now(); const end = t + +dur * 1000;
while (performance.now() < end) { req(0); await frame(); frames++; }
const el = (performance.now() - t) / 1000;
console.log(JSON.stringify({ server: ver.trim(), w: W, h: H, first_frame: first, full_fps: +(frames / el).toFixed(1), kbps: Math.round(bytes * 8 / el / 1000), avg_frame_kb: +(bytes / frames / 1024).toFixed(1) }));
sock.destroy();
