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


def arrow(x1, x2, y):
    return (f'<line x1="{x1}" y1="{y}" x2="{x2 - 7}" y2="{y}" stroke="{ACCENT}" stroke-width="2.4" stroke-linecap="round"/>'
            f'<path d="M{x2 - 9} {y - 6} L{x2} {y} L{x2 - 9} {y + 6} Z" fill="{ACCENT}"/>')


def frame(w, h, body, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-label="{escape(title)}">'
            f'<rect x="1" y="1" width="{w - 2}" height="{h - 2}" rx="16" fill="{CARD}" stroke="{LINE}" stroke-width="1.5"/>'
            f'{body}</svg>\n')


def flow(steps, title, w=1200, box_h=96, top=58, highlight=()):
    """Soldan sağa numaralı kutular: (başlık, alt satır)."""
    n, gap, pad = len(steps), 34, 28
    bw = (w - 2 * pad - gap * (n - 1)) / n
    h = top + box_h + 26
    out = [text(pad, 36, title, 17, 700, INK, "start")]
    for i, (t, sub) in enumerate(steps):
        x = pad + i * (bw + gap)
        hl = i in highlight
        out.append(f'<rect x="{x:.1f}" y="{top}" width="{bw:.1f}" height="{box_h}" rx="12" fill="{SOFT if hl else "#faf9f7"}" stroke="{ACCENT if hl else LINE}" stroke-width="1.6"/>')
        out.append(f'<circle cx="{x + 22:.1f}" cy="{top + 24}" r="12" fill="{ACCENT}"/>')
        out.append(text(x + 22, top + 29, str(i + 1), 13, 700, "#fff"))
        out.append(text(x + bw / 2 + 10, top + 30, t, 17, 700))
        for k, line in enumerate(sub.split("\n")):
            out.append(text(x + bw / 2, top + 58 + k * 19, line, 13.5, 400, MUTED))
        if i < n - 1:
            out.append(arrow(x + bw + 5, x + bw + gap - 5, top + box_h / 2))
    return frame(w, h, "".join(out), title)


def disk(parts, title, legend, w=1200):
    """microSD: orantılı bölüm çubuğu ve altında kısa açıklamalar."""
    pad, top, bar_h = 28, 58, 74
    total = sum(p[2] for p in parts)
    out = [text(pad, 36, title, 17, 700, INK, "start")]
    x = pad
    inner = w - 2 * pad
    for i, (name, sub, weight, kind) in enumerate(parts):
        pw = inner * weight / total
        fill = {"cfg": "#f3f1ed", "boot": "#fde7d3", "root": "#fbd3ae", "data": SOFT}[kind]
        stroke = ACCENT if kind == "data" else LINE
        r = 10
        out.append(f'<rect x="{x + 2:.1f}" y="{top}" width="{pw - 4:.1f}" height="{bar_h}" rx="{r}" fill="{fill}" stroke="{stroke}" stroke-width="1.6"/>')
        out.append(text(x + pw / 2, top + 31, name, 15.5, 700, INK, mono=True))
        out.append(text(x + pw / 2, top + 54, sub, 12.5, 400, MUTED))
        x += pw
    ly = top + bar_h + 34
    lx = pad
    for color, label in legend:
        out.append(f'<rect x="{lx}" y="{ly - 12}" width="14" height="14" rx="4" fill="{color}" stroke="{LINE}"/>')
        out.append(text(lx + 22, ly, label, 13.5, 400, MUTED, "start"))
        lx += 22 + len(label) * 7.4 + 34
    return frame(w, ly + 22, "".join(out), title)


DIAGRAMS = {
    "architecture": {
        "tr": lambda: flow([
            ("Donanım", "Pi 4 / 400 uyumlu ARM64 kart\nGPIO · I²C · SPI · kamera"),
            ("İşletim sistemi", "Linux 6.18 · systemd\nsalt okunur kök · A/B güncelleme"),
            ("Servisler", "makerd (Rust)\nAPI · terminal · yerel AI"),
            ("Masaüstü", "Ekranda, tarayıcıda\nve telefonda aynı arayüz"),
        ], "MakerOS katmanları", highlight=(3,)),
        "en": lambda: flow([
            ("Hardware", "Pi 4 / 400 compatible ARM64\nGPIO · I²C · SPI · camera"),
            ("Operating system", "Linux 6.18 · systemd\nread-only root · A/B updates"),
            ("Services", "makerd (Rust)\nAPI · terminal · local AI"),
            ("Desktop", "Same interface on screen,\nin a browser and on a phone"),
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
        ], "microSD kart düzeni", [("#fde7d3", "Açılış dosyaları"), ("#fbd3ae", "Sistem (salt okunur, iki kopya)"), (SOFT, "Sizin verileriniz (güncellemede korunur)")]),
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
            ("Çekirdek", "Linux başlar,\nsistem salt okunur bağlanır"),
            ("Veriler", "DATA bölümü bağlanır,\nayarlar ve dosyalar gelir"),
            ("Servisler", "makerd, ağ ve\nyerel AI başlar"),
            ("Masaüstü", "Ekranda ve ağda\nkullanıma hazır"),
        ], "Açılış", highlight=(4,)),
        "en": lambda: flow([
            ("Power", "Board starts,\nactive system is chosen"),
            ("Kernel", "Linux starts, system\nmounted read-only"),
            ("Data", "DATA partition mounted,\nsettings and files load"),
            ("Services", "makerd, network\nand local AI start"),
            ("Desktop", "Ready on screen\nand over the network"),
        ], "Boot", highlight=(4,)),
    },
    "update": {
        "tr": lambda: flow([
            ("Denetle", "Ayarlar › Genel ›\nYazılım güncelleme"),
            ("İndir", "İmzalı paket indirilir\nve doğrulanır"),
            ("Yaz", "Kullanılmayan sistem\nkopyasına yazılır"),
            ("Dene", "Yeniden başlatılır,\nyeni sürüm bir kez denenir"),
            ("Onayla", "Sorun yoksa kalıcı olur,\nvarsa eskiye döner"),
        ], "Güncelleme", highlight=(4,)),
        "en": lambda: flow([
            ("Check", "Settings › General ›\nSoftware update"),
            ("Download", "Signed bundle is\ndownloaded and verified"),
            ("Write", "Written to the unused\nsystem copy"),
            ("Try", "Reboots and tries\nthe new version once"),
            ("Confirm", "Kept if healthy,\notherwise rolls back"),
        ], "Update", highlight=(4,)),
    },
    "packages": {
        "tr": lambda: flow([
            ("apt install", "Terminalde her zamanki\ngibi paket istersiniz"),
            ("Debian kutusu", "Paket, microSD'deki\nkişisel kutunuza kurulur"),
            ("Aktarım", "Komutlar ve uygulamalar\nMakerOS'a bağlanır"),
            ("Kullan", "Terminalde çalıştırın,\nmenüden açın"),
        ], "Paket kurma", highlight=(3,)),
        "en": lambda: flow([
            ("apt install", "Ask for a package in the\nterminal as usual"),
            ("Debian box", "Installed into your personal\nbox on the microSD"),
            ("Export", "Commands and apps are\nlinked into MakerOS"),
            ("Use", "Run it in the terminal,\nopen it from the menu"),
        ], "Installing packages", highlight=(3,)),
    },
}

if __name__ == "__main__":
    for name, langs in DIAGRAMS.items():
        for lang, fn in langs.items():
            p = OUT / (f"{name}.svg" if lang == "tr" else f"{name}.en.svg")
            p.write_text(fn(), encoding="utf-8")
            print(p.relative_to(OUT.parents[2]))
