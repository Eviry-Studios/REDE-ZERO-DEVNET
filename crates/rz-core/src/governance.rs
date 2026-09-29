//! Governança bicameral (ADR-0008, `spec/GOVERNANCE.md`).
//!
//! * **Câmara econômica** — ZERO bloqueado × multiplicador de compromisso
//!   temporal (1× a 4×). Linear no valor: dividir entre identidades não
//!   aumenta o peso (GOV-INV-4).
//! * **Câmara de contribuição** — pontos de contribuição verificável com
//!   meia-vida, não transferíveis (GOV-INV-5).
//!
//! Aprovação exige quórum e limiar nas duas câmaras. Toda a apuração é uma
//! função determinística do estado (GOV-INV-3), em aritmética inteira.

use std::collections::BTreeMap;
use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{Address, Hash32, PublicKey};

/// Escala fixa para multiplicadores e pontos (6 casas decimais).
pub const SCALE: u64 = 1_000_000;
/// Máximo de alterações de parâmetro em uma proposta.
pub const MAX_PARAM_CHANGES: usize = 16;

// ------------------------------------------------------------- parâmetros

/// Parâmetros de governança, em **blocos** (nunca tempo de relógio).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceParams {
    pub deposit: u64,
    pub analysis_blocks: u64,
    pub voting_blocks: u64,
    pub ordinary_delay_blocks: u64,
    pub constitutional_delay_blocks: u64,
    pub lock_min_blocks: u64,
    pub lock_max_blocks: u64,
    pub contribution_half_life_blocks: u64,
}

impl GovernanceParams {
    /// Valores iniciais de `spec/GOVERNANCE.md §2`, convertidos em blocos
    /// para o intervalo de bloco informado.
    pub fn for_block_ms(block_ms: u64) -> Self {
        let blocks = |secs: u64| (secs.saturating_mul(1000) / block_ms.max(1)).max(1);
        const DAY: u64 = 86_400;
        Self {
            deposit: 100 * crate::UNITS_PER_ZERO,
            analysis_blocks: blocks(2 * DAY),
            voting_blocks: blocks(7 * DAY),
            ordinary_delay_blocks: blocks(7 * DAY),
            constitutional_delay_blocks: blocks(30 * DAY),
            lock_min_blocks: blocks(7 * DAY),
            lock_max_blocks: blocks(365 * DAY),
            contribution_half_life_blocks: blocks(182 * DAY),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.deposit == 0 {
            return Err("depósito de governança deve ser positivo");
        }
        if self.voting_blocks == 0 {
            return Err("período de votação deve ser positivo");
        }
        if self.lock_min_blocks == 0 || self.lock_max_blocks <= self.lock_min_blocks {
            return Err("limites de bloqueio inválidos");
        }
        if self.contribution_half_life_blocks == 0 {
            return Err("meia-vida deve ser positiva");
        }
        Ok(())
    }
}

impl Default for GovernanceParams {
    fn default() -> Self {
        Self::for_block_ms(2_000)
    }
}

impl Encode for GovernanceParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.deposit)
            .u64(self.analysis_blocks)
            .u64(self.voting_blocks)
            .u64(self.ordinary_delay_blocks)
            .u64(self.constitutional_delay_blocks)
            .u64(self.lock_min_blocks)
            .u64(self.lock_max_blocks)
            .u64(self.contribution_half_life_blocks);
    }
}

impl Decode for GovernanceParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            deposit: d.u64()?,
            analysis_blocks: d.u64()?,
            voting_blocks: d.u64()?,
            ordinary_delay_blocks: d.u64()?,
            constitutional_delay_blocks: d.u64()?,
            lock_min_blocks: d.u64()?,
            lock_max_blocks: d.u64()?,
            contribution_half_life_blocks: d.u64()?,
        })
    }
}

/// Parâmetros vigentes do protocolo, mantidos no estado. Só mudam por
/// proposta aprovada e ativada (REQ-040, REQ-092).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolParams {
    pub min_fee: u64,
    pub max_block_txs: u32,
    pub governance: GovernanceParams,
    pub consensus: crate::consensus::ConsensusParams,
}

impl ProtocolParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_block_txs == 0 || self.max_block_txs as usize > crate::limits::MAX_BLOCK_TXS {
            return Err("max_block_txs fora dos limites");
        }
        self.consensus.validate()?;
        self.governance.validate()
    }
}

impl Encode for ProtocolParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.min_fee)
            .u32(self.max_block_txs)
            .put(&self.governance)
            .put(&self.consensus);
    }
}

