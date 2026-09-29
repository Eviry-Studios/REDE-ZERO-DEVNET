//! Genesis e identificação da rede (`SPEC §18`, `SPEC §66`, `SPEC §67`).

use std::collections::BTreeSet;
use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{context, hash, Address, Hash32, PublicKey};

use crate::consensus::ConsensusParams;
use crate::governance::GovernanceParams;
use crate::limits::{MAX_ALLOCATIONS, MAX_NETWORK_ID_LEN, MAX_VALIDATORS};
use crate::market::{AssetId, AssetInfo, MAX_ASSETS, MAX_ASSET_NETWORK_LEN, MAX_ASSET_REF_LEN};
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

/// Validador inicial, com o ZERO que já nasce bloqueado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenesisValidator {
    pub key: PublicKey,
    pub stake: u64,
}

impl GenesisValidator {
    pub fn new(key: PublicKey, stake: u64) -> Self {
        Self { key, stake }
    }
}

impl Encode for GenesisValidator {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.key).u64(self.stake);
    }
}

impl Decode for GenesisValidator {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            key: d.get()?,
            stake: d.u64()?,
        })
    }
}

/// Ativo externo **de teste** declarado no Genesis de uma DEVNET
/// (`Verification::DevnetGenesis`). Não representa nada na rede de origem:
/// pontes verificáveis estão A DEFINIR (ADR-0014, `SPEC §42`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenesisAsset {
    pub network: String,
    pub asset_ref: String,
    pub decimals: u8,
    /// Saldos iniciais, ordenados por endereço e sem repetição.
    pub allocations: Vec<Allocation>,
}

impl GenesisAsset {
    pub fn id(&self) -> AssetId {
        AssetId::external(&self.network, &self.asset_ref)
    }

    pub fn supply(&self) -> Result<u64, GenesisError> {
        self.allocations.iter().try_fold(0u64, |acc, a| {
            acc.checked_add(a.amount)
                .ok_or(GenesisError::SupplyOverflow)
        })
    }

    pub fn info(&self) -> Result<AssetInfo, GenesisError> {
        Ok(AssetInfo {
            network: self.network.clone(),
            asset_ref: self.asset_ref.clone(),
            decimals: self.decimals,
            verification: crate::market::Verification::DevnetGenesis,
            supply: self.supply()?,
        })
    }
}

impl Encode for GenesisAsset {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.network)
            .str(&self.asset_ref)
            .u8(self.decimals)
            .list(&self.allocations);
    }
}

impl Decode for GenesisAsset {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            network: d.str(MAX_ASSET_NETWORK_LEN)?,
            asset_ref: d.str(MAX_ASSET_REF_LEN)?,
            decimals: d.u8()?,
            allocations: d.list(MAX_ALLOCATIONS)?,
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
    /// Parâmetros do consenso Zero-BFT (ADR-0012).
    pub consensus: ConsensusParams,
    pub min_fee: u64,
    pub max_block_txs: u32,
    /// Validadores iniciais. Depois do Genesis, o conjunto é aberto: qualquer
    /// conta pode bloquear ZERO e se candidatar (ADR-0012).
    pub validators: Vec<GenesisValidator>,
    /// Estado monetário inicial, ordenado por endereço e sem repetições.
    pub allocations: Vec<Allocation>,
    /// Parâmetros iniciais de governança (ADR-0008).
    pub governance: GovernanceParams,
    /// Ativos externos de teste (somente DEVNET; ADR-0014).
    pub assets: Vec<GenesisAsset>,
}

impl Encode for Genesis {
    fn encode(&self, e: &mut Encoder) {
        e.u16(self.protocol_version)
            .put(&self.kind)
            .str(&self.network_id)
            .put(&self.consensus)
            .u64(self.min_fee)
            .u32(self.max_block_txs)
            .list(&self.validators)
            .list(&self.allocations)
            .put(&self.governance)
            .list(&self.assets);
    }
}

