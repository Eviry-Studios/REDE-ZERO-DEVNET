//! Assinatura em anel CLSAG (Goodell, Noether, RandomRun — 2019), sobre
//! Ristretto255.
//!
//! Prova, para um anel de saídas `(P_i, C_i)` e um compromisso auxiliar
//! `C'` (pseudo-saída), que o assinante:
//!
//! 1. conhece `p` com `P_l = p·G` para algum índice secreto `l`;
//! 2. conhece `z` com `C_l − C' = z·G` (mesmo valor, outra ocultação);
//! 3. produziu a imagem de chave `I = p·Hp(P_l)`.
//!
//! Sem revelar `l`. A mesma nota sempre gera a mesma `I`, o que permite
//! detectar gasto duplo.

use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::{Identity, MultiscalarMul};
use rand_core::{CryptoRng, RngCore};

use crate::hash::{context, g, hash_to_scalar, key_image_base};
use crate::{decode_point, decode_scalar, encode_point};

/// Assinatura: desafio inicial, respostas (uma por membro) e `D = z·Hp(P_l)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clsag {
    pub c0: [u8; 32],
    pub s: Vec<[u8; 32]>,
    pub d: [u8; 32],
}

/// Membro do anel: chave de uso único e compromisso, codificados.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingMember {
    pub one_time_key: [u8; 32],
    pub commitment: [u8; 32],
}

struct Prepared {
    p: Vec<RistrettoPoint>,
    c_offset: Vec<RistrettoPoint>,
    hp: Vec<RistrettoPoint>,
    mu_p: Scalar,
    mu_c: Scalar,
    round_prefix: Vec<u8>,
}

fn prepare(
    ring: &[RingMember],
    pseudo_out: &[u8; 32],
    key_image: &[u8; 32],
    d: &[u8; 32],
    message: &[u8; 32],
) -> Option<Prepared> {
    let c_pseudo = decode_point(pseudo_out)?;
    let mut p = Vec::with_capacity(ring.len());
    let mut c_offset = Vec::with_capacity(ring.len());
    let mut hp = Vec::with_capacity(ring.len());
    let mut transcript = Vec::with_capacity(ring.len() * 64 + 128);
    for m in ring {
        p.push(decode_point(&m.one_time_key)?);
        c_offset.push(decode_point(&m.commitment)? - c_pseudo);
        hp.push(key_image_base(&m.one_time_key));
        transcript.extend_from_slice(&m.one_time_key);
        transcript.extend_from_slice(&m.commitment);
    }
    transcript.extend_from_slice(pseudo_out);
    transcript.extend_from_slice(key_image);
    transcript.extend_from_slice(d);
    let mu_p = hash_to_scalar(context::CLSAG_AGG_P, &[&transcript]);
    let mu_c = hash_to_scalar(context::CLSAG_AGG_C, &[&transcript]);
    transcript.extend_from_slice(message);
    Some(Prepared {
        p,
        c_offset,
        hp,
        mu_p,
        mu_c,
        round_prefix: transcript,
    })
}

fn round(prefix: &[u8], l: &RistrettoPoint, r: &RistrettoPoint) -> Scalar {
    hash_to_scalar(
        context::CLSAG_ROUND,
        &[prefix, &encode_point(l), &encode_point(r)],
    )
}

