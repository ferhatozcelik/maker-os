#!/usr/bin/env python3
"""results/*.json → README.md (sıralama, Mermaid grafikleri, analiz) ve docs/ (GitHub Pages: etkileşimli grafikler).

    python3 tools/render.py           # üret
    python3 tools/render.py --check   # yalnız doğrula (CI, PR'lar)

GitHub Actions her push'ta çalıştırır; yeni bir sonuç eklemek için results/ klasörüne .json koymak yeter.
"""
import argparse
import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RES = ROOT / "results"
CAT = {"cpu": "CPU tek çekirdek", "multi": "CPU çok çekirdek", "mem": "Bellek", "disk": "Disk"}
CAT_EN = {"cpu": "CPU single-core", "multi": "CPU multi-core", "mem": "Memory", "disk": "Disk"}
START, END = "<!-- BENCH:START -->", "<!-- BENCH:END -->"
# Liste şimdilik Raspberry Pi OS ve MakerOS içindir (betik her sistemde çalışır; puan referansı M1 Pro · macOS).
ALLOWED = "Raspberry Pi OS ya da MakerOS"


def allowed(d):
    s = d.get("system") or {}
    return s.get("os_id") in ("makeros", "raspbian") or str(s.get("model", "")).startswith("Raspberry Pi")


def fail(msg):
    sys.exit(f"hata: {msg}")


def load():
    out = []
    for p in sorted(RES.glob("*.json")):
        try:
            d = json.loads(p.read_text(encoding="utf-8"))
        except ValueError as e:
            fail(f"{p.name}: geçerli JSON değil ({e})")
        for k in ("schema", "version", "name", "date", "system", "results", "scores"):
            if k not in d:
                fail(f"{p.name}: '{k}' alanı yok (MakerOS Bench çıktısı mı?)")
        if d["schema"] != 1:
            fail(f"{p.name}: desteklenmeyen şema {d['schema']}")
        sc = d["scores"]
        if not isinstance(sc.get("total"), int) or not isinstance(sc.get("categories"), dict):
            fail(f"{p.name}: scores alanı bozuk")
        if len(d["name"]) > 80 or re.search(r"[<>|`\n]", d["name"]):
            fail(f"{p.name}: name en fazla 80 karakter; < > | ` içeremez")
        if not allowed(d):
            fail(f"{p.name}: liste şimdilik yalnız {ALLOWED} sonuçlarını kabul eder ({d['system'].get('os', '?')})")
        d["_file"] = p.name
        d["_md"] = p.with_suffix(".md").name if p.with_suffix(".md").exists() else ""
        out.append(d)
    out.sort(key=lambda d: -d["scores"]["total"])
    return out


def short(d, n=28):
    s = d["name"]
    return s if len(s) <= n else s[: n - 1] + "…"


def mermaid_bar(title, labels, values, ymax=None):
    ymax = ymax or max(values + [1])
    ymax = int(math.ceil(ymax * 1.15 / 100.0) * 100)
    lab = ", ".join(json.dumps(x, ensure_ascii=False) for x in labels)
    init = '%%{init: {"themeVariables": {"xyChart": {"plotColorPalette": "#f07a1a"}}}}%%'  # MakerOS turuncusu
    return ["```mermaid", init, "xychart-beta", f'  title "{title}"', f"  x-axis [{lab}]", f'  y-axis "Puan" 0 --> {ymax}',
            f"  bar [{', '.join(str(v) for v in values)}]", "```"]


