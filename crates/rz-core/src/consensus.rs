//! Tipos do consenso Zero-BFT (ADR-0012, `spec/CONSENSUS.md`).
//!
//! * [`ValidatorSet`] — validadores ativos com poder de voto igual ao ZERO
//!   bloqueado (neutro a Sybil: dividir o bloqueio não aumenta o poder).
//! * [`Vote`] — pré-voto ou pré-compromisso assinado.
//! * [`Proposal`] — proposta de bloco assinada pelo proponente da rodada.
//! * [`Commit`] — certificado: pré-compromissos de mais de 2/3 do poder.

use std::collections::BTreeSet;
use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Hash32, PublicKey, SecretKey, Signature};

use crate::block::{Block, BlockId};

pub mod context {
    pub const VOTE: &str = "rede-zero/vote/v1";
    pub const PROPOSAL: &str = "rede-zero/proposal/v1";
    pub const PROPOSER: &str = "rede-zero/proposer/v1";
}

/// Máximo de validadores ativos decodificáveis.
pub const MAX_VALIDATORS_DECODE: usize = 1_000;

/// Limite de intervalos e temporizadores (10 minutos). Impede que um
/// parâmetro aprovado por governança torne os temporizadores dos Nodes
/// inutilizáveis (ou provoque estouro ao somar ao relógio).
pub const MAX_TIME_PARAM_MS: u64 = 600_000;

// ------------------------------------------------------------- parâmetros

/// Parâmetros do consenso (Genesis; ajustáveis por governança constitucional).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsensusParams {
    /// Intervalo mínimo entre blocos (espera após um commit).
    pub block_interval_ms: u64,
    /// O conjunto de validadores é recalculado a cada `epoch_blocks`.
    pub epoch_blocks: u64,
    pub max_validators: u32,
    /// Bloqueio mínimo para ser candidato a validador.
    pub min_bond: u64,
    /// Período em que ZERO desbloqueado ainda pode ser punido.
    pub unbonding_blocks: u64,
    /// Fração do bloqueio queimada por dupla assinatura, em pontos-base.
    pub slash_bps: u32,
    pub timeout_propose_ms: u64,
    pub timeout_prevote_ms: u64,
    pub timeout_precommit_ms: u64,
    /// Acréscimo a cada rodada sem decisão.
    pub timeout_delta_ms: u64,
}

impl ConsensusParams {
    pub fn devnet(block_interval_ms: u64) -> Self {
        Self {
            block_interval_ms,
            epoch_blocks: 100,
            max_validators: 100,
            min_bond: 1_000 * crate::UNITS_PER_ZERO,
            unbonding_blocks: (21 * 86_400_000 / block_interval_ms.max(1)).max(1),
            slash_bps: 500,
            timeout_propose_ms: 1_500,
            timeout_prevote_ms: 500,
            timeout_precommit_ms: 500,
            timeout_delta_ms: 500,
        }
    }

    /// Parâmetros curtos para testes e redes locais rápidas.
    pub fn fast(block_interval_ms: u64) -> Self {
        Self {
            block_interval_ms,
            epoch_blocks: 10,
            max_validators: 100,
            min_bond: 1,
            unbonding_blocks: 20,
            slash_bps: 500,
            timeout_propose_ms: 600,
            timeout_prevote_ms: 300,
            timeout_precommit_ms: 300,
            timeout_delta_ms: 200,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.epoch_blocks == 0 {
            return Err("epoch_blocks deve ser positivo");
        }
        if self.max_validators == 0 || self.max_validators as usize > MAX_VALIDATORS_DECODE {
            return Err("max_validators fora dos limites");
        }
        if self.slash_bps > 10_000 {
            return Err("slash_bps acima de 100%");
        }
        if self.unbonding_blocks == 0 {
            return Err("unbonding_blocks deve ser positivo");
        }
        let times = [
            self.block_interval_ms,
            self.timeout_propose_ms,
            self.timeout_prevote_ms,
            self.timeout_precommit_ms,
        ];
        if times.iter().any(|t| *t == 0 || *t > MAX_TIME_PARAM_MS) {
            return Err("intervalos e temporizadores fora dos limites");
        }
        if self.timeout_delta_ms > MAX_TIME_PARAM_MS {
            return Err("timeout_delta_ms fora dos limites");
        }
        Ok(())
    }
}

impl Encode for ConsensusParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.block_interval_ms)
            .u64(self.epoch_blocks)
            .u32(self.max_validators)
            .u64(self.min_bond)
            .u64(self.unbonding_blocks)
            .u32(self.slash_bps)
            .u64(self.timeout_propose_ms)
            .u64(self.timeout_prevote_ms)
            .u64(self.timeout_precommit_ms)
            .u64(self.timeout_delta_ms);
    }
}

