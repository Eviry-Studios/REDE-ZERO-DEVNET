//! Primitivas criptográficas da Rede Zero.
//!
//! Implementa `spec/CRYPTOGRAPHY.md` (ADR-0003):
//!
//! * hash BLAKE3 de 32 bytes com separação de domínio obrigatória;
//! * assinaturas Ed25519 com verificação estrita;
//! * mensagens assinadas sempre vinculadas a um contexto e a uma rede;
//! * Node ID e endereço de Wallet derivados da chave pública em domínios distintos.

pub mod context;
pub mod hex;
pub mod keyfile;

mod hash;
mod keys;

pub use hash::{hash, Hash32};
pub use keys::{signing_message, Address, CryptoError, NodeId, PublicKey, SecretKey, Signature};
