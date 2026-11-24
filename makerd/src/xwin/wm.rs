//! Xvfb için küçük pencere yöneticisi + Composite/Damage ile pencere başına yakalama.
//!
//! - Kök pencerede SubstructureRedirect: haritalama ve boyut istekleri buradan geçer.
//! - Tüm üst düzey pencereler (menüler dahil) Composite ile kendi pikselmaplerine yönlendirilir;
//!   üst üste binseler de her biri ayrı ayrı okunabilir.
//! - Damage (NonEmpty): değişen dikdörtgenler o pencereye abone olan akışlara dağıtılır.
//! - Girdi XTest ile verilir; hedef pencere gerekirse en üste alınır (X'teki yığın sırası
//!   görünmez, yalnız girdinin doğru pencereye düşmesi için).

use super::keys::{self, Keymap};
use super::{Display, SCREEN_H, SCREEN_W};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::damage::{ConnectionExt as _, ReportLevel};
use x11rb::protocol::xfixes::{ConnectionExt as _, CursorNotifyMask};
use x11rb::protocol::xproto::*;
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        WM_PROTOCOLS, WM_DELETE_WINDOW, WM_TAKE_FOCUS, WM_STATE, WM_CHANGE_STATE,
        UTF8_STRING, _NET_WM_NAME, _NET_WM_PID, _NET_WM_ICON,
        _NET_WM_WINDOW_TYPE, _NET_WM_WINDOW_TYPE_DROPDOWN_MENU, _NET_WM_WINDOW_TYPE_POPUP_MENU,
        _NET_WM_WINDOW_TYPE_TOOLTIP, _NET_WM_WINDOW_TYPE_COMBO, _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_WINDOW_TYPE_DND, _NET_WM_WINDOW_TYPE_MENU,
        _NET_SUPPORTED, _NET_SUPPORTING_WM_CHECK, _NET_ACTIVE_WINDOW, _NET_CLIENT_LIST,
        _NET_WM_STATE, _NET_FRAME_EXTENTS,
    }
}

/// Abone akışlara giden iletiler.
pub enum Msg {
    Damage(Vec<Rectangle>),
    Geom(u16, u16),
    Cursor(Arc<String>),
    Gone,
}

#[derive(Clone, Debug, Default)]
pub struct Win {
    pub id: u32,
    pub x: i16,
    pub y: i16,
    pub w: u16,
    pub h: u16,
    pub bw: u16,
    pub depth: u8,
    pub popup: bool,
    pub title: String,
    pub class: String,
    pub transient: u32,
    pub parent: u32,
    pub damage: u32,
    pub pixmap: u32,
    pub delete: bool,
    pub take_focus: bool,
    pub minw: u16,
    pub minh: u16,
    pub maxw: u16,
    pub maxh: u16,
    pub pid: u32,
}

impl Win {
    /// İçeriğin kökteki sol üst köşesi.
    pub fn origin(&self) -> (i32, i32) {
        (self.x as i32 + self.bw as i32, self.y as i32 + self.bw as i32)
    }

    pub fn info(&self) -> serde_json::Value {
        let (x, y) = self.origin();
        json!({
            "id": self.id, "title": self.title, "cls": self.class, "x": x, "y": y, "w": self.w, "h": self.h,
            "popup": self.popup, "parent": self.parent, "minw": self.minw, "minh": self.minh,
            "maxw": self.maxw, "maxh": self.maxh, "pid": self.pid,
        })
    }
}

#[derive(Default)]
struct State {
    wins: HashMap<u32, Win>,
    /// Pencere görüntü akışları.
    subs: HashMap<u32, Vec<(u64, mpsc::UnboundedSender<Msg>)>>,
    next_sub: u64,
    /// En son en üste alınan yönetilen pencere.
    top: u32,
    /// Son girdi alan yönetilen pencere (açılır menülerin sahibi tahmini).
    last_input: u32,
    focus: u32,
    cursor: Option<Arc<String>>,
    /// XTest ile basılı tutulan değişkenler (bit: 1 Shift, 2 Ctrl, 4 Alt, 8 Super).
    mods: u8,
    /// Basılı tuşlar: keysym → keycode (bırakırken aynı keycode).
    down: HashMap<u32, u8>,
}

pub struct Ctx {
    pub conn: RustConnection,
    pub root: Window,
    atoms: Atoms,
    region: u32,
    dismiss_win: Window,
    st: Mutex<State>,
    keymap: Mutex<Keymap>,
}

