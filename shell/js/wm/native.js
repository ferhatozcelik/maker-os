// Native arka uç (M1: yalnız başlatma). M3'te makeros-wm (wlroots) pencereleri makerd WS köprüsü
// üzerinden aynı WindowManager arayüzüne bağlanır:
//   compositor → kabuk: window.opened {id,app_id,title,rect} → wm.adopt(...)  ·  window.closed/focused/title
//   kabuk → compositor: view.render(win) → window.move/resize/focus/minimize · view.animate("min") → genie dikdörtgeni
// Pencere çerçevesini (trafik ışıkları) compositor tokens.json renkleriyle kendisi çizer (SSD).
import { post } from "../api.js";

export class NativeBackend {
  kind = "native";
  constructor(notify) { this.notify = notify; }
  // M1: pencere oluşturmak yerine makerd'den başlatma ister; görünür pencere M3 ile gelir.
  launch(app) { return post(`/apps/${encodeURIComponent(app.id)}/launch`); }
  create() { throw new Error("native pencereler M3 ile gelir"); }
}
