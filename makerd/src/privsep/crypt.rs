//! SHA-512 crypt (`$6$`), Ulrich Drepper'ın "Unix crypt using SHA-256 and SHA-512"
//! belirtimine göre. yescrypt (`$y$`) desteklenmez: MakerOS imajı /etc/login.defs
//! içinde ENCRYPT_METHOD SHA512 ile yapılandırılır.

use sha2::{Digest, Sha512};

const ITOA64: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ROUNDS_DEFAULT: u32 = 5000;
const ROUNDS_MIN: u32 = 1000;
const ROUNDS_MAX: u32 = 999_999_999;
const SALT_MAX: usize = 16;
const NEW_HASH_ROUNDS: u32 = 65536;
const PREFIX: &str = "$6$";
const ROUNDS_PREFIX: &str = "rounds=";

#[derive(Debug, PartialEq, Eq)]
pub enum CryptError {
    /// Tanınmayan özet biçimi (ör. yescrypt).
    Unsupported,
    BadRounds,
}

impl std::fmt::Display for CryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptError::Unsupported => f.write_str("desteklenmeyen parola özeti biçimi"),
            CryptError::BadRounds => f.write_str("geçersiz rounds alanı"),
        }
    }
}

/// Parolayı crypt(3) biçimli özetle sabit sürede karşılaştırır.
pub fn verify(password: &str, hash: &str) -> Result<bool, CryptError> {
    if !hash.starts_with(PREFIX) {
        return Err(CryptError::Unsupported);
    }
    let got = sha512_crypt(password, hash)?;
    Ok(crate::auth::constant_eq(got.as_bytes(), hash.as_bytes()))
}

/// Yeni rastgele tuzla `$6$rounds=65536$` özeti.
pub fn hash_password(password: &str) -> String {
    let mut raw = [0u8; SALT_MAX];
    rand::fill(&mut raw);
    let salt: String = raw.iter().map(|b| ITOA64[(*b & 0x3f) as usize] as char).collect();
    sha512_crypt(password, &format!("{PREFIX}{ROUNDS_PREFIX}{NEW_HASH_ROUNDS}${salt}")).expect("geçerli ayar")
}

/// `$6$[rounds=N$]salt[$...]` ayarıyla özeti hesaplar.
pub fn sha512_crypt(password: &str, setting: &str) -> Result<String, CryptError> {
    let mut rest = setting.strip_prefix(PREFIX).ok_or(CryptError::Unsupported)?;
    let (mut rounds, mut custom) = (ROUNDS_DEFAULT, false);
    if let Some(r) = rest.strip_prefix(ROUNDS_PREFIX) {
        let (num, after) = r.split_once('$').ok_or(CryptError::BadRounds)?;
        let n: u64 = num.parse().map_err(|_| CryptError::BadRounds)?;
        rounds = n.clamp(ROUNDS_MIN as u64, ROUNDS_MAX as u64) as u32;
        custom = true;
        rest = after;
    }
    let salt = rest.split('$').next().unwrap_or("");
    let salt = &salt.as_bytes()[..salt.len().min(SALT_MAX)];
    let sum = sha512_sum(password.as_bytes(), salt, rounds);
    let mut out = String::from(PREFIX);
    if custom {
        out.push_str(&format!("{ROUNDS_PREFIX}{rounds}$"));
    }
    out.push_str(std::str::from_utf8(salt).unwrap_or(""));
    out.push('$');
    for i in 0..21 {
        let (mut x, mut y, mut z) = (sum[i], sum[i + 21], sum[i + 42]);
        match i % 3 {
            1 => (x, y, z) = (y, z, x),
            2 => (x, y, z) = (z, x, y),
            _ => {}
        }
        b64_from_24(&mut out, x, y, z, 4);
    }
    b64_from_24(&mut out, 0, 0, sum[63], 2);
    Ok(out)
}

fn b64_from_24(out: &mut String, b2: u8, b1: u8, b0: u8, n: usize) {
    let mut w = (u32::from(b2) << 16) | (u32::from(b1) << 8) | u32::from(b0);
    for _ in 0..n {
        out.push(ITOA64[(w & 0x3f) as usize] as char);
        w >>= 6;
    }
}

fn repeat_to(src: &[u8], n: usize) -> Vec<u8> {
    src.iter().cycle().take(n).copied().collect()
}

fn sha512_sum(p: &[u8], s: &[u8], rounds: u32) -> Vec<u8> {
    let b = Sha512::new().chain_update(p).chain_update(s).chain_update(p).finalize();

    let mut ha = Sha512::new();
    ha.update(p);
    ha.update(s);
    ha.update(repeat_to(&b, p.len()));
    let mut i = p.len();
    while i > 0 {
        if i & 1 != 0 {
            ha.update(b);
        } else {
            ha.update(p);
        }
        i >>= 1;
    }
    let a = ha.finalize();

    let mut hp = Sha512::new();
    for _ in 0..p.len() {
        hp.update(p);
    }
    let pp = repeat_to(&hp.finalize(), p.len());

    let mut hs = Sha512::new();
    for _ in 0..16 + usize::from(a[0]) {
        hs.update(s);
    }
    let ss = repeat_to(&hs.finalize(), s.len());

    let mut c = a.to_vec();
    for r in 0..rounds {
        let mut h = Sha512::new();
        if r & 1 != 0 {
            h.update(&pp);
        } else {
            h.update(&c);
        }
        if r % 3 != 0 {
            h.update(&ss);
        }
        if r % 7 != 0 {
            h.update(&pp);
        }
        if r & 1 != 0 {
            h.update(&c);
        } else {
            h.update(&pp);
        }
        c = h.finalize().to_vec();
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drepper_vectors() {
        let cases = [
            ("Hello world!", "$6$saltstring", "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"),
            ("Hello world!", "$6$rounds=10000$saltstringsaltstring", "$6$rounds=10000$saltstringsaltst$OW1/O6BYHV6BcXZu8QVeXbDWra3Oeqh0sbHbbMCVNSnCM/UrjmM0Dp8vOuZeHBy/YTBmSK6H9qs/y3RnOaw5v."),
            ("This is just a test", "$6$rounds=5000$toolongsaltstring", "$6$rounds=5000$toolongsaltstrin$lQ8jolhgVRVhY4b5pZKaysCLi0QBxGoNeKQzQ3glMhwllF7oGDZxUhx1yxdYcz/e1JSbq3y6JMxxl8audkUEm0"),
        ];
        for (pw, setting, want) in cases {
            assert_eq!(sha512_crypt(pw, setting).unwrap(), want);
        }
    }

    #[test]
    fn hash_and_verify() {
        let h = hash_password("maker123");
        assert!(h.starts_with("$6$rounds=65536$"));
        assert_eq!(verify("maker123", &h), Ok(true));
        assert_eq!(verify("yanlis", &h), Ok(false));
        assert_eq!(verify("x", "$y$j9T$abc"), Err(CryptError::Unsupported));
    }
}
