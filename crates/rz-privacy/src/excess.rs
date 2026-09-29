//! Prova de excesso: demonstra que um conjunto de compromissos soma um valor
//! público, sem abrir os compromissos individuais.
//!
//! Usada ao blindar ZERO (transparente → privado): o remetente revela o
//! valor total `V`, e prova conhecer `e` tal que `Σ C_j − V·H = e·G`
//! (assinatura de Schnorr sobre o ponto de excesso `E`).

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use rand_core::{CryptoRng, RngCore};

use crate::hash::{context, g, h, hash_to_scalar};
use crate::{decode_point, decode_scalar, encode_point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcessProof {
    pub r: [u8; 32],
    pub s: [u8; 32],
}

/// Ponto de excesso `E = Σ C_j − V·H`.
pub fn excess_point(commitments: &[[u8; 32]], value: u64) -> Option<RistrettoPoint> {
    let mut sum = h() * -Scalar::from(value);
    for c in commitments {
        sum += decode_point(c)?;
    }
    Some(sum)
}

pub fn prove<R: RngCore + CryptoRng>(
    excess_secret: &Scalar,
    message: &[u8; 32],
    rng: &mut R,
) -> ExcessProof {
    let e_point = encode_point(&(g() * excess_secret));
    let k = Scalar::random(rng);
    let r = encode_point(&(g() * k));
    let c = hash_to_scalar(context::EXCESS, &[&e_point, &r, message]);
    ExcessProof {
        r,
        s: (k + c * excess_secret).to_bytes(),
    }
}

pub fn verify(
    commitments: &[[u8; 32]],
    value: u64,
    message: &[u8; 32],
    proof: &ExcessProof,
) -> bool {
    let (Some(e), Some(r), Some(s)) = (
        excess_point(commitments, value),
        decode_point(&proof.r),
        decode_scalar(&proof.s),
    ) else {
        return false;
    };
    let c = hash_to_scalar(context::EXCESS, &[&encode_point(&e), &proof.r, message]);
    g() * s == r + e * c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitment::commit;
    use rand_core::OsRng;

    #[test]
    fn proves_sum() {
        let (x1, x2) = (Scalar::random(&mut OsRng), Scalar::random(&mut OsRng));
        let cs = [
            encode_point(&commit(30, &x1)),
            encode_point(&commit(12, &x2)),
        ];
        let proof = prove(&(x1 + x2), &[1; 32], &mut OsRng);
        assert!(verify(&cs, 42, &[1; 32], &proof));
        assert!(!verify(&cs, 43, &[1; 32], &proof), "valor inflado");
        assert!(!verify(&cs, 42, &[2; 32], &proof), "outra mensagem");
    }
}