impl Decode for ProtocolParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            min_fee: d.u64()?,
            max_block_txs: d.u32()?,
            governance: d.get()?,
            consensus: d.get()?,
        })
    }
}

// ------------------------------------------------------------- categorias

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Ordinary = 0,
    Community = 1,
    Constitutional = 2,
}

impl Category {
    /// Quórum mínimo, em porcentagem do peso elegível de cada câmara.
    pub fn quorum_percent(self) -> u128 {
        match self {
            Category::Ordinary => 10,
            Category::Community => 5,
            Category::Constitutional => 20,
        }
    }

    fn threshold(self) -> Threshold {
        match self {
            Category::Ordinary | Category::Community => Threshold::Majority,
            Category::Constitutional => Threshold::TwoThirds,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::Ordinary => "ordinária",
            Category::Community => "comunidade",
            Category::Constitutional => "constitucional",
        }
    }
}

impl Encode for Category {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for Category {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(Category::Ordinary),
            1 => Ok(Category::Community),
            2 => Ok(Category::Constitutional),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Threshold {
    /// Estritamente mais da metade.
    Majority,
    /// Dois terços ou mais.
    TwoThirds,
}

impl Threshold {
    fn passes(self, yes: u128, no: u128) -> bool {
        let decided = yes + no;
        decided > 0
            && match self {
                Threshold::Majority => 2 * yes > decided,
                Threshold::TwoThirds => 3 * yes >= 2 * decided,
            }
    }
}

// ------------------------------------------------------------- alterações

/// Alteração de parâmetro. Cada parâmetro pertence a uma categoria fixa
/// (`spec/GOVERNANCE.md §6`). Não existe alteração capaz de mudar saldos de
/// endereços específicos nem de retirar ativos do Pool (GOV-INV-1, GOV-INV-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamChange {
    MinFee(u64),
    MaxBlockTxs(u32),
    GovDeposit(u64),
    AnalysisBlocks(u64),
    VotingBlocks(u64),
    OrdinaryDelay(u64),
    ConstitutionalDelay(u64),
    LockMinBlocks(u64),
    LockMaxBlocks(u64),
    ContributionHalfLife(u64),
    // Consenso Zero-BFT (ADR-0012); todos constitucionais.
    EpochBlocks(u64),
    MaxValidators(u64),
    MinBond(u64),
    UnbondingBlocks(u64),
    SlashBps(u64),
    BlockInterval(u64),
    TimeoutPropose(u64),
    TimeoutPrevote(u64),
    TimeoutPrecommit(u64),
    TimeoutDelta(u64),
}

impl ParamChange {
    pub fn category(&self) -> Category {
        match self {
            ParamChange::MinFee(_) | ParamChange::MaxBlockTxs(_) => Category::Ordinary,
            _ => Category::Constitutional,
        }
    }

    fn tag(&self) -> u8 {
        match self {
            ParamChange::MinFee(_) => 0,
            ParamChange::MaxBlockTxs(_) => 1,
            ParamChange::GovDeposit(_) => 2,
            ParamChange::AnalysisBlocks(_) => 3,
            ParamChange::VotingBlocks(_) => 4,
            ParamChange::OrdinaryDelay(_) => 5,
            ParamChange::ConstitutionalDelay(_) => 6,
            ParamChange::LockMinBlocks(_) => 7,
            ParamChange::LockMaxBlocks(_) => 8,
            ParamChange::ContributionHalfLife(_) => 9,
            ParamChange::EpochBlocks(_) => 10,
            ParamChange::MaxValidators(_) => 11,
            ParamChange::MinBond(_) => 12,
            ParamChange::UnbondingBlocks(_) => 13,
            ParamChange::SlashBps(_) => 14,
            ParamChange::BlockInterval(_) => 15,
            ParamChange::TimeoutPropose(_) => 16,
            ParamChange::TimeoutPrevote(_) => 17,
            ParamChange::TimeoutPrecommit(_) => 18,
            ParamChange::TimeoutDelta(_) => 19,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ParamChange::MinFee(_) => "min_fee",
            ParamChange::MaxBlockTxs(_) => "max_block_txs",
            ParamChange::GovDeposit(_) => "gov_deposit",
            ParamChange::AnalysisBlocks(_) => "analysis_blocks",
            ParamChange::VotingBlocks(_) => "voting_blocks",
            ParamChange::OrdinaryDelay(_) => "ordinary_delay_blocks",
            ParamChange::ConstitutionalDelay(_) => "constitutional_delay_blocks",
            ParamChange::LockMinBlocks(_) => "lock_min_blocks",
            ParamChange::LockMaxBlocks(_) => "lock_max_blocks",
            ParamChange::ContributionHalfLife(_) => "contribution_half_life_blocks",
            ParamChange::EpochBlocks(_) => "epoch_blocks",
            ParamChange::MaxValidators(_) => "max_validators",
            ParamChange::MinBond(_) => "min_bond",
            ParamChange::UnbondingBlocks(_) => "unbonding_blocks",
            ParamChange::SlashBps(_) => "slash_bps",
            ParamChange::BlockInterval(_) => "block_interval_ms",
            ParamChange::TimeoutPropose(_) => "timeout_propose_ms",
            ParamChange::TimeoutPrevote(_) => "timeout_prevote_ms",
            ParamChange::TimeoutPrecommit(_) => "timeout_precommit_ms",
            ParamChange::TimeoutDelta(_) => "timeout_delta_ms",
        }
    }

