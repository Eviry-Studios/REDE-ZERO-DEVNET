//! Estado da rede e função de transição (`SPEC §12–§14`, `§33–§36`,
//! `spec/STATE.md`, `spec/PRIVACY.md`).
//!
//! `S(n+1) = F(S(n), B(n+1))`. A única forma de alterar saldos é aplicar
//! transações válidas ou creditar taxas segundo a regra do bloco. Não existe
//! operação pública que permita `saldo += X` arbitrário (INV-003).
//!
//! O estado tem duas partes:
//!
//! * **transparente** — contas `endereço → (saldo, nonce)`;
//! * **privada** — lista de notas (saídas com valor oculto) e conjunto de
//!   imagens de chave já gastas, mais a **oferta privada** total, pública e
//!   rastreada por entradas e saídas públicas (defesa contra inflação oculta).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rz_codec::{Encode, Encoder};
use rz_crypto::{context, hash, Address, Hash32};
use rz_privacy::clsag::{self, RingMember};
use rz_privacy::note::OutputData;
use rz_privacy::{balance_holds, commitment, decode_point, excess};

use crate::consensus::{Validator, ValidatorSet};
use crate::genesis::{Genesis, GenesisError};
use crate::governance::{
    check_proposal_shape, decide, lock_weight, Category, ChamberTally, Contribution, Lock,
    Proposal, ProposalStatus, ProtocolParams, Tally, Vote, SCALE,
};
use crate::private::{
    accumulate, context as pctx, min_ring_size, shield_message, PrivateTx, ShieldedOutput,
    PRIVATE_TX_VERSION, RING_SIZE,
};
use crate::tx::{AccountTx, Transaction, TxError, TxKind};

/// Conta: saldo em unidades mínimas e contador de transações.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub balance: u64,
    pub nonce: u64,
}

/// Contexto de execução: rede e altura do bloco em que a transação entra.
///
/// Parâmetros ajustáveis (taxa mínima, limites, governança) vêm do **estado**
/// ([`State::params`]), pois podem mudar por governança.
#[derive(Clone, Debug)]
pub struct ExecParams<'a> {
    pub network_id: &'a str,
    pub height: u64,
}

impl<'a> ExecParams<'a> {
    /// Contexto na altura 1 (primeiro bloco após o Genesis).
    pub fn from_genesis(g: &'a Genesis) -> Self {
        Self::at(g, 1)
    }

    pub fn at(g: &'a Genesis, height: u64) -> Self {
        Self {
            network_id: &g.network_id,
            height,
        }
    }
}

/// Contexto do hash da parte de governança do estado.
pub const GOVERNANCE_ROOT: &str = "rede-zero/governance-root/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateError {
    Genesis(GenesisError),
    /// A soma dos saldos divergiu da oferta total (violação de INV-003).
    SupplyMismatch {
        expected: u64,
        actual: u128,
    },
    Overflow,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Genesis(e) => write!(f, "genesis inválido: {e}"),
            Self::SupplyMismatch { expected, actual } => {
                write!(
                    f,
                    "oferta divergente: esperado {expected}, encontrado {actual}"
                )
            }
            Self::Overflow => write!(f, "overflow aritmético"),
        }
    }
}

impl std::error::Error for StateError {}

/// Estado completo. Estruturas ordenadas garantem determinismo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    accounts: BTreeMap<Address, Account>,
    total_supply: u64,
    shielded_supply: u64,
    outputs: Vec<OutputData>,
    output_acc: Hash32,
    key_images: BTreeSet<[u8; 32]>,
    key_image_log: Vec<[u8; 32]>,
    key_image_acc: Hash32,
    params: ProtocolParams,
    locks: BTreeMap<u64, Lock>,
    next_lock_id: u64,
    proposals: BTreeMap<Hash32, Proposal>,
    contributions: BTreeMap<rz_crypto::PublicKey, Contribution>,
    approved_communities: BTreeSet<Hash32>,
    // Staking e consenso (ADR-0012).
    bonds: BTreeMap<rz_crypto::PublicKey, u64>,
    unbonding: Vec<Unbonding>,
    jailed: BTreeSet<rz_crypto::PublicKey>,
    /// Evidências já processadas (evita punir duas vezes a mesma infração).
    punished: BTreeSet<Hash32>,
    /// Conjunto que valida o **próximo** bloco.
    validators: ValidatorSet,
    /// Conjunto que validou o bloco mais recente.
    last_validators: ValidatorSet,
}

/// ZERO em período de desvinculação.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unbonding {
    pub owner: Address,
    pub amount: u64,
    pub release_height: u64,
}

/// Alteração de governança produzida por uma transação.
enum GovEffect {
    Lock(u64, Lock),
    Unlock(u64),
    Propose(Box<Proposal>),
    Vote(Hash32, Address, Vote),
    /// Punição de validador por evidência verificável.
    Slash(rz_crypto::PublicKey, Hash32),
    Bond(rz_crypto::PublicKey, u64),
    Unbond(rz_crypto::PublicKey, u64),
}

