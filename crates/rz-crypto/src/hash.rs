use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};

use crate::hex;

/// Hash de 32 bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Hash32(pub [u8; 32]);

impl Hash32 {
    pub const ZERO: Hash32 = Hash32([0u8; 32]);

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        hex::decode_array(s).map(Self)
    }
}

impl fmt::Display for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}…", &self.to_hex()[..12])
    }
}

impl Encode for Hash32 {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.0);
    }
}

impl Decode for Hash32 {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self(d.fixed()?))
    }
}

/// Hash com separação de domínio: `BLAKE3-derive-key(contexto, dados)`.
///
/// O contexto deve ser uma das constantes de [`crate::context`].
pub fn hash(context: &str, data: &[u8]) -> Hash32 {
    let mut h = blake3::Hasher::new_derive_key(context);
    h.update(data);
    Hash32(*h.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context;

    #[test]
    fn deterministic() {
        assert_eq!(hash(context::TX_ID, b"x"), hash(context::TX_ID, b"x"));
    }

    #[test]
    fn domain_separated() {
        assert_ne!(hash(context::TX_ID, b"x"), hash(context::BLOCK_ID, b"x"));
    }

    #[test]
    fn sensitive_to_input() {
        assert_ne!(hash(context::TX_ID, b"x"), hash(context::TX_ID, b"y"));
    }

    #[test]
    fn hex_roundtrip() {
        let h = hash(context::TX_ID, b"x");
        assert_eq!(Hash32::from_hex(&h.to_hex()), Some(h));
    }
}
