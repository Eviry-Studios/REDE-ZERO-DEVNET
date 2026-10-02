//! Pontuação local de mau comportamento (`SPEC §25–§26`, `SPEC §49`).
//!
//! A pontuação é **local**: cada Node decide sobre suas próprias conexões.
//! Não é reputação global e não é compartilhada — reputação protocolar
//! exige evidência verificável (`SPEC §48`).

/// Pontuação a partir da qual o par é desconectado e colocado em quarentena.
pub const BAN_THRESHOLD: u32 = 100;

/// Comportamentos objetivamente observáveis que geram penalidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offense {
    /// Mensagem que não decodifica.
    MalformedMessage,
    /// Transação com assinatura ou formato inválido.
    InvalidTransaction,
    /// Bloco que viola regras de bloco ou de consenso.
    InvalidBlock,
    /// Mensagem fora de ordem no protocolo (ex.: antes do HELLO).
    ProtocolViolation,
    /// Excedeu o limite de taxa.
    RateLimited,
}

impl Offense {
    pub fn penalty(self) -> u32 {
        match self {
            Offense::MalformedMessage => 25,
            Offense::InvalidTransaction => 10,
            Offense::InvalidBlock => 50,
            Offense::ProtocolViolation => 50,
            Offense::RateLimited => BAN_THRESHOLD,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct PeerScore {
    points: u32,
}

impl PeerScore {
    /// Registra uma infração; retorna `true` se o par deve ser desconectado.
    pub fn penalize(&mut self, offense: Offense) -> bool {
        self.points = self.points.saturating_add(offense.penalty());
        self.points >= BAN_THRESHOLD
    }

    pub fn points(&self) -> u32 {
        self.points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AT-P2P-005 — mensagens inválidas repetidas levam à desconexão
    #[test]
    fn at_p2p_005_repeated_offenses_ban() {
        let mut s = PeerScore::default();
        for _ in 0..9 {
            assert!(!s.penalize(Offense::InvalidTransaction));
        }
        assert!(s.penalize(Offense::InvalidTransaction));
    }

    #[test]
    fn rate_limit_is_immediate_ban() {
        assert!(PeerScore::default().penalize(Offense::RateLimited));
    }
}