/// Efeito calculado de uma transação, aplicado só após todas as verificações.
struct Effect {
    accounts: Vec<(Address, Account)>,
    new_outputs: Vec<OutputData>,
    spent: Vec<[u8; 32]>,
    shielded_supply: u64,
    fee: u64,
    gov: Option<GovEffect>,
}

impl State {
    /// Estado inicial definido pelo Genesis.
    pub fn from_genesis(genesis: &Genesis) -> Result<Self, StateError> {
        genesis.validate().map_err(StateError::Genesis)?;
        let accounts = genesis
            .allocations
            .iter()
            .map(|a| {
                (
                    a.address,
                    Account {
                        balance: a.amount,
                        nonce: 0,
                    },
                )
            })
            .collect();
        let total_supply = genesis.total_supply().map_err(StateError::Genesis)?;
        let initial = ValidatorSet::new(
            genesis
                .validators
                .iter()
                .map(|v| Validator {
                    key: v.key,
                    power: v.stake,
                })
                .collect(),
        );
        Ok(Self {
            accounts,
            total_supply,
            shielded_supply: 0,
            outputs: Vec::new(),
            output_acc: Hash32::ZERO,
            key_images: BTreeSet::new(),
            key_image_log: Vec::new(),
            key_image_acc: Hash32::ZERO,
            params: ProtocolParams {
                min_fee: genesis.min_fee,
                max_block_txs: genesis.max_block_txs,
                governance: genesis.governance.clone(),
                consensus: genesis.consensus.clone(),
            },
            locks: BTreeMap::new(),
            next_lock_id: 0,
            proposals: BTreeMap::new(),
            contributions: BTreeMap::new(),
            approved_communities: BTreeSet::new(),
            bonds: genesis
                .validators
                .iter()
                .map(|v| (v.key, v.stake))
                .collect(),
            unbonding: Vec::new(),
            jailed: BTreeSet::new(),
            punished: BTreeSet::new(),
            validators: initial.clone(),
            last_validators: initial,
        })
    }

    /// Conjunto de validadores do próximo bloco.
    pub fn validators(&self) -> &ValidatorSet {
        &self.validators
    }

    /// Conjunto que validou o bloco mais recente (verifica o seu commit).
    pub fn last_validators(&self) -> &ValidatorSet {
        &self.last_validators
    }

    pub fn bond_of(&self, key: &rz_crypto::PublicKey) -> u64 {
        self.bonds.get(key).copied().unwrap_or(0)
    }

    pub fn unbonding(&self) -> &[Unbonding] {
        &self.unbonding
    }

    pub fn is_jailed(&self, key: &rz_crypto::PublicKey) -> bool {
        self.jailed.contains(key)
    }

    fn staked_total(&self) -> u128 {
        self.bonds.values().map(|b| *b as u128).sum::<u128>()
            + self
                .unbonding
                .iter()
                .map(|u| u.amount as u128)
                .sum::<u128>()
    }

    /// Candidatos com bloqueio mínimo e não excluídos, maiores primeiro.
    fn select_validators(&self) -> ValidatorSet {
        let c = &self.params.consensus;
        let mut cands: Vec<(&rz_crypto::PublicKey, &u64)> = self
            .bonds
            .iter()
            .filter(|(k, b)| **b >= c.min_bond.max(1) && !self.jailed.contains(*k))
            .collect();
        cands.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        ValidatorSet::new(
            cands
                .into_iter()
                .take(c.max_validators as usize)
                .map(|(k, b)| Validator { key: *k, power: *b })
                .collect(),
        )
    }

    /// Parâmetros vigentes (iniciados pelo Genesis, alterados por governança).
    pub fn params(&self) -> &ProtocolParams {
        &self.params
    }

    pub fn locks(&self) -> impl Iterator<Item = (&u64, &Lock)> {
        self.locks.iter()
    }

    pub fn proposals(&self) -> impl Iterator<Item = &Proposal> {
        self.proposals.values()
    }

    pub fn proposal(&self, id: &Hash32) -> Option<&Proposal> {
        self.proposals.get(id)
    }

    /// Pontos de contribuição (escala [`SCALE`]) de uma identidade na altura.
    pub fn contribution(&self, key: &rz_crypto::PublicKey, height: u64) -> u64 {
        self.contributions.get(key).map_or(0, |c| {
            c.at(height, self.params.governance.contribution_half_life_blocks)
        })
    }

    /// Comunidades aprovadas por governança (hash do conteúdo publicado).
    pub fn approved_communities(&self) -> impl Iterator<Item = &Hash32> {
        self.approved_communities.iter()
    }

    fn locked_total(&self) -> u128 {
        self.locks.values().map(|l| l.amount as u128).sum()
    }

    fn deposits_held(&self) -> u128 {
        self.proposals
            .values()
            .filter(|p| p.status == ProposalStatus::Pending)
            .map(|p| p.deposit as u128)
            .sum()
    }