pub struct Bridge {
    pub display: Display,
    ctx: Mutex<Option<Arc<Ctx>>>,
    ctl: Mutex<Vec<mpsc::UnboundedSender<Arc<String>>>>,
}

pub const MOD_SHIFT: u8 = 1;
pub const MOD_CTRL: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

impl Bridge {
    pub fn new(display: Display) -> Self {
        Self { display, ctx: Mutex::new(None), ctl: Mutex::new(Vec::new()) }
    }

    pub fn ctx(&self) -> Option<Arc<Ctx>> {
        self.ctx.lock().unwrap().clone()
    }

    /// Denetim akışı aboneliği: ilk ileti güncel pencere listesidir.
    pub fn subscribe_ctl(&self) -> mpsc::UnboundedReceiver<Arc<String>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let list: Vec<serde_json::Value> = self.ctx().map(|c| c.st.lock().unwrap().wins.values().map(Win::info).collect()).unwrap_or_default();
        let _ = tx.send(Arc::new(json!({"t": "list", "wins": list, "ready": self.ctx().is_some()}).to_string()));
        self.ctl.lock().unwrap().push(tx);
        rx
    }

    fn broadcast(&self, v: serde_json::Value) {
        let s = Arc::new(v.to_string());
        self.ctl.lock().unwrap().retain(|tx| tx.send(s.clone()).is_ok());
    }

    /// X sunucusuna bağlanır ve bağlantı kopana kadar olay döngüsünü çalıştırır.
    pub fn run(&self) -> Result<(), Error> {
        let ctx = Arc::new(Ctx::connect(&self.display)?);
        *self.ctx.lock().unwrap() = Some(ctx.clone());
        tracing::info!(display = %self.display.name(), "x11 pencere köprüsü hazır");
        let res = (|| -> Result<(), Error> {
            if let Ok(tree) = ctx.conn.query_tree(ctx.root)?.reply() {
                for w in tree.children {
                    if let Ok(a) = ctx.conn.get_window_attributes(w)?.reply() {
                        if a.map_state == MapState::VIEWABLE {
                            self.adopt(&ctx, w, a.override_redirect);
                        }
                    }
                }
            }
            ctx.conn.flush()?;
            loop {
                let ev = ctx.conn.wait_for_event()?;
                self.handle(&ctx, ev);
                while let Some(ev) = ctx.conn.poll_for_event()? {
                    self.handle(&ctx, ev);
                }
                ctx.conn.flush()?;
            }
        })();
        *self.ctx.lock().unwrap() = None;
        let mut st = ctx.st.lock().unwrap();
        for (_, subs) in st.subs.drain() {
            for (_, tx) in subs {
                let _ = tx.send(Msg::Gone);
            }
        }
        drop(st);
        self.broadcast(json!({"t": "reset"}));
        res
    }

    fn handle(&self, c: &Arc<Ctx>, ev: Event) {
        match ev {
            Event::MapRequest(e) => {
                let _ = c.place(e.window);
                let _ = c.conn.change_property32(PropMode::REPLACE, e.window, c.atoms.WM_STATE, c.atoms.WM_STATE, &[1, 0]);
                let _ = c.conn.map_window(e.window);
            }
            Event::ConfigureRequest(e) => {
                let mut aux = ConfigureWindowAux::from_configure_request(&e);
                // Pencere ekranın içinde kalmalı: dışarıda kalan kısım fareyle tıklanamaz.
                let w = aux.width.unwrap_or(e.width as u32).min(SCREEN_W as u32);
                let h = aux.height.unwrap_or(e.height as u32).min(SCREEN_H as u32);
                if aux.width.is_some() {
                    aux.width = Some(w);
                }
                if aux.height.is_some() {
                    aux.height = Some(h);
                }
                if let Some(x) = aux.x {
                    aux.x = Some(x.clamp(0, (SCREEN_W as u32 - w) as i32));
                }
                if let Some(y) = aux.y {
                    aux.y = Some(y.clamp(0, (SCREEN_H as u32 - h) as i32));
                }
                // Yığın isteklerini yok say: sıra girdiye göre köprü tarafından yönetilir.
                aux.stack_mode = None;
                aux.sibling = None;
                let _ = c.conn.configure_window(e.window, &aux);
            }
            Event::MapNotify(e) if e.event == c.root => self.adopt(c, e.window, e.override_redirect),
            Event::UnmapNotify(e) if e.event == c.root => self.forget(c, e.window),
            Event::DestroyNotify(e) if e.event == c.root => self.forget(c, e.window),
            Event::ConfigureNotify(e) if e.event == c.root => {
                let mut st = c.st.lock().unwrap();
                let Some(w) = st.wins.get_mut(&e.window) else { return };
                let resized = w.w != e.width || w.h != e.height;
                (w.x, w.y, w.w, w.h, w.bw) = (e.x, e.y, e.width, e.height, e.border_width);
                if resized {
                    let _ = c.conn.free_pixmap(w.pixmap);
                    if let Ok(p) = c.conn.generate_id() {
                        w.pixmap = p;
                        let _ = c.conn.composite_name_window_pixmap(e.window, p);
                    }
                }
                let info = w.info();
                let (ww, wh) = (w.w, w.h);
                if resized {
                    c.send(&mut st, e.window, || Msg::Geom(ww, wh));
                }
                drop(st);
                self.broadcast(json!({"t": "geom", "win": info}));
            }
            Event::PropertyNotify(e) => {
                let a = &c.atoms;
                if e.atom == a._NET_WM_NAME || e.atom == u32::from(AtomEnum::WM_NAME) {
                    let title = c.title(e.window);
                    let changed = match c.st.lock().unwrap().wins.get_mut(&e.window) {
                        Some(w) if w.title != title => {
                            w.title = title.clone();
                            true
                        }
                        _ => false,
                    };
                    if changed {
                        self.broadcast(json!({"t": "title", "id": e.window, "title": title}));
                    }
                } else if e.atom == u32::from(AtomEnum::WM_NORMAL_HINTS) {
                    let hints = c.hints(e.window);
                    let mut st = c.st.lock().unwrap();
                    if let Some(w) = st.wins.get_mut(&e.window) {
                        (w.minw, w.minh, w.maxw, w.maxh) = hints;
                    }
                }
            }
            Event::ClientMessage(e) => {
                let a = &c.atoms;
                let known = c.st.lock().unwrap().wins.contains_key(&e.window);
                if !known {
                    return;
                }
                if e.type_ == a._NET_ACTIVE_WINDOW {
                    self.broadcast(json!({"t": "activate", "id": e.window}));
                } else if e.type_ == a.WM_CHANGE_STATE && e.data.as_data32()[0] == 3 {
                    self.broadcast(json!({"t": "minimize", "id": e.window}));
                }
            }
            Event::DamageNotify(e) => {
                let _ = c.conn.damage_subtract(e.damage, NONE, c.region);
                let rects = match c.conn.xfixes_fetch_region(c.region).map(|ck| ck.reply()) {
                    Ok(Ok(r)) => r.rectangles,
                    _ => vec![e.area],
                };
                let mut st = c.st.lock().unwrap();
                c.send(&mut st, e.drawable, move || Msg::Damage(rects.clone()));
            }
            Event::XfixesCursorNotify(_) => {
                if let Some(cur) = c.cursor_json() {
                    let mut st = c.st.lock().unwrap();
                    st.cursor = Some(cur.clone());
                    for subs in st.subs.values() {
                        for (_, tx) in subs {
                            let _ = tx.send(Msg::Cursor(cur.clone()));
                        }
                    }
                }
            }
            Event::Error(e) => tracing::debug!(?e, "x11 hatası"),
            _ => {}
        }
    }

    /// Haritalanan kök çocuğunu izlemeye alır.
    fn adopt(&self, c: &Arc<Ctx>, wid: Window, or: bool) {
        if wid == c.dismiss_win || c.st.lock().unwrap().wins.contains_key(&wid) {
            return;
        }
        let Some(mut w) = c.describe(wid, or) else { return };
        {
            let mut st = c.st.lock().unwrap();
            if w.popup {
                let owner = if w.transient != 0 && st.wins.contains_key(&w.transient) { w.transient } else { st.last_input };
                // Sahibin de açılır pencere olduğu alt menülerde en üstteki yönetilen pencereye çık.
                w.parent = match st.wins.get(&owner) {
                    Some(o) if o.popup => o.parent,
                    Some(_) => owner,
                    None => st.wins.values().filter(|x| !x.popup).map(|x| x.id).next().unwrap_or(0),
                };
            } else {
                w.parent = if st.wins.get(&w.transient).is_some_and(|t| !t.popup) { w.transient } else { 0 };
                st.last_input = wid;
            }
            st.wins.insert(wid, w.clone());
            c.update_client_list(&st);
        }
        let _ = c.conn.flush();
        tracing::debug!(id = wid, title = %w.title, popup = w.popup, "pencere açıldı");
        self.broadcast(json!({"t": "open", "win": w.info()}));
    }

    fn forget(&self, c: &Arc<Ctx>, wid: Window) {
        let mut st = c.st.lock().unwrap();
        let Some(w) = st.wins.remove(&wid) else { return };
        let _ = c.conn.damage_destroy(w.damage);
        let _ = c.conn.free_pixmap(w.pixmap);
        if let Some(subs) = st.subs.remove(&wid) {
            for (_, tx) in subs {
                let _ = tx.send(Msg::Gone);
            }
        }
        if st.top == wid {
            st.top = 0;
        }
        if st.last_input == wid {
            st.last_input = 0;
        }
        c.update_client_list(&st);
        drop(st);
        self.broadcast(json!({"t": "close", "id": wid}));
    }
}

