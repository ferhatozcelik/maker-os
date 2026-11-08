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


def terminal(lines, title, kind="mac", w=760):
    """Terminal penceresi görseli. lines: (stil, metin); stil: cmd | out | ok | warn | dim | in."""
    bar = 34
    lh = 22
    h = bar + 22 + lh * len(lines) + 14
    bg, fg = ("#1e1f24", "#e6e6e9") if kind == "mac" else ("#012456", "#eeedf0")
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-label="{escape(title)}">',
           f'<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="12" fill="{bg}" stroke="#00000033"/>']
    if kind == "mac":
        out.append(f'<path d="M0.5 12.5 a12 12 0 0 1 12 -12 h{w - 25} a12 12 0 0 1 12 12 v{bar - 12} h-{w - 1} z" fill="#2c2d33"/>')
        for i, c in enumerate(("#ff5f57", "#febc2e", "#28c840")):
            out.append(f'<circle cx="{20 + i * 20}" cy="{bar / 2}" r="6" fill="{c}"/>')
        out.append(text(w / 2, bar / 2 + 5, title, 13, 600, "#a9abb3"))
    else:
        out.append(f'<path d="M0.5 12.5 a12 12 0 0 1 12 -12 h{w - 25} a12 12 0 0 1 12 12 v{bar - 12} h-{w - 1} z" fill="#f3f3f3"/>')
        out.append(text(16, bar / 2 + 5, title, 13, 600, "#1f1f1f", "start"))
        for i, g in enumerate(("—", "☐", "✕")):
            out.append(text(w - 90 + i * 32, bar / 2 + 5, g, 13, 400, "#1f1f1f"))
    colors = {"cmd": fg, "out": fg, "ok": "#4ade80", "warn": "#f87171", "dim": "#9ca3af", "in": "#fbbf24", "acc": ACCENT}
    y = bar + 26
    for style, line in lines:
        weight = 700 if style in ("ok", "warn", "acc") else 400
        if style == "cmd":
            prompt = "$ " if kind == "mac" else "PS> "
            out.append(f'<text x="18" y="{y}" font-family="{MONO}" font-size="14" fill="{fg}"><tspan fill="{ACCENT}" font-weight="700">{escape(prompt)}</tspan>{escape(line)}</text>')
        else:
            out.append(f'<text x="18" y="{y}" font-family="{MONO}" font-size="14" font-weight="{weight}" fill="{colors[style]}" xml:space="preserve">{escape(line)}</text>')
        y += lh
    out.append("</svg>\n")
    return "".join(out)


FLASH_TR = [
    ("cmd", 'sh -c "$(curl -fsSL https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.sh)"'),
    ("acc", "  MakerOS — microSD yazıcı"),
    ("out", "==> İmaj indiriliyor ve doğrulanıyor (SHA-256) … Doğrulandı."),
    ("out", "==> Harici diskler aranıyor"),
    ("out", "  1) /dev/disk4       31.9 GB  SD Card Reader"),
    ("warn", "UYARI: /dev/disk4 üzerindeki HER ŞEY silinecek."),
    ("in", "Onaylamak için disk adını yazın (disk4): disk4"),
    ("in", "Wi-Fi ağ adı (boş bırakılabilir): EvAgim"),
    ("dim", "5.9 GB yazıldı · 32 MB/s"),
    ("ok", "✓ Hazır! Kartı çıkarıp cihaza takın ve gücü verin."),
]
FLASH_EN = [
    ("cmd", 'sh -c "$(curl -fsSL https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.sh)"'),
    ("acc", "  MakerOS — microSD flasher"),
    ("out", "==> Downloading and verifying image (SHA-256) … Verified."),
    ("out", "==> Looking for external disks"),
    ("out", "  1) /dev/disk4       31.9 GB  SD Card Reader"),
    ("warn", "WARNING: EVERYTHING on /dev/disk4 will be erased."),
    ("in", "Type the disk name to confirm (disk4): disk4"),
    ("in", "Wi-Fi network name (optional): HomeWiFi"),
    ("dim", "5.9 GB written · 32 MB/s"),
    ("ok", "✓ Done! Remove the card, insert it into the board and power it on."),
]
WIN_TR = [
    ("cmd", "irm https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.ps1 | iex"),
    ("acc", "  MakerOS — microSD yazıcı"),
    ("out", "==> İmaj indiriliyor ve doğrulanıyor (SHA-256) … Doğrulandı."),
    ("out", "==> Harici diskler aranıyor"),
    ("out", "  1) disk2       31.9 GB  Generic SD/MMC USB Device"),
    ("warn", "UYARI: disk2 üzerindeki HER ŞEY silinecek."),
    ("in", "Onaylamak için disk adını yazın (disk2): disk2"),
    ("in", "Wi-Fi ağ adı (boş bırakılabilir): EvAgim"),
    ("dim", "Yazıldı: 5.9 GB · 210 s"),
    ("ok", "✓ Hazır! Kartı çıkarıp cihaza takın ve gücü verin."),
]
WIN_EN = [
    ("cmd", "irm https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.ps1 | iex"),
    ("acc", "  MakerOS — microSD flasher"),
    ("out", "==> Downloading and verifying image (SHA-256) … Verified."),
    ("out", "==> Looking for external disks"),
    ("out", "  1) disk2       31.9 GB  Generic SD/MMC USB Device"),
    ("warn", "WARNING: EVERYTHING on disk2 will be erased."),
    ("in", "Type the disk name to confirm (disk2): disk2"),
    ("in", "Wi-Fi network name (optional): HomeWiFi"),
    ("dim", "Written: 5.9 GB · 210 s"),
    ("ok", "✓ Done! Remove the card, insert it into the board and power it on."),
]

DIAGRAMS["flash"] = {
    "tr": lambda: flow([
        ("Komutu çalıştır", "Terminal / PowerShell'e\ntek satırı yapıştırın"),
        ("İndir", "En son imaj iner ve\nSHA-256 doğrulanır"),
        ("Kartı seç", "Yalnız harici diskler;\ndisk adını yazıp onaylayın"),
        ("Yaz ve tak", "Wi-Fi isteğe bağlı;\nkartı cihaza takın"),
    ], "microSD karta yazma", highlight=(3,)),
    "en": lambda: flow([
        ("Run the command", "Paste one line into\nTerminal / PowerShell"),
        ("Download", "Latest image downloads\nand SHA-256 is verified"),
        ("Pick the card", "External disks only;\ntype its name to confirm"),
        ("Write & insert", "Wi-Fi optional;\ninsert into the board"),
    ], "Flashing the microSD card", highlight=(3,)),
}
DIAGRAMS["flash-mac"] = {"tr": lambda: terminal(FLASH_TR, "Terminal — macOS / Linux", "mac", 900), "en": lambda: terminal(FLASH_EN, "Terminal — macOS / Linux", "mac", 900)}
DIAGRAMS["flash-windows"] = {"tr": lambda: terminal(WIN_TR, "Windows PowerShell (Yönetici)", "win", 900), "en": lambda: terminal(WIN_EN, "Windows PowerShell (Administrator)", "win", 900)}

if __name__ == "__main__":
    for name, langs in DIAGRAMS.items():
        for lang, fn in langs.items():
            p = OUT / (f"{name}.svg" if lang == "tr" else f"{name}.en.svg")
            p.write_text(fn(), encoding="utf-8")
            print(p.relative_to(OUT.parents[2]))
