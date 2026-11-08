#!/usr/bin/env python3
"""README diyagramlarını (yatay, sabit SVG) üretir: python3 docs/assets/diagrams/build.py

Her diyagram Türkçe (<ad>.svg) ve İngilizce (<ad>.en.svg) olarak yazılır. Metni değiştirmek için
aşağıdaki DIAGRAMS sözlüğünü düzenleyip betiği yeniden çalıştırın.
"""
from pathlib import Path
from html import escape

OUT = Path(__file__).resolve().parent
ACCENT, INK, MUTED, LINE, SOFT, CARD = "#f07a1a", "#17181b", "#5d6068", "#e2ddd5", "#fff4ea", "#ffffff"
FONT = "-apple-system, 'Segoe UI', Inter, Roboto, 'Helvetica Neue', Arial, sans-serif"
MONO = "ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace"


def text(x, y, s, size, weight=400, color=INK, anchor="middle", mono=False):
    fam = MONO if mono else FONT
    return f'<text x="{x}" y="{y}" font-family="{fam}" font-size="{size}" font-weight="{weight}" fill="{color}" text-anchor="{anchor}">{escape(s)}</text>'


def fit(s, width, size):
    """Yaklaşık metin genişliği kutuya sığmıyorsa uyarır (ortalama glif ≈ 0,54 em)."""
    if len(s) * size * 0.54 > width:
        print(f"UYARI: sığmayabilir ({len(s)} karakter, {width:.0f}px): {s}")


def arrow(x1, x2, y):
    return (f'<line x1="{x1}" y1="{y}" x2="{x2 - 7}" y2="{y}" stroke="{ACCENT}" stroke-width="2.4" stroke-linecap="round"/>'
            f'<path d="M{x2 - 9} {y - 6} L{x2} {y} L{x2 - 9} {y + 6} Z" fill="{ACCENT}"/>')


def frame(w, h, body, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-label="{escape(title)}">'
            f'<rect x="1" y="1" width="{w - 2}" height="{h - 2}" rx="16" fill="{CARD}" stroke="{LINE}" stroke-width="1.5"/>'
            f'{body}</svg>\n')


def flow(steps, title, w=1100, box_h=104, top=18, highlight=()):
    """Soldan sağa numaralı kutular: (başlık, alt satır)."""
    n, gap, pad = len(steps), 30, 18
    bw = (w - 2 * pad - gap * (n - 1)) / n
    h = top + box_h + 18
    out = []
    for i, (t, sub) in enumerate(steps):
        x = pad + i * (bw + gap)
        hl = i in highlight
        out.append(f'<rect x="{x:.1f}" y="{top}" width="{bw:.1f}" height="{box_h}" rx="12" fill="{SOFT if hl else "#faf9f7"}" stroke="{ACCENT if hl else LINE}" stroke-width="1.6"/>')
        out.append(f'<circle cx="{x + 22:.1f}" cy="{top + 25}" r="13" fill="{ACCENT}"/>')
        out.append(text(x + 22, top + 30.5, str(i + 1), 14, 700, "#fff"))
        out.append(text(x + bw / 2 + 10, top + 32, t, 20, 700))
        for k, line in enumerate(sub.split("\n")):
            fit(line, bw - 16, 15.5)
            out.append(text(x + bw / 2, top + 63 + k * 21, line, 15.5, 400, MUTED))
        if i < n - 1:
            out.append(arrow(x + bw + 5, x + bw + gap - 5, top + box_h / 2))
    return frame(w, h, "".join(out), title)


def disk(parts, title, legend, w=1100):
    """microSD: orantılı bölüm çubuğu ve altında kısa açıklamalar."""
    pad, top, bar_h = 18, 18, 82
    total = sum(p[2] for p in parts)
    out = []
    x = pad
    inner = w - 2 * pad
    for i, (name, sub, weight, kind) in enumerate(parts):
        pw = inner * weight / total
        fill = {"cfg": "#f3f1ed", "boot": "#fde7d3", "root": "#fbd3ae", "data": SOFT}[kind]
        stroke = ACCENT if kind == "data" else LINE
        r = 10
        out.append(f'<rect x="{x + 2:.1f}" y="{top}" width="{pw - 4:.1f}" height="{bar_h}" rx="{r}" fill="{fill}" stroke="{stroke}" stroke-width="1.6"/>')
        out.append(text(x + pw / 2, top + 35, name, 18, 700, INK, mono=True))
        fit(sub, pw - 12, 15)
        out.append(text(x + pw / 2, top + 61, sub, 15, 400, MUTED))
        x += pw
    ly = top + bar_h + 32
    lx = pad
    for color, label in legend:
        out.append(f'<rect x="{lx}" y="{ly - 13}" width="16" height="16" rx="4" fill="{color}" stroke="{ACCENT if color == SOFT else LINE}"/>')
        out.append(text(lx + 24, ly, label, 15, 400, MUTED, "start"))
        lx += 24 + len(label) * 8.2 + 34
    return frame(w, ly + 18, "".join(out), title)