impl Decode for ConsensusParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            block_interval_ms: d.u64()?,
            epoch_blocks: d.u64()?,
            max_validators: d.u32()?,
            min_bond: d.u64()?,
            unbonding_blocks: d.u64()?,
            slash_bps: d.u32()?,
            timeout_propose_ms: d.u64()?,
            timeout_prevote_ms: d.u64()?,
            timeout_precommit_ms: d.u64()?,
            timeout_delta_ms: d.u64()?,
        })
    }
}

// ------------------------------------------------------------- validadores

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validator {
    pub key: PublicKey,
    pub power: u64,
}

impl Encode for Validator {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.key).u64(self.power);
    }
}

impl Decode for Validator {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            key: d.get()?,
            power: d.u64()?,
        })
    }
}

/// Conjunto de validadores ativos, ordenado por chave.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ValidatorSet {
    validators: Vec<Validator>,
    total: u128,
}

impl ValidatorSet {
    /// Constrói o conjunto (ordena por chave e soma o poder). Entradas com
    /// poder zero ou chave repetida são descartadas.
    pub fn new(mut validators: Vec<Validator>) -> Self {
        validators.retain(|v| v.power > 0);
        validators.sort_by_key(|v| v.key);
        validators.dedup_by_key(|v| v.key);
        let total = validators.iter().map(|v| v.power as u128).sum();
        Self { validators, total }
    }

    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }

    pub fn len(&self) -> usize {
        self.validators.len()
    }

    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    pub fn total_power(&self) -> u128 {
        self.total
    }

    pub fn power_of(&self, key: &PublicKey) -> u64 {
        self.validators
            .binary_search_by_key(key, |v| v.key)
            .map(|i| self.validators[i].power)
            .unwrap_or(0)
    }

    pub fn contains(&self, key: &PublicKey) -> bool {
        self.power_of(key) > 0
    }

    /// Mais de 2/3 do poder total.
    pub fn is_supermajority(&self, power: u128) -> bool {
        power * 3 > self.total * 2
    }

    /// Mais de 1/3 do poder total (pelo menos um validador honesto).
    pub fn exceeds_one_third(&self, power: u128) -> bool {
        power * 3 > self.total
    }

    /// Proponente da rodada: escolha ponderada pelo poder, determinística e
    /// imprevisível antes de o bloco anterior existir:
    /// `H(PROPOSER, bloco_anterior ‖ altura ‖ rodada) mod poder_total`.
    pub fn proposer(&self, prev: &BlockId, height: u64, round: u32) -> Option<&PublicKey> {
        if self.total == 0 {
            return None;
        }
        let mut e = Encoder::new();
        e.put(prev).u64(height).u32(round);
        let h = hash(context::PROPOSER, &e.into_bytes());
        let mut x = [0u8; 16];
        x.copy_from_slice(&h.0[..16]);
        let mut target = u128::from_be_bytes(x) % self.total;
        for v in &self.validators {
            if target < v.power as u128 {
                return Some(&v.key);
            }
            target -= v.power as u128;
        }
        None
    }
}

impl Encode for ValidatorSet {
    fn encode(&self, e: &mut Encoder) {
        e.list(&self.validators);
    }
}

impl Decode for ValidatorSet {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self::new(d.list(MAX_VALIDATORS_DECODE)?))
    }
}

// ------------------------------------------------------------- votos

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VoteType {
    Prevote = 1,
    Precommit = 2,
}

impl Encode for VoteType {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for VoteType {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            1 => Ok(VoteType::Prevote),
            2 => Ok(VoteType::Precommit),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Voto assinado. `block = None` é o voto nulo ("nil").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    pub kind: VoteType,
    pub height: u64,
    pub round: u32,
    pub block: Option<BlockId>,
    pub validator: PublicKey,
    pub signature: Signature,
}

fn vote_payload(kind: VoteType, height: u64, round: u32, block: &Option<BlockId>) -> Vec<u8> {
    let mut e = Encoder::new();
    e.put(&kind).u64(height).u32(round).option(block);
    e.into_bytes()
}

impl Vote {
    pub fn sign(
        kind: VoteType,
        height: u64,
        round: u32,
        block: Option<BlockId>,
        key: &SecretKey,
        network_id: &str,
    ) -> Self {
        let signature = key.sign(
            context::VOTE,
            network_id,
            &vote_payload(kind, height, round, &block),
        );
        Self {
            kind,
            height,
            round,
            block,
            validator: key.public_key(),
            signature,
        }
    }

