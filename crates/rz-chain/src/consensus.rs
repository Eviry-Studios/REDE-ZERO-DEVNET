//! Interface de consenso e autoridade rotativa da DEVNET (ADR-0006).

use std::fmt;

use rz_core::{BlockHeader, BlockId, Genesis};
use rz_crypto::PublicKey;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsensusError {
    /// O produtor não é o responsável pelo slot.
    WrongProposer { slot: u64 },
    /// O slot começa no futuro além da tolerância.
    FutureSlot { slot: u64 },
}

impl fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongProposer { slot } => write!(f, "produtor incorreto para o slot {slot}"),
            Self::FutureSlot { slot } => write!(f, "slot {slot} no futuro"),
        }
    }
}

impl std::error::Error for ConsensusError {}

/// Regras que dependem do mecanismo de consenso escolhido.
///
/// Trocar o consenso significa fornecer outra implementação desta interface,
/// sem alterar transações, estado, blocos ou P2P (ADR-0006).
pub trait ConsensusEngine {
    /// Produtor esperado para um slot.
    fn expected_proposer<'g>(&self, genesis: &'g Genesis, slot: u64) -> Option<&'g PublicKey>;

    /// Valida as regras de consenso do cabeçalho.
    ///
    /// `now_ms` é `None` ao reprocessar blocos já persistidos, quando regras
    /// dependentes do relógio local não se aplicam.
    fn check_header(
        &self,
        genesis: &Genesis,
        header: &BlockHeader,
        now_ms: Option<u64>,
    ) -> Result<(), ConsensusError>;

    /// Escolha de fork: `true` se `a` deve ser preferido a `b`.
    fn prefer(&self, a: (u64, BlockId), b: (u64, BlockId)) -> bool;
}

/// Autoridade rotativa: `produtor(slot) = validadores[slot mod n]`.
///
/// **Somente DEVNET.** Ver `docs/adr/0006-consenso-devnet.md`.
#[derive(Clone, Debug)]
pub struct RoundRobin {
    /// Tolerância para blocos cujo slot ainda não começou (desvio de relógio).
    pub max_future_ms: u64,
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self {
            max_future_ms: 1_000,
        }
    }
}

impl ConsensusEngine for RoundRobin {
    fn expected_proposer<'g>(&self, genesis: &'g Genesis, slot: u64) -> Option<&'g PublicKey> {
        genesis.proposer_for_slot(slot)
    }

    fn check_header(
        &self,
        genesis: &Genesis,
        header: &BlockHeader,
        now_ms: Option<u64>,
    ) -> Result<(), ConsensusError> {
        if self.expected_proposer(genesis, header.slot) != Some(&header.proposer) {
            return Err(ConsensusError::WrongProposer { slot: header.slot });
        }
        if let Some(now) = now_ms {
            let start = genesis
                .slot_start_ms(header.slot)
                .ok_or(ConsensusError::FutureSlot { slot: header.slot })?;
            if start > now.saturating_add(self.max_future_ms) {
                return Err(ConsensusError::FutureSlot { slot: header.slot });
            }
        }
        Ok(())
    }

    /// Maior altura; em empate, menor BlockId.
    fn prefer(&self, a: (u64, BlockId), b: (u64, BlockId)) -> bool {
        a.0 > b.0 || (a.0 == b.0 && a.1 < b.1)
    }
}
