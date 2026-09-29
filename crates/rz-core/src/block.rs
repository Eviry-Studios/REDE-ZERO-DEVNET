//! Blocos e aplicação de blocos ao estado (`SPEC §15–§17`).
//!
//! Este módulo verifica tudo o que depende apenas do bloco, do pai e do
//! estado anterior. Regras de consenso (quem pode produzir, escolha de fork,
//! finalidade) ficam em `rz-chain`, atrás de uma interface substituível.

use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{context, hash, Hash32, PublicKey, SecretKey, Signature};

use crate::genesis::Genesis;
use crate::limits::MAX_BLOCK_TXS;
use crate::state::{ExecParams, State, StateError};
use crate::tx::{Transaction, TxError, TxId};

/// Versão atual do formato de bloco.
pub const BLOCK_VERSION: u16 = 1;

/// Identificador de bloco: `H(BLOCK_ID, enc(BlockHeader))`.
///
/// O identificador do "bloco 0" é o hash do Genesis.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BlockId(pub Hash32);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockId({:?})", self.0)
    }
}

impl Encode for BlockId {
    fn encode(&self, e: &mut Encoder) {
        self.0.encode(e);
    }
}

impl Decode for BlockId {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self(d.get()?))
    }
}

/// Cabeçalho do bloco. Não contém timestamp: regras temporais usam alturas,
/// o que evita manipulação de relógio (THR-CON-006) e metadados
/// desnecessários.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u16,
    pub height: u64,
    pub parent: BlockId,
    /// Rodada de consenso em que o bloco foi proposto (ADR-0012).
    pub round: u64,
    pub proposer: PublicKey,
    pub tx_root: Hash32,
    pub state_root: Hash32,
}

impl Encode for BlockHeader {
    fn encode(&self, e: &mut Encoder) {
        e.u16(self.version)
            .u64(self.height)
            .put(&self.parent)
            .u64(self.round)
            .put(&self.proposer)
            .put(&self.tx_root)
            .put(&self.state_root);
    }
}

impl Decode for BlockHeader {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            version: d.u16()?,
            height: d.u64()?,
            parent: d.get()?,
            round: d.u64()?,
            proposer: d.get()?,
            tx_root: d.get()?,
            state_root: d.get()?,
        })
    }
}

impl BlockHeader {
    pub fn id(&self) -> BlockId {
        BlockId(hash(context::BLOCK_ID, &self.to_canonical_bytes()))
    }
}

/// Cabeçalho com a assinatura do produtor — suficiente para provar, sem o
/// corpo do bloco, que o produtor assinou aquele cabeçalho.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedHeader {
    pub header: BlockHeader,
    pub signature: Signature,
}

impl SignedHeader {
    pub fn verify(&self, network_id: &str) -> bool {
        self.header
            .proposer
            .verify(
                context::BLOCK_SIGNATURE,
                network_id,
                &self.header.to_canonical_bytes(),
                &self.signature,
            )
            .is_ok()
    }
}

impl Encode for SignedHeader {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.header).put(&self.signature);
    }
}

impl Decode for SignedHeader {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: d.get()?,
            signature: d.get()?,
        })
    }
}

/// Bloco assinado pelo produtor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub txs: Vec<Transaction>,
    pub signature: Signature,
}

impl Encode for Block {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.header).list(&self.txs).put(&self.signature);
    }
}

impl Decode for Block {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            header: d.get()?,
            txs: d.list(MAX_BLOCK_TXS)?,
            signature: d.get()?,
        })
    }
}

/// Compromisso da lista ordenada de transações:
/// `H(TX_ROOT, list<TxId>)`.
pub fn tx_root(txs: &[Transaction]) -> Hash32 {
    let ids: Vec<TxId> = txs.iter().map(Transaction::id).collect();
    let mut e = Encoder::new();
    e.list(&ids);
    hash(context::TX_ROOT, &e.into_bytes())
}

impl Block {
    pub fn id(&self) -> BlockId {
        self.header.id()
    }