    pub fn verify(&self, network_id: &str) -> bool {
        self.validator
            .verify(
                context::VOTE,
                network_id,
                &vote_payload(self.kind, self.height, self.round, &self.block),
                &self.signature,
            )
            .is_ok()
    }

    /// Dois votos do mesmo validador, mesma altura, rodada e tipo, para
    /// blocos diferentes: dupla assinatura (evidência objetiva, `SPEC §49`).
    pub fn conflicts_with(&self, other: &Vote) -> bool {
        self.validator == other.validator
            && self.kind == other.kind
            && self.height == other.height
            && self.round == other.round
            && self.block != other.block
    }

    pub fn id(&self) -> Hash32 {
        hash(context::VOTE, &self.to_canonical_bytes())
    }
}

impl Encode for Vote {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.kind)
            .u64(self.height)
            .u32(self.round)
            .option(&self.block)
            .put(&self.validator)
            .put(&self.signature);
    }
}

impl Decode for Vote {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            kind: d.get()?,
            height: d.u64()?,
            round: d.u32()?,
            block: d.option()?,
            validator: d.get()?,
            signature: d.get()?,
        })
    }
}

// ------------------------------------------------------------- propostas

/// Proposta de bloco. `pol_round` é a rodada em que o bloco obteve mais de
/// 2/3 de pré-votos, quando reproposto (regra de travamento do Tendermint).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub height: u64,
    pub round: u32,
    pub pol_round: Option<u32>,
    pub block: Block,
    pub proposer: PublicKey,
    pub signature: Signature,
}

fn proposal_payload(height: u64, round: u32, pol_round: Option<u32>, block: &BlockId) -> Vec<u8> {
    let mut e = Encoder::new();
    e.u64(height).u32(round).option(&pol_round).put(block);
    e.into_bytes()
}

impl Proposal {
    pub fn sign(
        height: u64,
        round: u32,
        pol_round: Option<u32>,
        block: Block,
        key: &SecretKey,
        network_id: &str,
    ) -> Self {
        let signature = key.sign(
            context::PROPOSAL,
            network_id,
            &proposal_payload(height, round, pol_round, &block.id()),
        );
        Self {
            height,
            round,
            pol_round,
            block,
            proposer: key.public_key(),
            signature,
        }
    }

    pub fn verify(&self, network_id: &str) -> bool {
        self.proposer
            .verify(
                context::PROPOSAL,
                network_id,
                &proposal_payload(self.height, self.round, self.pol_round, &self.block.id()),
                &self.signature,
            )
            .is_ok()
    }

    pub fn id(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.height)
            .u32(self.round)
            .put(&self.block.id())
            .put(&self.proposer);
        hash(context::PROPOSAL, &e.into_bytes())
    }
}

impl Encode for Proposal {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.height)
            .u32(self.round)
            .option(&self.pol_round)
            .put(&self.block)
            .put(&self.proposer)
            .put(&self.signature);
    }
}

impl Decode for Proposal {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            height: d.u64()?,
            round: d.u32()?,
            pol_round: d.option()?,
            block: d.get()?,
            proposer: d.get()?,
            signature: d.get()?,
        })
    }
}

/// Conteúdo assinado de uma proposta, sem o bloco: evidência compacta de
/// equivocação do proponente (`ReportDoubleProposal`, `docs/AUDIT.md`
/// RZ-IR-07).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedProposal {
    pub height: u64,
    pub round: u32,
    pub pol_round: Option<u32>,
    pub block: BlockId,
    pub proposer: PublicKey,
    pub signature: Signature,
}

impl SignedProposal {
    pub fn verify(&self, network_id: &str) -> bool {
        self.proposer
            .verify(
                context::PROPOSAL,
                network_id,
                &proposal_payload(self.height, self.round, self.pol_round, &self.block),
                &self.signature,
            )
            .is_ok()
    }

