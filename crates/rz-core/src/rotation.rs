//! Rotação e recuperação de chaves de conta (`SPEC-ID-005`, ADR-0023,
//! `spec/KEYS.md`).
//!
//! O endereço de uma conta nasce do hash da sua primeira chave e **não
//! muda**. A rotação troca apenas a chave autorizada a assinar pela conta:
//! saldos, ativos, bloqueios, nomes e ordens continuam no mesmo endereço.
//!
//! Quem perdeu a chave usa uma **chave de recuperação** registrada antes:
//! ela agenda a troca, que só vale depois de um prazo em que a chave atual
//! ainda pode cancelá-la.
//!
//! Pontos de contribuição e vínculos de validador ficam presos à chave
//! (GOV-INV-5, REQ-046) e não migram.

use std::collections::BTreeMap;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Address, Hash32, PublicKey};

/// Contexto da prova de posse da nova chave.
pub const KEY_ROTATION: &str = "rede-zero/key-rotation/v1";
/// Contexto da raiz da parte de chaves do estado.
pub const KEYS_ROOT: &str = "rede-zero/keys-root/v1";

/// Prazo mínimo e máximo da recuperação, em blocos.
pub const MIN_RECOVERY_DELAY: u64 = 10;
pub const MAX_RECOVERY_DELAY: u64 = 10_000_000;

/// Mensagem que a nova chave assina para provar posse e aceitar controlar
/// `account`.
pub fn rotation_payload(account: &Address, new_key: &PublicKey) -> Vec<u8> {
    let mut e = Encoder::new();
    e.put(account).put(new_key);
    e.into_bytes()
}

/// Contexto das aprovações de guardiões (`spec/KEYS.md §4`).
pub const KEY_RECOVERY: &str = "rede-zero/key-recovery/v1";
/// Guardiões por conta.
pub const MAX_GUARDIANS: usize = 16;

/// Mensagem que cada guardião aprova: a conta, a nova chave e o contador de
/// recuperação vigente (aprovações de uma recuperação cancelada não valem
/// de novo).
pub fn recovery_payload(account: &Address, new_key: &PublicKey, nonce: u64) -> Vec<u8> {
    let mut e = Encoder::new();
    e.put(account).put(new_key).u64(nonce);
    e.into_bytes()
}

/// Aprovação de um guardião.
pub fn approve_recovery(
    guardian: &rz_crypto::SecretKey,
    network_id: &str,
    account: &Address,
    new_key: &PublicKey,
    nonce: u64,
) -> crate::community::Approval {
    crate::community::Approval {
        key: guardian.public_key(),
        signature: guardian.sign(
            KEY_RECOVERY,
            network_id,
            &recovery_payload(account, new_key, nonce),
        ),
    }
}

/// Recuperação de uma conta: `threshold` dos `guardians` aprovam a troca,
/// aplicada depois de `delay` blocos. Uma chave de recuperação única é o
/// caso 1-de-1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovery {
    pub guardians: Vec<PublicKey>,
    pub threshold: u8,
    pub delay: u64,
}

impl Recovery {
    pub fn single(key: PublicKey, delay: u64) -> Self {
        Self {
            guardians: vec![key],
            threshold: 1,
            delay,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.guardians.is_empty() || self.guardians.len() > MAX_GUARDIANS {
            return Err("guardiões: de 1 a 16");
        }
        let mut sorted = self.guardians.clone();
        sorted.sort_by_key(|k| *k.as_bytes());
        if sorted.windows(2).any(|w| w[0] == w[1]) {
            return Err("guardião repetido");
        }
        if self.threshold == 0 || self.threshold as usize > self.guardians.len() {
            return Err("limiar de guardiões inválido");
        }
        if !(MIN_RECOVERY_DELAY..=MAX_RECOVERY_DELAY).contains(&self.delay) {
            return Err("prazo de recuperação fora dos limites");
        }
        Ok(())
    }

    /// Ao menos `threshold` guardiões distintos aprovaram `payload`.
    pub fn approved(
        &self,
        network_id: &str,
        payload: &[u8],
        approvals: &[crate::community::Approval],
    ) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        for a in approvals {
            if !self.guardians.contains(&a.key)
                || !seen.insert(*a.key.as_bytes())
                || a.key
                    .verify(KEY_RECOVERY, network_id, payload, &a.signature)
                    .is_err()
            {
                return false;
            }
        }
        seen.len() >= self.threshold as usize
    }
}

impl Encode for Recovery {
    fn encode(&self, e: &mut Encoder) {
        e.list(&self.guardians).u8(self.threshold).u64(self.delay);
    }
}

impl Decode for Recovery {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            guardians: d.list(MAX_GUARDIANS)?,
            threshold: d.u8()?,
            delay: d.u64()?,
        })
    }
}

