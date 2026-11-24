//! Clipboard through the OSC 52 terminal escape sequence.

use std::io::Write;

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn copy(text: &str) {
    let mut o = std::io::stdout();
    let _ = write!(o, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = o.flush();
}

#[cfg(test)]
mod tests {
    #[test]
    fn b64() {
        assert_eq!(super::base64(b"hello"), "aGVsbG8=");
        assert_eq!(super::base64(b"hi!"), "aGkh");
        assert_eq!(super::base64(b"a"), "YQ==");
    }
}
