//! Contextos de separação de domínio (`spec/CRYPTOGRAPHY.md §3`).
//!
//! Cada uso de hash ou assinatura possui um contexto próprio e versionado.
//! Um valor calculado em um contexto nunca é válido em outro (THR-ID-002).

pub const NODE_ID: &str = "rede-zero/node-id/v1";
pub const ADDRESS: &str = "rede-zero/address/v1";
pub const TX_ID: &str = "rede-zero/tx-id/v1";
pub const TX_SIGNATURE: &str = "rede-zero/tx-signature/v1";
pub const BLOCK_ID: &str = "rede-zero/block-id/v1";
pub const BLOCK_SIGNATURE: &str = "rede-zero/block-signature/v1";
pub const TX_ROOT: &str = "rede-zero/tx-root/v1";
pub const STATE_ROOT: &str = "rede-zero/state-root/v1";
pub const GENESIS: &str = "rede-zero/genesis/v1";
