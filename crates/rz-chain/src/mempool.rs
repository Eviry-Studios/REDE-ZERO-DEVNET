//! Transações pendentes aguardando inclusão em bloco.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

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
            Self::Full => write!(f, "mempool cheio"),
        }
    }
}

impl std::error::Error for MempoolError {}

/// Mempool ordenado por `(remetente, nonce)`: a seleção é determinística e
/// respeita a sequência de nonces de cada conta.
pub struct Mempool {
    txs: BTreeMap<(Address, u64), Transaction>,
    ids: HashSet<TxId>,
    capacity: usize,
}

impl Mempool {
    pub fn new(capacity: usize) -> Self {
        Self {
            txs: BTreeMap::new(),
            ids: HashSet::new(),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    pub fn contains(&self, id: &TxId) -> bool {
        self.ids.contains(id)
    }

    /// Valida e insere. Transações com nonce futuro são aceitas (até
    /// [`MAX_NONCE_GAP`]); o saldo só é verificado de forma completa quando a
    /// transação se torna executável.
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
        tx.check_stateless(params.min_fee)
            .map_err(MempoolError::Invalid)?;
        tx.verify_signature(params.network_id)
            .map_err(MempoolError::Invalid)?;

        let sender = tx.body.sender_address();
        let current = state.account(&sender).nonce;
        if tx.body.nonce < current {
            return Err(MempoolError::Stale);
        }
        if tx.body.nonce - current > MAX_NONCE_GAP {
            return Err(MempoolError::NonceTooFar);
        }
        if tx.body.nonce == current {
            state
                .check_transaction(&tx, params)
                .map_err(MempoolError::Invalid)?;
        }
        let key = (sender, tx.body.nonce);
        if self.txs.contains_key(&key) {
            return Err(MempoolError::Conflict);
        }
        if self.txs.len() >= self.capacity {
            return Err(MempoolError::Full);
        }
        self.txs.insert(key, tx);
        self.ids.insert(id);
        Ok(id)
    }

    /// Seleciona até `max` transações executáveis em sequência sobre `state`.
    pub fn select(&self, state: &State, params: &ExecParams<'_>, max: usize) -> Vec<Transaction> {
        let mut scratch = state.clone();
        let mut out = Vec::new();
        for tx in self.txs.values() {
            if out.len() >= max {
                break;
            }
            if scratch.apply_transaction(tx, params).is_ok() {
                out.push(tx.clone());
            }
        }
        out
    }

    /// Remove transações cujo nonce já foi consumido no estado dado, ou que
    /// se tornaram inválidas.
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
            genesis_time_ms: 0,
            slot_duration_ms: 1_000,
            finality_depth: 3,
            min_fee: 1,
            max_block_txs: 100,
            validators: vec![SecretKey::from_seed([1; 32]).public_key()],
            allocations: vec![Allocation {
                address: rich().public_key().address(),
                amount: 1_000,
            }],
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
        // Inseridas fora de ordem.
        m.insert(tx(1, 10), &s, &p).unwrap();
        m.insert(tx(0, 10), &s, &p).unwrap();
        let sel = m.select(&s, &p, 10);
        assert_eq!(sel.iter().map(|t| t.body.nonce).collect::<Vec<_>>(), [0, 1]);

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
        bad.signature.0[0] ^= 1;
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
