//! Genesis e identificação da rede (`SPEC §18`, `SPEC §66`, `SPEC §67`).

use std::collections::BTreeSet;
use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{context, hash, Address, Hash32, PublicKey};

use crate::limits::{MAX_ALLOCATIONS, MAX_NETWORK_ID_LEN, MAX_VALIDATORS};
use crate::PROTOCOL_VERSION;

/// Tipo de ambiente. Um Node nunca aceita dados de uma rede diferente
/// (THR-ID-003, `AT-GEN-002`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkKind {
    Devnet = 0,
    Testnet = 1,
    Staging = 2,
    Mainnet = 3,
}

impl Encode for NetworkKind {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for NetworkKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(Self::Devnet),
            1 => Ok(Self::Testnet),
            2 => Ok(Self::Staging),
            3 => Ok(Self::Mainnet),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Saldo inicial de um endereço.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub address: Address,
    pub amount: u64,
}

impl Encode for Allocation {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.address).u64(self.amount);
    }
}

impl Decode for Allocation {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            address: d.get()?,
            amount: d.u64()?,
        })
    }
}

/// Definição completa do estado inicial e dos parâmetros da rede.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Genesis {
    pub protocol_version: u16,
    pub kind: NetworkKind,
    /// Identificador textual da rede, incluído em toda assinatura.
    pub network_id: String,
    /// Início do slot 0, em milissegundos desde a época Unix.
    pub genesis_time_ms: u64,
    pub slot_duration_ms: u64,
    /// Número de descendentes para um bloco ser considerado final (ADR-0006).
    pub finality_depth: u32,
    pub min_fee: u64,
    pub max_block_txs: u32,
    /// Conjunto de validadores da DEVNET (ADR-0006 — não serve para MAINNET).
    pub validators: Vec<PublicKey>,
    /// Estado monetário inicial, ordenado por endereço e sem repetições.
    pub allocations: Vec<Allocation>,
}

impl Encode for Genesis {
    fn encode(&self, e: &mut Encoder) {
        e.u16(self.protocol_version)
            .put(&self.kind)
            .str(&self.network_id)
            .u64(self.genesis_time_ms)
            .u64(self.slot_duration_ms)
            .u32(self.finality_depth)
            .u64(self.min_fee)
            .u32(self.max_block_txs)
            .list(&self.validators)
            .list(&self.allocations);
    }
}

impl Decode for Genesis {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            protocol_version: d.u16()?,
            kind: d.get()?,
            network_id: d.str(MAX_NETWORK_ID_LEN)?,
            genesis_time_ms: d.u64()?,
            slot_duration_ms: d.u64()?,
            finality_depth: d.u32()?,
            min_fee: d.u64()?,
            max_block_txs: d.u32()?,
            validators: d.list(MAX_VALIDATORS)?,
            allocations: d.list(MAX_ALLOCATIONS)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenesisError {
    UnsupportedProtocol(u16),
    InvalidNetworkId,
    NetworkKindMismatch,
    InvalidParameter(&'static str),
    NoValidators,
    DuplicateValidator,
    AllocationsNotSorted,
    ZeroAllocation,
    SupplyOverflow,
}

impl fmt::Display for GenesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProtocol(v) => write!(f, "versão de protocolo não suportada: {v}"),
            Self::InvalidNetworkId => write!(f, "identificador de rede inválido"),
            Self::NetworkKindMismatch => {
                write!(
                    f,
                    "identificador de rede não corresponde ao tipo de ambiente"
                )
            }
            Self::InvalidParameter(p) => write!(f, "parâmetro inválido: {p}"),
            Self::NoValidators => write!(f, "nenhum validador"),
            Self::DuplicateValidator => write!(f, "validador repetido"),
            Self::AllocationsNotSorted => {
                write!(f, "alocações devem estar ordenadas e sem repetição")
            }
            Self::ZeroAllocation => write!(f, "alocação com valor zero"),
            Self::SupplyOverflow => write!(f, "oferta inicial excede u64"),
        }
    }
}

impl std::error::Error for GenesisError {}

impl Genesis {
    /// Hash do Genesis: identifica a rede de forma inequívoca e serve como
    /// "Block ID" do bloco de altura 0.
    pub fn hash(&self) -> Hash32 {
        hash(context::GENESIS, &self.to_canonical_bytes())
    }

    /// Soma das alocações — oferta total inicial.
    pub fn total_supply(&self) -> Result<u64, GenesisError> {
        self.allocations.iter().try_fold(0u64, |acc, a| {
            acc.checked_add(a.amount)
                .ok_or(GenesisError::SupplyOverflow)
        })
    }