    /// Monta e assina um bloco sobre `parent_state`, aplicando `txs` na ordem
    /// dada. Transações inválidas fazem a montagem falhar: quem escolhe as
    /// transações (o mempool) deve filtrá-las antes.
    pub fn build(
        genesis: &Genesis,
        parent: BlockId,
        parent_height: u64,
        parent_state: &State,
        round: u64,
        txs: Vec<Transaction>,
        key: &SecretKey,
    ) -> Result<(Block, State), BlockError> {
        let proposer = key.public_key();
        let state = execute(genesis, parent_state, &proposer, &txs, parent_height + 1)?;
        let header = BlockHeader {
            version: BLOCK_VERSION,
            height: parent_height + 1,
            parent,
            round,
            proposer,
            tx_root: tx_root(&txs),
            state_root: state.root(),
        };
        let signature = key.sign(
            context::BLOCK_SIGNATURE,
            &genesis.network_id,
            &header.to_canonical_bytes(),
        );
        Ok((
            Block {
                header,
                txs,
                signature,
            },
            state,
        ))
    }

    pub fn signed_header(&self) -> SignedHeader {
        SignedHeader {
            header: self.header.clone(),
            signature: self.signature,
        }
    }

    pub fn verify_signature(&self, network_id: &str) -> Result<(), BlockError> {
        self.header
            .proposer
            .verify(
                context::BLOCK_SIGNATURE,
                network_id,
                &self.header.to_canonical_bytes(),
                &self.signature,
            )
            .map_err(|_| BlockError::Signature)
    }
}

/// Executa transações, distribui taxas e aplica as regras de fim de bloco
/// (contribuição e governança), de forma determinística.
fn execute(
    genesis: &Genesis,
    parent_state: &State,
    proposer: &PublicKey,
    txs: &[Transaction],
    height: u64,
) -> Result<State, BlockError> {
    if txs.len() > parent_state.params().max_block_txs as usize {
        return Err(BlockError::TooManyTransactions(txs.len()));
    }
    let bytes: usize = txs.iter().map(|t| t.to_canonical_bytes().len()).sum();
    if bytes > crate::limits::MAX_BLOCK_TX_BYTES {
        return Err(BlockError::TooLarge(bytes));
    }
    let params = ExecParams::at(genesis, height);
    let mut state = parent_state.clone();
    let mut fees: u64 = 0;
    for (index, tx) in txs.iter().enumerate() {
        let fee = state
            .apply_transaction(tx, &params)
            .map_err(|error| BlockError::Transaction { index, error })?;
        fees = fees.checked_add(fee).ok_or(BlockError::Overflow)?;
    }
    state
        .credit_fees(proposer.address(), fees)
        .map_err(BlockError::State)?;
    state
        .end_block(proposer, height, parent_state.validators().clone())
        .map_err(BlockError::State)?;
    // Defesa em profundidade: nenhum bloco pode alterar a oferta (INV-003).
    state.check_supply().map_err(BlockError::State)?;
    Ok(state)
}

