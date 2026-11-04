// Lucide SVG alt kümesini (vendor/lucide/svg/*.svg) tek bir ES modülüne çevirir:
//   node shell/tools/gen-icons.mjs  →  shell/js/icon-set.js
// (vendor/ dışında: simge eklendikçe değişir; vendor/ dosyaları uzun süre önbelleklenebilir.)
// Yeni simge: lucide-static paketinden svg'yi vendor/lucide/svg/ altına kopyalayıp betiği çalıştırın.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const dir = join(dirname(fileURLToPath(import.meta.url)), "../vendor/lucide");
const out = {};
for (const f of readdirSync(join(dir, "svg")).filter(f => f.endsWith(".svg")).sort()) {
  const src = readFileSync(join(dir, "svg", f), "utf8");
  const body = src.replace(/<!--[\s\S]*?-->/g, "").replace(/^[\s\S]*?<svg[^>]*>/, "").replace(/<\/svg>[\s\S]*$/, "")
    .replace(/\s*\n\s*/g, "").replace(/\s+\/>/g, "/>");
  out[f.slice(0, -4)] = body;
}
writeFileSync(join(dir, "../../js/icon-set.js"), "// Üretildi: tools/gen-icons.mjs — Lucide (ISC), bkz. vendor/lucide/LICENSE\nexport default " + JSON.stringify(out, null, 0).replace(/","/g, '",\n"') + ";\n");
console.log(Object.keys(out).length, "simge yazıldı");