impl Ctx {
    fn connect(d: &Display) -> Result<Self, Error> {
        let (conn, scr) = RustConnection::connect(Some(&d.name()))?;
        let root = conn.setup().roots[scr].root;
        conn.composite_query_version(0, 4)?.reply()?;
        conn.damage_query_version(1, 1)?.reply()?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        conn.xtest_get_version(2, 2)?.reply()?;
        conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY))?
            .check()
            .map_err(|e| format!("başka bir pencere yöneticisi çalışıyor: {e:?}"))?;
        conn.composite_redirect_subwindows(root, Redirect::AUTOMATIC)?.check()?;
        conn.xfixes_select_cursor_input(root, CursorNotifyMask::DISPLAY_CURSOR)?;
        let atoms = Atoms::new(&conn)?.reply()?;
        let region = conn.generate_id()?;
        conn.xfixes_create_region(region, &[])?;

        // EWMH: pencere yöneticisi var (Qt/GTK bazı davranışları buna göre seçer).
        let check = conn.generate_id()?;
        conn.create_window(0, check, root, -10, -10, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())?;
        conn.change_property32(PropMode::REPLACE, check, atoms._NET_SUPPORTING_WM_CHECK, AtomEnum::WINDOW, &[check])?;
        conn.change_property8(PropMode::REPLACE, check, atoms._NET_WM_NAME, atoms.UTF8_STRING, b"MakerOS")?;
        conn.change_property32(PropMode::REPLACE, root, atoms._NET_SUPPORTING_WM_CHECK, AtomEnum::WINDOW, &[check])?;
        conn.change_property32(PropMode::REPLACE, root, atoms._NET_SUPPORTED, AtomEnum::ATOM, &[atoms._NET_ACTIVE_WINDOW, atoms._NET_WM_NAME, atoms._NET_CLIENT_LIST, atoms._NET_SUPPORTING_WM_CHECK])?;

        // Açık menüyü kapatmak için tıklanacak 1×1 pencere (ekranın sağ alt köşesinde).
        let dismiss_win = conn.generate_id()?;
        conn.create_window(0, dismiss_win, root, SCREEN_W as i16 - 1, SCREEN_H as i16 - 1, 1, 1, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new().override_redirect(1))?;
        conn.map_window(dismiss_win)?;

        // Tuş tekrarı tarayıcıdan gelir; sunucu kendisi tekrarlamasın (takılı tuş olmaz).
        conn.change_keyboard_control(&ChangeKeyboardControlAux::new().auto_repeat_mode(AutoRepeatMode::OFF))?;
        let keymap = Keymap::load(&conn)?;
        conn.flush()?;
        Ok(Self { conn, root, atoms, region, dismiss_win, st: Mutex::new(State::default()), keymap: Mutex::new(keymap) })
    }

    fn send(&self, st: &mut State, wid: u32, mk: impl Fn() -> Msg) {
        if let Some(subs) = st.subs.get_mut(&wid) {
            subs.retain(|(_, tx)| tx.send(mk()).is_ok());
        }
    }

    /// Yeni yönetilen pencereyi ekrana sığdırır.
    fn place(&self, wid: Window) -> Result<(), Error> {
        let g = self.conn.get_geometry(wid)?.reply()?;
        let w = g.width.min(SCREEN_W);
        let h = g.height.min(SCREEN_H);
        let x = (g.x as i32).clamp(0, (SCREEN_W - w) as i32);
        let y = (g.y as i32).clamp(0, (SCREEN_H - h) as i32);
        if (x, y, w, h) != (g.x as i32, g.y as i32, g.width, g.height) {
            self.conn.configure_window(wid, &ConfigureWindowAux::new().x(x).y(y).width(w as u32).height(h as u32))?;
        }
        Ok(())
    }

    fn describe(&self, wid: Window, or: bool) -> Option<Win> {
        let a = self.conn.get_window_attributes(wid).ok()?.reply().ok()?;
        if a.class == WindowClass::INPUT_ONLY {
            return None;
        }
        let g = self.conn.get_geometry(wid).ok()?.reply().ok()?;
        // Ekran dışındaki (sürükle-bırak, ölçüm amaçlı) ve 1×1 pencereler gösterilmez.
        if g.width <= 2 && g.height <= 2 || g.x as i32 + g.width as i32 <= 0 || g.y as i32 + g.height as i32 <= 0 || g.x >= SCREEN_W as i16 || g.y >= SCREEN_H as i16 {
            return None;
        }
        let damage = self.conn.generate_id().ok()?;
        self.conn.damage_create(damage, wid, ReportLevel::NON_EMPTY).ok()?;
        let pixmap = self.conn.generate_id().ok()?;
        let _ = self.conn.composite_name_window_pixmap(wid, pixmap);
        let _ = self.conn.change_window_attributes(wid, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE));
        let at = &self.atoms;
        let types = self.prop32(wid, at._NET_WM_WINDOW_TYPE, AtomEnum::ATOM.into());
        let popup_type = types.iter().any(|t| {
            [at._NET_WM_WINDOW_TYPE_DROPDOWN_MENU, at._NET_WM_WINDOW_TYPE_POPUP_MENU, at._NET_WM_WINDOW_TYPE_TOOLTIP, at._NET_WM_WINDOW_TYPE_COMBO, at._NET_WM_WINDOW_TYPE_DND, at._NET_WM_WINDOW_TYPE_MENU].contains(t)
        });
        let protos = self.prop32(wid, at.WM_PROTOCOLS, AtomEnum::ATOM.into());
        let class = self.prop8(wid, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into());
        let class = class.split(|&b| b == 0).filter(|s| !s.is_empty()).next_back().map(|s| String::from_utf8_lossy(s).into_owned()).unwrap_or_default();
        let (minw, minh, maxw, maxh) = self.hints(wid);
        Some(Win {
            id: wid,
            x: g.x,
            y: g.y,
            w: g.width,
            h: g.height,
            bw: g.border_width,
            depth: g.depth,
            popup: or || popup_type,
            title: self.title(wid),
            class,
            transient: self.prop32(wid, AtomEnum::WM_TRANSIENT_FOR.into(), AtomEnum::WINDOW.into()).first().copied().unwrap_or(0),
            parent: 0,
            damage,
            pixmap,
            delete: protos.contains(&at.WM_DELETE_WINDOW),
            take_focus: protos.contains(&at.WM_TAKE_FOCUS),
            minw,
            minh,
            maxw,
            maxh,
            pid: self.prop32(wid, at._NET_WM_PID, AtomEnum::CARDINAL.into()).first().copied().unwrap_or(0),
        })
    }

    fn prop32(&self, wid: Window, prop: Atom, ty: Atom) -> Vec<u32> {
        match self.conn.get_property(false, wid, prop, ty, 0, 1024).map(|c| c.reply()) {
            Ok(Ok(r)) => r.value32().map(|v| v.collect()).unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn prop8(&self, wid: Window, prop: Atom, ty: Atom) -> Vec<u8> {
        match self.conn.get_property(false, wid, prop, ty, 0, 1024).map(|c| c.reply()) {
            Ok(Ok(r)) if r.format == 8 => r.value,
            _ => Vec::new(),
        }
    }

    fn title(&self, wid: Window) -> String {
        let t = self.prop8(wid, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING);
        if !t.is_empty() {
            return String::from_utf8_lossy(&t).into_owned();
        }
        // WM_NAME: STRING (Latin-1) ya da COMPOUND_TEXT; ASCII dışı baytlar Latin-1 sayılır.
        match self.conn.get_property(false, wid, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 1024).map(|c| c.reply()) {
            Ok(Ok(r)) if r.format == 8 => r.value.iter().map(|&b| b as char).collect(),
            _ => String::new(),
        }
    }

    /// WM_NORMAL_HINTS: (min_w, min_h, max_w, max_h); yoksa 0.
    fn hints(&self, wid: Window) -> (u16, u16, u16, u16) {
        let v = self.prop32(wid, AtomEnum::WM_NORMAL_HINTS.into(), AtomEnum::WM_SIZE_HINTS.into());
        if v.len() < 9 {
            return (0, 0, 0, 0);
        }
        let f = v[0];
        let c = |x: u32| x.min(u16::MAX as u32) as u16;
        let (mw, mh) = if f & 16 != 0 { (c(v[5]), c(v[6])) } else { (0, 0) };
        let (xw, xh) = if f & 32 != 0 { (c(v[7]), c(v[8])) } else { (0, 0) };
        (mw, mh, xw, xh)
    }

    fn update_client_list(&self, st: &State) {
        let ids: Vec<u32> = st.wins.values().filter(|w| !w.popup).map(|w| w.id).collect();
        let _ = self.conn.change_property32(PropMode::REPLACE, self.root, self.atoms._NET_CLIENT_LIST, AtomEnum::WINDOW, &ids);
    }

    fn cursor_json(&self) -> Option<Arc<String>> {
        let r = self.conn.xfixes_get_cursor_image().ok()?.reply().ok()?;
        let mut rgba = Vec::with_capacity(r.cursor_image.len() * 4);
        for p in &r.cursor_image {
            let a = (p >> 24) & 0xff;
            let un = |v: u32| if a == 0 { 0 } else { ((v * 255 + a / 2) / a).min(255) as u8 };
            rgba.extend_from_slice(&[un((p >> 16) & 0xff), un((p >> 8) & 0xff), un(p & 0xff), a as u8]);
        }
        use base64::Engine;
        let b = base64::engine::general_purpose::STANDARD.encode(&rgba);
        Some(Arc::new(json!({"t": "cursor", "w": r.width, "h": r.height, "hx": r.xhot, "hy": r.yhot, "serial": r.cursor_serial, "rgba": b}).to_string()))
    }

    // --- akışlar için ---------------------------------------------------------

    pub fn win(&self, wid: u32) -> Option<Win> {
        self.st.lock().unwrap().wins.get(&wid).cloned()
    }

    /// Pencere akışı aboneliği.
    pub fn subscribe(&self, wid: u32) -> Option<(u64, mpsc::UnboundedReceiver<Msg>, Option<Arc<String>>)> {
        let mut st = self.st.lock().unwrap();
        if !st.wins.contains_key(&wid) {
            return None;
        }
        let (tx, rx) = mpsc::unbounded_channel();
        st.next_sub += 1;
        let id = st.next_sub;
        st.subs.entry(wid).or_default().push((id, tx));
        if st.cursor.is_none() {
            drop(st);
            let cur = self.cursor_json();
            st = self.st.lock().unwrap();
            st.cursor = cur;
        }
        Some((id, rx, st.cursor.clone()))
    }

    pub fn unsubscribe(&self, wid: u32, id: u64) {
        let mut st = self.st.lock().unwrap();
        if let Some(v) = st.subs.get_mut(&wid) {
            v.retain(|(i, _)| *i != id);
        }
    }

    // --- girdi ---------------------------------------------------------------

    fn key_of(&self, ks: u32) -> Option<u8> {
        self.keymap.lock().unwrap().find(ks).map(|(k, _)| k)
    }

    fn fake(&self, ty: u8, detail: u8, x: i16, y: i16) {
        let _ = self.conn.xtest_fake_input(ty, detail, CURRENT_TIME, if ty == MOTION_NOTIFY_EVENT { self.root } else { NONE }, x, y, 0);
    }

    /// Basılı değişkenleri istenen duruma getirir.
    fn sync_mods(&self, st: &mut State, want: u8) {
        for (bit, ks) in [(MOD_SHIFT, keys::SHIFT_L), (MOD_CTRL, keys::CONTROL_L), (MOD_ALT, keys::ALT_L), (MOD_SUPER, keys::SUPER_L)] {
            if (st.mods & bit) != (want & bit) {
                if let Some(kc) = self.key_of(ks) {
                    self.fake(if want & bit != 0 { KEY_PRESS_EVENT } else { KEY_RELEASE_EVENT }, kc, 0, 0);
                }
                st.mods ^= bit;
            }
        }
    }

    /// Girdi alacak yönetilen pencereyi X'te en üste alır (açılır menüler onun da üstünde kalır).
    fn raise(&self, st: &mut State, wid: u32) {
        let Some(w) = st.wins.get(&wid) else { return };
        if w.popup {
            return;
        }
        st.last_input = wid;
        if st.top == wid {
            return;
        }
        st.top = wid;
        let _ = self.conn.configure_window(wid, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
        for p in st.wins.values().filter(|w| w.popup) {
            let _ = self.conn.configure_window(p.id, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
        }
    }

    /// Pencere içi (x,y) noktasına fareyi götürür.
    fn point(&self, st: &mut State, wid: u32, x: i32, y: i32) -> bool {
        let Some((ox, oy)) = st.wins.get(&wid).map(Win::origin) else { return false };
        self.raise(st, wid);
        let rx = (ox + x).clamp(0, SCREEN_W as i32 - 1) as i16;
        let ry = (oy + y).clamp(0, SCREEN_H as i32 - 1) as i16;
        self.fake(MOTION_NOTIFY_EVENT, 0, rx, ry);
        true
    }

    pub fn motion(&self, wid: u32, x: i32, y: i32, mods: u8) {
        let mut st = self.st.lock().unwrap();
        self.sync_mods(&mut st, mods);
        self.point(&mut st, wid, x, y);
        let _ = self.conn.flush();
    }

    pub fn button(&self, wid: u32, x: i32, y: i32, button: u8, down: bool, mods: u8) {
        let mut st = self.st.lock().unwrap();
        if down {
            self.focus_locked(&mut st, wid);
        }
        self.sync_mods(&mut st, mods);
        if self.point(&mut st, wid, x, y) {
            self.fake(if down { BUTTON_PRESS_EVENT } else { BUTTON_RELEASE_EVENT }, button, 0, 0);
        }
        let _ = self.conn.flush();
    }

    /// Tekerlek: dy>0 aşağı (düğme 5), dx>0 sağa (düğme 7); değerler adım sayısıdır.
    pub fn wheel(&self, wid: u32, x: i32, y: i32, dx: i32, dy: i32, mods: u8) {
        let mut st = self.st.lock().unwrap();
        self.sync_mods(&mut st, mods);
        if self.point(&mut st, wid, x, y) {
            for (n, pos, neg) in [(dy, 5u8, 4u8), (dx, 7, 6)] {
                let b = if n > 0 { pos } else { neg };
                for _ in 0..n.unsigned_abs().min(20) {
                    self.fake(BUTTON_PRESS_EVENT, b, 0, 0);
                    self.fake(BUTTON_RELEASE_EVENT, b, 0, 0);
                }
            }
        }
        let _ = self.conn.flush();
    }

    /// Tuş. `printable`: tarayıcının ürettiği karakter (düzen uygulanmış); Ctrl/Alt/Super yoksa
    /// Shift karakterin X'teki düzeyine göre ayarlanır.
    pub fn key(&self, wid: u32, ks: u32, down: bool, mods: u8, printable: bool) {
        let mut st = self.st.lock().unwrap();
        if down {
            if st.focus != wid {
                self.focus_locked(&mut st, wid);
            }
            let r = self.keymap.lock().unwrap().resolve(&self.conn, ks);
            let Some((kc, shift)) = r else { return };
            let want = if printable && mods & (MOD_CTRL | MOD_ALT | MOD_SUPER) == 0 { (mods & !MOD_SHIFT) | if shift { MOD_SHIFT } else { 0 } } else { mods };
            self.sync_mods(&mut st, want);
            self.fake(KEY_PRESS_EVENT, kc, 0, 0);
            st.down.insert(ks, kc);
        } else {
            let kc = st.down.remove(&ks).or_else(|| self.key_of(ks));
            if let Some(kc) = kc {
                self.fake(KEY_RELEASE_EVENT, kc, 0, 0);
            }
        }
        let _ = self.conn.flush();
    }

    /// Metni (yapıştırma, IME) karakter karakter yazar.
    pub fn text(&self, wid: u32, s: &str) {
        for ch in s.chars().take(4096) {
            let ks = match ch {
                '\n' => 0xff0d,
                '\t' => 0xff09,
                c if c.is_control() => continue,
                c => keys::keysym_of_char(c),
            };
            self.key(wid, ks, true, 0, true);
            self.key(wid, ks, false, 0, true);
        }
        let mut st = self.st.lock().unwrap();
        self.sync_mods(&mut st, 0);
        let _ = self.conn.flush();
    }

    /// Basılı tuş ve değişkenleri bırakır (pencere odağı kaybedince).
    pub fn release_all(&self) {
        let mut st = self.st.lock().unwrap();
        for (_, kc) in std::mem::take(&mut st.down) {
            self.fake(KEY_RELEASE_EVENT, kc, 0, 0);
        }
        self.sync_mods(&mut st, 0);
        let _ = self.conn.flush();
    }

    fn focus_locked(&self, st: &mut State, wid: u32) {
        let Some(w) = st.wins.get(&wid).cloned() else { return };
        if w.popup {
            return;
        }
        self.raise(st, wid);
        st.focus = wid;
        let _ = self.conn.set_input_focus(InputFocus::POINTER_ROOT, wid, CURRENT_TIME);
        if w.take_focus {
            let ev = ClientMessageEvent::new(32, wid, self.atoms.WM_PROTOCOLS, [self.atoms.WM_TAKE_FOCUS, CURRENT_TIME, 0, 0, 0]);
            let _ = self.conn.send_event(false, wid, EventMask::NO_EVENT, ev);
        }
        let _ = self.conn.change_property32(PropMode::REPLACE, self.root, self.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, &[wid]);
    }

    pub fn focus(&self, wid: u32) {
        let mut st = self.st.lock().unwrap();
        self.focus_locked(&mut st, wid);
        let _ = self.conn.flush();
    }

    /// Pencereyi kapatmayı ister (WM_DELETE_WINDOW; desteklemiyorsa istemciyi keser).
    pub fn close(&self, wid: u32) {
        let Some(w) = self.win(wid) else { return };
        if w.delete {
            let ev = ClientMessageEvent::new(32, wid, self.atoms.WM_PROTOCOLS, [self.atoms.WM_DELETE_WINDOW, CURRENT_TIME, 0, 0, 0]);
            let _ = self.conn.send_event(false, wid, EventMask::NO_EVENT, ev);
        } else {
            let _ = self.conn.kill_client(wid);
        }
        let _ = self.conn.flush();
    }

    /// Kabukta pencere dışına tıklandı: açık menüyü kapatmak için boş bir noktaya tıkla.
    pub fn dismiss(&self) {
        let has_popup = self.st.lock().unwrap().wins.values().any(|w| w.popup);
        if !has_popup {
            return;
        }
        let _ = self.conn.configure_window(self.dismiss_win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
        self.fake(MOTION_NOTIFY_EVENT, 0, SCREEN_W as i16 - 1, SCREEN_H as i16 - 1);
        self.fake(BUTTON_PRESS_EVENT, 1, 0, 0);
        self.fake(BUTTON_RELEASE_EVENT, 1, 0, 0);
        let _ = self.conn.flush();
    }

    /// Kabuktaki pencere boyutu (içerik alanı) değişti.
    pub fn resize(&self, wid: u32, w: u16, h: u16) {
        let Some(win) = self.win(wid) else { return };
        if win.popup {
            return;
        }
        let lim = |v: u16, min: u16, max: u16, screen: u16| {
            let v = v.max(min.max(1));
            let v = if max > 0 { v.min(max) } else { v };
            v.min(screen)
        };
        let nw = lim(w, win.minw, win.maxw, SCREEN_W);
        let nh = lim(h, win.minh, win.maxh, SCREEN_H);
        if (nw, nh) == (win.w, win.h) {
            return;
        }
        let x = (win.x as i32).clamp(0, (SCREEN_W - nw) as i32);
        let y = (win.y as i32).clamp(0, (SCREEN_H - nh) as i32);
        let _ = self.conn.configure_window(wid, &ConfigureWindowAux::new().x(x).y(y).width(nw as u32).height(nh as u32));
        let _ = self.conn.flush();
    }

    /// Pencerenin (ya da bir bölgesinin) piksellerini BGRX/BGRA olarak okur.
    pub fn grab(&self, wid: u32, r: Rectangle) -> Option<(Vec<u8>, u8)> {
        let (pix, bw, depth) = {
            let st = self.st.lock().unwrap();
            let w = st.wins.get(&wid)?;
            (w.pixmap, w.bw, w.depth)
        };
        let img = self.conn.get_image(ImageFormat::Z_PIXMAP, pix, r.x + bw as i16, r.y + bw as i16, r.width, r.height, !0).ok()?.reply().ok()?;
        (img.data.len() >= r.width as usize * r.height as usize * 4).then_some((img.data, depth))
    }
}