/// Assina `message` com o membro `index` do anel.
///
/// `one_time_secret = p` e `commitment_secret = z` devem satisfazer
/// `P_index = p·G` e `C_index − C' = z·G`; caso contrário retorna `None`.
#[allow(clippy::too_many_arguments)]
pub fn sign<R: RngCore + CryptoRng>(
    ring: &[RingMember],
    index: usize,
    one_time_secret: &Scalar,
    commitment_secret: &Scalar,
    pseudo_out: &[u8; 32],
    message: &[u8; 32],
    rng: &mut R,
) -> Option<(Clsag, [u8; 32])> {
    let n = ring.len();
    if n == 0 || index >= n {
        return None;
    }
    let hp_l = key_image_base(&ring[index].one_time_key);
    let key_image = encode_point(&(hp_l * one_time_secret));
    let d = encode_point(&(hp_l * commitment_secret));
    let pre = prepare(ring, pseudo_out, &key_image, &d, message)?;

    // Confere que os segredos correspondem ao membro indicado.
    if g() * one_time_secret != pre.p[index] || g() * commitment_secret != pre.c_offset[index] {
        return None;
    }

    let i_point = hp_l * one_time_secret;
    let d_point = hp_l * commitment_secret;
    let w_tilde = i_point * pre.mu_p + d_point * pre.mu_c;

    let alpha = Scalar::random(rng);
    let mut s = vec![Scalar::ZERO; n];
    let mut c = vec![Scalar::ZERO; n];
    let next = (index + 1) % n;
    c[next] = round(&pre.round_prefix, &(g() * alpha), &(hp_l * alpha));

    let mut i = next;
    while i != index {
        s[i] = Scalar::random(rng);
        let w_i = pre.p[i] * pre.mu_p + pre.c_offset[i] * pre.mu_c;
        let l = RistrettoPoint::multiscalar_mul([s[i], c[i]], [g(), w_i]);
        let r = RistrettoPoint::multiscalar_mul([s[i], c[i]], [pre.hp[i], w_tilde]);
        let j = (i + 1) % n;
        c[j] = round(&pre.round_prefix, &l, &r);
        i = j;
    }
    s[index] = alpha - c[index] * (pre.mu_p * one_time_secret + pre.mu_c * commitment_secret);

    Some((
        Clsag {
            c0: c[0].to_bytes(),
            s: s.iter().map(Scalar::to_bytes).collect(),
            d,
        },
        key_image,
    ))
}

