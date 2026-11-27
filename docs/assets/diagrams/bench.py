#!/usr/bin/env python3
"""README başarım grafiklerini üretir: python3 docs/assets/diagrams/bench.py

Veri: benchmark/boards.json (Geekbench 5, Linux). Her grafik Türkçe (<ad>.svg) ve İngilizce (<ad>.en.svg) yazılır:
  bench-same    aynı Pi 4, aynı 1.5 GHz: MakerOS (önce / sonra) ve diğer Linux dağıtımları
  bench-boards  kartlar arası: MakerOS Pi 4 = 1× kabul edilerek tek ve çok çekirdek kat farkı
"""
import json
from build import ACCENT, INK, MUTED, LINE, OUT, text, frame

DATA = json.loads((OUT.parents[2] / "benchmark" / "boards.json").read_text(encoding="utf-8"))
ME = DATA["makeros"]
SAME_C, OLD_C = "#fbd3ae", "#e3d6c6"
GRAY_D, GRAY_L = "#8d8981", "#d9d4cc"
ME_L = "#f8b77a"


def num(v, lang, dec=0):
    s = f"{v:,.{dec}f}"
    return s.replace(",", "§").replace(".", ",").replace("§", ".") if lang == "tr" else s


def pct(a, b, lang):
    s = f"{(a / b - 1) * 100:+.1f}%".replace("-", "−")
    return s.replace(".", ",") if lang == "tr" else s


T = {
    "tr": {
        "same_t": "Aynı Pi 4, aynı 1.5 GHz", "same_n": "Geekbench 5 · uzun olan daha iyi · fark: diğer Linux ortancasına göre",
        "single": "Tek çekirdek", "multi": "Çok çekirdek",
        "before": "MakerOS · 26 Kasım", "before2": "HA kapsayıcısı bir çekirdeği meşgul",
        "other": "Diğer Linux", "other2": "ortanca · 25 sonuç",
        "after": "MakerOS · 27 Kasım", "after2": "düzeltme sonrası · 2 ölçüm",
        "boards_t": "Pi 4 nerede duruyor?", "boards_n": "MakerOS Pi 4 = 1×  ·  Geekbench 5 ortancaları  ·  çok çekirdeğe göre sıralı",
        "leg": [(GRAY_D, "Tek çekirdek"), (GRAY_L, "Çok çekirdek"), (ACCENT, "MakerOS")],
    },
    "en": {
        "same_t": "Same Pi 4, same 1.5 GHz", "same_n": "Geekbench 5 · longer is better · delta vs. other Linux median",
        "single": "Single-core", "multi": "Multi-core",
        "before": "MakerOS · 26 Nov", "before2": "HA container kept a core busy",
        "other": "Other Linux", "other2": "median · 25 results",
        "after": "MakerOS · 27 Nov", "after2": "after the fix · 2 runs",
        "boards_t": "Where does the Pi 4 stand?", "boards_n": "MakerOS Pi 4 = 1×  ·  Geekbench 5 medians  ·  sorted by multi-core",
        "leg": [(GRAY_D, "Single-core"), (GRAY_L, "Multi-core"), (ACCENT, "MakerOS")],
    },
}


def same_chart(lang, w=1100):
    t = T[lang]
    other = next(b for b in DATA["boards"] if b.get("same_hw"))
    rows = [
        (t["before"], t["before2"], ME["before"]["single"], ME["before"]["multi"], OLD_C, False),
        (t["other"], t["other2"], other["single"][1], other["multi"][1], SAME_C, None),
        (t["after"], t["after2"], ME["single"], ME["multi"], ACCENT, True),
    ]
    pad, gap, label_w, row_h = 28, 40, 260, 62
    pw = (w - 2 * pad - label_w - gap) / 2
    out = [text(pad, 42, t["same_t"], 22, 700, INK, "start"), text(w - pad, 42, t["same_n"], 13.5, 400, MUTED, "end")]
    top = 100
    for k, key in enumerate(("single", "multi")):
        x0 = pad + label_w + k * (pw + gap)
        out.append(text(x0, top - 18, t[key], 17, 700, INK, "start"))
        vmax = max(r[2 + k] for r in rows)
        scale = (pw - 130) / vmax
        ref = rows[1][2 + k]
        for i, (_l, _s, s, m, color, mine) in enumerate(rows):
            v = (s, m)[k]
            y = top + i * row_h
            out.append(f'<rect x="{x0}" y="{y}" width="{v * scale:.1f}" height="34" rx="6" fill="{color}"/>')
            out.append(text(x0 + v * scale + 10, y + 23, num(v, lang), 16, 700, INK, "start"))
            if mine is not None:
                col = "#1f7a3a" if v >= ref else "#b3261e"
                out.append(text(x0 + v * scale + 52, y + 23, pct(v, ref, lang), 14, 700, col, "start"))
        rx = x0 + ref * scale
        out.append(f'<line x1="{rx:.1f}" y1="{top - 6}" x2="{rx:.1f}" y2="{top + 3 * row_h - 22}" stroke="{INK}" stroke-opacity=".35" stroke-dasharray="4 4"/>')
    for i, (lab, sub, *_r) in enumerate(rows):
        y = top + i * row_h
        out.append(text(pad, y + 15, lab, 16, 700 if rows[i][5] else 600, INK, "start"))
        out.append(text(pad, y + 34, sub, 13, 400, MUTED, "start"))
    return frame(w, top + 3 * row_h + 4, "".join(out), t["same_t"])