    /// Interpreta `nome=valor` (usado por interfaces).
    pub fn parse(name: &str, value: &str) -> Option<Self> {
        let v: u64 = value.parse().ok()?;
        Some(match name {
            "min_fee" => ParamChange::MinFee(v),
            "max_block_txs" => ParamChange::MaxBlockTxs(u32::try_from(v).ok()?),
            "gov_deposit" => ParamChange::GovDeposit(v),
            "analysis_blocks" => ParamChange::AnalysisBlocks(v),
            "voting_blocks" => ParamChange::VotingBlocks(v),
            "ordinary_delay_blocks" => ParamChange::OrdinaryDelay(v),
            "constitutional_delay_blocks" => ParamChange::ConstitutionalDelay(v),
            "lock_min_blocks" => ParamChange::LockMinBlocks(v),
            "lock_max_blocks" => ParamChange::LockMaxBlocks(v),
            "contribution_half_life_blocks" => ParamChange::ContributionHalfLife(v),
            "epoch_blocks" => ParamChange::EpochBlocks(v),
            "max_validators" => ParamChange::MaxValidators(v),
            "min_bond" => ParamChange::MinBond(v),
            "unbonding_blocks" => ParamChange::UnbondingBlocks(v),
            "slash_bps" => ParamChange::SlashBps(v),
            "block_interval_ms" => ParamChange::BlockInterval(v),
            "timeout_propose_ms" => ParamChange::TimeoutPropose(v),
            "timeout_prevote_ms" => ParamChange::TimeoutPrevote(v),
            "timeout_precommit_ms" => ParamChange::TimeoutPrecommit(v),
            "timeout_delta_ms" => ParamChange::TimeoutDelta(v),
            _ => return None,
        })
    }

    pub fn value(&self) -> u64 {
        match *self {
            ParamChange::MaxBlockTxs(v) => u64::from(v),
            ParamChange::MinFee(v)
            | ParamChange::GovDeposit(v)
            | ParamChange::AnalysisBlocks(v)
            | ParamChange::VotingBlocks(v)
            | ParamChange::OrdinaryDelay(v)
            | ParamChange::ConstitutionalDelay(v)
            | ParamChange::LockMinBlocks(v)
            | ParamChange::LockMaxBlocks(v)
            | ParamChange::ContributionHalfLife(v)
            | ParamChange::EpochBlocks(v)
            | ParamChange::MaxValidators(v)
            | ParamChange::MinBond(v)
            | ParamChange::UnbondingBlocks(v)
            | ParamChange::SlashBps(v)
            | ParamChange::BlockInterval(v)
            | ParamChange::TimeoutPropose(v)
            | ParamChange::TimeoutPrevote(v)
            | ParamChange::TimeoutPrecommit(v)
            | ParamChange::TimeoutDelta(v) => v,
        }
    }

