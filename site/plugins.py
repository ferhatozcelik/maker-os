#!/usr/bin/env python3
"""Eklenti mağazası verisi: makeros-plugins deposunun plugins/<id>/plugin.json dosyalarından
tanıtım sayfasının okuduğu plugins.json ve simgeleri üretir (MakerOS'taki mağazayla aynı kaynak).

    python3 site/plugins.py <makeros-plugins klasörü> <çıktı klasörü>

Çıktı: <çıktı>/plugins.json, <çıktı>/icons/<id>.<uzantı>
"""
import json
import re
import shutil
import sys
from pathlib import Path

REPO = "https://github.com/ferhatozcelik/makeros-plugins"
ICON_EXT = {".svg", ".png", ".webp"}
MAX_ICON = 32 << 10
ID = re.compile(r"^[a-z0-9][a-z0-9-]{0,39}$")


def text(v):
    """plugin.json'da metin ya düz dizgedir ya da {"tr": …, "en": …}."""
    if isinstance(v, dict):
        return {"tr": str(v.get("tr") or v.get("en") or ""), "en": str(v.get("en") or v.get("tr") or "")}
    return {"tr": str(v or ""), "en": str(v or "")}


def main(src: Path, out: Path):
    out.mkdir(parents=True, exist_ok=True)
    (out / "icons").mkdir(exist_ok=True)
    items = []
    for d in sorted((src / "plugins").iterdir()):
        mf = d / "plugin.json"
        if not d.is_dir() or not mf.is_file() or not ID.match(d.name):
            continue
        m = json.loads(mf.read_text(encoding="utf-8"))
        if m.get("id") != d.name:
            continue
        tags = []
        if m.get("widget"):
            tags.append("widget")
        if m.get("theme"):
            tags.append("theme")
        if m.get("app", True) is not False:
            tags.append("app")
        icon = ""
        ip = d / str(m.get("icon") or "")
        if m.get("icon") and ip.is_file() and ip.suffix.lower() in ICON_EXT and ip.stat().st_size <= MAX_ICON:
            icon = f"icons/{d.name}{ip.suffix.lower()}"
            shutil.copyfile(ip, out / icon)
        items.append({
            "id": d.name,
            "name": str(m.get("name") or d.name),
            "version": str(m.get("version") or ""),
            "description": text(m.get("description")),
            "author": str(m.get("author") or ""),
            "icon": icon,
            "tags": tags,
            "url": f"{REPO}/tree/main/plugins/{d.name}",
        })
    (out / "plugins.json").write_text(json.dumps({"repo": REPO, "plugins": items}, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"{len(items)} eklenti → {out / 'plugins.json'}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(Path(sys.argv[1]), Path(sys.argv[2]))
