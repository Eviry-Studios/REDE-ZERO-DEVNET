//! Compromissos de Pedersen e provas de faixa (Bulletproofs).
//!
//! `C = v·H + x·G`, onde `v` é o valor e `x` o fator de ocultação. O
//! compromisso esconde `v` e é aditivamente homomórfico, o que permite
//! verificar balanços sem revelar valores.

use std::sync::OnceLock;

use bulletproofs::{BulletproofGens, PedersenGens, RangeProof};
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use merlin::Transcript;

use crate::hash::{context, g, h};

/// Bits da prova de faixa: valores em `[0, 2^64)`.
pub const RANGE_BITS: usize = 64;
/// Tamanho máximo aceito para uma prova serializada.
pub const MAX_RANGE_PROOF_LEN: usize = 1024;

fn pc_gens() -> PedersenGens {
    PedersenGens {
        B: h(),
        B_blinding: g(),
    }
}

fn bp_gens() -> &'static BulletproofGens {
    static GENS: OnceLock<BulletproofGens> = OnceLock::new();
    GENS.get_or_init(|| BulletproofGens::new(RANGE_BITS, 1))
}

pub fn commit(value: u64, blinding: &Scalar) -> RistrettoPoint {
    h() * Scalar::from(value) + g() * blinding
}

/// Prova que o compromisso `commit(value, blinding)` contém um valor em
/// `[0, 2^64)` — impede "valores negativos" que criariam moeda.
pub fn prove_range(value: u64, blinding: &Scalar) -> Vec<u8> {
    let mut t = Transcript::new(context::RANGE_PROOF);
    let (proof, _) =
        RangeProof::prove_single(bp_gens(), &pc_gens(), &mut t, value, blinding, RANGE_BITS)
            .expect("parâmetros fixos e válidos");
    proof.to_bytes()
}

pub fn verify_range(commitment: &[u8; 32], proof: &[u8]) -> bool {
    if proof.len() > MAX_RANGE_PROOF_LEN {
        return false;
    }
    let Ok(proof) = RangeProof::from_bytes(proof) else {
        return false;
    };
    let c = curve25519_dalek::ristretto::CompressedRistretto(*commitment);
    if c.decompress().is_none() {
        return false;
    }
    let mut t = Transcript::new(context::RANGE_PROOF);
    proof
        .verify_single(bp_gens(), &pc_gens(), &mut t, &c, RANGE_BITS)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode_point;
    use rand_core::OsRng;

    #[test]
    fn homomorphic() {
        let (a, b) = (Scalar::random(&mut OsRng), Scalar::random(&mut OsRng));
        assert_eq!(commit(3, &a) + commit(4, &b), commit(7, &(a + b)));
    }

    #[test]
    fn range_proof_roundtrip() {
        let x = Scalar::random(&mut OsRng);
        let c = encode_point(&commit(1_000, &x));
        let proof = prove_range(1_000, &x);
        assert!(verify_range(&c, &proof));
        // Prova não serve para outro compromisso.
        let other = encode_point(&commit(1_001, &x));
        assert!(!verify_range(&other, &proof));
    }

    #[test]
    fn garbage_proof_rejected() {
        let c = encode_point(&commit(1, &Scalar::ONE));
        assert!(!verify_range(&c, &[0u8; 10]));
        assert!(!verify_range(&c, &vec![0u8; MAX_RANGE_PROOF_LEN + 1]));
    }
}