    pub fn apply(&self, p: &mut ProtocolParams) {
        match *self {
            ParamChange::MinFee(v) => p.min_fee = v,
            ParamChange::MaxBlockTxs(v) => p.max_block_txs = v,
            ParamChange::GovDeposit(v) => p.governance.deposit = v,
            ParamChange::AnalysisBlocks(v) => p.governance.analysis_blocks = v,
            ParamChange::VotingBlocks(v) => p.governance.voting_blocks = v,
            ParamChange::OrdinaryDelay(v) => p.governance.ordinary_delay_blocks = v,
            ParamChange::ConstitutionalDelay(v) => p.governance.constitutional_delay_blocks = v,
            ParamChange::LockMinBlocks(v) => p.governance.lock_min_blocks = v,
            ParamChange::LockMaxBlocks(v) => p.governance.lock_max_blocks = v,
            ParamChange::ContributionHalfLife(v) => p.governance.contribution_half_life_blocks = v,
            ParamChange::EpochBlocks(v) => p.consensus.epoch_blocks = v,
            ParamChange::MaxValidators(v) => {
                p.consensus.max_validators = u32::try_from(v).unwrap_or(u32::MAX)
            }
            ParamChange::MinBond(v) => p.consensus.min_bond = v,
            ParamChange::UnbondingBlocks(v) => p.consensus.unbonding_blocks = v,
            ParamChange::SlashBps(v) => {
                p.consensus.slash_bps = u32::try_from(v).unwrap_or(u32::MAX)
            }
            ParamChange::BlockInterval(v) => p.consensus.block_interval_ms = v,
            ParamChange::TimeoutPropose(v) => p.consensus.timeout_propose_ms = v,
            ParamChange::TimeoutPrevote(v) => p.consensus.timeout_prevote_ms = v,
            ParamChange::TimeoutPrecommit(v) => p.consensus.timeout_precommit_ms = v,
            ParamChange::TimeoutDelta(v) => p.consensus.timeout_delta_ms = v,
        }
    }
}

impl Encode for ParamChange {
    fn encode(&self, e: &mut Encoder) {
        e.u8(self.tag());
        match *self {
            ParamChange::MaxBlockTxs(v) => {
                e.u32(v);
            }
            _ => {
                e.u64(self.value());
            }
        }
    }
}

impl Decode for ParamChange {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(match d.u8()? {
            0 => ParamChange::MinFee(d.u64()?),
            1 => ParamChange::MaxBlockTxs(d.u32()?),
            2 => ParamChange::GovDeposit(d.u64()?),
            3 => ParamChange::AnalysisBlocks(d.u64()?),
            4 => ParamChange::VotingBlocks(d.u64()?),
            5 => ParamChange::OrdinaryDelay(d.u64()?),
            6 => ParamChange::ConstitutionalDelay(d.u64()?),
            7 => ParamChange::LockMinBlocks(d.u64()?),
            8 => ParamChange::LockMaxBlocks(d.u64()?),
            9 => ParamChange::ContributionHalfLife(d.u64()?),
            10 => ParamChange::EpochBlocks(d.u64()?),
            11 => ParamChange::MaxValidators(d.u64()?),
            12 => ParamChange::MinBond(d.u64()?),
            13 => ParamChange::UnbondingBlocks(d.u64()?),
            14 => ParamChange::SlashBps(d.u64()?),
            15 => ParamChange::BlockInterval(d.u64()?),
            16 => ParamChange::TimeoutPropose(d.u64()?),
            17 => ParamChange::TimeoutPrevote(d.u64()?),
            18 => ParamChange::TimeoutPrecommit(d.u64()?),
            19 => ParamChange::TimeoutDelta(d.u64()?),
            t => return Err(DecodeError::InvalidTag(t)),
        })
    }
}

/// Verifica a coerência de uma proposta (`spec/GOVERNANCE.md §6`).
pub fn check_proposal_shape(
    category: Category,
    params: &[ParamChange],
    current: &ProtocolParams,
) -> Result<(), &'static str> {
    if params.len() > MAX_PARAM_CHANGES {
        return Err("alterações demais");
    }
    let mut tags: Vec<u8> = params.iter().map(ParamChange::tag).collect();
    tags.sort_unstable();
    if tags.windows(2).any(|w| w[0] == w[1]) {
        return Err("parâmetro repetido");
    }
    if category == Category::Community && !params.is_empty() {
        return Err("proposta de comunidade não altera parâmetros");
    }
    if let Some(max) = params.iter().map(ParamChange::category).max() {
        if max != category {
            return Err("categoria não corresponde aos parâmetros alterados");
        }
    }
    let mut next = current.clone();
    for p in params {
        p.apply(&mut next);
    }
    next.validate()
}

// ------------------------------------------------------------- votos

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Yes = 0,
    No = 1,
    Abstain = 2,
}

impl Choice {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sim" | "yes" => Some(Choice::Yes),
            "nao" | "não" | "no" => Some(Choice::No),
            "abstencao" | "abstenção" | "abstain" => Some(Choice::Abstain),
            _ => None,
        }
    }
}

