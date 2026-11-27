#!/usr/bin/env python3
"""README başarım grafiklerini üretir: python3 docs/assets/diagrams/bench.py

Veri: benchmark/boards.json (Geekbench 5, Linux). Türkçe bench-boards.svg ve İngilizce bench-boards.en.svg yazılır.
"""
import json
from pathlib import Path
from build import ACCENT, INK, MUTED, LINE, SOFT, OUT, text, frame

DATA = json.loads((OUT.parents[2] / "benchmark" / "boards.json").read_text(encoding="utf-8"))
OTHER, SAME = "#d9d4cc", "#fbd3ae"

T = {
    "tr": {"title": "Geekbench 5 · kartlar arası karşılaştırma", "single": "Tek çekirdek", "multi": "Çok çekirdek",
           "note": "Çubuk: ortanca · çizgi: ortadaki %50 (p25–p75) · uzun olan daha iyi",
           "leg": [(ACCENT, "MakerOS (bu ölçüm)"), (SAME, "Aynı kart, diğer Linux (1.5 GHz)"), (OTHER, "Diğer kartlar")]},
    "en": {"title": "Geekbench 5 · board comparison", "single": "Single-core", "multi": "Multi-core",
           "note": "Bar: median · line: middle 50% (p25–p75) · longer is better",
           "leg": [(ACCENT, "MakerOS (this run)"), (SAME, "Same board, other Linux (1.5 GHz)"), (OTHER, "Other boards")]},
}


def rows(lang):
    m = DATA["makeros"]
    r = [(b.get("label_" + lang, b["label"]), b["single"], b["multi"], "same" if b.get("same_hw") else "other") for b in DATA["boards"]]
    r.append((m["label"], [None, m["single"], None], [None, m["multi"], None], "me"))
    return r


def panel(x0, y0, w, title, items, key, row_h=34, label_w=190):
    vmax = max((v[2] or v[1]) for v in (it[1 if key == "single" else 2] for it in items))
    scale = (w - label_w - 70) / vmax
    out = [text(x0, y0, title, 19, 700, INK, "start")]
    items = sorted(items, key=lambda it: it[1 if key == "single" else 2][1])
    for i, (label, s, m, kind) in enumerate(items):
        lo, med, hi = s if key == "single" else m
        y = y0 + 22 + i * row_h
        bx = x0 + label_w
        fill = {"me": ACCENT, "same": SAME, "other": OTHER}[kind]
        weight = 700 if kind == "me" else 400
        out.append(text(bx - 12, y + 18, label, 14.5, weight, INK if kind != "other" else MUTED, "end"))
        out.append(f'<rect x="{bx}" y="{y + 4}" width="{med * scale:.1f}" height="{row_h - 12}" rx="5" fill="{fill}"/>')
        if lo and hi:
            out.append(f'<line x1="{bx + lo * scale:.1f}" y1="{y + row_h / 2 - 2}" x2="{bx + hi * scale:.1f}" y2="{y + row_h / 2 - 2}" stroke="{INK}" stroke-opacity=".45" stroke-width="1.6"/>')
            for v in (lo, hi):
                out.append(f'<line x1="{bx + v * scale:.1f}" y1="{y + row_h / 2 - 7}" x2="{bx + v * scale:.1f}" y2="{y + row_h / 2 + 3}" stroke="{INK}" stroke-opacity=".45" stroke-width="1.6"/>')
        end = max(med, hi or 0) * scale
        out.append(text(bx + end + 8, y + 18, str(med), 14, 700 if kind == "me" else 600, INK, "start"))
    return out, 22 + len(items) * row_h


def chart(lang, w=1100):
    t = T[lang]
    items = rows(lang)
    pad, gap = 28, 36
    pw = (w - 2 * pad - gap) / 2
    out = [text(pad, 42, t["title"], 22, 700, INK, "start"), text(w - pad, 42, t["note"], 13.5, 400, MUTED, "end")]
    a, h = panel(pad, 84, pw, t["single"], items, "single")
    b, _ = panel(pad + pw + gap, 84, pw, t["multi"], items, "multi")
    out += a + b
    y = 84 + h + 22
    out.append(f'<line x1="{pad}" y1="{y - 10}" x2="{w - pad}" y2="{y - 10}" stroke="{LINE}"/>')
    x = pad
    for color, name in t["leg"]:
        out.append(f'<rect x="{x}" y="{y + 4}" width="14" height="14" rx="3" fill="{color}"/>')
        out.append(text(x + 22, y + 16, name, 14, 400, MUTED, "start"))
        x += 30 + len(name) * 7.6
    return frame(w, y + 38, "".join(out), t["title"])


if __name__ == "__main__":
    for lang in ("tr", "en"):
        p = OUT / ("bench-boards.svg" if lang == "tr" else "bench-boards.en.svg")
        p.write_text(chart(lang), encoding="utf-8")
        print(p.relative_to(OUT.parents[2]))
