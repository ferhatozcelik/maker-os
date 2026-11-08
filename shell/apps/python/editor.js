// Hafif kod düzenleyici: textarea + renklendirilmiş katman (Python, TOML, Markdown, JSON düz) + satır numaraları.
// Dış kütüphane yok (Pi'nin kendi ekranında da hızlı). Tab/Shift+Tab girinti, Enter'da otomatik girinti
// (":" sonrası +4), ⌘/ yorum satırı, ⌘S kaydet (onSave).
const esc = s => s.replace(/[&<>]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
const PY_KW = new Set("False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield match case".split(" "));
const PY_BI = new Set("print len range int str float list dict set tuple open enumerate zip map filter sorted min max sum abs round isinstance super self input type bool bytes Exception".split(" "));

function highlightPy(src) {
  // Sırayla: üçlü tırnak, yorum, dizge, sayı, ad.
  const re = /("""[\s\S]*?(?:"""|$)|'''[\s\S]*?(?:'''|$))|(#[^\n]*)|([rbfuRBFU]{0,2}"(?:\\.|[^"\\\n])*"?|[rbfuRBFU]{0,2}'(?:\\.|[^'\\\n])*'?)|(\b\d[\d_]*(?:\.\d+)?(?:e[+-]?\d+)?\b)|(@\w+)|([A-Za-z_]\w*)/g;
  let out = "", last = 0, m, prev = "";
  while ((m = re.exec(src))) {
    out += esc(src.slice(last, m.index));
    const [tok, tri, com, str, num, dec, id] = m;
    if (tri || str) out += `<span class="s">${esc(tok)}</span>`;
    else if (com) out += `<span class="c">${esc(tok)}</span>`;
    else if (num) out += `<span class="n">${tok}</span>`;
    else if (dec) out += `<span class="d">${esc(tok)}</span>`;
    else if (id) {
      if (PY_KW.has(id)) out += `<span class="k">${id}</span>`;
      else if (prev === "def" || prev === "class") out += `<span class="f">${id}</span>`;
      else if (PY_BI.has(id)) out += `<span class="b">${id}</span>`;
      else out += id;
      prev = id;
    }
    last = re.lastIndex;
  }
  return out + esc(src.slice(last));
}
function highlightToml(src) {
  return src.split("\n").map(l => {
    if (/^\s*#/.test(l)) return `<span class="c">${esc(l)}</span>`;
    if (/^\s*\[/.test(l)) return `<span class="k">${esc(l)}</span>`;
    const m = l.match(/^(\s*[\w.-]+)(\s*=)(.*)$/);
    if (m) return `<span class="f">${esc(m[1])}</span>${esc(m[2])}${esc(m[3]).replace(/(&quot;|")(.*?)("|$)/g, '<span class="s">"$2"</span>')}`;
    return esc(l);
  }).join("\n");
}
const LANG = name => /\.pyw?$/i.test(name) ? highlightPy : /\.toml$/i.test(name) ? highlightToml : esc;

export function createEditor({ onSave, onChange } = {}) {
  const wrap = document.createElement("div");
  wrap.className = "ed";
  wrap.innerHTML = '<div class="ed-ln"></div><div class="ed-in"><pre class="ed-hl" aria-hidden="true"></pre><textarea class="ed-ta" spellcheck="false" autocapitalize="off" autocomplete="off" wrap="off"></textarea></div>';
  const ta = wrap.querySelector("textarea"), hl = wrap.querySelector(".ed-hl"), ln = wrap.querySelector(".ed-ln");
  let hi = esc, raf = 0, lines = 0;
  const paint = () => {
    raf = 0;
    hl.innerHTML = hi(ta.value) + "\n";
    const n = ta.value.split("\n").length;
    if (n !== lines) { lines = n; ln.textContent = Array.from({ length: n }, (_, i) => i + 1).join("\n"); }
  };
  const sched = () => { if (!raf) raf = requestAnimationFrame(paint); };
  ta.addEventListener("input", () => { sched(); onChange?.(); });
  ta.addEventListener("scroll", () => { hl.style.transform = `translate(${-ta.scrollLeft}px, ${-ta.scrollTop}px)`; ln.style.transform = `translateY(${-ta.scrollTop}px)`; });
  const edit = (start, end, text, selStart, selEnd) => {
    ta.setSelectionRange(start, end);
    // execCommand korur: geri al (⌘Z) çalışır. Yoksa doğrudan.
    if (!document.execCommand?.("insertText", false, text)) { ta.setRangeText(text, start, end, "end"); ta.dispatchEvent(new Event("input")); }
    if (selStart != null) ta.setSelectionRange(selStart, selEnd ?? selStart);
  };
  ta.addEventListener("keydown", e => {
    const v = ta.value, s = ta.selectionStart, en = ta.selectionEnd;
    const lineStart = v.lastIndexOf("\n", s - 1) + 1;
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") { e.preventDefault(); onSave?.(); return; }
    if (e.key === "Tab") {
      e.preventDefault();
      const blockEnd = v.indexOf("\n", en - (en > s && v[en - 1] === "\n" ? 1 : 0));
      const bEnd = blockEnd < 0 ? v.length : blockEnd;
      if (s === en && !e.shiftKey) return edit(s, en, "    ");
      const block = v.slice(lineStart, bEnd);
      const out = e.shiftKey ? block.replace(/^( {1,4}|\t)/gm, "") : block.replace(/^/gm, "    ");
      return edit(lineStart, bEnd, out, lineStart, lineStart + out.length);
    }
    if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      const line = v.slice(lineStart, s);
      let ind = line.match(/^\s*/)[0];
      if (/:\s*(#.*)?$/.test(line)) ind += "    ";
      return edit(s, en, "\n" + ind);
    }
    if ((e.metaKey || e.ctrlKey) && e.key === "/") {
      e.preventDefault();
      const bEnd = (i => (i < 0 ? v.length : i))(v.indexOf("\n", en));
      const block = v.slice(lineStart, bEnd), ls = block.split("\n");
      const all = ls.every(l => /^\s*#/.test(l) || !l.trim());
      const out = ls.map(l => (all ? l.replace(/^(\s*)# ?/, "$1") : l.trim() ? l.replace(/^(\s*)/, "$1# ") : l)).join("\n");
      return edit(lineStart, bEnd, out, lineStart, lineStart + out.length);
    }
    if (e.key === "Backspace" && s === en && s > lineStart && /^ +$/.test(v.slice(lineStart, s)) && (s - lineStart) % 4 === 0) {
      e.preventDefault();
      return edit(s - 4, s, "");
    }
  });
  return {
    el: wrap,
    get value() { return ta.value; },
    set(text, name = "") { hi = LANG(name); ta.value = text; ta.scrollTop = 0; ta.scrollLeft = 0; lines = 0; paint(); ta.dispatchEvent(new Event("scroll")); },
    focus() { ta.focus(); },
    setReadOnly(ro) { ta.readOnly = ro; },
  };
}

export const EDITOR_CSS = `
.ed { position: relative; display: flex; min-height: 0; flex: 1; font: 13px/1.55 var(--font-mono, ui-monospace, monospace); background: var(--ed-bg, #1b1d22); color: #e6e6e6; border-radius: 10px; overflow: hidden; }
.ed-ln { flex: none; padding: 10px 8px 10px 12px; text-align: right; color: #6b7080; white-space: pre; user-select: none; min-width: 38px; }
.ed-in { position: relative; flex: 1; min-width: 0; overflow: hidden; }
.ed-hl, .ed-ta { margin: 0; padding: 10px 12px; font: inherit; tab-size: 4; white-space: pre; letter-spacing: normal; word-spacing: normal; }
.ed-hl { position: absolute; inset: 0; pointer-events: none; color: inherit; }
.ed-ta { position: absolute; inset: 0; width: 100%; height: 100%; resize: none; border: 0; outline: none; background: transparent; color: transparent; caret-color: #fff; overflow: auto; }
.ed-ta::selection { background: rgba(120, 160, 255, .35); color: transparent; }
.ed .k { color: #ff8a5c; } .ed .s { color: #a5d6a7; } .ed .c { color: #7d8597; font-style: italic; } .ed .n { color: #f8c555; }
.ed .f { color: #82aaff; } .ed .b { color: #4fd1c5; } .ed .d { color: #c792ea; }
`;