impl Encode for Choice {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for Choice {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(Choice::Yes),
            1 => Ok(Choice::No),
            2 => Ok(Choice::Abstain),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    pub key: PublicKey,
    pub choice: Choice,
}

impl Encode for Vote {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.key).put(&self.choice);
    }
}

impl Decode for Vote {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            key: d.get()?,
            choice: d.get()?,
        })
    }
}

// ------------------------------------------------------------- bloqueios

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    pub owner: Address,
    pub amount: u64,
    pub locked_at: u64,
    pub unlock_height: u64,
}

impl Encode for Lock {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.owner)
            .u64(self.amount)
            .u64(self.locked_at)
            .u64(self.unlock_height);
    }
}

impl Decode for Lock {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            owner: d.get()?,
            amount: d.u64()?,
            locked_at: d.u64()?,
            unlock_height: d.u64()?,
        })
    }
}

/// Bloqueio com seu identificador (usado em consultas de interfaces).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    pub id: u64,
    pub lock: Lock,
}

impl Encode for LockEntry {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.id).put(&self.lock);
    }
}

impl Decode for LockEntry {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: d.u64()?,
            lock: d.get()?,
        })
    }
}

/// Multiplicador de compromisso `m(d)` em escala [`SCALE`]: de 1× a 4×.
pub fn commitment_multiplier(duration: u64, g: &GovernanceParams) -> u128 {
    let span = g.lock_max_blocks.saturating_sub(g.lock_min_blocks).max(1);
    let d = duration
        .min(g.lock_max_blocks)
        .saturating_sub(g.lock_min_blocks);
    SCALE as u128 + 3 * SCALE as u128 * d as u128 / span as u128
}

/// Peso econômico de um bloqueio para uma proposta: só conta se criado
/// **antes** da submissão e se permanecer bloqueado até o fim da votação.
pub fn lock_weight(lock: &Lock, proposal: &Proposal, g: &GovernanceParams) -> u128 {
    if lock.locked_at >= proposal.submitted_at || lock.unlock_height < proposal.voting_end {
        return 0;
    }
    lock.amount as u128 * commitment_multiplier(lock.unlock_height - lock.locked_at, g)
}

// ------------------------------------------------------------- contribuição

/// `2^(−i/16)` em escala [`SCALE`], para `i = 0..15`.
const DECAY_TABLE: [u64; 16] = [
    1_000_000, 957_603, 917_004, 878_126, 840_896, 805_245, 771_105, 738_413, 707_107, 677_128,
    648_420, 620_929, 594_604, 569_394, 545_254, 522_137,
];

/// Decaimento exponencial determinístico: `v × 2^(−dt/meia_vida)`,
/// em aritmética inteira (meias-vidas inteiras por deslocamento, fração por
/// tabela de 1/16 de meia-vida).
pub fn decay(value: u64, dt: u64, half_life: u64) -> u64 {
    let hl = half_life.max(1);
    let halvings = dt / hl;
    if halvings >= 64 {
        return 0;
    }
    let v = value >> halvings;
    let idx = ((dt % hl) as u128 * 16 / hl as u128) as usize;
    (v as u128 * DECAY_TABLE[idx.min(15)] as u128 / SCALE as u128) as u64
}

/// Pontos de contribuição de uma identidade de Node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contribution {
    /// Pontos em escala [`SCALE`], válidos em `updated_at`.
    pub points: u64,
    pub updated_at: u64,
}

impl Contribution {
    pub fn at(&self, height: u64, half_life: u64) -> u64 {
        decay(
            self.points,
            height.saturating_sub(self.updated_at),
            half_life,
        )
    }
}

impl Encode for Contribution {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.points).u64(self.updated_at);
    }
}

impl Decode for Contribution {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            points: d.u64()?,
            updated_at: d.u64()?,
        })
    }
}

// ------------------------------------------------------------- propostas

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Em análise ou votação (conforme a altura).
    Pending = 0,
    Approved = 1,
    Rejected = 2,
    NoQuorum = 3,
    Activated = 4,
    /// Aprovada, mas os parâmetros resultantes eram inválidos na ativação.
    ActivationFailed = 5,
}

impl ProposalStatus {
    pub fn name(self) -> &'static str {
        match self {
            ProposalStatus::Pending => "pendente",
            ProposalStatus::Approved => "aprovada (aguardando ativação)",
            ProposalStatus::Rejected => "rejeitada",
            ProposalStatus::NoQuorum => "sem quórum",
            ProposalStatus::Activated => "ativada",
            ProposalStatus::ActivationFailed => "falha na ativação",
        }
    }
}

