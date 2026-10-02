//! Transações pendentes aguardando inclusão em bloco.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use rz_codec::Encode;
use rz_core::limits::MAX_BLOCK_TX_BYTES;
use rz_core::state::ExecParams;
use rz_core::{State, Transaction, TxError, TxId};
use rz_crypto::Address;

/// Distância máxima entre o nonce da transação e o nonce atual da conta.
/// Limita quanto espaço um único remetente pode ocupar (THR-P2P-002).
pub const MAX_NONCE_GAP: u64 = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MempoolError {
    Invalid(TxError),
    Duplicate,
    /// Já existe outra transação com o mesmo remetente e nonce.
    Conflict,
    /// Nonce já utilizado.
    Stale,
    NonceTooFar,
    /// Nonce futuro sem a transação anterior do mesmo remetente pendente.
    NonceGap,
    Full,
}

impl fmt::Display for MempoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "transação inválida: {e}"),
            Self::Duplicate => write!(f, "transação já conhecida"),
            Self::Conflict => write!(f, "conflito com transação pendente de mesmo nonce"),
            Self::Stale => write!(f, "nonce já utilizado"),
            Self::NonceTooFar => write!(f, "nonce distante demais do atual"),
            Self::NonceGap => write!(f, "nonce não contíguo às transações pendentes"),
            Self::Full => write!(f, "mempool cheio"),
        }
    }
}

impl std::error::Error for MempoolError {}

/// Mempool determinístico.
///
/// * Transações de conta: ordenadas por `(remetente, nonce)`, respeitando a
///   sequência de nonces de cada conta.
/// * Transações privadas: ordenadas por identificador; conflitos detectados
///   pelas imagens de chave (duas transações gastando a mesma nota).
pub struct Mempool {
    txs: BTreeMap<(Address, u64), Transaction>,
    private: BTreeMap<TxId, Transaction>,
    key_images: HashSet<[u8; 32]>,
    ids: HashSet<TxId>,
    capacity: usize,
}

