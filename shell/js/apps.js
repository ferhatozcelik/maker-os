// Uygulama kaydı: GET /apps (makerd) + kabuğun yerleşik meta verisi (pencere boyutu, arama anahtarları).
import { get } from "./api.js";
import { t } from "./i18n.js";
import { pref, prefs } from "./prefs.js";
import { themeById, registerPluginThemes, themeIcon, cacheBrand } from "./themes.js";

const B = "/shell/";
// Kısa ad → yerleşik bilgiler. makerd listesinde olmasa da bu uygulamalar her zaman görünür.
const BUILTIN = {
  settings: { w: 880, h: 590, keys: "ayarlar sistem tercihler settings preferences system", single: true },
  terminal: { w: 760, h: 480, keys: "terminal konsol kabuk shell bash console komut", rawCtrl: true, single: true },
  activity: { w: 880, h: 580, keys: "etkinlik monitörü görev yöneticisi süreç işlem cpu bellek activity monitor task manager process", single: true },
  files: { w: 820, h: 520, keys: "dosyalar dosya gezgini klasör yükle indir files folder upload download" },
  python: { w: 900, h: 600, keys: "python proje şablon uv sanal ortam kod yapay zeka nesne tespiti project template code ai object detection", single: true },
  agents: { w: 880, h: 600, keys: "ai yapay zeka ajan claude codex gemini opencode ollama agent llm", single: true },
  camera: { w: 760, h: 520, keys: "kamera fotoğraf video camera picamera rpicam", single: true },
  gpio: { w: 720, h: 640, keys: "gpio pin donanım elektronik led buton hardware header", single: true },
  plugins: { w: 880, h: 580, keys: "eklenti uygulama zip içe aktar dışa aktar paylaş plugin extension import export", single: true },
  browser: { w: 1180, h: 760, keys: "tarayıcı internet web sitesi chromium chrome browser", single: true },
};
export const DEFAULT_DOCK = ["makeros.files", "makeros.browser", "makeros.terminal", "makeros.python", "makeros.agents", "makeros.camera", "makeros.gpio", "makeros.plugins", "makeros.activity", "makeros.settings"];

// Ayarlar sayfaları (kenar çubuğu + Bul + derin bağlantı #sayfa)
export const SETTINGS_PAGES = [
  ["wifi", "wifi", "kablosuz wireless ağ"], ["network", "network", "ethernet ip ağ hotspot erişim noktası"],
  ["general", "info", "hakkında genel cihaz adı hostname sürüm about"], ["datetime", "clock", "tarih saat saat dilimi ntp time timezone"],
  ["language", "languages", "dil bölge klavye düzeni language region locale"], ["keyboard", "keyboard", "klavye kısayollar shortcuts keys"],
  ["appearance", "palette", "görünüm tema koyu açık vurgu rengi sistem ui menü çubuğu raf gizle appearance dark mode theme"],
  ["startup", "play", "otomatik başlatma başlangıç cron zamanlayıcı görev python script uygulama autostart schedule"],
  ["user", "user", "kullanıcı parola şifre hesap user password"], ["power", "power", "güç yeniden başlat kapat power reboot shutdown"],
];

const short = id => id.replace(/^makeros\./, "");
function normalize(a) {
  const s = short(a.id), b = BUILTIN[s] || {};
  return { kind: "web", categories: [], ...a, short: s, w: b.w || a.w || 800, h: b.h || a.h || 540, keys: (b.keys || "") + " " + a.name,
    single: !!b.single, rawCtrl: !!b.rawCtrl, baseIcon: a.icon || `${B}icons/${s}.svg`, icon: a.icon || `${B}icons/${s}.svg` };
}
const builtinList = () => Object.keys(BUILTIN).map(s => normalize({ id: "makeros." + s, name: t("app." + s), url: `${B}apps/${s}/` }));

// Temaya özel uygulamalar (ör. Konya HackerSpace sitesi): yalnız o tema etkinken listede ve rafta.
const themeApps = () => (themeById(pref("look")).apps || []).map(a => ({ ...normalize({ ...a, categories: ["Theme"] }), theme: true }));
// Simgeler etkin temanın simge paketinden (ör. KHS) gelir; yoksa yerleşik.
const iconize = a => ({ ...a, icon: themeIcon(pref("look"), a.short === "makeros" ? a.id : a.short, a.baseIcon || a.icon) });
const withTheme = list => [...list.filter(a => !a.theme), ...themeApps()].map(iconize);
export let APPS = withTheme(builtinList());
prefs.on("change", k => { if (k === "look") APPS = withTheme(APPS); });
export const appById = id => APPS.find(a => a.id === id);

const isBrowser = a => !!a && (a.id === "chromium" || a.id === "makeros.browser" || (a.categories || []).includes("WebBrowser"));
export { isBrowser };

export async function loadApps() {
  try {
    const list = await get("/apps");
    if (Array.isArray(list) && list.length) {
      const got = list.map(a => {
        const item = normalize(a);
        if (isBrowser(item)) {
          item.kind = "web";
          item.url = `${B}apps/browser/`;
          item.w = 1180;
          item.h = 760;
          item.single = true;
        }
        return item;
      }), ids = new Set(got.map(a => a.id));
      APPS = [...got, ...builtinList().filter(a => !ids.has(a.id))];
    }
  } catch (e) { console.warn("GET /apps:", e.message); }
  // Web arayüzlü eklentiler uygulama gibi açılır (makerd /plugin/<id>/ yönlendirmesi).
  try {
    const pl = await get("/plugins");
    registerPluginThemes(pl); // eklenti temaları (ör. Konya HackerSpace)
    cacheBrand(pref("look")); // giriş ekranı için marka önbelleği
    const extra = pl.filter(p => p.web_port && p.app !== false).map(p => normalize({ id: "makeros.plugin." + p.id, name: p.name, url: `/plugin/${p.id}/`, categories: ["Plugin"],
      icon: p.icon ? `/api/v1/files/raw?path=${encodeURIComponent("~/.local/share/makeros/plugins/" + p.id + "/" + p.icon)}` : `${B}icons/plugins.svg` }));
    APPS = [...APPS.filter(a => !a.id.startsWith("makeros.plugin.")), ...extra];
  } catch { /* eklenti hizmeti yok */ }
  APPS = withTheme(APPS);
  // Tarayıcı tek bir yerleşik uygulama olarak listelenir (makeros.browser - "Tarayıcı").
  // Mükerrer / yerel Chromium girişi menülerde ve kenar çubuğunda ayrı gösterilmez.
  if (APPS.some(a => a.id === "makeros.browser")) APPS = APPS.filter(a => a.id !== "chromium");
  prefs.emit("change", "look", pref("look")); // eklenti teması etkinse duvar kâğıdı/marka/raf yenilensin
  return APPS;
}
