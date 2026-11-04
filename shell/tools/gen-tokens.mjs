// tokens.json → css/tokens.css   (node shell/tools/gen-tokens.mjs)
// Açık tema :root'ta; koyu tema [data-theme=dark] ve tema "otomatik" iken
// prefers-color-scheme ile. Vurgu rengi [data-accent=<ad>] ile seçilir.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const t = JSON.parse(readFileSync(join(root, "tokens.json"), "utf8"));
const vars = (o, pre = "") => Object.entries(o).map(([k, v]) => `  --${pre}${k}: ${v};`).join("\n");
const css = `/* ÜRETİLDİ — elle düzenlemeyin. Kaynak: shell/tokens.json, betik: shell/tools/gen-tokens.mjs */
:root {
${vars(t.primitive)}
${vars(t.component, "mk-")}
${vars(t.theme.light)}
  --accent: ${t.accent.orange};
  color-scheme: light;
}
:root[data-theme="dark"] {
${vars(t.theme.dark)}
  color-scheme: dark;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
${vars(t.theme.dark).replace(/^/gm, "  ")}
    color-scheme: dark;
  }
}
:root { --on-accent: #fff; }
:root[data-accent="lime"] { --on-accent: #0c0f1a; }
${Object.entries(t.accent).map(([k, v]) => `:root[data-accent="${k}"] { --accent: ${v}; }`).join("\n")}
`;
writeFileSync(join(root, "css/tokens.css"), css);
console.log("css/tokens.css yazıldı", css.length, "bayt");
