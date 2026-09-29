//! Cadeia, consenso e mempool da Rede Zero.
//!
//! * [`bft`] — máquina de estados do consenso Zero-BFT (ADR-0012), sem E/S.
//! * [`Chain`] — blocos finalizados, cada um com certificado verificável.
//! * [`Mempool`] — transações pendentes.

pub mod bft;
mod chain;
mod mempool;

#[cfg(test)]
mod bft_tests;

pub use bft::{App, Bft, Output, Reaction, Step, Timeout};
pub use chain::{testing, Chain, ChainError};
pub use mempool::{Mempool, MempoolError};