DIAGRAMS = {
    "architecture": {
        "tr": lambda: flow([
            ("Donanım", "Pi 4 uyumlu ARM64 kart\nGPIO · I²C · SPI · kamera"),
            ("İşletim sistemi", "Linux 6.18 · systemd\nsalt okunur · A/B"),
            ("Servisler", "makerd (Rust)\nAPI · terminal · yerel AI"),
            ("Masaüstü", "Ekranda, tarayıcıda\nve telefonda aynı"),
        ], "MakerOS katmanları", highlight=(3,)),
        "en": lambda: flow([
            ("Hardware", "Pi 4 compatible ARM64\nGPIO · I²C · SPI · camera"),
            ("Operating system", "Linux 6.18 · systemd\nread-only · A/B updates"),
            ("Services", "makerd (Rust)\nAPI · terminal · local AI"),
            ("Desktop", "Same on screen, in a\nbrowser and on a phone"),
        ], "MakerOS layers", highlight=(3,)),
    },
    "sd-card": {
        "tr": lambda: disk([
            ("MOSCFG", "ayarlar", 1.1, "cfg"),
            ("BOOT_A", "açılış A", 1.1, "boot"),
            ("BOOT_B", "açılış B", 1.1, "boot"),
            ("ROOT_A", "sistem A", 1.7, "root"),
            ("ROOT_B", "sistem B", 1.7, "root"),
            ("DATA", "dosyalarınız · ayarlar · paketler", 4.0, "data"),
        ], "microSD kart düzeni", [("#fde7d3", "Açılış dosyaları"), ("#fbd3ae", "Sistem (salt okunur, iki kopya)"), (SOFT, "Verileriniz (güncellemede korunur)")]),
        "en": lambda: disk([
            ("MOSCFG", "settings", 1.1, "cfg"),
            ("BOOT_A", "boot A", 1.1, "boot"),
            ("BOOT_B", "boot B", 1.1, "boot"),
            ("ROOT_A", "system A", 1.7, "root"),
            ("ROOT_B", "system B", 1.7, "root"),
            ("DATA", "your files · settings · packages", 4.0, "data"),
        ], "microSD card layout", [("#fde7d3", "Boot files"), ("#fbd3ae", "System (read-only, two copies)"), (SOFT, "Your data (kept across updates)")]),
    },
    "boot": {
        "tr": lambda: flow([
            ("Güç", "Kart açılır,\netkin sistem seçilir"),
            ("Çekirdek", "Linux başlar, sistem\nsalt okunur açılır"),
            ("Veriler", "Ayarlar ve dosyalar\nyerine gelir"),
            ("Servisler", "makerd, ağ ve\nyerel AI başlar"),
            ("Masaüstü", "Ekranda ve ağda\nkullanıma hazır"),
        ], "Açılış", highlight=(4,)),
        "en": lambda: flow([
            ("Power", "Board starts, active\nsystem is chosen"),
            ("Kernel", "Linux starts, system\nopens read-only"),
            ("Data", "Settings and files\nare loaded"),
            ("Services", "makerd, network\nand local AI start"),
            ("Desktop", "Ready on screen\nand over the network"),
        ], "Boot", highlight=(4,)),
    },
    "update": {
        "tr": lambda: flow([
            ("Denetle", "Ayarlar › Genel ›\nYazılım güncelleme"),
            ("İndir", "İmzalı paket iner\nve doğrulanır"),
            ("Yaz", "Boştaki sistem\nkopyasına yazılır"),
            ("Dene", "Yeni sürüm bir kez\ndenenir"),
            ("Onayla", "Sorun yoksa kalıcı,\nvarsa eskiye döner"),
        ], "Güncelleme", highlight=(4,)),
        "en": lambda: flow([
            ("Check", "Settings › General ›\nSoftware update"),
            ("Download", "Signed bundle is\nfetched and verified"),
            ("Write", "Written to the spare\nsystem copy"),
            ("Try", "New version is\ntried once"),
            ("Confirm", "Kept if healthy,\notherwise rolls back"),
        ], "Update", highlight=(4,)),
    },
    "packages": {
        "tr": lambda: flow([
            ("apt install", "Terminalde her zamanki\ngibi paket istenir"),
            ("Debian kutusu", "microSD'deki kişisel\nkutunuza kurulur"),
            ("Aktarım", "Komutlar ve uygulamalar\nMakerOS'a bağlanır"),
            ("Kullan", "Terminalde çalıştırın,\nmenüden açın"),
        ], "Paket kurma", highlight=(3,)),
        "en": lambda: flow([
            ("apt install", "Ask for a package in\nthe terminal as usual"),
            ("Debian box", "Installed into your box\non the microSD"),
            ("Export", "Commands and apps are\nlinked into MakerOS"),
            ("Use", "Run in the terminal,\nopen from the menu"),
        ], "Installing packages", highlight=(3,)),
    },
}

if __name__ == "__main__":
    for name, langs in DIAGRAMS.items():
        for lang, fn in langs.items():
            p = OUT / (f"{name}.svg" if lang == "tr" else f"{name}.en.svg")
            p.write_text(fn(), encoding="utf-8")
            print(p.relative_to(OUT.parents[2]))