impl Encode for ProposalStatus {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for ProposalStatus {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(match d.u8()? {
            0 => ProposalStatus::Pending,
            1 => ProposalStatus::Approved,
            2 => ProposalStatus::Rejected,
            3 => ProposalStatus::NoQuorum,
            4 => ProposalStatus::Activated,
            5 => ProposalStatus::ActivationFailed,
            t => return Err(DecodeError::InvalidTag(t)),
        })
    }
}

/// Resultado de apuração por câmara.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChamberTally {
    pub yes: u128,
    pub no: u128,
    pub abstain: u128,
    pub eligible: u128,
    pub largest_voter: u128,
}

impl ChamberTally {
    pub fn participation(&self) -> u128 {
        self.yes + self.no + self.abstain
    }

    fn quorum(&self, percent: u128) -> bool {
        self.eligible > 0 && self.participation() * 100 >= percent * self.eligible
    }

    /// Um único votante com mais de 1/3 da participação.
    fn concentrated(&self) -> bool {
        self.participation() > 0 && self.largest_voter * 3 > self.participation()
    }
}

fn put_u128(e: &mut Encoder, v: u128) {
    e.fixed(&v.to_be_bytes());
}

fn get_u128(d: &mut Decoder<'_>) -> Result<u128, DecodeError> {
    Ok(u128::from_be_bytes(d.fixed()?))
}

impl Encode for ChamberTally {
    fn encode(&self, e: &mut Encoder) {
        for v in [
            self.yes,
            self.no,
            self.abstain,
            self.eligible,
            self.largest_voter,
        ] {
            put_u128(e, v);
        }
    }
}

impl Decode for ChamberTally {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            yes: get_u128(d)?,
            no: get_u128(d)?,
            abstain: get_u128(d)?,
            eligible: get_u128(d)?,
            largest_voter: get_u128(d)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tally {
    pub economic: ChamberTally,
    pub contribution: ChamberTally,
}

impl Encode for Tally {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.economic).put(&self.contribution);
    }
}

impl Decode for Tally {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            economic: d.get()?,
            contribution: d.get()?,
        })
    }
}

/// Decide o resultado a partir da apuração (`spec/GOVERNANCE.md §5`).
pub fn decide(category: Category, t: &Tally) -> ProposalStatus {
    let q = category.quorum_percent();
    if !t.economic.quorum(q) || !t.contribution.quorum(q) {
        return ProposalStatus::NoQuorum;
    }
    let base = category.threshold();
    // Freio de concentração: votante dominante em uma câmara endurece a outra.
    let th_e = if t.contribution.concentrated() {
        Threshold::TwoThirds
    } else {
        base
    };
    let th_c = if t.economic.concentrated() {
        Threshold::TwoThirds
    } else {
        base
    };
    if th_e.passes(t.economic.yes, t.economic.no)
        && th_c.passes(t.contribution.yes, t.contribution.no)
    {
        ProposalStatus::Approved
    } else {
        ProposalStatus::Rejected
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub id: Hash32,
    pub proposer: Address,
    pub category: Category,
    pub content_hash: Hash32,
    pub params: Vec<ParamChange>,
    pub release_id: Option<Hash32>,
    pub deposit: u64,
    pub submitted_at: u64,
    pub voting_start: u64,
    pub voting_end: u64,
    pub activation_height: u64,
    pub status: ProposalStatus,
    pub votes: BTreeMap<Address, Vote>,
    pub tally: Option<Tally>,
}

impl Proposal {
    /// Codificação sem a lista de votos (para consultas de interfaces).
    pub fn encode_summary(&self, e: &mut Encoder) {
        e.put(&self.id)
            .put(&self.proposer)
            .put(&self.category)
            .put(&self.content_hash)
            .list(&self.params)
            .option(&self.release_id)
            .u64(self.deposit)
            .u64(self.submitted_at)
            .u64(self.voting_start)
            .u64(self.voting_end)
            .u64(self.activation_height)
            .put(&self.status)
            .option(&self.tally);
    }
}

impl Encode for Proposal {
    fn encode(&self, e: &mut Encoder) {
        self.encode_summary(e);
        e.u32(u32::try_from(self.votes.len()).unwrap_or(u32::MAX));
        for (addr, v) in &self.votes {
            e.put(addr).put(v);
        }
    }
}

/// Resumo de proposta, como enviado a interfaces (sem votos individuais).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalSummary(pub Proposal);

impl Encode for ProposalSummary {
    fn encode(&self, e: &mut Encoder) {
        self.0.encode_summary(e);
        e.u32(u32::try_from(self.0.votes.len()).unwrap_or(u32::MAX));
    }
}

impl Decode for ProposalSummary {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let p = Proposal {
            id: d.get()?,
            proposer: d.get()?,
            category: d.get()?,
            content_hash: d.get()?,
            params: d.list(MAX_PARAM_CHANGES)?,
            release_id: d.option()?,
            deposit: d.u64()?,
            submitted_at: d.u64()?,
            voting_start: d.u64()?,
            voting_end: d.u64()?,
            activation_height: d.u64()?,
            status: d.get()?,
            votes: BTreeMap::new(),
            tally: d.option()?,
        };
        let _vote_count = d.u32()?;
        Ok(Self(p))
    }
}