def boards_chart(lang, w=1100):
    t = T[lang]
    items = [(b.get("label_" + lang, b["label"]), b["single"][1], b["multi"][1], False) for b in DATA["boards"] if not b.get("same_hw")]
    items.append((ME["label"], ME["single"], ME["multi"], True))
    items.sort(key=lambda r: -r[2])
    pad, label_w, row_h = 28, 190, 48
    x0 = pad + label_w
    xmax = max(max(s / ME["single"], m / ME["multi"]) for _, s, m, _ in items)
    scale = (w - x0 - pad - 70) / xmax
    top = 96
    out = [text(pad, 42, t["boards_t"], 22, 700, INK, "start"), text(w - pad, 42, t["boards_n"], 13.5, 400, MUTED, "end")]
    for g in (1, 2, 3, 4):
        if g <= xmax:
            gx = x0 + g * scale
            out.append(f'<line x1="{gx:.1f}" y1="{top - 12}" x2="{gx:.1f}" y2="{top + len(items) * row_h - 8}" stroke="{INK if g == 1 else LINE}" stroke-opacity="{0.45 if g == 1 else 1}" stroke-width="1.2"{" stroke-dasharray=\"4 4\"" if g == 1 else ""}/>')
            out.append(text(gx, top - 18, f"{g}×", 13, 700 if g == 1 else 400, INK if g == 1 else MUTED))
    for i, (lab, s, m, mine) in enumerate(items):
        y = top + i * row_h
        rs, rm = s / ME["single"], m / ME["multi"]
        cs, cm = (ACCENT, ME_L) if mine else (GRAY_D, GRAY_L)
        out.append(text(x0 - 12, y + 25, lab, 15, 700 if mine else 400, INK if mine else MUTED, "end"))
        out.append(f'<rect x="{x0}" y="{y + 4}" width="{rs * scale:.1f}" height="16" rx="4" fill="{cs}"/>')
        out.append(f'<rect x="{x0}" y="{y + 22}" width="{rm * scale:.1f}" height="16" rx="4" fill="{cm}"/>')
        out.append(text(x0 + rs * scale + 8, y + 17, num(rs, lang, 2) + "×", 12.5, 600, INK, "start"))
        out.append(text(x0 + rm * scale + 8, y + 35, num(rm, lang, 2) + "×", 12.5, 600, INK, "start"))
    y = top + len(items) * row_h + 8
    out.append(f'<line x1="{pad}" y1="{y}" x2="{w - pad}" y2="{y}" stroke="{LINE}"/>')
    x = pad
    for color, name in t["leg"]:
        out.append(f'<rect x="{x}" y="{y + 14}" width="14" height="14" rx="3" fill="{color}"/>')
        out.append(text(x + 22, y + 26, name, 14, 400, MUTED, "start"))
        x += 34 + len(name) * 7.6
    return frame(w, y + 46, "".join(out), t["boards_t"])


if __name__ == "__main__":
    for name, fn in (("bench-same", same_chart), ("bench-boards", boards_chart)):
        for lang in ("tr", "en"):
            p = OUT / (f"{name}.svg" if lang == "tr" else f"{name}.en.svg")
            p.write_text(fn(lang), encoding="utf-8")
            print(p.relative_to(OUT.parents[2]))