    /// Mesmo proponente, altura e rodada, com conteúdo diferente: o
    /// proponente de uma rodada assina exatamente uma proposta.
    pub fn conflicts_with(&self, other: &SignedProposal) -> bool {
        self.proposer == other.proposer
            && self.height == other.height
            && self.round == other.round
            && (self.block != other.block || self.pol_round != other.pol_round)
    }
}

impl Proposal {
    /// Conteúdo assinado (sem o bloco), para evidência.
    pub fn signed(&self) -> SignedProposal {
        SignedProposal {
            height: self.height,
            round: self.round,
            pol_round: self.pol_round,
            block: self.block.id(),
            proposer: self.proposer,
            signature: self.signature,
        }
    }
}

impl Encode for SignedProposal {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.height)
            .u32(self.round)
            .option(&self.pol_round)
            .put(&self.block)
            .put(&self.proposer)
            .put(&self.signature);
    }
}

impl Decode for SignedProposal {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            height: d.u64()?,
            round: d.u32()?,
            pol_round: d.option()?,
            block: d.get()?,
            proposer: d.get()?,
            signature: d.get()?,
        })
    }
}

// ------------------------------------------------------------- commit

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitSig {
    pub validator: PublicKey,
    pub signature: Signature,
}

impl Encode for CommitSig {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.validator).put(&self.signature);
    }
}

impl Decode for CommitSig {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            validator: d.get()?,
            signature: d.get()?,
        })
    }
}

/// Certificado de finalidade de um bloco: pré-compromissos de mais de 2/3
/// do poder, na mesma rodada, para o mesmo bloco. Verificável por qualquer
/// Node que conheça o conjunto de validadores (AC-CON-001).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub height: u64,
    pub round: u32,
    pub block: BlockId,
    pub signatures: Vec<CommitSig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitError {
    WrongBlock,
    UnknownValidator,
    DuplicateValidator,
    BadSignature,
    InsufficientPower,
}

impl fmt::Display for CommitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongBlock => write!(f, "certificado de outro bloco ou altura"),
            Self::UnknownValidator => write!(f, "assinatura de quem não é validador"),
            Self::DuplicateValidator => write!(f, "validador repetido"),
            Self::BadSignature => write!(f, "assinatura inválida"),
            Self::InsufficientPower => write!(f, "menos de 2/3 do poder"),
        }
    }
}

impl std::error::Error for CommitError {}

impl Commit {
    /// Monta um certificado a partir de pré-compromissos já verificados.
    pub fn from_votes<'a>(votes: impl IntoIterator<Item = &'a Vote>) -> Option<Self> {
        let mut it = votes.into_iter().peekable();
        let first = it.peek()?;
        let (height, round, block) = (first.height, first.round, first.block?);
        let signatures = it
            .filter(|v| {
                v.kind == VoteType::Precommit
                    && v.height == height
                    && v.round == round
                    && v.block == Some(block)
            })
            .map(|v| CommitSig {
                validator: v.validator,
                signature: v.signature,
            })
            .collect();
        Some(Self {
            height,
            round,
            block,
            signatures,
        })
    }

    pub fn verify(
        &self,
        height: u64,
        block: &BlockId,
        set: &ValidatorSet,
        network_id: &str,
    ) -> Result<(), CommitError> {
        if self.height != height || &self.block != block {
            return Err(CommitError::WrongBlock);
        }
        let payload = vote_payload(VoteType::Precommit, height, self.round, &Some(*block));
        let mut seen = BTreeSet::new();
        let mut power: u128 = 0;
        for s in &self.signatures {
            let p = set.power_of(&s.validator);
            if p == 0 {
                return Err(CommitError::UnknownValidator);
            }
            if !seen.insert(s.validator) {
                return Err(CommitError::DuplicateValidator);
            }
            s.validator
                .verify(context::VOTE, network_id, &payload, &s.signature)
                .map_err(|_| CommitError::BadSignature)?;
            power += p as u128;
        }
        if !set.is_supermajority(power) {
            return Err(CommitError::InsufficientPower);
        }
        Ok(())
    }
}

impl Encode for Commit {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.height)
            .u32(self.round)
            .put(&self.block)
            .list(&self.signatures);
    }
}

impl Decode for Commit {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            height: d.u64()?,
            round: d.u32()?,
            block: d.get()?,
            signatures: d.list(MAX_VALIDATORS_DECODE)?,
        })
    }
}

/// Bloco acompanhado do seu certificado de finalidade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedBlock {
    pub block: Block,
    pub commit: Commit,
}

