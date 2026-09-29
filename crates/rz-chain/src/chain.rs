//! Árvore de blocos, escolha de fork, finalidade e evidências (`SPEC §19–§22`).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rz_core::{apply_block, Block, BlockError, BlockHeader, BlockId, Genesis, State, StateError};
use rz_crypto::PublicKey;

use crate::consensus::{ConsensusEngine, ConsensusError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainError {
    Genesis(StateError),
    /// O pai ainda não é conhecido — o chamador pode solicitar sincronização.
    UnknownParent(BlockId),
    /// O bloco criaria um fork abaixo de um bloco já finalizado.
    ConflictsWithFinality,
    Consensus(ConsensusError),
    Block(BlockError),
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Genesis(e) => write!(f, "genesis: {e}"),
            Self::UnknownParent(id) => write!(f, "pai desconhecido: {id}"),
            Self::ConflictsWithFinality => write!(f, "conflita com bloco finalizado"),
            Self::Consensus(e) => write!(f, "consenso: {e}"),
            Self::Block(e) => write!(f, "bloco: {e}"),
        }
    }
}

impl std::error::Error for ChainError {}

/// Dois blocos distintos assinados pelo mesmo produtor para o mesmo slot.
///
/// É evidência objetiva e verificável por qualquer Node (THR-CON-004,
/// `SPEC §52`): basta conferir as duas assinaturas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Equivocation {
    pub proposer: PublicKey,
    pub slot: u64,
    pub first: BlockHeader,
    pub second: BlockHeader,
}

/// Resultado da importação de um bloco.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportOutcome {
    AlreadyKnown,
    /// Bloco válido que não alterou a ponta da cadeia.
    Stored,
    /// Bloco tornou-se a nova ponta. `reorg` indica troca de ramificação.
    NewTip {
        reorg: bool,
    },
}

struct Entry {
    block: Option<Block>, // None para o Genesis
    parent: BlockId,
    height: u64,
    slot: Option<u64>,
    state: Option<Arc<State>>,
}

/// Cadeia local de um Node.
pub struct Chain<C: ConsensusEngine> {
    genesis: Genesis,
    genesis_id: BlockId,
    engine: C,
    entries: HashMap<BlockId, Entry>,
    tip: BlockId,
    /// `canonical[h]` = BlockId da cadeia preferida na altura `h`.
    canonical: Vec<BlockId>,
    finalized: BlockId,
    slots: HashMap<(PublicKey, u64), BlockHeader>,
    evidence: Vec<Equivocation>,
}

impl<C: ConsensusEngine> Chain<C> {
    pub fn new(genesis: Genesis, engine: C) -> Result<Self, ChainError> {
        let state = State::from_genesis(&genesis).map_err(ChainError::Genesis)?;
        let genesis_id = BlockId(genesis.hash());
        let mut entries = HashMap::new();
        entries.insert(
            genesis_id,
            Entry {
                block: None,
                parent: genesis_id,
                height: 0,
                slot: None,
                state: Some(Arc::new(state)),
            },
        );
        Ok(Self {
            genesis,
            genesis_id,
            engine,
            entries,
            tip: genesis_id,
            canonical: vec![genesis_id],
            finalized: genesis_id,
            slots: HashMap::new(),
            evidence: Vec::new(),
        })
    }

    pub fn genesis(&self) -> &Genesis {
        &self.genesis
    }

    pub fn genesis_id(&self) -> BlockId {
        self.genesis_id
    }

    pub fn engine(&self) -> &C {
        &self.engine
    }

    pub fn tip(&self) -> BlockId {
        self.tip
    }

    pub fn height(&self) -> u64 {
        self.entry(&self.tip).height
    }

    /// Slot da ponta (`None` se a ponta é o Genesis).
    pub fn tip_slot(&self) -> Option<u64> {
        self.entry(&self.tip).slot
    }

    pub fn finalized(&self) -> BlockId {
        self.finalized
    }

    pub fn finalized_height(&self) -> u64 {
        self.entry(&self.finalized).height
    }

    /// Estado após a ponta da cadeia.
    pub fn state(&self) -> Arc<State> {
        self.state_of(&self.tip)
            .expect("o estado da ponta é sempre mantido")
    }

    pub fn state_of(&self, id: &BlockId) -> Option<Arc<State>> {
        self.entries.get(id)?.state.clone()
    }

    pub fn contains(&self, id: &BlockId) -> bool {
        self.entries.contains_key(id)
    }

