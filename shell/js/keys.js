// Klavye kısayolları. "Mod" = Super/Meta (⌘). Uzak tarayıcıda Meta çoğu zaman ana makine
// işletim sistemince yakalanır (ör. Mac'te ⌘Space) → Ctrl yedek "Mod" olarak sayılır
// (Ayarlar → Klavye: ctrlCmd = auto|on|off). Terminal gibi ham Ctrl isteyen uygulamalarda yedek kapalıdır.
import { Emitter } from "./lib.js";
import { pref } from "./prefs.js";

export const SHORTCUTS = [
  ["find", "Mod+Space", "Ctrl+Space", "Alt+Space"],
  ["switch", "Mod+Tab", "Alt+Tab"],
  ["switchBack", "Mod+Shift+Tab", "Alt+Shift+Tab"],
  ["nextWin", "Mod+`"],
  ["quit", "Mod+Q"],
  ["close", "Mod+W"],
  ["min", "Mod+M"],
  ["hide", "Mod+H"],
  ["prefs", "Mod+,"],
  ["new", "Mod+N"],
  ["overview", "Ctrl+ArrowUp", "Mod+ArrowUp", "F3"],
  ["desktop", "Mod+D", "Mod+F3"],
  ["apps", "F4", "Mod+Shift+A"],
  ["full", "Ctrl+Mod+F", "F11"],
  ["lock", "Ctrl+Mod+Q"],
  ["nc", "Mod+Shift+N"],
  ["left", "Mod+Alt+ArrowLeft"],
  ["right", "Mod+Alt+ArrowRight"],
  ["terminal", "Ctrl+Alt+T", "Mod+Enter"],
  ["activity", "Mod+Alt+Escape"],
  ["help", "Mod+/"],
].map(([id, ...combos]) => ({ id, combos: combos.map(parse) }));

function parse(s) {
  const p = s.split("+"), key = p.pop() || "+";
  return { s, key, mod: p.includes("Mod"), ctrl: p.includes("Ctrl"), alt: p.includes("Alt"), shift: p.includes("Shift") };
}
const CODE = { "`": "Backquote", ",": "Comma", "/": "Slash", Space: "Space" };
function keyMatches(e, key) {
  if (/^[A-Z]$/.test(key)) return e.code === "Key" + key || e.key.toLocaleUpperCase("en") === key;
  if (/^\d$/.test(key)) return e.code === "Digit" + key;
  if (CODE[key]) return e.code === CODE[key] || e.key === (key === "Space" ? " " : key);
  return e.key === key || e.code === key;
}
const PUNCT = k => k.length === 1 && !/[A-Z0-9]/.test(k);

export function ctrlAsMod(rawCtrl) {
  if (rawCtrl) return false;
  const v = pref("ctrlCmd");
  return v === "on" || (v === "auto" && !/WPE|cog/i.test(navigator.userAgent));
}

export function match(e, rawCtrl = false) {
  const fb = ctrlAsMod(rawCtrl);
  for (const sc of SHORTCUTS) for (const c of sc.combos) {
    if (!keyMatches(e, c.key)) continue;
    let meta = e.metaKey, ctrl = e.ctrlKey;
    if (c.mod && !meta && fb && ctrl && !c.ctrl) { meta = true; ctrl = false; }
    if (c.mod !== meta || c.ctrl !== ctrl || c.alt !== e.altKey) continue;
    if (!PUNCT(c.key) && c.shift !== e.shiftKey) continue;
    return sc.id;
  }
  return null;
}

// Görüntüleme: mac (⌘⇧Q) ve PC (Super+Shift+Q)
const SYM = { Mod: "⌘", Ctrl: "⌃", Alt: "⌥", Shift: "⇧" }, KEYN = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Tab: "⇥", Enter: "↩", Escape: "esc", Space: "Space" };
export const macLabel = c => ["Ctrl", "Alt", "Shift", "Mod"].filter(m => c[m.toLowerCase()]).map(m => SYM[m]).join("") + (KEYN[c.key] || c.key);
export const pcLabel = c => c.s.replace("Mod", "Super").replace(/Arrow(\w+)/, (_, d) => KEYN["Arrow" + d]).replace("Escape", "Esc");
export const shortcutFor = id => SHORTCUTS.find(s => s.id === id)?.combos[0];

// Dağıtıcı: kabuk eylemleri "action" ile; değiştirici bırakılınca "modup".
export const keys = new Emitter();
// win: window (kabuk ya da iframe). rawCtrl(): o an odakta ham Ctrl isteyen uygulama var mı
export function attach(win, rawCtrl = () => false) {
  win.addEventListener("keydown", e => {
    if (e.repeat && !/Tab|`/.test(e.key)) return;
    const id = match(e, rawCtrl());
    if (id) {
      e.preventDefault(); e.stopPropagation();
      keys.emit("action", id, e);
    } else if (e.key === "Escape") keys.emit("escape", e);
    else keys.emit("key", e);
  }, true);
  win.addEventListener("keyup", e => { if (["Meta", "Alt", "Control", "OS"].includes(e.key)) keys.emit("modup", e); }, true);
}
