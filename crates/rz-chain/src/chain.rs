//! Cadeia finalizada pelo consenso Zero-BFT (`SPEC §19–§22`, ADR-0012).
//!
//! Cada bloco entra na cadeia apenas com um [`Commit`] válido: pré-
//! compromissos de mais de 2/3 do poder do conjunto de validadores vigente.
//! Não há escolha de fork nem reorganização: um bloco aceito é final
//! (`SPEC §21`), e dados apresentados por outro Node só são aceitos se
//! verificáveis (AC-CON-002).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use rz_core::consensus::CommitError;
use rz_core::{
    apply_block, Block, BlockError, BlockId, CommittedBlock, Genesis, State, StateError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainError {
    Genesis(StateError),
    /// O bloco não é o próximo da cadeia (lacuna ou já conhecido).
    NotNext {
        expected: u64,
        got: u64,
    },
    /// O pai não é a ponta atual.
    WrongParent,
    /// O proponente não é o sorteado para a altura e rodada.
    WrongProposer,
    Commit(CommitError),
    Block(BlockError),
    /// O bloco diverge de um ponto de verificação configurado pelo operador.
    Checkpoint {
        height: u64,
    },
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Genesis(e) => write!(f, "genesis: {e}"),
            Self::NotNext { expected, got } => {
                write!(
                    f,
                    "bloco fora de ordem: esperado {expected}, recebido {got}"
                )
            }
            Self::WrongParent => write!(f, "pai diferente da ponta"),
            Self::WrongProposer => write!(f, "proponente incorreto"),
            Self::Commit(e) => write!(f, "certificado: {e}"),
            Self::Block(e) => write!(f, "bloco: {e}"),
            Self::Checkpoint { height } => {
                write!(
                    f,
                    "bloco diverge do ponto de verificação na altura {height}"
                )
            }
        }
    }
}

impl std::error::Error for ChainError {}

/// Cadeia local de um Node: blocos finalizados e o estado após a ponta.
pub struct Chain {
    genesis: Genesis,
    genesis_id: BlockId,
    blocks: Vec<CommittedBlock>,
    state: Arc<State>,
    /// Pontos de verificação (altura → bloco), defesa contra ataque de longo
    /// alcance com chaves já desvinculadas (THR-CON-001, `docs/AUDIT.md`
    /// RZ-IR-08). Fornecidos pelo operador a partir de fontes em que confia;
    /// o protocolo não embute nenhum.
    checkpoints: BTreeMap<u64, BlockId>,
}

impl Chain {
    pub fn new(genesis: Genesis) -> Result<Self, ChainError> {
        let state = State::from_genesis(&genesis).map_err(ChainError::Genesis)?;
        Ok(Self {
            genesis_id: BlockId(genesis.hash()),
            genesis,
            blocks: Vec::new(),
            state: Arc::new(state),
            checkpoints: BTreeMap::new(),
        })
    }

    /// Define pontos de verificação. Falha se a cadeia local já divergir de
    /// algum deles.
    pub fn set_checkpoints(
        &mut self,
        checkpoints: impl IntoIterator<Item = (u64, BlockId)>,
    ) -> Result<(), ChainError> {
        let checkpoints: BTreeMap<u64, BlockId> = checkpoints.into_iter().collect();
        for (h, id) in &checkpoints {
            if let Some(b) = self.block_at(*h) {
                if b.id() != *id {
                    return Err(ChainError::Checkpoint { height: *h });
                }
            }
        }
        self.checkpoints = checkpoints;
        Ok(())
    }

    pub fn genesis(&self) -> &Genesis {
        &self.genesis
    }

    pub fn genesis_id(&self) -> BlockId {
        self.genesis_id
    }

    pub fn height(&self) -> u64 {
        self.blocks.len() as u64
    }

    /// Identificador do último bloco finalizado (o Genesis se nenhum).
    pub fn tip(&self) -> BlockId {
        self.blocks.last().map_or(self.genesis_id, |c| c.block.id())
    }

    /// Estado após a ponta.
    pub fn state(&self) -> Arc<State> {
        self.state.clone()
    }

    pub fn block_at(&self, height: u64) -> Option<&Block> {
        self.committed_at(height).map(|c| &c.block)
    }

    pub fn committed_at(&self, height: u64) -> Option<&CommittedBlock> {
        let i = usize::try_from(height.checked_sub(1)?).ok()?;
        self.blocks.get(i)
    }

