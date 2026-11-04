// Kabuk bağlamı: bileşenler arası paylaşılan tekil nesne (main.js doldurur).
// S.wm (WindowManager), S.events (Events), S.user, S.sys (son /system), S.net (son /net),
// S.launch(app|id, {path}), S.openSettings(sayfa), S.lock(), S.logout(), S.power(eylem), S.toast(metin)
import { Emitter } from "./lib.js";
export const S = new Emitter();