    pub fn block(&self, id: &BlockId) -> Option<&Block> {
        self.entries.get(id)?.block.as_ref()
    }

    /// Bloco da cadeia preferida em uma altura (`None` para 0 ou além da ponta).
    pub fn block_at(&self, height: u64) -> Option<&Block> {
        let id = self.canonical.get(usize::try_from(height).ok()?)?;
        self.block(id)
    }

    pub fn evidence(&self) -> &[Equivocation] {
        &self.evidence
    }

    fn entry(&self, id: &BlockId) -> &Entry {
        &self.entries[id]
    }

    /// Ancestral de `id` na altura `height`.
    fn ancestor_at(&self, mut id: BlockId, height: u64) -> Option<BlockId> {
        loop {
            let e = self.entries.get(&id)?;
            if e.height == height {
                return Some(id);
            }
            if e.height < height || e.height == 0 {
                return None;
            }
            id = e.parent;
        }
    }

    /// Importa um bloco recebido da rede ou do disco.
    pub fn import(
        &mut self,
        block: Block,
        now_ms: Option<u64>,
    ) -> Result<ImportOutcome, ChainError> {
        let id = block.id();
        if self.entries.contains_key(&id) {
            return Ok(ImportOutcome::AlreadyKnown);
        }
        let parent_id = block.header.parent;
        let (parent_height, parent_slot, parent_state) = match self.entries.get(&parent_id) {
            Some(p) => (p.height, p.slot, p.state.clone()),
            None => return Err(ChainError::UnknownParent(parent_id)),
        };

        // Nenhum fork pode começar abaixo do bloco finalizado (ADR-0006).
        let fin_h = self.finalized_height();
        if parent_height < fin_h || self.ancestor_at(parent_id, fin_h) != Some(self.finalized) {
            return Err(ChainError::ConflictsWithFinality);
        }
        let parent_state = parent_state.ok_or(ChainError::ConflictsWithFinality)?;

        self.engine
            .check_header(&self.genesis, &block.header, now_ms)
            .map_err(ChainError::Consensus)?;

        let state = apply_block(
            &self.genesis,
            parent_id,
            parent_height,
            parent_slot,
            &parent_state,
            &block,
        )
        .map_err(ChainError::Block)?;

        self.record_slot(&block.header);

        let height = block.header.height;
        let slot = block.header.slot;
        self.entries.insert(
            id,
            Entry {
                block: Some(block),
                parent: parent_id,
                height,
                slot: Some(slot),
                state: Some(Arc::new(state)),
            },
        );

        if self.engine.prefer((height, id), (self.height(), self.tip)) {
            let reorg = parent_id != self.tip;
            self.set_tip(id);
            Ok(ImportOutcome::NewTip { reorg })
        } else {
            Ok(ImportOutcome::Stored)
        }
    }

    fn record_slot(&mut self, header: &BlockHeader) {
        let key = (header.proposer, header.slot);
        match self.slots.get(&key) {
            Some(first) if first.id() != header.id() => {
                self.evidence.push(Equivocation {
                    proposer: header.proposer,
                    slot: header.slot,
                    first: first.clone(),
                    second: header.clone(),
                });
            }
            Some(_) => {}
            None => {
                self.slots.insert(key, header.clone());
            }
        }
    }

    fn set_tip(&mut self, id: BlockId) {
        self.tip = id;

        // Reconstrói o índice canônico a partir da nova ponta até o ancestral comum.
        let height = self.entry(&id).height as usize;
        self.canonical.truncate(height + 1);
        self.canonical.resize(height + 1, self.genesis_id);
        let mut cur = id;
        loop {
            let (h, parent) = {
                let e = self.entry(&cur);
                (e.height as usize, e.parent)
            };
            if self.canonical[h] == cur && h < height {
                break;
            }
            self.canonical[h] = cur;
            if h == 0 {
                break;
            }
            cur = parent;
        }

        self.advance_finality();
    }

    fn advance_finality(&mut self) {
        let depth = u64::from(self.genesis.finality_depth);
        let Some(target) = self.height().checked_sub(depth) else {
            return;
        };
        if target <= self.finalized_height() {
            return;
        }
        self.finalized = self.canonical[target as usize];
        self.prune();
    }