impl Decode for Genesis {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            protocol_version: d.u16()?,
            kind: d.get()?,
            network_id: d.str(MAX_NETWORK_ID_LEN)?,
            consensus: d.get()?,
            min_fee: d.u64()?,
            max_block_txs: d.u32()?,
            validators: d.list(MAX_VALIDATORS)?,
            allocations: d.list(MAX_ALLOCATIONS)?,
            governance: d.get()?,
            assets: d.list(MAX_ASSETS)?,
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

    /// Oferta total inicial: alocações livres + ZERO bloqueado dos validadores.
    pub fn total_supply(&self) -> Result<u64, GenesisError> {
        self.allocations
            .iter()
            .map(|a| a.amount)
            .chain(self.validators.iter().map(|v| v.stake))
            .try_fold(0u64, |acc, x| {
                acc.checked_add(x).ok_or(GenesisError::SupplyOverflow)
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
        self.consensus
            .validate()
            .map_err(GenesisError::InvalidParameter)?;
        if self.max_block_txs == 0 || self.max_block_txs as usize > crate::limits::MAX_BLOCK_TXS {
            return Err(GenesisError::InvalidParameter("max_block_txs"));
        }
        if self.validators.is_empty() {
            return Err(GenesisError::NoValidators);
        }
        let unique: BTreeSet<_> = self.validators.iter().map(|v| v.key).collect();
        if unique.len() != self.validators.len() {
            return Err(GenesisError::DuplicateValidator);
        }
        if self.validators.len() > self.consensus.max_validators as usize {
            return Err(GenesisError::InvalidParameter(
                "validadores acima do máximo",
            ));
        }
        if self
            .validators
            .iter()
            .any(|v| v.stake == 0 || v.stake < self.consensus.min_bond)
        {
            return Err(GenesisError::InvalidParameter(
                "bloqueio de validador abaixo do mínimo",
            ));
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
        self.governance
            .validate()
            .map_err(GenesisError::InvalidParameter)?;
        self.validate_assets()
    }

    fn validate_assets(&self) -> Result<(), GenesisError> {
        if self.assets.is_empty() {
            return Ok(());
        }
        // Sem ponte verificável, ativos externos só existem como teste
        // (SPEC §42, THR-POOL-002).
        if self.kind != NetworkKind::Devnet {
            return Err(GenesisError::InvalidParameter(
                "ativos externos exigem ponte verificável (A DEFINIR)",
            ));
        }
        let mut ids = BTreeSet::new();
        for a in &self.assets {
            AssetInfo::check_names(&a.network, &a.asset_ref)
                .map_err(GenesisError::InvalidParameter)?;
            if !ids.insert(a.id()) {
                return Err(GenesisError::InvalidParameter("ativo repetido"));
            }
            if !a
                .allocations
                .windows(2)
                .all(|w| w[0].address < w[1].address)
            {
                return Err(GenesisError::AllocationsNotSorted);
            }
            if a.allocations.iter().any(|x| x.amount == 0) {
                return Err(GenesisError::ZeroAllocation);
            }
            a.supply()?;
        }
        Ok(())
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
            consensus: ConsensusParams::fast(200),
            min_fee: 1,
            max_block_txs: 100,
            validators: vec![GenesisValidator::new(v, 10_000)],
            allocations,
            governance: GovernanceParams {
                deposit: 1_000,
                analysis_blocks: 2,
                voting_blocks: 3,
                ordinary_delay_blocks: 2,
                constitutional_delay_blocks: 4,
                lock_min_blocks: 5,
                lock_max_blocks: 105,
                contribution_half_life_blocks: 16,
            },
            assets: vec![],
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
        g.validators.push(g.validators[0].clone());
        assert_eq!(g.validate(), Err(GenesisError::DuplicateValidator));
    }

    #[test]
    fn rejects_supply_overflow() {
        let mut g = sample();
        g.allocations[0].amount = u64::MAX;
        assert_eq!(g.validate(), Err(GenesisError::SupplyOverflow));
    }
}
