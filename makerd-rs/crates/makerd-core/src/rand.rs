//! Çekirdek rastgele sayı üreteci (getrandom(2)); kütüphane bağımlılığı yok.

use std::io::Read;

/// `buf`'u kriptografik rastgele baytlarla doldurur.
pub fn fill(buf: &mut [u8]) {
    let mut off = 0;
    while off < buf.len() {
        let rest = &mut buf[off..];
        // SAFETY: rest geçerli, yazılabilir bir dilimdir.
        let r = unsafe { libc::getrandom(rest.as_mut_ptr().cast(), rest.len(), 0) };
        if r > 0 {
            off += r as usize;
            continue;
        }
        if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        // getrandom yoksa (çok eski çekirdek) /dev/urandom.
        let mut f = std::fs::File::open("/dev/urandom").expect("rastgele kaynak yok");
        f.read_exact(rest).expect("rastgele kaynak okunamadı");
        return;
    }
}

pub fn bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    fill(&mut b);
    b
}

/// n rastgele baytın base64url gösterimi (Go `auth.RandomToken`).
pub fn token(n: usize) -> String {
    let mut b = vec![0u8; n];
    fill(&mut b);
    crate::b64::encode_url(&b)
}

/// n rastgele baytın küçük harf onaltılık gösterimi.
pub fn hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Kriptografik olmayan hızlı üreteç (mock dalgalanmaları için).
pub struct XorShift(u64);

impl XorShift {
    pub fn new() -> Self {
        Self(u64::from_le_bytes(bytes::<8>()) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// [0,1) aralığında.
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// [0,n) aralığında.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }
}

impl Default for XorShift {
    fn default() -> Self {
        Self::new()
    }
}