def analysis(rows):
    if len(rows) < 2:
        return ["Henüz tek sonuç var. Diğer sistemde (Raspberry Pi OS ya da MakerOS) çalıştırıp `results/` klasörüne",
                "eklediğinizde bu bölüm iki sistemi kendiliğinden karşılaştırır."]
    lines = []
    top = rows[0]
    lines.append(f"- **En hızlı:** {top['name']} — genel {top['scores']['total']:,} puan.")
    for d in rows[1:]:
        r = top["scores"]["total"] / max(1, d["scores"]["total"])
        lines.append(f"- {d['name']}: {d['scores']['total']:,} puan → en hızlının **{1 / r:.0%}**'i ({r:.1f}× daha yavaş).")
    lines.append("")
    lines.append("**Kategori liderleri**")
    lines.append("")
    for c, cname in CAT.items():
        have = [d for d in rows if c in d["scores"]["categories"]]
        if not have:
            continue
        best = max(have, key=lambda d: d["scores"]["categories"][c])
        worst = min(have, key=lambda d: d["scores"]["categories"][c])
        gap = best["scores"]["categories"][c] / max(1, worst["scores"]["categories"][c])
        lines.append(f"- {cname}: **{best['name']}** ({best['scores']['categories'][c]:,})"
                     + (f"; en düşükle fark {gap:.1f}×" if len(have) > 1 else ""))
    lines.append("")
    lines.append("**Sistem başına güçlü ve zayıf yön** (kategori puanlarının kendi genel puanına oranı)")
    lines.append("")
    for d in rows:
        cats = d["scores"]["categories"]
        if len(cats) < 2:
            continue
        tot = max(1, d["scores"]["total"])
        strong = max(cats, key=lambda c: cats[c])
        weak = min(cats, key=lambda c: cats[c])
        note = ""
        th = (d.get("thermal") or {}).get("after") or {}
        if th.get("throttled") not in (None, "0x0"):
            note = " ⚠ test sırasında kısma (throttling) oldu"
        sc = d["results"].get("multi.int", 0) / max(1e-9, d["results"].get("cpu.int", 0))
        lines.append(f"- {d['name']}: güçlü **{CAT[strong]}** ({cats[strong] / tot:.1f}×), zayıf **{CAT[weak]}** "
                     f"({cats[weak] / tot:.1f}×); çok çekirdek ölçeklenmesi {sc:.1f}× / {d['system'].get('cores')} çekirdek{note}")
    return lines


def readme_block(rows):
    L = [START, "", f"_{len(rows)} sonuç · `tools/render.py` üretir, elle düzenlemeyin._", ""]
    if not rows:
        return "\n".join(L + [f"Henüz sonuç yok. İlk sonuçlar {ALLOWED} üzerinde alınacak.", "", END])
    L += ["## Sıralama", "", "| # | Sistem | İşletim sistemi | Genel | CPU 1 | CPU çok | Bellek | Disk | Rapor |",
          "|---:|---|---|---:|---:|---:|---:|---:|---|"]
    for i, d in enumerate(rows, 1):
        c = d["scores"]["categories"]
        g = lambda k: f"{c[k]:,}" if k in c else "–"
        rep = f"[rapor](results/{d['_md']})" if d["_md"] else ""
        L.append(f"| {i} | {d['name']} | {d['system'].get('os', '?')} | **{d['scores']['total']:,}** | {g('cpu')} | {g('multi')} | {g('mem')} | {g('disk')} | {rep} |")
    L += ["", "Referans (Apple M1 Pro · macOS · Python 3.9) = 1000 puan. Yüksek = iyi. "
          "Etkileşimli grafikler: [GitHub Pages](https://ferhatozcelik.github.io/maker-os/benchmark/).", "", "## Grafikler", ""]
    labels = [short(d) for d in rows]
    L += mermaid_bar("Genel puan", labels, [d["scores"]["total"] for d in rows]) + [""]
    for c, cname in CAT.items():
        vals = [d["scores"]["categories"].get(c, 0) for d in rows]
        if any(vals):
            L += mermaid_bar(cname, labels, vals) + [""]
    L += ["## Analiz", ""] + analysis(rows) + ["", END]
    return "\n".join(L)


def site_data(rows):
    keep = ("name", "date", "version", "mode", "duration_s", "system", "thermal", "results", "scores")
    return [{**{k: d.get(k) for k in keep}, "file": d["_file"], "report": d["_md"]} for d in rows]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    rows = load()
    print(f"{len(rows)} sonuç doğrulandı")
    if a.check:
        return
    readme = ROOT / "README.md"
    text = readme.read_text(encoding="utf-8")
    if START not in text or END not in text:
        fail("README.md içinde BENCH:START/END işaretleri yok")
    i, j = text.index(START), text.index(END) + len(END)
    readme.write_text(text[:i] + readme_block(rows) + text[j:], encoding="utf-8")
    docs = ROOT / "docs"
    docs.mkdir(exist_ok=True)
    (docs / "data.json").write_text(json.dumps(site_data(rows), ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    for d in rows:  # raporlar Pages'te de açılsın
        if d["_md"]:
            (docs / "results").mkdir(exist_ok=True)
            (docs / "results" / d["_md"]).write_text((RES / d["_md"]).read_text(encoding="utf-8"), encoding="utf-8")
    print("README.md ve docs/data.json güncellendi")


if __name__ == "__main__":
    main()
