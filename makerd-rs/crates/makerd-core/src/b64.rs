//! base64url (dolgusuz) kodlama: Go `base64.RawURLEncoding` ile aynı çıktı.

const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode_url(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(ALPHA[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn vectors() {
        assert_eq!(super::encode_url(b""), "");
        assert_eq!(super::encode_url(b"f"), "Zg");
        assert_eq!(super::encode_url(b"fo"), "Zm8");
        assert_eq!(super::encode_url(b"foo"), "Zm9v");
        assert_eq!(super::encode_url(&[0xfb, 0xff]), "-_8");
    }
}
