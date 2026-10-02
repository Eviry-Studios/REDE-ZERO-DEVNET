//! Hashes para escalares e pontos, com separação de domínio, e geradores.

use std::sync::OnceLock;

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;

pub mod context {
    pub const PEDERSEN_H: &str = "rede-zero/pedersen-h/v1";
    pub const VIEW_KEY: &str = "rede-zero/shielded-view-key/v1";
    pub const SPEND_KEY: &str = "rede-zero/shielded-spend-key/v1";
    pub const OUTPUT_KEY: &str = "rede-zero/output-key/v1";
    pub const OUTPUT_BLINDING: &str = "rede-zero/output-blinding/v1";
    pub const OUTPUT_AMOUNT: &str = "rede-zero/output-amount/v1";
    pub const KEY_IMAGE_BASE: &str = "rede-zero/key-image-base/v1";
    pub const CLSAG_AGG_P: &str = "rede-zero/clsag-agg-p/v1";
    pub const CLSAG_AGG_C: &str = "rede-zero/clsag-agg-c/v1";
    pub const CLSAG_ROUND: &str = "rede-zero/clsag-round/v1";
    pub const EXCESS: &str = "rede-zero/excess-proof/v1";
    pub const RANGE_PROOF: &[u8] = b"rede-zero/range-proof/v1";
}

fn xof64(ctx: &str, parts: &[&[u8]]) -> [u8; 64] {
    let mut h = blake3::Hasher::new_derive_key(ctx);
    for p in parts {
        h.update(&(p.len() as u64).to_be_bytes());
        h.update(p);
    }
    let mut out = [0u8; 64];
    h.finalize_xof().fill(&mut out);
    out
}

/// `Hs(ctx, partes)` — escalar uniforme (redução de 512 bits).
///
/// Cada parte recebe prefixo de comprimento, evitando ambiguidade.
pub fn hash_to_scalar(ctx: &str, parts: &[&[u8]]) -> Scalar {
    Scalar::from_bytes_mod_order_wide(&xof64(ctx, parts))
}

/// `Hp(ctx, partes)` — ponto de logaritmo discreto desconhecido
/// (mapeamento Elligator de Ristretto sobre 512 bits uniformes).
pub fn hash_to_point(ctx: &str, parts: &[&[u8]]) -> RistrettoPoint {
    RistrettoPoint::from_uniform_bytes(&xof64(ctx, parts))
}

/// Máscara de 8 bytes derivada de um segredo.
pub fn hash_mask8(ctx: &str, parts: &[&[u8]]) -> [u8; 8] {
    let full = xof64(ctx, parts);
    let mut out = [0u8; 8];
    out.copy_from_slice(&full[..8]);
    out
}

/// Gerador base `G` (chaves e fatores de ocultação).
pub fn g() -> RistrettoPoint {
    RISTRETTO_BASEPOINT_POINT
}

/// Gerador de valores `H`, com logaritmo discreto em relação a `G`
/// desconhecido por qualquer pessoa (hash para ponto de uma constante).
pub fn h() -> RistrettoPoint {
    static H: OnceLock<RistrettoPoint> = OnceLock::new();
    *H.get_or_init(|| hash_to_point(context::PEDERSEN_H, &[]))
}

/// Base da imagem de chave para uma chave de uso único: `Hp(P)`.
pub fn key_image_base(one_time_key: &[u8; 32]) -> RistrettoPoint {
    hash_to_point(context::KEY_IMAGE_BASE, &[one_time_key])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h_is_not_g() {
        assert_ne!(h(), g());
    }

    #[test]
    fn length_prefix_prevents_ambiguity() {
        assert_ne!(
            hash_to_scalar("t", &[b"ab", b"c"]),
            hash_to_scalar("t", &[b"a", b"bc"])
        );
    }

    #[test]
    fn domain_separation() {
        assert_ne!(hash_to_scalar("a", &[b"x"]), hash_to_scalar("b", &[b"x"]));
    }
}