impl Mempool {
    pub fn new(capacity: usize) -> Self {
        Self {
            txs: BTreeMap::new(),
            private: BTreeMap::new(),
            key_images: HashSet::new(),
            ids: HashSet::new(),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.txs.len() + self.private.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, id: &TxId) -> bool {
        self.ids.contains(id)
    }

    /// Valida e insere.
    ///
    /// Uma transação de conta com nonce futuro só é aceita se a transação
    /// anterior do mesmo remetente já estiver pendente (nonces contíguos, até
    /// [`MAX_NONCE_GAP`]). Assim, a primeira transação de cada sequência
    /// sempre passa pela verificação completa de saldo, e contas sem fundos
    /// não conseguem ocupar o mempool (THR-P2P-002).
    ///
    /// Transações privadas são sempre verificadas por completo (anel, provas
    /// e balanço) antes de entrar.
    pub fn insert(
        &mut self,
        tx: Transaction,
        state: &State,
        params: &ExecParams<'_>,
    ) -> Result<TxId, MempoolError> {
        let id = tx.id();
        if self.ids.contains(&id) {
            return Err(MempoolError::Duplicate);
        }
        if self.len() >= self.capacity {
            return Err(MempoolError::Full);
        }
        match &tx {
            Transaction::Account(a) => {
                a.check_stateless(state.params().min_fee)
                    .map_err(MempoolError::Invalid)?;
                a.verify_signature(params.network_id)
                    .map_err(MempoolError::Invalid)?;
                let sender = a.body.sender_address();
                let nonce = a.body.nonce;
                let current = state.account(&sender).nonce;
                if nonce < current {
                    return Err(MempoolError::Stale);
                }
                if nonce - current > MAX_NONCE_GAP {
                    return Err(MempoolError::NonceTooFar);
                }
                if nonce == current {
                    state
                        .check_transaction(&tx, params)
                        .map_err(MempoolError::Invalid)?;
                } else if !self.txs.contains_key(&(sender, nonce - 1)) {
                    return Err(MempoolError::NonceGap);
                }
                if self.txs.contains_key(&(sender, nonce)) {
                    return Err(MempoolError::Conflict);
                }
                self.txs.insert((sender, nonce), tx);
            }
            Transaction::Private(p) => {
                if p.inputs
                    .iter()
                    .any(|i| self.key_images.contains(&i.key_image))
                {
                    return Err(MempoolError::Conflict);
                }
                state
                    .check_transaction(&tx, params)
                    .map_err(MempoolError::Invalid)?;
                for i in &p.inputs {
                    self.key_images.insert(i.key_image);
                }
                self.private.insert(id, tx);
            }
        }
        self.ids.insert(id);
        Ok(id)
    }

    /// Seleciona até `max` transações executáveis em sequência sobre `state`.
    pub fn select(&self, state: &State, params: &ExecParams<'_>, max: usize) -> Vec<Transaction> {
        let mut scratch = state.clone();
        let mut out = Vec::new();
        let mut bytes = 0usize;
        let mut fuel = 0u64;
        let max_fuel = state.params().runtime.max_block_fuel;
        for tx in self.txs.values().chain(self.private.values()) {
            if out.len() >= max {
                break;
            }
            // Respeita o limite de bytes do bloco (`MAX_BLOCK_TX_BYTES`) e o
            // de combustível do Exonet Runtime.
            let size = tx.to_canonical_bytes().len();
            if bytes + size > MAX_BLOCK_TX_BYTES || fuel.saturating_add(tx.fuel()) > max_fuel {
                continue;
            }
            if scratch.apply_transaction(tx, params).is_ok() {
                bytes += size;
                fuel += tx.fuel();
                out.push(tx.clone());
            }
        }
        out
    }

    /// Remove transações já incluídas ou que se tornaram inválidas.
    pub fn prune(&mut self, state: &State, params: &ExecParams<'_>) {
        let ids = &mut self.ids;
        self.txs.retain(|(sender, nonce), tx| {
            let current = state.account(sender).nonce;
            let keep = *nonce > current
                || (*nonce == current && state.check_transaction(tx, params).is_ok());
            if !keep {
                ids.remove(&tx.id());
            }
            keep
        });
        // Uma transação privada só deixa de ser válida quando alguma de suas
        // notas é gasta — verificação barata, sem refazer provas.
        let key_images = &mut self.key_images;
        self.private.retain(|id, tx| {
            let Transaction::Private(p) = tx else {
                return false;
            };
            let keep = !p.inputs.iter().any(|i| state.is_spent(&i.key_image));
            if !keep {
                ids.remove(id);
                for i in &p.inputs {
                    key_images.remove(&i.key_image);
                }
            }
            keep
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rz_core::{Allocation, Genesis, NetworkKind, TxBody, TxKind, PROTOCOL_VERSION};
    use rz_crypto::SecretKey;

    fn rich() -> SecretKey {
        SecretKey::from_seed([20; 32])
    }

    fn genesis() -> Genesis {
        Genesis {
            protocol_version: PROTOCOL_VERSION,
            kind: NetworkKind::Devnet,
            network_id: "rede-zero-devnet-test".into(),
            consensus: rz_core::ConsensusParams::fast(1_000),
            min_fee: 1,
            max_block_txs: 100,
            validators: vec![rz_core::GenesisValidator::new(
                SecretKey::from_seed([1; 32]).public_key(),
                100,
            )],
            allocations: vec![Allocation {
                address: rich().public_key().address(),
                amount: 1_000,
            }],
            governance: rz_core::GovernanceParams::default(),
            assets: vec![],
            bridges: vec![],
        }
    }

    fn tx(nonce: u64, amount: u64) -> Transaction {
        TxBody {
            version: 1,
            sender: rich().public_key(),
            nonce,
            fee: 1,
            kind: TxKind::Transfer {
                to: SecretKey::from_seed([30; 32]).public_key().address(),
                amount,
            },
        }
        .sign(&rich(), "rede-zero-devnet-test")
        .unwrap()
    }

    #[test]
    fn insert_select_prune() {
        let g = genesis();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let mut m = Mempool::new(10);
        m.insert(tx(0, 10), &s, &p).unwrap();
        m.insert(tx(1, 10), &s, &p).unwrap();
        let sel = m.select(&s, &p, 10);
        assert_eq!(
            sel.iter()
                .map(|t| t.as_account().unwrap().body.nonce)
                .collect::<Vec<_>>(),
            [0, 1]
        );

        s.apply_transaction(&sel[0], &p).unwrap();
        m.prune(&s, &p);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn rejects_duplicates_and_conflicts() {
        let g = genesis();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let mut m = Mempool::new(10);
        m.insert(tx(0, 10), &s, &p).unwrap();
        assert_eq!(m.insert(tx(0, 10), &s, &p), Err(MempoolError::Duplicate));
        assert_eq!(m.insert(tx(0, 11), &s, &p), Err(MempoolError::Conflict));
    }

    #[test]
    fn rejects_invalid() {
        let g = genesis();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let mut m = Mempool::new(10);
        let mut bad = tx(0, 10);
        if let Transaction::Account(a) = &mut bad {
            a.signature.0[0] ^= 1;
        }
        assert!(matches!(
            m.insert(bad, &s, &p),
            Err(MempoolError::Invalid(_))
        ));
        assert!(matches!(
            m.insert(tx(0, 5_000), &s, &p),
            Err(MempoolError::Invalid(TxError::InsufficientBalance { .. }))
        ));
        assert_eq!(
            m.insert(tx(MAX_NONCE_GAP + 1, 1), &s, &p),
            Err(MempoolError::NonceTooFar)
        );
    }

    // THR-P2P-002 — conta sem fundos não ocupa o mempool com nonces futuros.
    #[test]
    fn future_nonce_requires_contiguous_pending() {
        let g = genesis();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let mut m = Mempool::new(10);
        assert_eq!(m.insert(tx(1, 10), &s, &p), Err(MempoolError::NonceGap));

        let broke = SecretKey::from_seed([99; 32]);
        let spam = TxBody {
            version: 1,
            sender: broke.public_key(),
            nonce: 5,
            fee: 1,
            kind: TxKind::Transfer {
                to: rich().public_key().address(),
                amount: 1,
            },
        }
        .sign(&broke, "rede-zero-devnet-test")
        .unwrap();
        assert_eq!(m.insert(spam, &s, &p), Err(MempoolError::NonceGap));
        assert!(m.is_empty());
    }

    #[test]
    fn capacity() {
        let g = genesis();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let mut m = Mempool::new(1);
        m.insert(tx(0, 1), &s, &p).unwrap();
        assert_eq!(m.insert(tx(1, 1), &s, &p), Err(MempoolError::Full));
    }

    #[test]
    fn select_skips_unaffordable() {
        let g = genesis();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let mut m = Mempool::new(10);
        m.insert(tx(0, 900), &s, &p).unwrap();
        m.insert(tx(1, 900), &s, &p).unwrap(); // sem saldo após a primeira
        assert_eq!(m.select(&s, &p, 10).len(), 1);
    }
}