/// Aplica um bloco recebido ao estado do pai, verificando todas as regras que
/// não dependem do consenso. Retorna o novo estado.
///
/// `S(n+1) = F(S(n), B(n+1))`
pub fn apply_block(
    genesis: &Genesis,
    parent: BlockId,
    parent_height: u64,
    parent_state: &State,
    block: &Block,
) -> Result<State, BlockError> {
    let h = &block.header;
    if h.version != BLOCK_VERSION {
        return Err(BlockError::UnsupportedVersion(h.version));
    }
    if h.parent != parent {
        return Err(BlockError::WrongParent);
    }
    if Some(h.height) != parent_height.checked_add(1) {
        return Err(BlockError::WrongHeight {
            expected: parent_height.saturating_add(1),
            got: h.height,
        });
    }
    if h.tx_root != tx_root(&block.txs) {
        return Err(BlockError::TxRootMismatch);
    }
    block.verify_signature(&genesis.network_id)?;
    let state = execute(genesis, parent_state, &h.proposer, &block.txs, h.height)?;
    if state.root() != h.state_root {
        return Err(BlockError::StateRootMismatch);
    }
    Ok(state)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockError {
    UnsupportedVersion(u16),
    WrongParent,
    WrongHeight {
        expected: u64,
        got: u64,
    },
    TxRootMismatch,
    StateRootMismatch,
    Signature,
    TooManyTransactions(usize),
    /// Transações somam mais que `MAX_BLOCK_TX_BYTES`.
    TooLarge(usize),
    Transaction {
        index: usize,
        error: TxError,
    },
    State(StateError),
    Overflow,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "versão de bloco não suportada: {v}"),
            Self::WrongParent => write!(f, "Previous Block ID incorreto"),
            Self::WrongHeight { expected, got } => {
                write!(
                    f,
                    "altura inconsistente: esperado {expected}, recebido {got}"
                )
            }
            Self::TxRootMismatch => write!(f, "tx_root não confere"),
            Self::StateRootMismatch => write!(f, "state_root não confere"),
            Self::Signature => write!(f, "assinatura do bloco inválida"),
            Self::TooManyTransactions(n) => write!(f, "transações demais: {n}"),
            Self::TooLarge(n) => write!(f, "transações somam {n} bytes, acima do limite"),
            Self::Transaction { index, error } => write!(f, "transação {index} inválida: {error}"),
            Self::State(e) => write!(f, "estado inválido: {e}"),
            Self::Overflow => write!(f, "overflow aritmético"),
        }
    }
}

