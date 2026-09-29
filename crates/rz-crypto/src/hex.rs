//! Hexadecimal minúsculo, sem dependências externas.

const DIGITS: &[u8; 16] = b"0123456789abcdef";

pub fn encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    s
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub fn decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if s.len() % 2 != 0 {
        return None;
    }
    s.chunks_exact(2)
        .map(|p| Some((nibble(p[0])? << 4) | nibble(p[1])?))
        .collect()
}

pub fn decode_array<const N: usize>(s: &str) -> Option<[u8; N]> {
    decode(s)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let data = [0u8, 1, 0xab, 0xff];
        assert_eq!(encode(&data), "0001abff");
        assert_eq!(decode("0001ABff").unwrap(), data);
    }

    #[test]
    fn rejects_invalid() {
        assert!(decode("abc").is_none());
        assert!(decode("zz").is_none());
        assert!(decode_array::<2>("00").is_none());
    }
}
