//! Núcleo protocolar da Rede Zero: ZERO, transações, estado, blocos e Genesis.
//!
//! Tudo neste crate é determinístico: sem relógio, sem aleatoriedade, sem
//! ponto flutuante e sem dependência de ordem de iteração não definida
//! (THR-TX-005). A mesma entrada produz o mesmo estado em qualquer Node.

pub mod amount;
pub mod block;
pub mod community;
pub mod consensus;
pub mod defense;
#[cfg(test)]
mod defense_tests;
pub mod genesis;
pub mod governance;
#[cfg(test)]
mod governance_tests;
pub mod limits;
pub mod market;
#[cfg(test)]
mod market_tests;
pub mod private;
pub mod state;
pub mod tx;

pub use amount::{format_zero, parse_zero, UNITS_PER_ZERO};
pub use block::{apply_block, Block, BlockError, BlockHeader, BlockId, SignedHeader};
pub use consensus::{
    Commit, CommittedBlock, ConsensusParams, Proposal, SignedProposal, Validator, ValidatorSet,
    Vote, VoteType,
};
pub use genesis::{Allocation, Genesis, GenesisError, GenesisValidator, NetworkKind};
pub use governance::{
    Category, Choice, GovernanceParams, ParamChange, ProposalStatus, ProtocolParams,
};
pub use private::{PrivateTx, RingInput, ShieldedOutput, Unshield};
pub use state::{Account, State, StateError};
pub use tx::{AccountTx, Transaction, TxBody, TxError, TxId, TxKind};

/// Versão do protocolo implementada por este crate.
pub const PROTOCOL_VERSION: u16 = 1;
