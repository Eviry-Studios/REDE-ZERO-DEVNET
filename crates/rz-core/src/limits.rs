//! Limites estáticos de decodificação (THR-P2P-002, THR-CON-005).
//!
//! Protegem contra alocação excessiva antes da validação semântica. Limites
//! de consenso mais restritos (ex.: transações por bloco) vêm do Genesis.

/// Comprimento máximo do identificador de rede.
pub const MAX_NETWORK_ID_LEN: usize = 64;
/// Máximo absoluto de transações em um bloco decodificável.
pub const MAX_BLOCK_TXS: usize = 10_000;
/// Máximo de validadores no Genesis.
pub const MAX_VALIDATORS: usize = 1_000;
/// Máximo de alocações iniciais no Genesis.
pub const MAX_ALLOCATIONS: usize = 100_000;