/// Verifica a assinatura. Rejeita pontos e escalares não canônicos e a
/// imagem de chave identidade.
pub fn verify(
    ring: &[RingMember],
    pseudo_out: &[u8; 32],
    key_image: &[u8; 32],
    message: &[u8; 32],
    sig: &Clsag,
) -> bool {
    let n = ring.len();
    if n == 0 || sig.s.len() != n {
        return false;
    }
    let (Some(i_point), Some(d_point), Some(c0)) = (
        decode_point(key_image),
        decode_point(&sig.d),
        decode_scalar(&sig.c0),
    ) else {
        return false;
    };
    if i_point == RistrettoPoint::identity() {
        return false;
    }
    let Some(pre) = prepare(ring, pseudo_out, key_image, &sig.d, message) else {
        return false;
    };
    let w_tilde = i_point * pre.mu_p + d_point * pre.mu_c;
    let mut c = c0;
    for i in 0..n {
        let Some(s_i) = decode_scalar(&sig.s[i]) else {
            return false;
        };
        let w_i = pre.p[i] * pre.mu_p + pre.c_offset[i] * pre.mu_c;
        let l = RistrettoPoint::multiscalar_mul([s_i, c], [g(), w_i]);
        let r = RistrettoPoint::multiscalar_mul([s_i, c], [pre.hp[i], w_tilde]);
        c = round(&pre.round_prefix, &l, &r);
    }
    c == c0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitment::commit;
    use rand_core::OsRng;

    struct Fixture {
        ring: Vec<RingMember>,
        index: usize,
        p: Scalar,
        z: Scalar,
        pseudo: [u8; 32],
    }

    fn fixture(n: usize, index: usize) -> Fixture {
        let mut ring = Vec::new();
        let p = Scalar::random(&mut OsRng);
        let x_in = Scalar::random(&mut OsRng);
        let y = Scalar::random(&mut OsRng);
        for i in 0..n {
            if i == index {
                ring.push(RingMember {
                    one_time_key: encode_point(&(g() * p)),
                    commitment: encode_point(&commit(50, &x_in)),
                });
            } else {
                ring.push(RingMember {
                    one_time_key: encode_point(&(g() * Scalar::random(&mut OsRng))),
                    commitment: encode_point(&commit(i as u64, &Scalar::random(&mut OsRng))),
                });
            }
        }
        Fixture {
            ring,
            index,
            p,
            z: x_in - y,
            pseudo: encode_point(&commit(50, &y)),
        }
    }

    const MSG: [u8; 32] = [7; 32];

    #[test]
    fn sign_verify_every_position() {
        for index in 0..11 {
            let f = fixture(11, index);
            let (sig, ki) =
                sign(&f.ring, f.index, &f.p, &f.z, &f.pseudo, &MSG, &mut OsRng).unwrap();
            assert!(
                verify(&f.ring, &f.pseudo, &ki, &MSG, &sig),
                "posição {index}"
            );
        }
    }

    #[test]
    fn ring_of_one() {
        let f = fixture(1, 0);
        let (sig, ki) = sign(&f.ring, 0, &f.p, &f.z, &f.pseudo, &MSG, &mut OsRng).unwrap();
        assert!(verify(&f.ring, &f.pseudo, &ki, &MSG, &sig));
    }

    #[test]
    fn key_image_is_stable_across_rings() {
        let f = fixture(5, 2);
        let (_, ki1) = sign(&f.ring, 2, &f.p, &f.z, &f.pseudo, &MSG, &mut OsRng).unwrap();
        let mut other = fixture(5, 4);
        other.ring[4] = f.ring[2];
        let (_, ki2) = sign(&other.ring, 4, &f.p, &f.z, &f.pseudo, &MSG, &mut OsRng).unwrap();
        assert_eq!(
            ki1, ki2,
            "mesma nota em anéis diferentes → mesma imagem de chave"
        );
    }

    #[test]
    fn rejects_tampering() {
        let f = fixture(4, 1);
        let (sig, ki) = sign(&f.ring, 1, &f.p, &f.z, &f.pseudo, &MSG, &mut OsRng).unwrap();
        assert!(!verify(&f.ring, &f.pseudo, &ki, &[8; 32], &sig), "mensagem");
        let mut s2 = sig.clone();
        s2.s[0][0] ^= 1;
        assert!(!verify(&f.ring, &f.pseudo, &ki, &MSG, &s2), "resposta");
        let mut ring = f.ring.clone();
        ring.swap(0, 2);
        assert!(!verify(&ring, &f.pseudo, &ki, &MSG, &sig), "anel");
        let other_ki = encode_point(&(g() * Scalar::random(&mut OsRng)));
        assert!(
            !verify(&f.ring, &f.pseudo, &other_ki, &MSG, &sig),
            "imagem de chave"
        );
        let other_pseudo = encode_point(&commit(51, &Scalar::random(&mut OsRng)));
        assert!(
            !verify(&f.ring, &other_pseudo, &ki, &MSG, &sig),
            "pseudo-saída"
        );
        let identity = encode_point(&RistrettoPoint::identity());
        assert!(!verify(&f.ring, &f.pseudo, &identity, &MSG, &sig));
    }

    #[test]
    fn cannot_sign_without_secret() {
        let f = fixture(4, 1);
        let wrong = Scalar::random(&mut OsRng);
        assert!(sign(&f.ring, 1, &wrong, &f.z, &f.pseudo, &MSG, &mut OsRng).is_none());
    }

    #[test]
    fn amount_mismatch_cannot_sign() {
        // Pseudo-saída com valor diferente: C_l − C' não é múltiplo de G.
        let f = fixture(3, 0);
        let y = Scalar::random(&mut OsRng);
        let pseudo = encode_point(&commit(49, &y));
        assert!(sign(&f.ring, 0, &f.p, &f.z, &pseudo, &MSG, &mut OsRng).is_none());
    }
}
