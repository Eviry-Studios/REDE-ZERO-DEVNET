//! Chaves e endereços privados (endereços furtivos com duas chaves).
//!
//! * chave de visualização `a` (pública `A = a·G`): reconhece saídas recebidas;
//! * chave de gasto `b` (pública `B = b·G`): autoriza gastos.
//!
//! Separar as duas permite, no futuro, entregar apenas a chave de
//! visualização a um auditor escolhido pelo usuário, sem dar poder de gasto.

use std::fmt;

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;

use crate::hash::{context, g, hash_to_scalar};
use crate::{decode_point, encode_point};

/// Prefixo textual de endereços privados.
pub const ADDRESS_PREFIX: &str = "zs";

/// Segredos privados, derivados deterministicamente da semente da Wallet.
#[derive(Clone)]
pub struct ShieldedSecret {
    pub view: Scalar,
    pub spend: Scalar,
}

impl ShieldedSecret {
    /// Deriva as chaves a partir da semente de 32 bytes da Wallet, em
    /// domínios distintos da chave transparente Ed25519.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            view: hash_to_scalar(context::VIEW_KEY, &[seed]),
            spend: hash_to_scalar(context::SPEND_KEY, &[seed]),
        }
    }

    pub fn address(&self) -> ShieldedAddress {
        ShieldedAddress {
            view: g() * self.view,
            spend: g() * self.spend,
        }
    }
}

impl fmt::Debug for ShieldedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ShieldedSecret(<oculto>)")
    }
}

/// Endereço privado público: `(A, B)`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShieldedAddress {
    pub view: RistrettoPoint,
    pub spend: RistrettoPoint,
}

impl ShieldedAddress {
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&encode_point(&self.view));
        out[32..].copy_from_slice(&encode_point(&self.spend));
        out
    }

    pub fn from_bytes(bytes: &[u8; 64]) -> Option<Self> {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        a.copy_from_slice(&bytes[..32]);
        b.copy_from_slice(&bytes[32..]);
        Some(Self {
            view: decode_point(&a)?,
            spend: decode_point(&b)?,
        })
    }

    /// Texto: `zs` + 128 dígitos hexadecimais.
    pub fn encode(&self) -> String {
        let mut s = String::from(ADDRESS_PREFIX);
        for b in self.to_bytes() {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    pub fn decode(s: &str) -> Option<Self> {
        let hex = s.strip_prefix(ADDRESS_PREFIX)?;
        if hex.len() != 128 {
            return None;
        }
        let mut bytes = [0u8; 64];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
        }
        Self::from_bytes(&bytes)
    }
}

impl fmt::Display for ShieldedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for ShieldedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ShieldedAddress({}…)", &self.encode()[..14])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_roundtrip() {
        let s = ShieldedSecret::from_seed(&[1; 32]);
        let a = s.address();
        assert_eq!(ShieldedSecret::from_seed(&[1; 32]).address(), a);
        assert_eq!(ShieldedAddress::decode(&a.encode()), Some(a));
        assert_ne!(s.view, s.spend);
    }

    #[test]
    fn rejects_invalid() {
        assert!(ShieldedAddress::decode("zs00").is_none());
        assert!(ShieldedAddress::decode(&"zz".repeat(65)).is_none());
        // 0xff…ff não é codificação válida de Ristretto.
        assert!(ShieldedAddress::decode(&format!("zs{}", "ff".repeat(64))).is_none());
    }
}
