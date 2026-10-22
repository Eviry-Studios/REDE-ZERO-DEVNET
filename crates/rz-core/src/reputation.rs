//! Reputação (`spec/REPUTATION.md`, ADR-0020, REQ-046..050, `SPEC §48–49`).
//!
//! A reputação de um Node é o seu saldo de pontos de contribuição: ligado à
//! identidade criptográfica (nunca ao hardware, REQ-046), derivado apenas de
//! eventos verificáveis pelo protocolo e com decaimento exponencial, de modo
//! que um histórico positivo não dá imunidade permanente (REQ-048).
//!
//! O saldo fica em `State::contributions` (o mesmo que pesa na câmara de
//! contribuição da governança). Este módulo guarda o **registro** dos eventos
//! que não aparecem diretamente nos blocos, para que qualquer um verifique de
//! onde veio cada mudança (REQ-047).

use std::collections::VecDeque;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Hash32, PublicKey};

use crate::defense::Scope;

/// Contexto da raiz do registro de reputação.
pub const REPUTATION_ROOT: &str = "rede-zero/reputation-root/v1";
/// Eventos guardados (os mais recentes).
pub const MAX_EVENTS: usize = 4096;
/// Eventos devolvidos numa consulta.
pub const MAX_EVENTS_PER_MSG: usize = 64;

/// Evento que alterou a reputação de um Node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepEventKind {
    /// Contribuição defensiva atestada por > 2/3 após o encerramento do
    /// incidente (`SPEC §57`).
    DefenseContribution { incident: Hash32, role: Scope },
    /// Punição por evidência verificável (`SPEC §49`): os pontos são zerados.
    Slashed { evidence: Hash32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepEvent {
    pub key: PublicKey,
    pub kind: RepEventKind,
    /// Pontos depois do evento (escala `governance::SCALE`).
    pub points_after: u64,
    pub height: u64,
}

impl Encode for RepEvent {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.key);
        match &self.kind {
            RepEventKind::DefenseContribution { incident, role } => {
                e.u8(0).put(incident).put(role);
            }
            RepEventKind::Slashed { evidence } => {
                e.u8(1).put(evidence);
            }
        }
        e.u64(self.points_after).u64(self.height);
    }
}

impl Decode for RepEvent {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let key = d.get()?;
        let kind = match d.u8()? {
            0 => RepEventKind::DefenseContribution {
                incident: d.get()?,
                role: d.get()?,
            },
            1 => RepEventKind::Slashed { evidence: d.get()? },
            t => return Err(DecodeError::InvalidTag(t)),
        };
        Ok(Self {
            key,
            kind,
            points_after: d.u64()?,
            height: d.u64()?,
        })
    }
}

/// Registro dos eventos recentes de reputação.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReputationLog {
    pub events: VecDeque<RepEvent>,
}

impl ReputationLog {
    pub fn push(&mut self, ev: RepEvent) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(ev);
    }

    /// Eventos mais recentes de um Node (do mais novo ao mais antigo).
    pub fn of(&self, key: &PublicKey) -> Vec<RepEvent> {
        self.events
            .iter()
            .rev()
            .filter(|e| e.key == *key)
            .take(MAX_EVENTS_PER_MSG)
            .cloned()
            .collect()
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.events.len() as u64);
        for ev in &self.events {
            e.put(ev);
        }
        hash(REPUTATION_ROOT, &e.into_bytes())
    }
}
