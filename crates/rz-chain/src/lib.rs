//! Cadeia, consenso e mempool da Rede Zero.
//!
//! O consenso está isolado atrás de [`ConsensusEngine`] (ADR-0006). A
//! implementação [`RoundRobin`] serve **apenas à DEVNET**.

mod chain;
mod consensus;
mod mempool;

pub use chain::{Chain, ChainError, Equivocation, ImportOutcome};
pub use consensus::{ConsensusEngine, ConsensusError, RoundRobin};
pub use mempool::{Mempool, MempoolError};
