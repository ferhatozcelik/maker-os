// Kullanıcı duvar kâğıdı: resmi tarayıcıda küçültüp (en fazla 2560 px, JPEG) makerd'ye yükler.
// Masaüstü için bulanık bir kopya da üretilir (--wall-blur: kilit ekranı, "lite" efekt katmanı, Launchpad).
// makerd: PUT/DELETE /api/v1/wallpaper/{desktop,desktop-blur,lock}; GET herkese açık (kilit ekranı girişten önce).
import { BASE, ApiError } from "./api.js";

function pick() {
  return new Promise(res => {
    const i = Object.assign(document.createElement("input"), { type: "file", accept: "image/jpeg,image/png,image/webp" });
    i.onchange = () => res(i.files?.[0] || null);
    i.click();
  });
}
async function bitmap(file) {
  try { return await createImageBitmap(file); } catch { throw new Error("Resim okunamadı (JPEG, PNG ya da WebP seçin)."); }
}
function draw(bmp, maxW, blur = 0) {
  const k = Math.min(1, maxW / bmp.width), w = Math.round(bmp.width * k), h = Math.round(bmp.height * k);
  const c = Object.assign(document.createElement("canvas"), { width: w, height: h });
  const g = c.getContext("2d");
  if (blur) { g.filter = `blur(${blur}px)`; g.drawImage(bmp, -blur * 2, -blur * 2, w + blur * 4, h + blur * 4); } else g.drawImage(bmp, 0, 0, w, h);
  return new Promise(r => c.toBlob(r, "image/jpeg", blur ? 0.8 : 0.88));
}
async function upload(name, blob) {
  const r = await fetch(`${BASE}/wallpaper/${name}`, { method: "PUT", headers: { "X-Maker": "1", "Content-Type": "image/jpeg" }, body: blob, credentials: "same-origin" });
  if (!r.ok) { let m = r.statusText; try { m = (await r.json()).error?.message || m; } catch { /* yok */ } throw new ApiError(r.status, "upload", m); }
}
export async function removeWallpaper(name) {
  await fetch(`${BASE}/wallpaper/${name}`, { method: "DELETE", headers: { "X-Maker": "1" }, credentials: "same-origin" });
}
/** Dosya seçtirir ve yükler. kind: "desktop" | "lock". Seçilmezse false. */
export async function chooseWallpaper(kind = "desktop") {
  const f = await pick();
  if (!f) return false;
  const bmp = await bitmap(f);
  const blob = await draw(bmp, 2560);
  await upload(kind, blob);
  if (kind === "desktop") {
    await removeWallpaper("lock").catch(() => {});
  }
  return true;
}