/// Recuperação agendada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingRecovery {
    pub new_key: PublicKey,
    /// Altura a partir da qual a troca é aplicada (no fim do bloco).
    pub at: u64,
}

impl Encode for PendingRecovery {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.new_key).u64(self.at);
    }
}

impl Decode for PendingRecovery {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            new_key: d.get()?,
            at: d.u64()?,
        })
    }
}

/// Parte do estado dedicada a chaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyState {
    /// Conta → chave autorizada, quando difere da chave de origem.
    pub authorized: BTreeMap<Address, PublicKey>,
    /// Chave autorizada → conta que ela controla (inverso de `authorized`,
    /// exceto quando a conta volta para a chave de origem).
    pub delegated: BTreeMap<PublicKey, Address>,
    pub recovery: BTreeMap<Address, Recovery>,
    pub pending: BTreeMap<Address, PendingRecovery>,
    /// Contador de recuperação por conta; muda a cada configuração,
    /// agendamento ou rotação.
    pub recovery_nonce: BTreeMap<Address, u64>,
}

/// Resultado da resolução de uma chave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStatus {
    /// A chave assina pela conta.
    Active(Address),
    /// A chave foi substituída e não assina mais por ninguém.
    Replaced { account: Address, by: PublicKey },
}

impl KeyState {
    /// Conta controlada por `key`.
    pub fn status(&self, key: &PublicKey) -> KeyStatus {
        if let Some(a) = self.delegated.get(key) {
            return KeyStatus::Active(*a);
        }
        let a = key.address();
        match self.authorized.get(&a) {
            Some(k) if k != key => KeyStatus::Replaced { account: a, by: *k },
            _ => KeyStatus::Active(a),
        }
    }

    /// Chave atual de `account`, se a conta já girou.
    pub fn current_key(&self, account: &Address) -> Option<PublicKey> {
        self.authorized.get(account).copied()
    }

    /// Endereço que nenhuma chave controla: o endereço de origem de uma
    /// chave hoje delegada a outra conta. ZERO enviado para ele ficaria
    /// preso.
    pub fn is_shadowed(&self, address: &Address) -> bool {
        self.delegated
            .iter()
            .any(|(k, a)| k.address() == *address && a != address)
    }

    /// Aplica a troca: `account` passa a ser controlada por `new_key`.
    pub fn rotate(&mut self, account: Address, new_key: PublicKey) {
        if let Some(old) = self.authorized.remove(&account) {
            self.delegated.remove(&old);
        }
        if new_key.address() != account {
            self.authorized.insert(account, new_key);
            self.delegated.insert(new_key, account);
        } else {
            // Volta para a chave de origem: nenhuma indireção.
            self.authorized.insert(account, new_key);
        }
        self.pending.remove(&account);
        self.bump(account);
    }

    /// Contador de recuperação vigente da conta.
    pub fn nonce(&self, account: &Address) -> u64 {
        self.recovery_nonce.get(account).copied().unwrap_or(0)
    }

    /// Invalida aprovações já dadas para a conta.
    pub fn bump(&mut self, account: Address) {
        let n = self.recovery_nonce.entry(account).or_default();
        *n = n.saturating_add(1);
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.authorized.len() as u64);
        for (a, k) in &self.authorized {
            e.put(a).put(k);
        }
        e.u64(self.recovery.len() as u64);
        for (a, r) in &self.recovery {
            e.put(a).put(r);
        }
        e.u64(self.pending.len() as u64);
        for (a, p) in &self.pending {
            e.put(a).put(p);
        }
        e.u64(self.recovery_nonce.len() as u64);
        for (a, n) in &self.recovery_nonce {
            e.put(a).u64(*n);
        }
        hash(KEYS_ROOT, &e.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rz_crypto::SecretKey;

    #[test]
    fn rotation_chain_and_return_to_origin() {
        let (a, b, c) = (
            SecretKey::from_seed([1; 32]).public_key(),
            SecretKey::from_seed([2; 32]).public_key(),
            SecretKey::from_seed([3; 32]).public_key(),
        );
        let acc = a.address();
        let mut s = KeyState::default();
        assert_eq!(s.status(&a), KeyStatus::Active(acc));
        s.rotate(acc, b);
        assert_eq!(s.status(&b), KeyStatus::Active(acc));
        assert_eq!(
            s.status(&a),
            KeyStatus::Replaced {
                account: acc,
                by: b
            }
        );
        assert!(s.is_shadowed(&b.address()));
        s.rotate(acc, c);
        assert!(matches!(s.status(&b), KeyStatus::Active(x) if x == b.address()));
        assert_eq!(s.status(&c), KeyStatus::Active(acc));
        assert!(!s.is_shadowed(&b.address()));
        s.rotate(acc, a);
        assert_eq!(s.status(&a), KeyStatus::Active(acc));
        assert!(matches!(s.status(&c), KeyStatus::Active(x) if x == c.address()));
    }
}