impl fmt::Display for ProposalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> GovernanceParams {
        GovernanceParams {
            deposit: 100,
            analysis_blocks: 2,
            voting_blocks: 5,
            ordinary_delay_blocks: 3,
            constitutional_delay_blocks: 10,
            lock_min_blocks: 10,
            lock_max_blocks: 110,
            contribution_half_life_blocks: 16,
        }
    }

    #[test]
    fn multiplier_range() {
        let g = g();
        assert_eq!(commitment_multiplier(10, &g), SCALE as u128);
        assert_eq!(commitment_multiplier(60, &g), 2_500_000);
        assert_eq!(commitment_multiplier(110, &g), 4 * SCALE as u128);
        assert_eq!(commitment_multiplier(10_000, &g), 4 * SCALE as u128);
    }

    #[test]
    fn decay_halves() {
        assert_eq!(decay(SCALE, 0, 16), SCALE);
        assert_eq!(decay(SCALE, 16, 16), SCALE / 2);
        assert_eq!(decay(SCALE, 32, 16), SCALE / 4);
        assert_eq!(decay(SCALE, 8, 16), 707_107);
        assert_eq!(decay(SCALE, 16 * 64, 16), 0);
        // Monótono.
        let mut last = u64::MAX;
        for dt in 0..100 {
            let v = decay(SCALE, dt, 16);
            assert!(v <= last);
            last = v;
        }
    }

    #[test]
    fn decay_table_matches_formula() {
        for (i, v) in DECAY_TABLE.iter().enumerate() {
            let exact = (SCALE as f64 * 2f64.powf(-(i as f64) / 16.0)).round() as u64;
            assert_eq!(*v, exact, "entrada {i}");
        }
    }

    fn tally(e: (u128, u128, u128, u128), c: (u128, u128, u128, u128)) -> Tally {
        let mk = |(y, n, a, el): (u128, u128, u128, u128)| ChamberTally {
            yes: y,
            no: n,
            abstain: a,
            eligible: el,
            largest_voter: 0,
        };
        Tally {
            economic: mk(e),
            contribution: mk(c),
        }
    }

    #[test]
    fn both_chambers_required() {
        let ok = tally((60, 40, 0, 1000), (6, 4, 0, 100));
        assert_eq!(decide(Category::Ordinary, &ok), ProposalStatus::Approved);
        let e_only = tally((90, 10, 0, 1000), (4, 6, 0, 100));
        assert_eq!(
            decide(Category::Ordinary, &e_only),
            ProposalStatus::Rejected
        );
        let c_only = tally((40, 60, 0, 1000), (9, 1, 0, 100));
        assert_eq!(
            decide(Category::Ordinary, &c_only),
            ProposalStatus::Rejected
        );
    }

    #[test]
    fn quorum_per_chamber() {
        let low_e = tally((9, 0, 0, 1000), (100, 0, 0, 100));
        assert_eq!(decide(Category::Ordinary, &low_e), ProposalStatus::NoQuorum);
        let empty = tally((0, 0, 0, 0), (10, 0, 0, 10));
        assert_eq!(decide(Category::Ordinary, &empty), ProposalStatus::NoQuorum);
    }

    #[test]
    fn constitutional_needs_two_thirds() {
        let t = tally((65, 35, 0, 500), (65, 35, 0, 500));
        assert_eq!(
            decide(Category::Constitutional, &t),
            ProposalStatus::Rejected
        );
        let t = tally((67, 33, 0, 500), (2, 1, 0, 10));
        assert_eq!(
            decide(Category::Constitutional, &t),
            ProposalStatus::Approved
        );
    }

    #[test]
    fn exact_half_is_not_majority() {
        let t = tally((50, 50, 0, 100), (50, 50, 0, 100));
        assert_eq!(decide(Category::Ordinary, &t), ProposalStatus::Rejected);
    }

    #[test]
    fn concentration_brake() {
        let mut t = tally((60, 40, 0, 1000), (60, 40, 0, 1000));
        assert_eq!(decide(Category::Ordinary, &t), ProposalStatus::Approved);
        // Um votante detém 50% da câmara econômica: contribuição exige 2/3.
        t.economic.largest_voter = 50;
        assert_eq!(decide(Category::Ordinary, &t), ProposalStatus::Rejected);
        t.contribution.yes = 70;
        t.contribution.no = 30;
        assert_eq!(decide(Category::Ordinary, &t), ProposalStatus::Approved);
    }

    #[test]
    fn shape_rules() {
        let cur = ProtocolParams {
            min_fee: 1,
            max_block_txs: 10,
            governance: g(),
            consensus: crate::consensus::ConsensusParams::fast(200),
        };
        assert!(check_proposal_shape(Category::Ordinary, &[ParamChange::MinFee(5)], &cur).is_ok());
        // Parâmetro constitucional com categoria ordinária: inválido.
        assert!(
            check_proposal_shape(Category::Ordinary, &[ParamChange::GovDeposit(5)], &cur).is_err()
        );
        // Categoria mais alta que a necessária também é rejeitada.
        assert!(
            check_proposal_shape(Category::Constitutional, &[ParamChange::MinFee(5)], &cur)
                .is_err()
        );
        assert!(
            check_proposal_shape(Category::Community, &[ParamChange::MinFee(5)], &cur).is_err()
        );
        assert!(check_proposal_shape(Category::Community, &[], &cur).is_ok());
        assert!(check_proposal_shape(
            Category::Ordinary,
            &[ParamChange::MinFee(5), ParamChange::MinFee(6)],
            &cur
        )
        .is_err());
        assert!(
            check_proposal_shape(Category::Ordinary, &[ParamChange::MaxBlockTxs(0)], &cur).is_err()
        );
        // Parâmetros de consenso: constitucionais e validados após aplicados.
        let slash =
            |v| check_proposal_shape(Category::Constitutional, &[ParamChange::SlashBps(v)], &cur);
        assert!(slash(1_000).is_ok());
        assert!(slash(10_001).is_err());
        assert!(slash(u64::MAX).is_err());
        assert!(
            check_proposal_shape(Category::Ordinary, &[ParamChange::MinBond(5)], &cur).is_err()
        );
        assert!(check_proposal_shape(
            Category::Constitutional,
            &[ParamChange::TimeoutPropose(0)],
            &cur
        )
        .is_err());
        // Temporizadores absurdos travariam (ou derrubariam) os Nodes.
        for bad in [
            ParamChange::TimeoutPropose(u64::MAX),
            ParamChange::BlockInterval(crate::consensus::MAX_TIME_PARAM_MS + 1),
            ParamChange::TimeoutDelta(u64::MAX),
        ] {
            assert!(check_proposal_shape(Category::Constitutional, &[bad], &cur).is_err());
        }
    }

    #[test]
    fn consensus_params_roundtrip() {
        for name in [
            "epoch_blocks",
            "max_validators",
            "min_bond",
            "unbonding_blocks",
            "slash_bps",
            "block_interval_ms",
            "timeout_propose_ms",
            "timeout_prevote_ms",
            "timeout_precommit_ms",
            "timeout_delta_ms",
        ] {
            let p = ParamChange::parse(name, "7").unwrap();
            assert_eq!(p.name(), name);
            assert_eq!(p.value(), 7);
            assert_eq!(p.category(), Category::Constitutional);
            assert_eq!(
                ParamChange::from_canonical_bytes(&p.to_canonical_bytes()).unwrap(),
                p
            );
        }
    }

    #[test]
    fn param_parse_roundtrip() {
        let p = ParamChange::parse("min_fee", "42").unwrap();
        assert_eq!(p, ParamChange::MinFee(42));
        assert_eq!(
            ParamChange::from_canonical_bytes(&p.to_canonical_bytes()).unwrap(),
            p
        );
        assert!(ParamChange::parse("saldo_de_fulano", "1").is_none());
    }
}