    /// Verifica a coerência do Genesis antes de qualquer uso.
    pub fn validate(&self) -> Result<(), GenesisError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(GenesisError::UnsupportedProtocol(self.protocol_version));
        }
        let id = &self.network_id;
        if id.is_empty()
            || id.len() > MAX_NETWORK_ID_LEN
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(GenesisError::InvalidNetworkId);
        }
        // O nome da rede deve deixar o ambiente explícito (`SPEC §66`).
        let marker = match self.kind {
            NetworkKind::Devnet => "devnet",
            NetworkKind::Testnet => "testnet",
            NetworkKind::Staging => "staging",
            NetworkKind::Mainnet => "mainnet",
        };
        if !id.contains(marker) {
            return Err(GenesisError::NetworkKindMismatch);
        }
        if self.slot_duration_ms == 0 {
            return Err(GenesisError::InvalidParameter("slot_duration_ms"));
        }
        if self.finality_depth == 0 {
            return Err(GenesisError::InvalidParameter("finality_depth"));
        }
        if self.max_block_txs == 0 || self.max_block_txs as usize > crate::limits::MAX_BLOCK_TXS {
            return Err(GenesisError::InvalidParameter("max_block_txs"));
        }
        if self.validators.is_empty() {
            return Err(GenesisError::NoValidators);
        }
        let unique: BTreeSet<_> = self.validators.iter().collect();
        if unique.len() != self.validators.len() {
            return Err(GenesisError::DuplicateValidator);
        }
        if !self
            .allocations
            .windows(2)
            .all(|w| w[0].address < w[1].address)
        {
            return Err(GenesisError::AllocationsNotSorted);
        }
        if self.allocations.iter().any(|a| a.amount == 0) {
            return Err(GenesisError::ZeroAllocation);
        }
        self.total_supply()?;
        Ok(())
    }

    /// Validador responsável pelo slot (ADR-0006).
    pub fn proposer_for_slot(&self, slot: u64) -> Option<&PublicKey> {
        if self.validators.is_empty() {
            return None;
        }
        let idx = (slot % self.validators.len() as u64) as usize;
        self.validators.get(idx)
    }

    /// Instante de início de um slot, em milissegundos.
    pub fn slot_start_ms(&self, slot: u64) -> Option<u64> {
        slot.checked_mul(self.slot_duration_ms)?
            .checked_add(self.genesis_time_ms)
    }

    /// Slot correspondente a um instante (`None` antes do Genesis).
    pub fn slot_at(&self, now_ms: u64) -> Option<u64> {
        now_ms
            .checked_sub(self.genesis_time_ms)
            .map(|dt| dt / self.slot_duration_ms.max(1))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rz_crypto::SecretKey;

    pub fn sample() -> Genesis {
        let v = SecretKey::from_seed([1; 32]).public_key();
        let mut allocations = vec![
            Allocation {
                address: SecretKey::from_seed([2; 32]).public_key().address(),
                amount: 1_000_000,
            },
            Allocation {
                address: SecretKey::from_seed([3; 32]).public_key().address(),
                amount: 500,
            },
        ];
        allocations.sort_by_key(|a| a.address);
        Genesis {
            protocol_version: PROTOCOL_VERSION,
            kind: NetworkKind::Devnet,
            network_id: "rede-zero-devnet-test".into(),
            genesis_time_ms: 1_000,
            slot_duration_ms: 2_000,
            finality_depth: 3,
            min_fee: 1,
            max_block_txs: 100,
            validators: vec![v],
            allocations,
        }
    }

    // AT-GEN-001 — Genesis conhecido
    #[test]
    fn at_gen_001_known_genesis() {
        let g = sample();
        g.validate().unwrap();
        assert_eq!(g.hash(), sample().hash());
        let decoded = Genesis::from_canonical_bytes(&g.to_canonical_bytes()).unwrap();
        assert_eq!(decoded, g);
    }

    // AT-GEN-002 — Genesis de rede diferente tem hash diferente
    #[test]
    fn at_gen_002_other_network() {
        let mut other = sample();
        other.network_id = "rede-zero-devnet-outra".into();
        assert_ne!(other.hash(), sample().hash());
    }

    #[test]
    fn network_kind_must_match_name() {
        let mut g = sample();
        g.kind = NetworkKind::Mainnet;
        assert_eq!(g.validate(), Err(GenesisError::NetworkKindMismatch));
    }

    #[test]
    fn rejects_unsorted_allocations() {
        let mut g = sample();
        g.allocations.reverse();
        assert_eq!(g.validate(), Err(GenesisError::AllocationsNotSorted));
    }

    #[test]
    fn rejects_duplicate_validators() {
        let mut g = sample();
        g.validators.push(g.validators[0]);
        assert_eq!(g.validate(), Err(GenesisError::DuplicateValidator));
    }

    #[test]
    fn rejects_supply_overflow() {
        let mut g = sample();
        g.allocations[0].amount = u64::MAX;
        assert_eq!(g.validate(), Err(GenesisError::SupplyOverflow));
    }

    #[test]
    fn slots() {
        let g = sample();
        assert_eq!(g.slot_at(999), None);
        assert_eq!(g.slot_at(1_000), Some(0));
        assert_eq!(g.slot_at(4_999), Some(1));
        assert_eq!(g.slot_start_ms(2), Some(5_000));
    }
}