    /// Verifica um bloco candidato à próxima altura sem alterar a cadeia:
    /// altura, pai, proponente sorteado e todas as regras de bloco.
    /// Retorna o estado resultante.
    pub fn check_next(&self, block: &Block) -> Result<State, ChainError> {
        let h = &block.header;
        let expected = self.height() + 1;
        if h.height != expected {
            return Err(ChainError::NotNext {
                expected,
                got: h.height,
            });
        }
        if h.parent != self.tip() {
            return Err(ChainError::WrongParent);
        }
        let round = u32::try_from(h.round).map_err(|_| ChainError::WrongProposer)?;
        if self
            .state
            .validators()
            .proposer(&self.tip(), h.height, round)
            != Some(&h.proposer)
        {
            return Err(ChainError::WrongProposer);
        }
        apply_block(&self.genesis, self.tip(), self.height(), &self.state, block)
            .map_err(ChainError::Block)
    }

    /// Acrescenta um bloco finalizado, verificando o certificado.
    pub fn commit(&mut self, cb: CommittedBlock) -> Result<(), ChainError> {
        let id = cb.block.id();
        let h = cb.block.header.height;
        if self.checkpoints.get(&h).is_some_and(|c| *c != id) {
            return Err(ChainError::Checkpoint { height: h });
        }
        cb.commit
            .verify(
                cb.block.header.height,
                &id,
                self.state.validators(),
                &self.genesis.network_id,
            )
            .map_err(ChainError::Commit)?;
        let state = self.check_next(&cb.block)?;
        self.state = Arc::new(state);
        self.blocks.push(cb);
        Ok(())
    }
}

/// Utilitários para testes: produz blocos finalizados com um conjunto de
/// chaves de validadores.
pub mod testing {
    use rz_core::{Commit, Vote, VoteType};
    use rz_crypto::SecretKey;

    use super::*;

