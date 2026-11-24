//! Keysym → keycode eşlemesi. Tarayıcı karakteri (ya da tuş adını) keysym olarak gönderir;
//! X sunucusunun tuş haritasında yoksa (ör. US haritasında 'ş', '€') boş bir keycode
//! geçici olarak o keysym'e atanır (xdotool'un yöntemi). Böylece istemcinin klavye
//! düzeni ne olursa olsun yazılan karakter programa aynen ulaşır.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;

pub const SHIFT_L: u32 = 0xffe1;
pub const CONTROL_L: u32 = 0xffe3;
pub const ALT_L: u32 = 0xffe9;
pub const SUPER_L: u32 = 0xffeb;

pub struct Keymap {
    min: u8,
    per: usize,
    syms: Vec<u32>,
    /// Yeniden atanabilir (boş) keycode'lar ve sıradaki indeks.
    spare: Vec<u8>,
    next: usize,
}

impl Keymap {
    pub fn load<C: Connection>(c: &C) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = c.setup();
        let (min, max) = (s.min_keycode, s.max_keycode);
        let r = c.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let per = r.keysyms_per_keycode.max(1) as usize;
        let syms = r.keysyms;
        let mut spare: Vec<u8> = (0..=(max - min) as usize).filter(|i| syms[i * per..(i + 1) * per].iter().all(|&k| k == 0)).map(|i| min + i as u8).collect();
        // En yüksek boş keycode'lar önce (alt numaralar bazı programlarda özel anlam taşıyabilir).
        spare.reverse();
        spare.truncate(8);
        Ok(Self { min, per, syms, spare, next: 0 })
    }

    /// keysym'in keycode'u ve gereken Shift: (keycode, shift?).
    pub fn find(&self, ks: u32) -> Option<(u8, bool)> {
        let n = self.syms.len() / self.per;
        // Önce Shift'siz düzey, sonra Shift'li (düzey 0/1); 2+ düzeyler (AltGr) kullanılmaz.
        for lvl in 0..self.per.min(2) {
            for i in 0..n {
                if self.syms[i * self.per + lvl] == ks {
                    return Some((self.min + i as u8, lvl == 1));
                }
            }
        }
        // Küçük harf keysym'i (ör. 'a') yalnız büyük harf düzeyinde kayıtlıysa da kabul.
        None
    }

    /// keysym'i bir keycode'a yerleştirir (gerekirse boş keycode'u yeniden atar).
    pub fn resolve<C: Connection>(&mut self, c: &C, ks: u32) -> Option<(u8, bool)> {
        if let Some(r) = self.find(ks) {
            return Some(r);
        }
        if self.spare.is_empty() {
            return None;
        }
        let kc = self.spare[self.next % self.spare.len()];
        self.next += 1;
        let mut row = vec![0u32; self.per];
        row[0] = ks;
        if self.per > 1 {
            row[1] = ks;
        }
        c.change_keyboard_mapping(1, kc, self.per as u8, &row).ok()?;
        let i = (kc - self.min) as usize;
        self.syms[i * self.per..(i + 1) * self.per].copy_from_slice(&row);
        Some((kc, false))
    }
}

/// Unicode karakterden keysym (Latin-1 doğrudan, diğerleri 0x01000000 + kod noktası).
pub fn keysym_of_char(c: char) -> u32 {
    let cp = c as u32;
    match cp {
        0x20..=0x7e | 0xa0..=0xff => cp,
        _ => 0x0100_0000 | cp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms() {
        assert_eq!(keysym_of_char('a'), 0x61);
        assert_eq!(keysym_of_char('ü'), 0xfc);
        assert_eq!(keysym_of_char('ş'), 0x0100_015f);
        let km = Keymap { min: 8, per: 2, syms: vec![0, 0, 0x61, 0x41, 0x32, 0x40], spare: vec![8], next: 0 };
        assert_eq!(km.find(0x61), Some((9, false)));
        assert_eq!(km.find(0x40), Some((10, true)));
        assert_eq!(km.find(0x1b), None);
    }
}