impl Encode for CommittedBlock {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.block).put(&self.commit);
    }
}

impl Decode for CommittedBlock {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            block: d.get()?,
            commit: d.get()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NET: &str = "rede-zero-devnet-test";

    fn keys(n: u8) -> Vec<SecretKey> {
        (1..=n).map(|i| SecretKey::from_seed([i; 32])).collect()
    }

    fn set(ks: &[SecretKey], powers: &[u64]) -> ValidatorSet {
        ValidatorSet::new(
            ks.iter()
                .zip(powers)
                .map(|(k, p)| Validator {
                    key: k.public_key(),
                    power: *p,
                })
                .collect(),
        )
    }

    #[test]
    fn thresholds() {
        let ks = keys(4);
        let s = set(&ks, &[1, 1, 1, 1]);
        assert!(!s.is_supermajority(2));
        assert!(s.is_supermajority(3));
        assert!(!s.exceeds_one_third(1));
        assert!(s.exceeds_one_third(2));
    }

    #[test]
    fn proposer_deterministic_and_weighted() {
        let ks = keys(3);
        let s = set(&ks, &[1, 1, 98]);
        let prev = BlockId(Hash32([7; 32]));
        assert_eq!(s.proposer(&prev, 5, 0), s.proposer(&prev, 5, 0));
        let heavy = ks[2].public_key();
        let hits = (0..1000u32)
            .filter(|r| s.proposer(&prev, 1, *r) == Some(&heavy))
            .count();
        assert!(hits > 930, "{hits}");
    }

    #[test]
    fn commit_verification() {
        let ks = keys(4);
        let s = set(&ks, &[10, 10, 10, 10]);
        let b = BlockId(Hash32([3; 32]));
        let votes: Vec<Vote> = ks[..3]
            .iter()
            .map(|k| Vote::sign(VoteType::Precommit, 9, 1, Some(b), k, NET))
            .collect();
        let c = Commit::from_votes(&votes).unwrap();
        assert!(c.verify(9, &b, &s, NET).is_ok());

        // 2 de 4 não bastam.
        let c2 = Commit::from_votes(&votes[..2]).unwrap();
        assert_eq!(
            c2.verify(9, &b, &s, NET),
            Err(CommitError::InsufficientPower)
        );
        // Outro bloco ou altura.
        assert_eq!(
            c.verify(9, &BlockId(Hash32([4; 32])), &s, NET),
            Err(CommitError::WrongBlock)
        );
        // Assinatura repetida para inflar poder.
        let mut dup = c2.clone();
        dup.signatures.push(dup.signatures[0].clone());
        assert_eq!(
            dup.verify(9, &b, &s, NET),
            Err(CommitError::DuplicateValidator)
        );
        // Rede diferente.
        assert_eq!(
            c.verify(9, &b, &s, "outra-devnet"),
            Err(CommitError::BadSignature)
        );
        // Assinante fora do conjunto.
        let intruso = SecretKey::from_seed([99; 32]);
        let mut bad = c.clone();
        bad.signatures.push(CommitSig {
            validator: intruso.public_key(),
            signature: Vote::sign(VoteType::Precommit, 9, 1, Some(b), &intruso, NET).signature,
        });
        assert_eq!(
            bad.verify(9, &b, &s, NET),
            Err(CommitError::UnknownValidator)
        );
    }

    #[test]
    fn vote_signature_and_conflict() {
        let k = SecretKey::from_seed([1; 32]);
        let a = Vote::sign(
            VoteType::Prevote,
            1,
            0,
            Some(BlockId(Hash32([1; 32]))),
            &k,
            NET,
        );
        let b = Vote::sign(VoteType::Prevote, 1, 0, None, &k, NET);
        assert!(a.verify(NET) && b.verify(NET));
        assert!(a.conflicts_with(&b));
        let c = Vote::sign(VoteType::Precommit, 1, 0, None, &k, NET);
        assert!(!a.conflicts_with(&c), "tipos diferentes não conflitam");
        let mut t = a.clone();
        t.round = 1;
        assert!(!t.verify(NET));
    }

    #[test]
    fn codec_roundtrip() {
        let k = SecretKey::from_seed([1; 32]);
        let v = Vote::sign(VoteType::Precommit, 3, 2, None, &k, NET);
        assert_eq!(
            Vote::from_canonical_bytes(&v.to_canonical_bytes()).unwrap(),
            v
        );
    }
}
