//! Primitivas de privacidade transacional da Rede Zero (ADR-0009,
//! `spec/PRIVACY.md`).
//!
//! Tudo opera no grupo **Ristretto255**, de ordem prima: não há cofator,
//! pontos de torção nem múltiplas codificações do mesmo elemento.
//!
//! | Propriedade (REQ-024) | Mecanismo |
//! | --- | --- |
//! | Destinatário | endereços furtivos (chave de uso único por saída) |
//! | Valor | compromissos de Pedersen + Bulletproofs |
//! | Remetente e ligação entre transações | assinatura em anel CLSAG + imagem de chave |
//!
//! **Aviso:** implementação de referência para a DEVNET, ainda não auditada.
//! A privacidade oferecida é probabilística e não constitui anonimato
//! absoluto (REQ-028).

pub mod clsag;
pub mod codec;
pub mod commitment;
pub mod excess;
pub mod hash;
pub mod keys;
pub mod note;

use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
pub use curve25519_dalek::scalar::Scalar;

/// Ponto codificado (32 bytes). Só é aceito se decodificar em um elemento
/// válido de Ristretto255.
pub fn decode_point(bytes: &[u8; 32]) -> Option<RistrettoPoint> {
    CompressedRistretto(*bytes).decompress()
}

/// Escalar codificado (32 bytes). Rejeita representações não canônicas,
/// evitando maleabilidade.
pub fn decode_scalar(bytes: &[u8; 32]) -> Option<Scalar> {
    Option::from(Scalar::from_canonical_bytes(*bytes))
}

pub fn encode_point(p: &RistrettoPoint) -> [u8; 32] {
    p.compress().to_bytes()
}

/// Verifica a equação de balanço de uma transação privada:
/// `Σ pseudo_saídas = Σ compromissos_de_saída + público·H`,
/// onde `público` = taxa + valor retirado para conta transparente.
pub fn balance_holds(pseudo_outs: &[[u8; 32]], outputs: &[[u8; 32]], public: u64) -> bool {
    let mut lhs = RistrettoPoint::default();
    for c in pseudo_outs {
        match decode_point(c) {
            Some(p) => lhs += p,
            None => return false,
        }
    }
    let mut rhs = hash::h() * Scalar::from(public);
    for c in outputs {
        match decode_point(c) {
            Some(p) => rhs += p,
            None => return false,
        }
    }
    lhs == rhs
}