impl std::error::Error for BlockError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::tests::sample;
    use crate::tx::{TxBody, TxKind, TX_VERSION};

    fn validator() -> SecretKey {
        SecretKey::from_seed([1; 32])
    }

    fn rich() -> SecretKey {
        SecretKey::from_seed([2; 32])
    }

    fn tx(nonce: u64, amount: u64) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: rich().public_key(),
            nonce,
            fee: 2,
            kind: TxKind::Transfer {
                to: SecretKey::from_seed([60; 32]).public_key().address(),
                amount,
            },
        }
        .sign(&rich(), "rede-zero-devnet-test")
        .unwrap()
    }

    fn setup() -> (Genesis, State, BlockId) {
        let g = sample();
        let s = State::from_genesis(&g).unwrap();
        let id = BlockId(g.hash());
        (g, s, id)
    }

    fn build(txs: Vec<Transaction>) -> (Genesis, State, BlockId, Block, State) {
        let (g, s, gid) = setup();
        let (b, s2) = Block::build(&g, gid, 0, &s, 1, txs, &validator()).unwrap();
        (g, s, gid, b, s2)
    }

    // AT-BLOCK-001 — bloco válido
    #[test]
    fn at_block_001_valid() {
        let (g, s, gid, b, s2) = build(vec![tx(0, 10), tx(1, 20)]);
        let applied = apply_block(&g, gid, 0, &s, &b).unwrap();
        assert_eq!(applied, s2);
        // Taxas creditadas ao produtor.
        assert_eq!(
            applied.account(&validator().public_key().address()).balance,
            4
        );
        applied.check_supply().unwrap();
    }

    #[test]
    fn codec_roundtrip() {
        let (_, _, _, b, _) = build(vec![tx(0, 10)]);
        assert_eq!(
            Block::from_canonical_bytes(&b.to_canonical_bytes()).unwrap(),
            b
        );
    }

    // AT-BLOCK-002 — bloco com transação inválida
    #[test]
    fn at_block_002_invalid_tx() {
        let (g, s, gid) = setup();
        let err =
            Block::build(&g, gid, 0, &s, 1, vec![tx(0, 10), tx(0, 10)], &validator()).unwrap_err();
        assert!(matches!(err, BlockError::Transaction { index: 1, .. }));
    }

    // AT-BLOCK-003 — Previous Block ID incorreto
    #[test]
    fn at_block_003_wrong_parent() {
        let (g, s, _, b, _) = build(vec![]);
        assert_eq!(
            apply_block(&g, BlockId(Hash32([7; 32])), 0, &s, &b),
            Err(BlockError::WrongParent)
        );
    }

    // AT-BLOCK-004 — bloco adulterado
    #[test]
    fn at_block_004_tampered() {
        let (g, s, gid, b, _) = build(vec![tx(0, 10)]);

        let mut t = b.clone();
        t.txs.clear();
        assert_eq!(
            apply_block(&g, gid, 0, &s, &t),
            Err(BlockError::TxRootMismatch)
        );

        let mut t = b.clone();
        t.header.state_root = Hash32([1; 32]);
        assert_eq!(apply_block(&g, gid, 0, &s, &t), Err(BlockError::Signature));

        let mut t = b.clone();
        t.signature.0[5] ^= 1;
        assert_eq!(apply_block(&g, gid, 0, &s, &t), Err(BlockError::Signature));
    }

    // AT-BLOCK-005 — altura inconsistente
    #[test]
    fn at_block_005_height() {
        let (g, s, gid, b, _) = build(vec![]);
        assert!(matches!(
            apply_block(&g, gid, 5, &s, &b),
            Err(BlockError::WrongHeight { .. })
        ));
    }

    // Produtor honesto mas estado declarado errado → rejeitado.
    #[test]
    fn wrong_state_root_rejected() {
        let (g, s, gid) = setup();
        let header = BlockHeader {
            version: BLOCK_VERSION,
            height: 1,
            parent: gid,
            round: 1,
            proposer: validator().public_key(),
            tx_root: tx_root(&[]),
            state_root: Hash32([9; 32]),
        };
        let signature = validator().sign(
            context::BLOCK_SIGNATURE,
            &g.network_id,
            &header.to_canonical_bytes(),
        );
        let b = Block {
            header,
            txs: vec![],
            signature,
        };
        assert_eq!(
            apply_block(&g, gid, 0, &s, &b),
            Err(BlockError::StateRootMismatch)
        );
    }

    // AT-DET-003 — mesmo bloco, mesmo resultado
    #[test]
    fn at_det_003_same_block() {
        let (g, s, gid, b, _) = build(vec![tx(0, 10)]);
        let a = apply_block(&g, gid, 0, &s, &b).unwrap();
        let c = apply_block(&g, gid, 0, &s, &b).unwrap();
        assert_eq!(a.root(), c.root());
    }

    #[test]
    fn too_many_transactions() {
        let (mut g, _, _) = setup();
        g.max_block_txs = 1;
        let s = State::from_genesis(&g).unwrap();
        let gid = BlockId(g.hash());
        let err =
            Block::build(&g, gid, 0, &s, 1, vec![tx(0, 1), tx(1, 1)], &validator()).unwrap_err();
        assert_eq!(err, BlockError::TooManyTransactions(2));
    }

    // Todo bloco válido precisa caber num quadro P2P (THR-CON-005).
    #[test]
    fn oversized_block_rejected() {
        let (mut g, _, _) = setup();
        g.max_block_txs = 10_000;
        let s = State::from_genesis(&g).unwrap();
        let gid = BlockId(g.hash());
        let to = rz_privacy::keys::ShieldedSecret::from_seed(&[5; 32]).address();
        let shield = crate::private::build_shield(&rich(), &g.network_id, 0, 1, &to, 10).unwrap();
        let size = shield.to_canonical_bytes().len();
        let n = crate::limits::MAX_BLOCK_TX_BYTES / size + 1;
        // O limite é verificado antes de executar: repetir a mesma transação basta.
        let err = Block::build(&g, gid, 0, &s, 0, vec![shield; n], &validator()).unwrap_err();
        assert!(matches!(err, BlockError::TooLarge(b) if b > crate::limits::MAX_BLOCK_TX_BYTES));
    }
}