    /// Descarta estados e ramificações que não podem mais ser usados.
    fn prune(&mut self) {
        let fin_h = self.finalized_height();
        let finalized = self.finalized;
        let canonical = &self.canonical;
        // Remove blocos que não pertencem à cadeia finalizada.
        let stale: Vec<BlockId> = self
            .entries
            .iter()
            .filter(|(id, e)| e.height <= fin_h && canonical.get(e.height as usize) != Some(id))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.entries.remove(&id);
        }
        // Mantém estados apenas do bloco finalizado em diante.
        for (id, e) in self.entries.iter_mut() {
            if e.height < fin_h && *id != finalized {
                e.state = None;
            }
        }
        // Remove ramificações cujo ancestral na altura finalizada não é o finalizado.
        let orphans: Vec<BlockId> = self
            .entries
            .keys()
            .filter(|id| {
                let h = self.entries[*id].height;
                h > fin_h && self.ancestor_at(**id, fin_h) != Some(finalized)
            })
            .copied()
            .collect();
        for id in orphans {
            self.entries.remove(&id);
        }
        self.slots
            .retain(|_, h| h.height + u64::from(self.genesis.finality_depth) >= fin_h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoundRobin;
    use rz_core::{Allocation, NetworkKind, TxBody, TxKind, PROTOCOL_VERSION};
    use rz_crypto::{Hash32, SecretKey};

    fn validators() -> Vec<SecretKey> {
        (1..=3).map(|i| SecretKey::from_seed([i; 32])).collect()
    }

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
            validators: validators().iter().map(SecretKey::public_key).collect(),
            allocations: vec![Allocation {
                address: rich().public_key().address(),
                amount: 1_000_000,
            }],
        }
    }

    fn chain() -> Chain<RoundRobin> {
        Chain::new(genesis(), RoundRobin::default()).unwrap()
    }

    /// Produz um bloco sobre `parent` no slot dado, com o validador correto.
    fn produce(
        c: &Chain<RoundRobin>,
        parent: BlockId,
        slot: u64,
        txs: Vec<rz_core::Transaction>,
    ) -> Block {
        let g = c.genesis();
        let vs = validators();
        let key = &vs[(slot % 3) as usize];
        let p = &c.entries[&parent];
        Block::build(
            g,
            parent,
            p.height,
            p.state.as_ref().unwrap(),
            slot,
            txs,
            key,
        )
        .unwrap()
        .0
    }

    fn transfer(nonce: u64, to_seed: u8) -> rz_core::Transaction {
        TxBody {
            version: 1,
            sender: rich().public_key(),
            nonce,
            fee: 1,
            kind: TxKind::Transfer {
                to: SecretKey::from_seed([to_seed; 32]).public_key().address(),
                amount: 100,
            },
        }
        .sign(&rich(), "rede-zero-devnet-test")
        .unwrap()
    }

    // AT-CON-001 — determinação de estado
    #[test]
    fn at_con_001_linear_chain() {
        let mut c = chain();
        let mut parent = c.genesis_id();
        for slot in 1..=5 {
            let b = produce(&c, parent, slot, vec![]);
            parent = b.id();
            assert_eq!(
                c.import(b, None).unwrap(),
                ImportOutcome::NewTip { reorg: false }
            );
        }
        assert_eq!(c.height(), 5);
        assert_eq!(c.finalized_height(), 2);
        assert_eq!(c.block_at(5).unwrap().id(), c.tip());
    }

    // AT-CON-002 — candidato inválido (produtor errado)
    #[test]
    fn at_con_002_wrong_proposer() {
        let mut c = chain();
        let g = c.genesis().clone();
        let s = c.state();
        // Slot 1 pertence ao validador 1 (índice 1); assina com o validador 0.
        let (b, _) = Block::build(&g, c.genesis_id(), 0, &s, 1, vec![], &validators()[0]).unwrap();
        assert_eq!(
            c.import(b, None),
            Err(ChainError::Consensus(ConsensusError::WrongProposer {
                slot: 1
            }))
        );
    }

    #[test]
    fn future_slot_rejected() {
        let mut c = chain();
        let b = produce(&c, c.genesis_id(), 10, vec![]);
        assert_eq!(
            c.import(b.clone(), Some(5_000)),
            Err(ChainError::Consensus(ConsensusError::FutureSlot {
                slot: 10
            }))
        );
        assert!(c.import(b, Some(10_000)).is_ok());
    }

    #[test]
    fn unknown_parent() {
        let mut c = chain();
        let b1 = produce(&c, c.genesis_id(), 1, vec![]);
        let mut c2 = chain();
        c2.import(b1.clone(), None).unwrap();
        let b2 = produce(&c2, b1.id(), 2, vec![]);
        assert_eq!(c.import(b2, None), Err(ChainError::UnknownParent(b1.id())));
    }

    // AT-FORK-001 / AT-CON-003 — fork concorrente resolvido deterministicamente
    #[test]
    fn at_fork_001_deterministic_choice() {
        let mut c1 = chain();
        let mut c2 = chain();
        let g = c1.genesis_id();
        let a = produce(&c1, g, 1, vec![transfer(0, 30)]);
        let b = produce(&c1, g, 2, vec![transfer(0, 31)]);
        // Ordem de chegada diferente em cada Node.
        c1.import(a.clone(), None).unwrap();
        c1.import(b.clone(), None).unwrap();
        c2.import(b.clone(), None).unwrap();
        c2.import(a.clone(), None).unwrap();
        assert_eq!(c1.tip(), c2.tip());
        assert_eq!(c1.tip(), a.id().min(b.id()));
        assert_eq!(c1.state().root(), c2.state().root());

        // Ramificação mais longa vence.
        let loser = if c1.tip() == a.id() { b } else { a };
        let ext = produce(&c1, loser.id(), 3, vec![]);
        assert_eq!(
            c1.import(ext.clone(), None).unwrap(),
            ImportOutcome::NewTip { reorg: true }
        );
        assert_eq!(c1.tip(), ext.id());
        assert_eq!(c1.block_at(1).unwrap().id(), loser.id());
    }

    // AT-FORK-002 — ramificação com estado inválido não é aceita
    #[test]
    fn at_fork_002_invalid_branch() {
        let mut c = chain();
        let g = c.genesis_id();
        let good = produce(&c, g, 1, vec![]);
        c.import(good, None).unwrap();

        let mut bad = produce(&c, g, 2, vec![]);
        bad.header.state_root = Hash32([1; 32]);
        let key = &validators()[2];
        bad.signature = key.sign(
            rz_crypto::context::BLOCK_SIGNATURE,
            &c.genesis().network_id,
            &rz_codec::Encode::to_canonical_bytes(&bad.header),
        );
        assert_eq!(
            c.import(bad, None),
            Err(ChainError::Block(BlockError::StateRootMismatch))
        );
        assert_eq!(c.height(), 1);
    }

    // AT-DS-002 — double spend concorrente em ramificações diferentes
    #[test]
    fn at_ds_002_concurrent_double_spend() {
        let mut c = chain();
        let g = c.genesis_id();
        let a = produce(&c, g, 1, vec![transfer(0, 40)]);
        let b = produce(&c, g, 2, vec![transfer(0, 41)]);
        c.import(a, None).unwrap();
        c.import(b, None).unwrap();
        let s = c.state();
        let r40 = s
            .account(&SecretKey::from_seed([40; 32]).public_key().address())
            .balance;
        let r41 = s
            .account(&SecretKey::from_seed([41; 32]).public_key().address())
            .balance;
        // Apenas uma das transações conflitantes vale no estado final.
        assert_eq!(r40 + r41, 100);
    }

    #[test]
    fn finality_blocks_deep_reorg() {
        let mut c = chain();
        let g = c.genesis_id();
        let side = produce(&c, g, 1, vec![]);
        let mut parent = g;
        for slot in 2..=6 {
            let b = produce(&c, parent, slot, vec![]);
            parent = b.id();
            c.import(b, None).unwrap();
        }
        assert!(c.finalized_height() >= 1);
        assert_eq!(c.import(side, None), Err(ChainError::ConflictsWithFinality));
    }

    #[test]
    fn equivocation_detected() {
        let mut c = chain();
        let g = c.genesis_id();
        let a = produce(&c, g, 1, vec![]);
        let b = produce(&c, g, 1, vec![transfer(0, 50)]);
        c.import(a, None).unwrap();
        c.import(b, None).unwrap();
        assert_eq!(c.evidence().len(), 1);
        let ev = &c.evidence()[0];
        assert_eq!(ev.slot, 1);
        assert_ne!(ev.first.id(), ev.second.id());
    }

    #[test]
    fn already_known() {
        let mut c = chain();
        let b = produce(&c, c.genesis_id(), 1, vec![]);
        c.import(b.clone(), None).unwrap();
        assert_eq!(c.import(b, None).unwrap(), ImportOutcome::AlreadyKnown);
    }
}
