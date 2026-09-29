//! Saídas privadas (notas): criação pelo remetente e detecção pelo
//! destinatário.
//!
//! Remetente, para o endereço `(A, B)`:
//!
//! ```text
//! r  ← aleatório;  R = r·G
//! s  = r·A                        (segredo compartilhado; destinatário: s = a·R)
//! k  = Hs(OUTPUT_KEY, s)
//! P  = k·G + B                    (chave de uso único)
//! x  = Hs(OUTPUT_BLINDING, s)     (fator de ocultação)
//! C  = v·H + x·G                  (compromisso do valor)
//! e  = v ⊕ H8(OUTPUT_AMOUNT, s)   (valor cifrado)
//! ```
//!
//! Quem gasta a nota conhece `p = k + b`, com `P = p·G`.

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use rand_core::{CryptoRng, RngCore};

use crate::commitment::{commit, prove_range};
use crate::hash::{context, g, hash_mask8, hash_to_scalar, key_image_base};
use crate::keys::{ShieldedAddress, ShieldedSecret};
use crate::{decode_point, encode_point};

/// Dados públicos de uma saída, como ficam registrados na cadeia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputData {
    pub one_time_key: [u8; 32],
    pub tx_pub: [u8; 32],
    pub commitment: [u8; 32],
    pub enc_amount: [u8; 8],
}

/// Saída recém-criada, com prova de faixa e segredos do remetente.
pub struct NewOutput {
    pub data: OutputData,
    pub range_proof: Vec<u8>,
    pub blinding: Scalar,
}

/// Nota reconhecida como própria pela Wallet.
#[derive(Clone, Debug)]
pub struct OwnedNote {
    pub amount: u64,
    pub blinding: Scalar,
    /// Chave privada de uso único `p` (com `P = p·G`).
    pub one_time_secret: Scalar,
}

fn shared_scalars(shared: &RistrettoPoint) -> (Scalar, Scalar, [u8; 8]) {
    let s = encode_point(shared);
    (
        hash_to_scalar(context::OUTPUT_KEY, &[&s]),
        hash_to_scalar(context::OUTPUT_BLINDING, &[&s]),
        hash_mask8(context::OUTPUT_AMOUNT, &[&s]),
    )
}

/// Cria uma saída de `amount` para `to`.
pub fn create_output<R: RngCore + CryptoRng>(
    to: &ShieldedAddress,
    amount: u64,
    rng: &mut R,
) -> NewOutput {
    let r = Scalar::random(rng);
    let tx_pub = g() * r;
    let (k, blinding, mask) = shared_scalars(&(to.view * r));
    let one_time_key = g() * k + to.spend;
    let commitment = commit(amount, &blinding);
    let mut enc_amount = amount.to_be_bytes();
    for (e, m) in enc_amount.iter_mut().zip(mask) {
        *e ^= m;
    }
    NewOutput {
        data: OutputData {
            one_time_key: encode_point(&one_time_key),
            tx_pub: encode_point(&tx_pub),
            commitment: encode_point(&commitment),
            enc_amount,
        },
        range_proof: prove_range(amount, &blinding),
        blinding,
    }
}

/// Verifica se a saída pertence a `secret` e, em caso positivo, recupera
/// valor e segredos. Saídas com compromisso inconsistente são ignoradas.
pub fn scan_output(secret: &ShieldedSecret, out: &OutputData) -> Option<OwnedNote> {
    let r_pub = decode_point(&out.tx_pub)?;
    let p = decode_point(&out.one_time_key)?;
    let (k, blinding, mask) = shared_scalars(&(r_pub * secret.view));
    if p - g() * k != g() * secret.spend {
        return None;
    }
    let mut amount = out.enc_amount;
    for (a, m) in amount.iter_mut().zip(mask) {
        *a ^= m;
    }
    let amount = u64::from_be_bytes(amount);
    if encode_point(&commit(amount, &blinding)) != out.commitment {
        return None;
    }
    Some(OwnedNote {
        amount,
        blinding,
        one_time_secret: k + secret.spend,
    })
}

/// Imagem de chave `I = p·Hp(P)`: única por nota, revelada ao gastar.
/// Impede gasto duplo sem revelar qual nota foi gasta.
pub fn key_image(one_time_secret: &Scalar, one_time_key: &[u8; 32]) -> [u8; 32] {
    encode_point(&(key_image_base(one_time_key) * one_time_secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitment::verify_range;
    use rand_core::OsRng;

    #[test]
    fn recipient_finds_note_others_do_not() {
        let alice = ShieldedSecret::from_seed(&[1; 32]);
        let eve = ShieldedSecret::from_seed(&[2; 32]);
        let out = create_output(&alice.address(), 777, &mut OsRng);
        assert!(verify_range(&out.data.commitment, &out.range_proof));

        let note = scan_output(&alice, &out.data).expect("nota da Alice");
        assert_eq!(note.amount, 777);
        assert_eq!(note.blinding, out.blinding);
        assert_eq!(
            encode_point(&(g() * note.one_time_secret)),
            out.data.one_time_key
        );
        assert!(scan_output(&eve, &out.data).is_none());
    }

    #[test]
    fn outputs_to_same_address_are_unlinkable() {
        let alice = ShieldedSecret::from_seed(&[1; 32]).address();
        let a = create_output(&alice, 1, &mut OsRng);
        let b = create_output(&alice, 1, &mut OsRng);
        assert_ne!(a.data.one_time_key, b.data.one_time_key);
        assert_ne!(a.data.commitment, b.data.commitment);
        assert_ne!(a.data.enc_amount, b.data.enc_amount);
    }

    #[test]
    fn tampered_amount_ignored() {
        let alice = ShieldedSecret::from_seed(&[1; 32]);
        let mut out = create_output(&alice.address(), 5, &mut OsRng).data;
        out.enc_amount[7] ^= 1;
        assert!(scan_output(&alice, &out).is_none());
    }

    #[test]
    fn key_image_deterministic_per_note() {
        let alice = ShieldedSecret::from_seed(&[1; 32]);
        let a = create_output(&alice.address(), 1, &mut OsRng).data;
        let b = create_output(&alice.address(), 1, &mut OsRng).data;
        let na = scan_output(&alice, &a).unwrap();
        let nb = scan_output(&alice, &b).unwrap();
        let ia = key_image(&na.one_time_secret, &a.one_time_key);
        assert_eq!(ia, key_image(&na.one_time_secret, &a.one_time_key));
        assert_ne!(ia, key_image(&nb.one_time_secret, &b.one_time_key));
    }
}