    /// Monta, pelo proponente sorteado da rodada 0, e finaliza com as
    /// assinaturas de todos os validadores em `keys`.
    pub fn next_block(
        chain: &Chain,
        keys: &[SecretKey],
        txs: Vec<rz_core::Transaction>,
    ) -> CommittedBlock {
        let state = chain.state();
        let h = chain.height() + 1;
        let proposer = *state
            .validators()
            .proposer(&chain.tip(), h, 0)
            .expect("conjunto não vazio");
        let key = keys
            .iter()
            .find(|k| k.public_key() == proposer)
            .expect("chave do proponente");
        let (block, _) = Block::build(
            chain.genesis(),
            chain.tip(),
            chain.height(),
            &state,
            0,
            txs,
            key,
        )
        .expect("bloco válido");
        let votes: Vec<Vote> = keys
            .iter()
            .filter(|k| state.validators().contains(&k.public_key()))
            .map(|k| {
                Vote::sign(
                    VoteType::Precommit,
                    h,
                    0,
                    Some(block.id()),
                    k,
                    &chain.genesis().network_id,
                )
            })
            .collect();
        let commit = Commit::from_votes(&votes).expect("votos");
        CommittedBlock { block, commit }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::next_block;
    use super::*;
    use rz_core::{
        Allocation, Commit, ConsensusParams, GenesisValidator, GovernanceParams, NetworkKind,
        TxBody, TxKind, Vote, VoteType, PROTOCOL_VERSION,
    };
    use rz_crypto::SecretKey;

    const NET: &str = "rede-zero-devnet-test";

    fn validators() -> Vec<SecretKey> {
        (1..=4).map(|i| SecretKey::from_seed([i; 32])).collect()
    }

    fn rich() -> SecretKey {
        SecretKey::from_seed([20; 32])
    }

    fn genesis() -> Genesis {
        Genesis {
            protocol_version: PROTOCOL_VERSION,
            kind: NetworkKind::Devnet,
            network_id: NET.into(),
            consensus: ConsensusParams::fast(200),
            min_fee: 1,
            max_block_txs: 100,
            validators: validators()
                .iter()
                .map(|k| GenesisValidator::new(k.public_key(), 100))
                .collect(),
            allocations: vec![Allocation {
                address: rich().public_key().address(),
                amount: 1_000_000,
            }],
            governance: GovernanceParams::default(),
        }
    }

    fn transfer(nonce: u64, to: u8) -> rz_core::Transaction {
        TxBody {
            version: 1,
            sender: rich().public_key(),
            nonce,
            fee: 1,
            kind: TxKind::Transfer {
                to: SecretKey::from_seed([to; 32]).public_key().address(),
                amount: 100,
            },
        }
        .sign(&rich(), NET)
        .unwrap()
    }

    // AT-CON-001 — cadeia finalizada determinística
    #[test]
    fn at_con_001_commits_advance() {
        let mut c = Chain::new(genesis()).unwrap();
        let ks = validators();
        for i in 0..5 {
            let cb = next_block(&c, &ks, vec![transfer(i, 30)]);
            c.commit(cb).unwrap();
        }
        assert_eq!(c.height(), 5);
        assert_eq!(c.committed_at(5).unwrap().block.id(), c.tip());
        c.state().check_supply().unwrap();
    }

    // AT-CON-002 — candidato inválido não é aceito por ser apresentado
    #[test]
    fn at_con_002_insufficient_commit_rejected() {
        let mut c = Chain::new(genesis()).unwrap();
        let ks = validators();
        let mut cb = next_block(&c, &ks, vec![]);
        cb.commit.signatures.truncate(2); // 2 de 4: não é maioria qualificada
        assert_eq!(
            c.commit(cb),
            Err(ChainError::Commit(CommitError::InsufficientPower))
        );
        assert_eq!(c.height(), 0);
    }

    #[test]
    fn wrong_proposer_rejected() {
        let mut c = Chain::new(genesis()).unwrap();
        let ks = validators();
        let state = c.state();
        let expected = *state.validators().proposer(&c.tip(), 1, 0).unwrap();
        let wrong = ks.iter().find(|k| k.public_key() != expected).unwrap();
        let (block, _) = Block::build(c.genesis(), c.tip(), 0, &state, 0, vec![], wrong).unwrap();
        let votes: Vec<Vote> = ks
            .iter()
            .map(|k| Vote::sign(VoteType::Precommit, 1, 0, Some(block.id()), k, NET))
            .collect();
        let cb = CommittedBlock {
            block,
            commit: Commit::from_votes(&votes).unwrap(),
        };
        assert_eq!(c.commit(cb), Err(ChainError::WrongProposer));
    }

    // AT-SYNC-002 — bloco adulterado com certificado de outro bloco
    #[test]
    fn tampered_block_with_foreign_commit_rejected() {
        let mut c = Chain::new(genesis()).unwrap();
        let ks = validators();
        let good = next_block(&c, &ks, vec![transfer(0, 31)]);
        // Corpo adulterado: o cabeçalho (e o certificado) continuam iguais,
        // mas a raiz de transações não confere.
        let mut bad = good.clone();
        bad.block.txs.clear();
        assert_eq!(
            c.commit(bad),
            Err(ChainError::Block(rz_core::BlockError::TxRootMismatch))
        );
        // Cabeçalho adulterado: o certificado não corresponde ao novo bloco.
        let mut bad = good.clone();
        bad.block.header.state_root = rz_crypto::Hash32([1; 32]);
        assert_eq!(
            c.commit(bad),
            Err(ChainError::Commit(CommitError::WrongBlock))
        );
        c.commit(good).unwrap();
    }

    // RZ-IR-08 — ponto de verificação: uma cadeia alternativa, mesmo com
    // certificados válidos (chaves antigas), é recusada na altura fixada.
    #[test]
    fn checkpoint_rejects_alternative_history() {
        let ks = validators();
        let mut honest = Chain::new(genesis()).unwrap();
        let a = next_block(&honest, &ks, vec![transfer(0, 50)]);
        honest.commit(a.clone()).unwrap();

        let mut fresh = Chain::new(genesis()).unwrap();
        fresh.set_checkpoints([(1, a.block.id())]).unwrap();
        let alternative = next_block(&fresh, &ks, vec![transfer(0, 51)]);
        assert_eq!(
            fresh.commit(alternative),
            Err(ChainError::Checkpoint { height: 1 })
        );
        fresh.commit(a).unwrap();

        // Uma cadeia local que já diverge é detectada ao configurar.
        let mut other = Chain::new(genesis()).unwrap();
        let b = next_block(&other, &ks, vec![transfer(0, 52)]);
        other.commit(b).unwrap();
        assert_eq!(
            other.set_checkpoints([(1, honest.tip())]),
            Err(ChainError::Checkpoint { height: 1 })
        );
    }

    // Finalidade: um bloco final não pode ser substituído (sem reorganização).
    #[test]
    fn committed_height_cannot_be_replaced() {
        let mut c = Chain::new(genesis()).unwrap();
        let ks = validators();
        let a = next_block(&c, &ks, vec![transfer(0, 40)]);
        let b = next_block(&c, &ks, vec![transfer(0, 41)]);
        c.commit(a).unwrap();
        assert!(matches!(c.commit(b), Err(ChainError::NotNext { .. })));
    }
}