    fn governance_root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.put(&self.params).u64(self.next_lock_id);
        e.u64(self.locks.len() as u64);
        for (id, l) in &self.locks {
            e.u64(*id).put(l);
        }
        e.u64(self.proposals.len() as u64);
        for p in self.proposals.values() {
            e.put(p);
        }
        e.u64(self.contributions.len() as u64);
        for (k, c) in &self.contributions {
            e.put(k).put(c);
        }
        e.u64(self.approved_communities.len() as u64);
        for c in &self.approved_communities {
            e.put(c);
        }
        // Staking e conjunto de validadores.
        e.u64(self.bonds.len() as u64);
        for (k, b) in &self.bonds {
            e.put(k).u64(*b);
        }
        e.u64(self.unbonding.len() as u64);
        for u in &self.unbonding {
            e.put(&u.owner).u64(u.amount).u64(u.release_height);
        }
        e.u64(self.jailed.len() as u64);
        for k in &self.jailed {
            e.put(k);
        }
        e.u64(self.punished.len() as u64);
        for h in &self.punished {
            e.put(h);
        }
        e.put(&self.validators).put(&self.last_validators);
        hash(GOVERNANCE_ROOT, &e.into_bytes())
    }

    pub fn account(&self, address: &Address) -> Account {
        self.accounts.get(address).copied().unwrap_or_default()
    }

    pub fn total_supply(&self) -> u64 {
        self.total_supply
    }

    /// Total de ZERO em notas privadas (público, sem revelar notas).
    pub fn shielded_supply(&self) -> u64 {
        self.shielded_supply
    }

    pub fn accounts(&self) -> impl Iterator<Item = (&Address, &Account)> {
        self.accounts.iter()
    }

    /// Todas as notas, por índice global.
    pub fn outputs(&self) -> &[OutputData] {
        &self.outputs
    }

    /// Imagens de chave gastas, na ordem em que foram registradas.
    pub fn key_image_log(&self) -> &[[u8; 32]] {
        &self.key_image_log
    }

    pub fn is_spent(&self, key_image: &[u8; 32]) -> bool {
        self.key_images.contains(key_image)
    }

    /// Compromisso criptográfico do estado (`spec/STATE.md §3`).
    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.total_supply).u64(self.shielded_supply);
        e.u32(u32::try_from(self.accounts.len()).unwrap_or(u32::MAX));
        for (addr, acc) in &self.accounts {
            e.put(addr).u64(acc.balance).u64(acc.nonce);
        }
        e.u64(self.outputs.len() as u64)
            .put(&self.output_acc)
            .u64(self.key_image_log.len() as u64)
            .put(&self.key_image_acc)
            .put(&self.governance_root());
        hash(context::STATE_ROOT, &e.into_bytes())
    }

    /// Invariante monetária:
    /// `Σ saldos + oferta_privada + bloqueado + depósitos_retidos = oferta_total`.
    pub fn check_supply(&self) -> Result<(), StateError> {
        let actual: u128 = self
            .accounts
            .values()
            .map(|a| a.balance as u128)
            .sum::<u128>()
            + self.shielded_supply as u128
            + self.locked_total()
            + self.deposits_held()
            + self.staked_total();
        if actual != self.total_supply as u128 {
            return Err(StateError::SupplyMismatch {
                expected: self.total_supply,
                actual,
            });
        }
        Ok(())
    }

    /// Valida uma transação contra este estado **sem alterá-lo**.
    pub fn check_transaction(&self, tx: &Transaction, p: &ExecParams<'_>) -> Result<(), TxError> {
        self.plan(tx, p).map(|_| ())
    }

    /// Aplica uma transação válida e retorna a taxa a ser distribuída.
    ///
    /// A operação é atômica: em caso de erro, o estado não é modificado.
    pub fn apply_transaction(
        &mut self,
        tx: &Transaction,
        p: &ExecParams<'_>,
    ) -> Result<u64, TxError> {
        let effect = self.plan(tx, p)?;
        for (addr, acc) in effect.accounts {
            self.accounts.insert(addr, acc);
        }
        for out in effect.new_outputs {
            self.output_acc = accumulate(
                pctx::OUTPUT_ACC,
                &self.output_acc,
                &out.to_canonical_bytes(),
            );
            self.outputs.push(out);
        }
        for ki in effect.spent {
            self.key_image_acc = accumulate(pctx::KEY_IMAGE_ACC, &self.key_image_acc, &ki);
            self.key_images.insert(ki);
            self.key_image_log.push(ki);
        }
        self.shielded_supply = effect.shielded_supply;
        match effect.gov {
            None => {}
            Some(GovEffect::Lock(id, lock)) => {
                self.locks.insert(id, lock);
                self.next_lock_id = id + 1;
            }
            Some(GovEffect::Unlock(id)) => {
                self.locks.remove(&id);
            }
            Some(GovEffect::Propose(p)) => {
                self.proposals.insert(p.id, *p);
            }
            Some(GovEffect::Vote(id, voter, vote)) => {
                if let Some(p) = self.proposals.get_mut(&id) {
                    p.votes.insert(voter, vote);
                }
            }
            Some(GovEffect::Slash(key, evidence)) => self.slash(key, evidence, p.height),
            Some(GovEffect::Bond(key, amount)) => {
                *self.bonds.entry(key).or_default() += amount;
            }
            Some(GovEffect::Unbond(key, amount)) => {
                let remaining = self.bond_of(&key) - amount;
                if remaining == 0 {
                    self.bonds.remove(&key);
                } else {
                    self.bonds.insert(key, remaining);
                }
                self.unbonding.push(Unbonding {
                    owner: key.address(),
                    amount,
                    release_height: p
                        .height
                        .saturating_add(self.params.consensus.unbonding_blocks),
                });
            }
        }
        Ok(effect.fee)
    }

    /// Punição com evidência verificável (`SPEC §49`): queima `slash_bps` do
    /// bloqueio e das desvinculações pendentes, exclui do conjunto
    /// imediatamente e zera os pontos de contribuição.
    fn slash(&mut self, key: rz_crypto::PublicKey, evidence: Hash32, height: u64) {
        let bps = self.params.consensus.slash_bps as u128;
        let cut = |v: u64| (v as u128 * bps / 10_000) as u64;
        let mut burned = 0u64;
        if let Some(b) = self.bonds.get_mut(&key) {
            let c = cut(*b);
            *b -= c;
            burned += c;
        }
        let owner = key.address();
        for u in self.unbonding.iter_mut().filter(|u| u.owner == owner) {
            let c = cut(u.amount);
            u.amount -= c;
            burned += c;
        }
        self.unbonding.retain(|u| u.amount > 0);
        self.total_supply -= burned;
        self.jailed.insert(key);
        self.punished.insert(evidence);
        self.contributions.insert(
            key,
            Contribution {
                points: 0,
                updated_at: height,
            },
        );
        let remaining: Vec<Validator> = self
            .validators
            .validators()
            .iter()
            .filter(|v| v.key != key)
            .cloned()
            .collect();
        if !remaining.is_empty() {
            self.validators = ValidatorSet::new(remaining);
        }
    }

    /// Regras de fim de bloco (`spec/GOVERNANCE.md §1`, §5):
    ///
    /// 1. o produtor recebe 1 ponto de contribuição;
    /// 2. votações que encerram nesta altura são apuradas; o depósito é
    ///    devolvido (com quórum) ou queimado (sem quórum);
    /// 3. propostas aprovadas cuja ativação é nesta altura são aplicadas.
    pub(crate) fn end_block(
        &mut self,
        proposer: &rz_crypto::PublicKey,
        height: u64,
        used_validators: ValidatorSet,
    ) -> Result<(), StateError> {
        // Consenso: o conjunto que validou este bloco; nova época recalcula.
        self.last_validators = used_validators;
        let released: Vec<Unbonding> = self
            .unbonding
            .iter()
            .filter(|u| u.release_height <= height)
            .cloned()
            .collect();
        self.unbonding.retain(|u| u.release_height > height);
        for u in released {
            self.credit_fees(u.owner, u.amount)?;
        }
        if height.is_multiple_of(self.params.consensus.epoch_blocks) {
            let next = self.select_validators();
            if !next.is_empty() {
                self.validators = next;
            }
        }

        let hl = self.params.governance.contribution_half_life_blocks;
        let c = self.contributions.entry(*proposer).or_default();
        c.points = c
            .at(height, hl)
            .checked_add(SCALE)
            .ok_or(StateError::Overflow)?;
        c.updated_at = height;

        let closing: Vec<Hash32> = self
            .proposals
            .values()
            .filter(|p| p.status == ProposalStatus::Pending && p.voting_end == height)
            .map(|p| p.id)
            .collect();
        for id in closing {
            let tally = self.tally(&self.proposals[&id], height);
            let p = self.proposals.get_mut(&id).ok_or(StateError::Overflow)?;
            let status = decide(p.category, &tally);
            p.status = status;
            p.tally = Some(tally);
            let (deposit, proposer) = (p.deposit, p.proposer);
            if status == ProposalStatus::NoQuorum {
                // Queima: ninguém se beneficia de barrar propostas alheias.
                self.total_supply = self
                    .total_supply
                    .checked_sub(deposit)
                    .ok_or(StateError::Overflow)?;
            } else {
                self.credit_fees(proposer, deposit)?;
            }
        }

        let activating: Vec<Hash32> = self
            .proposals
            .values()
            .filter(|p| p.status == ProposalStatus::Approved && p.activation_height == height)
            .map(|p| p.id)
            .collect();
        for id in activating {
            let p = self.proposals.get_mut(&id).ok_or(StateError::Overflow)?;
            let mut next = self.params.clone();
            for change in &p.params {
                change.apply(&mut next);
            }
            if next.validate().is_ok() {
                p.status = ProposalStatus::Activated;
                if p.category == Category::Community {
                    self.approved_communities.insert(p.content_hash);
                }
                self.params = next;
            } else {
                p.status = ProposalStatus::ActivationFailed;
            }
        }
        Ok(())
    }

    /// Apuração bicameral determinística de uma proposta.
    fn tally(&self, p: &Proposal, height: u64) -> Tally {
        let g = &self.params.governance;
        let mut econ_by_owner: BTreeMap<Address, u128> = BTreeMap::new();
        let mut econ = ChamberTally::default();
        for l in self.locks.values() {
            let w = lock_weight(l, p, g);
            if w > 0 {
                *econ_by_owner.entry(l.owner).or_default() += w;
                econ.eligible += w;
            }
        }
        let mut contrib = ChamberTally::default();
        for c in self.contributions.values() {
            contrib.eligible += c.at(height, g.contribution_half_life_blocks) as u128;
        }
        for (voter, vote) in &p.votes {
            let we = econ_by_owner.get(voter).copied().unwrap_or(0);
            let wc = self.contribution(&vote.key, height) as u128;
            for (chamber, w) in [(&mut econ, we), (&mut contrib, wc)] {
                match vote.choice {
                    crate::governance::Choice::Yes => chamber.yes += w,
                    crate::governance::Choice::No => chamber.no += w,
                    crate::governance::Choice::Abstain => chamber.abstain += w,
                }
                chamber.largest_voter = chamber.largest_voter.max(w);
            }
        }
        Tally {
            economic: econ,
            contribution: contrib,
        }
    }

    /// Credita as taxas coletadas no bloco ao produtor (ADR-0005).
    ///
    /// Taxas já foram debitadas dos remetentes, portanto a oferta total não muda.
    pub(crate) fn credit_fees(&mut self, to: Address, fees: u64) -> Result<(), StateError> {
        if fees == 0 {
            return Ok(());
        }
        let acc = self.accounts.entry(to).or_default();
        acc.balance = acc.balance.checked_add(fees).ok_or(StateError::Overflow)?;
        Ok(())
    }

    fn plan(&self, tx: &Transaction, p: &ExecParams<'_>) -> Result<Effect, TxError> {
        match tx {
            Transaction::Account(a) => self.plan_account(a, p),
            Transaction::Private(t) => self.plan_private(t, p),
        }
    }

    fn plan_account(&self, tx: &AccountTx, p: &ExecParams<'_>) -> Result<Effect, TxError> {
        tx.check_stateless(self.params.min_fee)?;
        tx.verify_signature(p.network_id)?;

        let sender = tx.body.sender_address();
        let mut sender_acc = self.account(&sender);
        if tx.body.nonce != sender_acc.nonce {
            return Err(TxError::BadNonce {
                expected: sender_acc.nonce,
                got: tx.body.nonce,
            });
        }
        let amount = match &tx.body.kind {
            TxKind::Transfer { amount, .. }
            | TxKind::Shield { amount, .. }
            | TxKind::LockStake { amount, .. } => *amount,
            TxKind::Propose { deposit, .. } => *deposit,
            TxKind::Bond { amount } => *amount,
            TxKind::Unlock { .. }
            | TxKind::Vote { .. }
            | TxKind::ReportEquivocation { .. }
            | TxKind::Unbond { .. }
            | TxKind::ReportDoubleVote { .. }
            | TxKind::ReportDoubleProposal { .. } => 0,
        };
        let required = amount.checked_add(tx.body.fee).ok_or(TxError::Overflow)?;
        if sender_acc.balance < required {
            return Err(TxError::InsufficientBalance {
                balance: sender_acc.balance,
                required,
            });
        }
        sender_acc.balance -= required;
        sender_acc.nonce = sender_acc.nonce.checked_add(1).ok_or(TxError::Overflow)?;

        let mut effect = Effect {
            accounts: Vec::new(),
            new_outputs: Vec::new(),
            spent: Vec::new(),
            shielded_supply: self.shielded_supply,
            fee: tx.body.fee,
            gov: None,
        };
        let g = &self.params.governance;

        match &tx.body.kind {
            TxKind::Transfer { to, amount } => {
                if *to == sender {
                    sender_acc.balance = sender_acc
                        .balance
                        .checked_add(*amount)
                        .ok_or(TxError::Overflow)?;
                    effect.accounts.push((sender, sender_acc));
                } else {
                    let mut to_acc = self.account(to);
                    to_acc.balance = to_acc
                        .balance
                        .checked_add(*amount)
                        .ok_or(TxError::Overflow)?;
                    effect.accounts.push((sender, sender_acc));
                    effect.accounts.push((*to, to_acc));
                }
            }
            TxKind::Shield {
                amount,
                outputs,
                excess: proof,
            } => {
                verify_outputs(outputs)?;
                let commitments: Vec<[u8; 32]> =
                    outputs.iter().map(|o| o.data.commitment).collect();
                let msg = shield_message(tx.body.sender.as_bytes(), *amount, outputs);
                if !excess::verify(&commitments, *amount, &msg, proof) {
                    return Err(TxError::Excess);
                }
                effect.shielded_supply = self
                    .shielded_supply
                    .checked_add(*amount)
                    .ok_or(TxError::Overflow)?;
                effect.new_outputs = outputs.iter().map(|o| o.data.clone()).collect();
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::LockStake {
                amount,
                unlock_height,
            } => {
                let min = p.height.saturating_add(g.lock_min_blocks);
                let max = p.height.saturating_add(g.lock_max_blocks);
                if *unlock_height < min || *unlock_height > max {
                    return Err(TxError::Governance("duração do bloqueio fora dos limites"));
                }
                effect.gov = Some(GovEffect::Lock(
                    self.next_lock_id,
                    Lock {
                        owner: sender,
                        amount: *amount,
                        locked_at: p.height,
                        unlock_height: *unlock_height,
                    },
                ));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::Unlock { lock_id } => {
                let lock = self
                    .locks
                    .get(lock_id)
                    .ok_or(TxError::Governance("bloqueio inexistente"))?;
                if lock.owner != sender {
                    return Err(TxError::Governance("bloqueio pertence a outra conta"));
                }
                if p.height < lock.unlock_height {
                    return Err(TxError::Governance("bloqueio ainda não venceu"));
                }
                sender_acc.balance = sender_acc
                    .balance
                    .checked_add(lock.amount)
                    .ok_or(TxError::Overflow)?;
                effect.gov = Some(GovEffect::Unlock(*lock_id));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::Propose {
                category,
                content_hash,
                params,
                release_id,
                deposit,
            } => {
                if *deposit < g.deposit {
                    return Err(TxError::DepositTooLow {
                        deposit: *deposit,
                        min: g.deposit,
                    });
                }
                check_proposal_shape(*category, params, &self.params)
                    .map_err(TxError::Governance)?;
                let id = tx.id().0;
                if self.proposals.contains_key(&id) {
                    return Err(TxError::Governance("proposta já existe"));
                }
                let voting_start = p.height.saturating_add(g.analysis_blocks);
                let voting_end = voting_start.saturating_add(g.voting_blocks);
                let delay = match category {
                    Category::Ordinary => g.ordinary_delay_blocks,
                    Category::Community => 0,
                    Category::Constitutional => g.constitutional_delay_blocks,
                };
                effect.gov = Some(GovEffect::Propose(Box::new(Proposal {
                    id,
                    proposer: sender,
                    category: *category,
                    content_hash: *content_hash,
                    params: params.clone(),
                    release_id: *release_id,
                    deposit: *deposit,
                    submitted_at: p.height,
                    voting_start,
                    voting_end,
                    activation_height: voting_end.saturating_add(delay),
                    status: ProposalStatus::Pending,
                    votes: BTreeMap::new(),
                    tally: None,
                })));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::Vote { proposal, choice } => {
                let prop = self
                    .proposals
                    .get(proposal)
                    .ok_or(TxError::Governance("proposta inexistente"))?;
                if prop.status != ProposalStatus::Pending
                    || p.height < prop.voting_start
                    || p.height >= prop.voting_end
                {
                    return Err(TxError::Governance("fora do período de votação"));
                }
                effect.gov = Some(GovEffect::Vote(
                    *proposal,
                    sender,
                    Vote {
                        key: tx.body.sender,
                        choice: *choice,
                    },
                ));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::ReportEquivocation { first, second } => {
                let (a, b) = (&first.header, &second.header);
                if a.proposer != b.proposer {
                    return Err(TxError::InvalidEvidence("produtores diferentes"));
                }
                if a.height != b.height || a.round != b.round {
                    return Err(TxError::InvalidEvidence("altura ou rodada diferentes"));
                }
                if a.id() == b.id() {
                    return Err(TxError::InvalidEvidence("cabeçalhos idênticos"));
                }
                if !first.verify(p.network_id) || !second.verify(p.network_id) {
                    return Err(TxError::InvalidEvidence("assinatura inválida"));
                }
                let evidence = evidence_id(&a.proposer, a.height, a.round as u32, 0);
                if self.punished.contains(&evidence) {
                    return Err(TxError::InvalidEvidence("infração já punida"));
                }
                effect.gov = Some(GovEffect::Slash(a.proposer, evidence));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::ReportDoubleProposal { first, second } => {
                if !first.conflicts_with(second) {
                    return Err(TxError::InvalidEvidence("propostas não conflitantes"));
                }
                if !first.verify(p.network_id) || !second.verify(p.network_id) {
                    return Err(TxError::InvalidEvidence("assinatura inválida"));
                }
                let key = first.proposer;
                if self.bond_of(&key) == 0
                    && !self.unbonding.iter().any(|u| u.owner == key.address())
                {
                    return Err(TxError::InvalidEvidence("não é validador"));
                }
                // Mesmo identificador da equivocação de cabeçalho (tipo 0):
                // a mesma infração não é punida duas vezes.
                let evidence = evidence_id(&key, first.height, first.round, 0);
                if self.punished.contains(&evidence) {
                    return Err(TxError::InvalidEvidence("infração já punida"));
                }
                effect.gov = Some(GovEffect::Slash(key, evidence));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::Bond { amount } => {
                let key = tx.body.sender;
                if self.jailed.contains(&key) {
                    return Err(TxError::Staking("validador excluído por punição"));
                }
                let total = self
                    .bond_of(&key)
                    .checked_add(*amount)
                    .ok_or(TxError::Overflow)?;
                if total < self.params.consensus.min_bond {
                    return Err(TxError::Staking("abaixo do bloqueio mínimo de validador"));
                }
                effect.gov = Some(GovEffect::Bond(key, *amount));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::Unbond { amount } => {
                let key = tx.body.sender;
                let bonded = self.bond_of(&key);
                if *amount > bonded {
                    return Err(TxError::Staking("valor maior que o bloqueado"));
                }
                let remaining = bonded - amount;
                if remaining != 0 && remaining < self.params.consensus.min_bond {
                    return Err(TxError::Staking("restante abaixo do bloqueio mínimo"));
                }
                effect.gov = Some(GovEffect::Unbond(key, *amount));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::ReportDoubleVote { first, second } => {
                if !first.conflicts_with(second) {
                    return Err(TxError::InvalidEvidence("votos não conflitantes"));
                }
                if !first.verify(p.network_id) || !second.verify(p.network_id) {
                    return Err(TxError::InvalidEvidence("assinatura inválida"));
                }
                let key = first.validator;
                if self.bond_of(&key) == 0
                    && !self.unbonding.iter().any(|u| u.owner == key.address())
                {
                    return Err(TxError::InvalidEvidence("não é validador"));
                }
                let evidence = evidence_id(&key, first.height, first.round, first.kind as u8);
                if self.punished.contains(&evidence) {
                    return Err(TxError::InvalidEvidence("infração já punida"));
                }
                effect.gov = Some(GovEffect::Slash(key, evidence));
                effect.accounts.push((sender, sender_acc));
            }
        }
        Ok(effect)
    }

    fn plan_private(&self, tx: &PrivateTx, p: &ExecParams<'_>) -> Result<Effect, TxError> {
        // Verificações estruturais baratas primeiro (THR-P2P-002).
        if tx.version != PRIVATE_TX_VERSION {
            return Err(TxError::UnsupportedVersion(tx.version));
        }
        if tx.fee < self.params.min_fee {
            return Err(TxError::FeeTooLow {
                fee: tx.fee,
                min: self.params.min_fee,
            });
        }
        if tx.inputs.is_empty() {
            return Err(TxError::InvalidPrivate("sem entradas"));
        }
        if tx.signatures.len() != tx.inputs.len() {
            return Err(TxError::InvalidPrivate("número de assinaturas"));
        }
        if tx.outputs.is_empty() && tx.unshield.is_none() {
            return Err(TxError::InvalidPrivate("sem saídas"));
        }
        if tx.unshield.as_ref().is_some_and(|u| u.amount == 0) {
            return Err(TxError::ZeroAmount);
        }
        let public = tx.public_amount().ok_or(TxError::Overflow)?;

        let total = self.outputs.len() as u64;
        let ring_min = min_ring_size(self.outputs.len()).max(1);
        let mut seen = BTreeSet::new();
        for input in &tx.inputs {
            let n = input.ring.len();
            if n < ring_min || n > RING_SIZE {
                return Err(TxError::InvalidPrivate("tamanho do anel"));
            }
            if !input.ring.windows(2).all(|w| w[0] < w[1]) {
                return Err(TxError::InvalidPrivate("anel fora de ordem"));
            }
            if input.ring.last().is_some_and(|i| *i >= total) {
                return Err(TxError::InvalidPrivate("membro inexistente"));
            }
            if self.key_images.contains(&input.key_image) || !seen.insert(input.key_image) {
                return Err(TxError::KeyImageSpent);
            }
        }

        let pseudo: Vec<[u8; 32]> = tx.inputs.iter().map(|i| i.pseudo_out).collect();
        let commitments: Vec<[u8; 32]> = tx.outputs.iter().map(|o| o.data.commitment).collect();
        if !balance_holds(&pseudo, &commitments, public) {
            return Err(TxError::Balance);
        }

        let msg = tx.message(p.network_id);
        for (input, sig) in tx.inputs.iter().zip(&tx.signatures) {
            let ring: Vec<RingMember> = input
                .ring
                .iter()
                .map(|i| {
                    let o = &self.outputs[*i as usize];
                    RingMember {
                        one_time_key: o.one_time_key,
                        commitment: o.commitment,
                    }
                })
                .collect();
            if !clsag::verify(&ring, &input.pseudo_out, &input.key_image, &msg, sig) {
                return Err(TxError::RingSignature);
            }
        }
        verify_outputs(&tx.outputs)?;

        let shielded_supply = self
            .shielded_supply
            .checked_sub(public)
            .ok_or(TxError::ShieldedSupplyUnderflow)?;

        let mut accounts = Vec::new();
        if let Some(u) = &tx.unshield {
            let mut acc = self.account(&u.to);
            acc.balance = acc.balance.checked_add(u.amount).ok_or(TxError::Overflow)?;
            accounts.push((u.to, acc));
        }
        Ok(Effect {
            accounts,
            new_outputs: tx.outputs.iter().map(|o| o.data.clone()).collect(),
            spent: tx.inputs.iter().map(|i| i.key_image).collect(),
            shielded_supply,
            fee: tx.fee,
            gov: None,
        })
    }
}

/// Identificador de uma infração: `(validador, altura, rodada, tipo)`.
fn evidence_id(key: &rz_crypto::PublicKey, height: u64, round: u32, kind: u8) -> Hash32 {
    let mut e = Encoder::new();
    e.put(key).u64(height).u32(round).u8(kind);
    hash("rede-zero/evidence/v1", &e.into_bytes())
}

/// Pontos válidos e prova de faixa válida para cada saída.
fn verify_outputs(outputs: &[ShieldedOutput]) -> Result<(), TxError> {
    for o in outputs {
        if decode_point(&o.data.one_time_key).is_none() || decode_point(&o.data.tx_pub).is_none() {
            return Err(TxError::InvalidPrivate("ponto inválido"));
        }
        if !commitment::verify_range(&o.data.commitment, &o.range_proof) {
            return Err(TxError::RangeProof);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::tests::sample;
    use crate::tx::{TxBody, TX_VERSION};
    use rz_crypto::SecretKey;

    fn rich() -> SecretKey {
        SecretKey::from_seed([2; 32])
    }

    fn transfer(from: &SecretKey, nonce: u64, to: Address, amount: u64, fee: u64) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: from.public_key(),
            nonce,
            fee,
            kind: TxKind::Transfer { to, amount },
        }
        .sign(from, "rede-zero-devnet-test")
        .unwrap()
    }

    fn bob() -> Address {
        SecretKey::from_seed([50; 32]).public_key().address()
    }

    // AT-TX-001 / AT-ZERO-001 / AT-STATE-001 — transição válida
    #[test]
    fn valid_transfer() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let before = s.account(&rich().public_key().address()).balance;
        let fee = s
            .apply_transaction(&transfer(&rich(), 0, bob(), 100, 5), &p)
            .unwrap();
        assert_eq!(fee, 5);
        let a = s.account(&rich().public_key().address());
        assert_eq!(a.balance, before - 105);
        assert_eq!(a.nonce, 1);
        assert_eq!(s.account(&bob()).balance, 100);
    }

    // AT-TX-006 / AT-MONEY-003 — saldo insuficiente não produz saldo negativo
    #[test]
    fn insufficient_balance() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let snapshot = s.clone();
        let err = s
            .apply_transaction(&transfer(&rich(), 0, bob(), 2_000_000, 1), &p)
            .unwrap_err();
        assert!(matches!(err, TxError::InsufficientBalance { .. }));
        assert_eq!(s, snapshot, "estado não pode mudar após erro");
    }

    // AT-TX-007 / AT-DS-001 — replay e double spend
    #[test]
    fn replay_and_double_spend() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let tx1 = transfer(&rich(), 0, bob(), 900_000, 1);
        let tx2 = transfer(
            &rich(),
            0,
            SecretKey::from_seed([51; 32]).public_key().address(),
            900_000,
            1,
        );
        s.apply_transaction(&tx1, &p).unwrap();
        assert_eq!(
            s.apply_transaction(&tx1, &p),
            Err(TxError::BadNonce {
                expected: 1,
                got: 0
            })
        );
        assert!(s.apply_transaction(&tx2, &p).is_err());
    }

    // AT-MONEY-002 / AT-ZERO-002 — soma monetária preservada
    #[test]
    fn supply_preserved() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let supply = s.total_supply();
        let fee = s
            .apply_transaction(&transfer(&rich(), 0, bob(), 10, 3), &p)
            .unwrap();
        // Antes do crédito da taxa, a soma é menor exatamente pela taxa.
        assert!(s.check_supply().is_err());
        s.credit_fees(bob(), fee).unwrap();
        s.check_supply().unwrap();
        assert_eq!(s.total_supply(), supply);
    }

    // AT-STATE-003 / AT-DET-001 — determinismo
    #[test]
    fn deterministic_root() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let run = || {
            let mut s = State::from_genesis(&g).unwrap();
            s.apply_transaction(&transfer(&rich(), 0, bob(), 10, 3), &p)
                .unwrap();
            s.root()
        };
        assert_eq!(run(), run());
        assert_ne!(run(), State::from_genesis(&g).unwrap().root());
    }

    #[test]
    fn self_transfer_only_costs_fee() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let me = rich().public_key().address();
        let before = s.account(&me).balance;
        s.apply_transaction(&transfer(&rich(), 0, me, 10, 3), &p)
            .unwrap();
        assert_eq!(s.account(&me).balance, before - 3);
    }

    #[test]
    fn amount_plus_fee_overflow() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        assert_eq!(
            s.check_transaction(&transfer(&rich(), 0, bob(), u64::MAX, 1), &p),
            Err(TxError::Overflow)
        );
    }

    #[test]
    fn unknown_account_cannot_spend() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let nobody = SecretKey::from_seed([77; 32]);
        assert!(matches!(
            s.check_transaction(&transfer(&nobody, 0, bob(), 1, 1), &p),
            Err(TxError::InsufficientBalance { .. })
        ));
    }
}
