//! Node da DEVNET da Rede Zero.
//!
//! Integra cadeia (`rz-chain`), transporte (`rz-p2p`) e armazenamento local.
//! A Wallet é um programa separado (`rz-wallet`) e nunca entrega chaves ao
//! Node (`SPEC-WAL-001`, `SPEC-WAL-003`).

mod node;
mod store;

pub use node::{now_ms, LogLevel, Node, NodeConfig, NodeStatus};
pub use store::BlockStore;
