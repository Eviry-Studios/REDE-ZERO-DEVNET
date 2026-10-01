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

use crate::community::{self, Community, CommunityState, CommunityStatus, NameKind, NameRecord};
use crate::consensus::{Validator, ValidatorSet};
use crate::defense::{
    self, Credential, DefenseActionRecord, DefenseMode, DefenseRecord, DefenseState, Incident,
    IncidentStatus, Quorum,
};
use crate::genesis::{Genesis, GenesisError};
use crate::governance::{
    check_proposal_shape, decide, lock_weight, Category, ChamberTally, Contribution, Lock,
    Proposal, ProposalStatus, ProtocolParams, Tally, Vote, SCALE,
};
use crate::market::{
    self, quote_ceil, quote_floor, AssetId, MarketParams, MarketState, Order, Side,
};
use crate::private::{
    accumulate, context as pctx, min_ring_size, shield_message, PrivateTx, ShieldedOutput,
    PRIVATE_TX_VERSION, RING_SIZE,
};
use crate::runtime::{self, ModuleRecord, Receipt, RuntimeState};
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
    /// A conservação de um ativo externo foi violada.
    AssetMismatch(AssetId),
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
            Self::AssetMismatch(a) => write!(f, "conservação do ativo {a} violada"),
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
    /// Grande Mercado e Pool permanente (ADR-0014).
    market: MarketState,
    /// Comunidades e nomes `zero://` (ADR-0015).
    communities: CommunityState,
    /// Defesa da Exonet (ADR-0016).
    defense: DefenseState,
    /// Exonet Runtime (ADR-0018).
    runtime: RuntimeState,
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
    market: Option<MarketEffect>,
    community: Option<CommunityEffect>,
    defense: Option<DefenseEffect>,
    runtime: Option<RuntimeEffect>,
}

/// Alteração do Exonet Runtime produzida por uma transação. Chamadas são
/// executadas na aplicação; uma falha dentro do módulo não invalida a
/// transação (a taxa e o combustível reservado são pagos).
enum RuntimeEffect {
    /// A taxa por byte (já debitada) vai para o Pool.
    Publish {
        id: Hash32,
        record: ModuleRecord,
        fee: u64,
    },
    Bind {
        community: Hash32,
        module: Option<Hash32>,
    },
    Call {
        community: Hash32,
        module: Hash32,
        method: String,
        args: Vec<u8>,
        fuel: u64,
        caller: Address,
    },
}

/// Alteração de defesa produzida por uma transação. As decisões atestadas
/// incrementam `seq`; ações de portadores de credencial, não.
enum DefenseEffect {
    /// Novo modo (ou renovação), evidência e, ao entrar em INCIDENTE, o
    /// incidente aberto.
    Mode {
        to: DefenseMode,
        evidence: Hash32,
        open: Option<Hash32>,
        height: u64,
    },
    Update {
        incident: Hash32,
        status: IncidentStatus,
        evidence: Hash32,
    },
    Close {
        incident: Hash32,
        archive: Hash32,
        height: u64,
    },
    Grant(Box<Credential>),
    Revoke(Hash32),
    Action {
        incident: Hash32,
        record: DefenseActionRecord,
    },
    Contribution {
        node: rz_crypto::PublicKey,
        record: DefenseRecord,
    },
}

/// Alteração de Comunidades ou nomes produzida por uma transação.
enum CommunityEffect {
    Declare(Box<Community>),
    Position {
        proposal: Hash32,
        community: Hash32,
        choice: crate::governance::Choice,
    },
    Update {
        community: Hash32,
        manifest_hash: Hash32,
        version: u32,
        rule: Option<community::DecisionRule>,
        height: u64,
    },
    /// Novo nome; a taxa (já debitada) vai para o Pool.
    RegisterName {
        kind: NameKind,
        name: String,
        record: NameRecord,
        fee: u64,
    },
    UpdateName {
        kind: NameKind,
        name: String,
        target: Hash32,
        new_owner: Option<Address>,
    },
}

/// Alteração do Grande Mercado ou do Pool produzida por uma transação.
enum MarketEffect {
    Transfer {
        asset: AssetId,
        from: Address,
        to: Address,
        amount: u64,
    },
    /// Depósito no Pool. Para ZERO, o saldo já foi debitado da conta.
    PoolDeposit {
        asset: AssetId,
        from: Address,
        amount: u64,
    },
    /// Nova ordem. Numa compra, o ZERO reservado já foi debitado da conta.
    Place(Order),
    Cancel(Hash32),
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
                market: MarketParams::default(),
                communities: community::CommunityParams::default(),
                defense: defense::DefenseParams::default(),
                runtime: runtime::RuntimeParams::default(),
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
            market: {
                let mut m = MarketState::default();
                for a in &genesis.assets {
                    let id = a.id();
                    m.assets.insert(id, a.info().map_err(StateError::Genesis)?);
                    for al in &a.allocations {
                        m.balances.insert((al.address, id), al.amount);
                    }
                }
                m
            },
            communities: CommunityState::default(),
            defense: DefenseState::default(),
            runtime: RuntimeState::default(),
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

    /// Grande Mercado e Pool permanente.
    pub fn market(&self) -> &MarketState {
        &self.market
    }

    /// Comunidades e nomes `zero://`.
    pub fn communities(&self) -> &CommunityState {
        &self.communities
    }

    /// Estado de defesa da Exonet.
    pub fn runtime(&self) -> &RuntimeState {
        &self.runtime
    }

    pub fn defense(&self) -> &DefenseState {
        &self.defense
    }

    /// Acesso de teste aos parâmetros (simula uma proposta já ativada).
    #[cfg(test)]
    pub(crate) fn params_mut(&mut self) -> &mut ProtocolParams {
        &mut self.params
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
            .put(&self.governance_root())
            .put(&self.market.root())
            .put(&self.communities.root())
            .put(&self.defense.root())
            .put(&self.runtime.root());
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
            + self.staked_total()
            + self.market.zero_escrow()
            + self.market.pool_balance(&AssetId::ZERO) as u128;
        if let Err(asset) = self.market.check_assets() {
            return Err(StateError::AssetMismatch(asset));
        }
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
        if let Some(m) = effect.market {
            self.apply_market(m).map_err(|_| TxError::Overflow)?;
        }
        if let Some(c) = effect.community {
            self.apply_community(c).map_err(|_| TxError::Overflow)?;
        }
        if let Some(d) = effect.defense {
            self.apply_defense(d);
        }
        if let Some(r) = effect.runtime {
            self.apply_runtime(r, tx.id().0, p.height)
                .map_err(|_| TxError::Overflow)?;
        }
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
                // A declaração de uma Comunidade em votação não expira antes
                // da ativação.
                if p.category == Category::Community {
                    if let Some(c) = self.communities.communities.get_mut(&p.content_hash) {
                        c.expires_at = c.expires_at.max(p.activation_height.saturating_add(1));
                    }
                }
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
        // Defesa: modo vencido sem renovação desce um nível (REQ-059).
        self.expire_defense(height);
        // Grande Mercado: vencimentos e leilões dos livros que mudaram.
        self.settle_market(height)?;
        // Declarações de Comunidade vencidas sem reconhecimento liberam o nome.
        self.communities
            .communities
            .retain(|_, c| c.status == CommunityStatus::Recognized || c.expires_at > height);

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
                    // Reconhecimento (N-7): a declaração passa a Comunidade
                    // reconhecida, com nome `.comunidade`.
                    if let Some(c) = self.communities.communities.get_mut(&p.content_hash) {
                        if c.status == CommunityStatus::Declared {
                            c.status = CommunityStatus::Recognized;
                        }
                    }
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
            TxKind::PoolDeposit { asset, amount } if asset.is_zero() => *amount,
            // Compra: o ZERO do limite é reservado na colocação.
            TxKind::PlaceOrder {
                side: Side::Buy,
                amount,
                price,
                ..
            } => quote_ceil(*amount, *price).ok_or(TxError::Overflow)?,
            TxKind::PoolDeposit { .. }
            | TxKind::PlaceOrder { .. }
            | TxKind::TransferAsset { .. }
            | TxKind::CancelOrder { .. } => 0,
            TxKind::RegisterName { .. } => self.params.communities.name_fee,
            TxKind::DeclareCommunity { .. }
            | TxKind::CommunityPosition { .. }
            | TxKind::UpdateCommunity { .. }
            | TxKind::UpdateName { .. } => 0,
            TxKind::DefenseTransition { .. }
            | TxKind::IncidentUpdate { .. }
            | TxKind::CloseIncident { .. }
            | TxKind::GrantCredential { .. }
            | TxKind::RevokeCredential { .. }
            | TxKind::DefenseAction { .. }
            | TxKind::AttestContribution { .. } => 0,
            TxKind::PublishModule { code } => (code.len() as u64)
                .checked_mul(self.params.runtime.module_byte_fee)
                .ok_or(TxError::Overflow)?,
            TxKind::BindModule { .. } | TxKind::CallModule { .. } => 0,
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
            market: None,
            community: None,
            defense: None,
            runtime: None,
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
                // Validação técnica (N-7): a proposta de categoria Comunidade
                // aponta para uma declaração pendente e válida.
                if *category == Category::Community {
                    match self.communities.communities.get(content_hash) {
                        Some(c)
                            if c.status == CommunityStatus::Declared && c.expires_at > p.height => {
                        }
                        _ => {
                            return Err(TxError::Community(
                                "proposta de Comunidade sem declaração pendente",
                            ))
                        }
                    }
                }
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
            TxKind::TransferAsset { asset, to, amount } => {
                self.registered(asset)?;
                if self.market.balance(&sender, asset) < *amount {
                    return Err(TxError::Market("saldo do ativo insuficiente"));
                }
                effect.market = Some(MarketEffect::Transfer {
                    asset: *asset,
                    from: sender,
                    to: *to,
                    amount: *amount,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::PoolDeposit { asset, amount } => {
                if !asset.is_zero() {
                    self.registered(asset)?;
                    if self.market.balance(&sender, asset) < *amount {
                        return Err(TxError::Market("saldo do ativo insuficiente"));
                    }
                }
                effect.market = Some(MarketEffect::PoolDeposit {
                    asset: *asset,
                    from: sender,
                    amount: *amount,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::PlaceOrder {
                asset,
                side,
                amount,
                price,
                expires_at,
            } => {
                self.registered(asset)?;
                let mp = &self.params.market;
                if *expires_at <= p.height
                    || *expires_at > p.height.saturating_add(mp.order_lifetime_blocks)
                {
                    return Err(TxError::Market("validade fora dos limites"));
                }
                if self.market.orders.len() >= mp.max_open_orders as usize {
                    return Err(TxError::Market("livro de ordens cheio"));
                }
                if self.market.orders_of(&sender) >= mp.max_orders_per_account as usize {
                    return Err(TxError::Market("ordens demais para esta conta"));
                }
                // Ordem com valor nulo no limite (poeira) nunca executaria.
                if quote_floor(*amount, *price).ok_or(TxError::Overflow)? == 0 {
                    return Err(TxError::Market("ordem abaixo do valor mínimo"));
                }
                let escrow = match side {
                    Side::Buy => quote_ceil(*amount, *price).ok_or(TxError::Overflow)?,
                    Side::Sell => {
                        if self.market.balance(&sender, asset) < *amount {
                            return Err(TxError::Market("saldo do ativo insuficiente"));
                        }
                        *amount
                    }
                };
                effect.market = Some(MarketEffect::Place(Order {
                    id: tx.id().0,
                    owner: sender,
                    asset: *asset,
                    side: *side,
                    price: *price,
                    remaining: *amount,
                    escrow,
                    placed_at: p.height,
                    expires_at: *expires_at,
                }));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::CancelOrder { order } => {
                // Só o autor cancela (THR-MKT-002, AT-MKT-004).
                match self.market.orders.get(order) {
                    None => return Err(TxError::Market("ordem inexistente")),
                    Some(o) if o.owner != sender => {
                        return Err(TxError::Market("ordem pertence a outra conta"))
                    }
                    Some(_) => {}
                }
                effect.market = Some(MarketEffect::Cancel(*order));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::DeclareCommunity {
                name,
                manifest_hash,
                rule,
            } => {
                let cp = &self.params.communities;
                if self.communities.pending() >= cp.max_pending as usize {
                    return Err(TxError::Community("declarações pendentes demais"));
                }
                self.communities
                    .name_available(name, NameKind::Community)
                    .map_err(TxError::Community)?;
                effect.community = Some(CommunityEffect::Declare(Box::new(Community {
                    id: tx.id().0,
                    name: name.clone(),
                    declarant: sender,
                    manifest_hash: *manifest_hash,
                    version: 1,
                    rule: rule.clone(),
                    status: CommunityStatus::Declared,
                    declared_at: p.height,
                    expires_at: p.height.saturating_add(cp.declaration_ttl_blocks),
                    history: vec![(1, *manifest_hash, p.height)],
                })));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::CommunityPosition {
                community,
                proposal,
                choice,
                approvals,
            } => {
                let c = self.recognized(community)?;
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
                let payload = community::position_payload(community, proposal, *choice);
                if !c.rule.verify(p.network_id, &payload, approvals) {
                    return Err(TxError::Community(
                        "posição sem aprovação válida da regra declarada",
                    ));
                }
                effect.community = Some(CommunityEffect::Position {
                    proposal: *proposal,
                    community: *community,
                    choice: *choice,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::UpdateCommunity {
                community,
                manifest_hash,
                version,
                rule,
                approvals,
            } => {
                let c = self.recognized(community)?;
                if *version <= c.version {
                    return Err(TxError::Community("versão deve aumentar"));
                }
                let payload = community::update_payload(community, manifest_hash, *version, rule);
                if !c.rule.verify(p.network_id, &payload, approvals) {
                    return Err(TxError::Community(
                        "atualização sem aprovação válida da regra vigente",
                    ));
                }
                effect.community = Some(CommunityEffect::Update {
                    community: *community,
                    manifest_hash: *manifest_hash,
                    version: *version,
                    rule: rule.clone(),
                    height: p.height,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::RegisterName { name, kind, target } => {
                self.communities
                    .name_available(name, *kind)
                    .map_err(TxError::Community)?;
                effect.community = Some(CommunityEffect::RegisterName {
                    kind: *kind,
                    name: name.clone(),
                    record: NameRecord {
                        owner: sender,
                        target: *target,
                        registered_at: p.height,
                    },
                    fee: self.params.communities.name_fee,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::PublishModule { code } => {
                let id = runtime::module_id(code);
                if self.runtime.modules.contains_key(&id) {
                    return Err(TxError::Runtime("módulo já publicado"));
                }
                runtime::validate_module(code).map_err(|_| {
                    TxError::Runtime("módulo recusado: formato, ponto flutuante ou importações")
                })?;
                effect.runtime = Some(RuntimeEffect::Publish {
                    id,
                    record: ModuleRecord {
                        code: code.clone(),
                        publisher: sender,
                        published_at: p.height,
                    },
                    fee: amount,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::BindModule {
                community,
                module,
                approvals,
            } => {
                let c = self.recognized(community)?;
                if let Some(m) = module {
                    if !self.runtime.modules.contains_key(m) {
                        return Err(TxError::Runtime("módulo não publicado"));
                    }
                }
                let seq = self.runtime.bindings.get(community).map_or(0, |b| b.seq);
                let payload = runtime::bind_payload(community, module, seq);
                if !c.rule.verify(p.network_id, &payload, approvals) {
                    return Err(TxError::Runtime(
                        "vinculação sem aprovação válida da regra da Comunidade",
                    ));
                }
                effect.runtime = Some(RuntimeEffect::Bind {
                    community: *community,
                    module: *module,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::CallModule {
                community,
                method,
                args,
                fuel,
            } => {
                let rp = &self.params.runtime;
                if *fuel > rp.max_call_fuel {
                    return Err(TxError::Runtime("combustível acima do limite por chamada"));
                }
                let needed = rp
                    .fuel_fee(*fuel)
                    .and_then(|f| f.checked_add(self.params.min_fee))
                    .ok_or(TxError::Overflow)?;
                if tx.body.fee < needed {
                    return Err(TxError::FeeTooLow {
                        fee: tx.body.fee,
                        min: needed,
                    });
                }
                let module = self
                    .runtime
                    .module_of(community)
                    .ok_or(TxError::Runtime("Comunidade sem módulo vinculado"))?;
                effect.runtime = Some(RuntimeEffect::Call {
                    community: *community,
                    module,
                    method: method.clone(),
                    args: args.clone(),
                    fuel: *fuel,
                    caller: sender,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::UpdateName {
                name,
                kind,
                target,
                new_owner,
            } => {
                match self.communities.names.get(&(*kind, name.clone())) {
                    None => return Err(TxError::Community("nome não registrado")),
                    Some(r) if r.owner != sender => {
                        return Err(TxError::Community("nome pertence a outra conta"))
                    }
                    Some(_) => {}
                }
                effect.community = Some(CommunityEffect::UpdateName {
                    kind: *kind,
                    name: name.clone(),
                    target: *target,
                    new_owner: *new_owner,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::DefenseTransition {
                to,
                evidence,
                seq,
                attestations,
            } => {
                let d = &self.defense;
                let dp = &self.params.defense;
                let cur = d.mode;
                let quorum = if *to < cur {
                    // Descer é fácil: uma minoria relevante encerra a
                    // emergência (REQ-059).
                    Quorum::OneThird
                } else {
                    match to {
                        DefenseMode::Normal => {
                            return Err(TxError::Defense("NORMAL não se renova"))
                        }
                        DefenseMode::Vigilance => Quorum::OneThird,
                        DefenseMode::Incident => Quorum::TwoThirds,
                        DefenseMode::CyberWar => {
                            // Só a partir de um incidente persistente
                            // (AT-CYBER-001/002).
                            let opened = d
                                .active_incident
                                .and_then(|id| d.incidents.get(&id))
                                .map(|i| i.opened_at);
                            match opened {
                                Some(at)
                                    if cur >= DefenseMode::Incident
                                        && p.height.saturating_sub(at)
                                            >= dp.war_persistence_blocks => {}
                                _ => {
                                    return Err(TxError::Defense(
                                        "guerra exige incidente aberto e persistente",
                                    ))
                                }
                            }
                            Quorum::TwoThirds
                        }
                    }
                };
                let payload = defense::payload::transition(*seq, *to, evidence);
                self.check_attested(p.network_id, *seq, &payload, attestations, quorum)?;
                let open = (*to >= DefenseMode::Incident && d.active_incident.is_none())
                    .then(|| tx.id().0);
                effect.defense = Some(DefenseEffect::Mode {
                    to: *to,
                    evidence: *evidence,
                    open,
                    height: p.height,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::IncidentUpdate {
                incident,
                status,
                evidence,
                seq,
                attestations,
            } => {
                let inc = self.active_incident(incident)?;
                let next = match inc.status {
                    IncidentStatus::Open => IncidentStatus::Contained,
                    IncidentStatus::Contained => IncidentStatus::Recovered,
                    _ => return Err(TxError::Defense("incidente não pode avançar")),
                };
                if *status != next {
                    return Err(TxError::Defense("estado fora de ordem"));
                }
                if inc.evidence.len() >= defense::MAX_EVIDENCE_PER_INCIDENT {
                    return Err(TxError::Defense("evidências demais"));
                }
                let payload = defense::payload::incident_update(*seq, incident, *status, evidence);
                self.check_attested(
                    p.network_id,
                    *seq,
                    &payload,
                    attestations,
                    Quorum::TwoThirds,
                )?;
                effect.defense = Some(DefenseEffect::Update {
                    incident: *incident,
                    status: *status,
                    evidence: *evidence,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::CloseIncident {
                incident,
                archive,
                seq,
                attestations,
            } => {
                let inc = self.active_incident(incident)?;
                // Encerramento só com contenção e recuperação atestadas
                // (`SPEC §56`, AT-INC-002).
                if inc.status != IncidentStatus::Recovered {
                    return Err(TxError::Defense("incidente ainda não contido e recuperado"));
                }
                let payload = defense::payload::close(*seq, incident, archive);
                self.check_attested(
                    p.network_id,
                    *seq,
                    &payload,
                    attestations,
                    Quorum::TwoThirds,
                )?;
                effect.defense = Some(DefenseEffect::Close {
                    incident: *incident,
                    archive: *archive,
                    height: p.height,
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::GrantCredential {
                incident,
                holder,
                scopes,
                expires_at,
                seq,
                attestations,
            } => {
                self.active_incident(incident)?;
                let dp = &self.params.defense;
                if *expires_at <= p.height
                    || *expires_at > p.height.saturating_add(dp.credential_max_blocks)
                {
                    return Err(TxError::Defense("validade fora dos limites"));
                }
                // Histórico verificável: validador vigente ou pontos de
                // contribuição suficientes (`ARCHITECTURE §38`).
                let eligible = self.validators.contains(holder)
                    || self.contribution(holder, p.height) >= dp.credential_min_contribution;
                if !eligible {
                    return Err(TxError::Defense("portador sem histórico verificável"));
                }
                let payload = defense::payload::grant(*seq, incident, holder, scopes, *expires_at);
                self.check_attested(
                    p.network_id,
                    *seq,
                    &payload,
                    attestations,
                    Quorum::TwoThirds,
                )?;
                effect.defense = Some(DefenseEffect::Grant(Box::new(Credential {
                    id: tx.id().0,
                    incident: *incident,
                    holder: *holder,
                    scopes: scopes.clone(),
                    granted_at: p.height,
                    expires_at: *expires_at,
                    revoked: false,
                })));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::RevokeCredential {
                credential,
                seq,
                attestations,
            } => {
                match self.defense.credentials.get(credential) {
                    Some(c) if !c.revoked => {}
                    _ => return Err(TxError::Defense("credencial inexistente ou já revogada")),
                }
                let payload = defense::payload::revoke(*seq, credential);
                self.check_attested(p.network_id, *seq, &payload, attestations, Quorum::OneThird)?;
                effect.defense = Some(DefenseEffect::Revoke(*credential));
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::DefenseAction {
                credential,
                scope,
                subject,
            } => {
                let c = self
                    .defense
                    .credentials
                    .get(credential)
                    .ok_or(TxError::Defense("credencial inexistente"))?;
                if c.holder != tx.body.sender {
                    return Err(TxError::Defense("credencial de outro portador"));
                }
                // Escopo, validade e revogação (AT-CYBER-003..005).
                if !c.allows(*scope, p.height) {
                    return Err(TxError::Defense(
                        "credencial vencida, revogada ou fora do escopo",
                    ));
                }
                let inc = self.active_incident(&c.incident)?;
                if inc.actions.len() >= defense::MAX_ACTIONS_PER_INCIDENT {
                    return Err(TxError::Defense("ações demais neste incidente"));
                }
                effect.defense = Some(DefenseEffect::Action {
                    incident: c.incident,
                    record: DefenseActionRecord {
                        credential: *credential,
                        holder: c.holder,
                        scope: *scope,
                        subject: *subject,
                        height: p.height,
                    },
                });
                effect.accounts.push((sender, sender_acc));
            }
            TxKind::AttestContribution {
                incident,
                node,
                role,
                evidence,
                seq,
                attestations,
            } => {
                match self.defense.incidents.get(incident) {
                    Some(i) if i.status == IncidentStatus::Closed => {}
                    _ => {
                        return Err(TxError::Defense(
                            "contribuição só é atestada após o encerramento",
                        ))
                    }
                }
                let payload = defense::payload::contribution(*seq, incident, node, *role, evidence);
                self.check_attested(
                    p.network_id,
                    *seq,
                    &payload,
                    attestations,
                    Quorum::TwoThirds,
                )?;
                effect.defense = Some(DefenseEffect::Contribution {
                    node: *node,
                    record: DefenseRecord {
                        incident: *incident,
                        role: *role,
                        evidence: *evidence,
                        height: p.height,
                    },
                });
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
            market: None,
            community: None,
            defense: None,
            runtime: None,
        })
    }
}

impl State {
    fn registered(&self, asset: &AssetId) -> Result<(), TxError> {
        if asset.is_zero() || !self.market.assets.contains_key(asset) {
            return Err(TxError::Market("ativo externo não registrado"));
        }
        Ok(())
    }

    fn add_asset(&mut self, owner: Address, asset: AssetId, amount: u64) -> Result<(), StateError> {
        let b = self.market.balances.entry((owner, asset)).or_default();
        *b = b.checked_add(amount).ok_or(StateError::Overflow)?;
        Ok(())
    }

    fn sub_asset(&mut self, owner: Address, asset: AssetId, amount: u64) -> Result<(), StateError> {
        let key = (owner, asset);
        let b = self.market.balances.get(&key).copied().unwrap_or(0);
        let left = b.checked_sub(amount).ok_or(StateError::Overflow)?;
        if left == 0 {
            self.market.balances.remove(&key);
        } else {
            self.market.balances.insert(key, left);
        }
        Ok(())
    }

    /// O Pool só cresce: esta é a única função que o altera.
    fn add_to_pool(&mut self, asset: AssetId, amount: u64) -> Result<(), StateError> {
        let v = self.market.pool.entry(asset).or_default();
        *v = v.checked_add(amount).ok_or(StateError::Overflow)?;
        Ok(())
    }

    /// Devolve o valor ainda reservado por uma ordem que sai do livro.
    fn refund_order(&mut self, o: &Order) -> Result<(), StateError> {
        match o.side {
            Side::Buy => self.credit_fees(o.owner, o.escrow),
            Side::Sell => self.add_asset(o.owner, o.asset, o.escrow),
        }
    }

    fn apply_market(&mut self, m: MarketEffect) -> Result<(), StateError> {
        match m {
            MarketEffect::Transfer {
                asset,
                from,
                to,
                amount,
            } => {
                self.sub_asset(from, asset, amount)?;
                self.add_asset(to, asset, amount)
            }
            MarketEffect::PoolDeposit {
                asset,
                from,
                amount,
            } => {
                if !asset.is_zero() {
                    self.sub_asset(from, asset, amount)?;
                }
                self.add_to_pool(asset, amount)
            }
            MarketEffect::Place(o) => {
                if o.side == Side::Sell {
                    self.sub_asset(o.owner, o.asset, o.escrow)?;
                }
                self.market.dirty.insert(o.asset);
                self.market.orders.insert(o.id, o);
                Ok(())
            }
            MarketEffect::Cancel(id) => {
                let o = self.market.orders.remove(&id).ok_or(StateError::Overflow)?;
                self.market.dirty.insert(o.asset);
                self.refund_order(&o)
            }
        }
    }

    /// Fim de bloco do Grande Mercado: ordens vencidas saem do livro com
    /// reembolso; cada livro alterado passa pelo leilão de preço uniforme.
    fn settle_market(&mut self, height: u64) -> Result<(), StateError> {
        let expired: Vec<Hash32> = self
            .market
            .orders
            .values()
            .filter(|o| o.expires_at <= height)
            .map(|o| o.id)
            .collect();
        for id in expired {
            if let Some(o) = self.market.orders.remove(&id) {
                self.refund_order(&o)?;
            }
        }
        let dirty = std::mem::take(&mut self.market.dirty);
        let fee_bps = self.params.market.fee_bps as u128;
        for asset in dirty {
            let auction = market::clear(self.market.orders.values().filter(|o| o.asset == asset));
            let Some(price) = auction.price else {
                continue;
            };
            for f in auction.fills {
                // Comprador: recebe o ativo, paga do valor reservado.
                let buyer = {
                    let o = self
                        .market
                        .orders
                        .get_mut(&f.buy)
                        .ok_or(StateError::Overflow)?;
                    o.remaining = o
                        .remaining
                        .checked_sub(f.base)
                        .ok_or(StateError::Overflow)?;
                    o.escrow = o.escrow.checked_sub(f.quote).ok_or(StateError::Overflow)?;
                    o.owner
                };
                self.add_asset(buyer, asset, f.base)?;
                // Vendedor: entrega o ativo reservado, recebe ZERO menos a
                // tarifa, que vai para o Pool permanente.
                let seller = {
                    let o = self
                        .market
                        .orders
                        .get_mut(&f.sell)
                        .ok_or(StateError::Overflow)?;
                    o.remaining = o
                        .remaining
                        .checked_sub(f.base)
                        .ok_or(StateError::Overflow)?;
                    o.escrow = o.escrow.checked_sub(f.base).ok_or(StateError::Overflow)?;
                    o.owner
                };
                let fee = (f.quote as u128 * fee_bps / 10_000) as u64;
                self.credit_fees(seller, f.quote - fee)?;
                if fee > 0 {
                    self.add_to_pool(AssetId::ZERO, fee)?;
                }
            }
            // Executadas por completo, ou com restante de valor nulo ao
            // próprio limite (poeira que nunca executaria).
            let done: Vec<Hash32> = self
                .market
                .orders
                .values()
                .filter(|o| o.asset == asset && (o.remaining == 0 || market::is_dust(o)))
                .map(|o| o.id)
                .collect();
            for id in done {
                if let Some(o) = self.market.orders.remove(&id) {
                    // Sobra de reserva de compras executadas abaixo do limite.
                    self.refund_order(&o)?;
                }
            }
            self.market.last_price.insert(asset, price);
        }
        Ok(())
    }
}

impl State {
    fn recognized(&self, id: &Hash32) -> Result<&Community, TxError> {
        match self.communities.communities.get(id) {
            Some(c) if c.status == CommunityStatus::Recognized => Ok(c),
            _ => Err(TxError::Community("Comunidade não reconhecida")),
        }
    }

    fn apply_community(&mut self, e: CommunityEffect) -> Result<(), StateError> {
        let cs = &mut self.communities;
        match e {
            CommunityEffect::Declare(c) => {
                cs.communities.insert(c.id, *c);
            }
            CommunityEffect::Position {
                proposal,
                community,
                choice,
            } => {
                cs.positions.insert((proposal, community), choice);
            }
            CommunityEffect::Update {
                community,
                manifest_hash,
                version,
                rule,
                height,
            } => {
                let c = cs
                    .communities
                    .get_mut(&community)
                    .ok_or(StateError::Overflow)?;
                c.manifest_hash = manifest_hash;
                c.version = version;
                if let Some(r) = rule {
                    c.rule = r;
                }
                c.history.push((version, manifest_hash, height));
                if c.history.len() > community::MAX_VERSION_HISTORY {
                    c.history.remove(0);
                }
            }
            CommunityEffect::RegisterName {
                kind,
                name,
                record,
                fee,
            } => {
                cs.names.insert((kind, name), record);
                self.add_to_pool(AssetId::ZERO, fee)?;
            }
            CommunityEffect::UpdateName {
                kind,
                name,
                target,
                new_owner,
            } => {
                let r = cs
                    .names
                    .get_mut(&(kind, name))
                    .ok_or(StateError::Overflow)?;
                r.target = target;
                if let Some(o) = new_owner {
                    r.owner = o;
                }
            }
        }
        Ok(())
    }
}

impl State {
    fn check_attested(
        &self,
        network_id: &str,
        seq: u64,
        payload: &[u8],
        attestations: &[defense::Attestation],
        quorum: Quorum,
    ) -> Result<(), TxError> {
        if seq != self.defense.seq {
            return Err(TxError::Defense("sequência de defesa desatualizada"));
        }
        // `validators` é o conjunto do próximo bloco, o vigente para esta
        // transação.
        let power = defense::attested_power(network_id, payload, attestations, &self.validators);
        match power {
            Some(p) if defense::meets(quorum, p, &self.validators) => Ok(()),
            Some(_) => Err(TxError::Defense("atestações abaixo do limiar")),
            None => Err(TxError::Defense("atestação inválida")),
        }
    }

    fn active_incident(&self, id: &Hash32) -> Result<&Incident, TxError> {
        match (self.defense.active_incident, self.defense.incidents.get(id)) {
            (Some(a), Some(i)) if a == *id && i.status.is_active() => Ok(i),
            _ => Err(TxError::Defense("incidente não está em vigor")),
        }
    }

    fn mode_duration(&self, mode: DefenseMode) -> u64 {
        let dp = &self.params.defense;
        match mode {
            DefenseMode::Normal => 0,
            DefenseMode::Vigilance => dp.vigilance_max_blocks,
            DefenseMode::Incident => dp.incident_max_blocks,
            DefenseMode::CyberWar => dp.war_max_blocks,
        }
    }

    /// Muda o modo; ao sair do nível de incidente, o incidente em vigor
    /// sai de vigor (`Lapsed`) e suas credenciais são revogadas.
    fn set_mode(&mut self, to: DefenseMode, height: u64) {
        if to < DefenseMode::Incident {
            if let Some(id) = self.defense.active_incident.take() {
                if let Some(i) = self.defense.incidents.get_mut(&id) {
                    if i.status.is_active() {
                        i.status = IncidentStatus::Lapsed;
                        i.closed_at = Some(height);
                    }
                }
                self.revoke_incident_credentials(&id);
            }
        }
        if to != self.defense.mode {
            self.defense.mode_since = height;
        }
        self.defense.mode = to;
        self.defense.mode_expires_at = match to {
            DefenseMode::Normal => 0,
            m => height.saturating_add(self.mode_duration(m)),
        };
    }

    fn revoke_incident_credentials(&mut self, incident: &Hash32) {
        for c in self.defense.credentials.values_mut() {
            if c.incident == *incident {
                c.revoked = true;
            }
        }
    }

    fn apply_defense(&mut self, e: DefenseEffect) {
        let attested = !matches!(e, DefenseEffect::Action { .. });
        match e {
            DefenseEffect::Mode {
                to,
                evidence,
                open,
                height,
            } => {
                self.defense.mode_evidence.push((height, to, evidence));
                if let Some(id) = open {
                    self.defense.incidents.insert(
                        id,
                        Incident {
                            id,
                            opened_at: height,
                            status: IncidentStatus::Open,
                            evidence: vec![evidence],
                            actions: Vec::new(),
                            closed_at: None,
                        },
                    );
                    self.defense.active_incident = Some(id);
                } else if to >= DefenseMode::Incident {
                    if let Some(i) = self
                        .defense
                        .active_incident
                        .and_then(|id| self.defense.incidents.get_mut(&id))
                    {
                        if i.evidence.len() < defense::MAX_EVIDENCE_PER_INCIDENT {
                            i.evidence.push(evidence);
                        }
                    }
                }
                self.set_mode(to, height);
            }
            DefenseEffect::Update {
                incident,
                status,
                evidence,
            } => {
                if let Some(i) = self.defense.incidents.get_mut(&incident) {
                    i.status = status;
                    i.evidence.push(evidence);
                }
            }
            DefenseEffect::Close {
                incident,
                archive,
                height,
            } => {
                if let Some(i) = self.defense.incidents.get_mut(&incident) {
                    i.status = IncidentStatus::Closed;
                    i.closed_at = Some(height);
                    // O pacote de evidências fica preservado (AC-DEF-010).
                    i.evidence.push(archive);
                }
                self.defense.active_incident = None;
                self.revoke_incident_credentials(&incident);
                // Retorno gradual: encerrado o incidente, a rede fica em
                // vigilância até vencer ou ser rebaixada (REQ-063).
                self.set_mode(DefenseMode::Vigilance, height);
            }
            DefenseEffect::Grant(c) => {
                self.defense.credentials.insert(c.id, *c);
            }
            DefenseEffect::Revoke(id) => {
                if let Some(c) = self.defense.credentials.get_mut(&id) {
                    c.revoked = true;
                }
            }
            DefenseEffect::Action { incident, record } => {
                if let Some(i) = self.defense.incidents.get_mut(&incident) {
                    i.actions.push(record);
                }
            }
            DefenseEffect::Contribution { node, record } => {
                self.defense.records.entry(node).or_default().push(record);
            }
        }
        if attested {
            self.defense.seq += 1;
        }
    }

    /// Fim de bloco: modo vencido desce um nível; credenciais vencidas
    /// simplesmente deixam de autorizar (`Credential::allows`).
    fn apply_runtime(
        &mut self,
        e: RuntimeEffect,
        tx: Hash32,
        height: u64,
    ) -> Result<(), StateError> {
        match e {
            RuntimeEffect::Publish { id, record, fee } => {
                self.runtime.modules.insert(id, record);
                self.add_to_pool(AssetId::ZERO, fee)?;
            }
            RuntimeEffect::Bind { community, module } => {
                let b = self.runtime.bindings.entry(community).or_default();
                b.module = module;
                b.seq = b.seq.saturating_add(1);
            }
            RuntimeEffect::Call {
                community,
                module,
                method,
                args,
                fuel,
                caller,
            } => {
                let Some(code) = self.runtime.modules.get(&module).map(|m| m.code.clone()) else {
                    return Ok(());
                };
                let o = runtime::execute(
                    &module,
                    &code,
                    self.runtime.storage.get(&community),
                    &runtime::Call {
                        community,
                        caller,
                        height,
                        method: &method,
                        input: &args,
                        fuel,
                        quota: self.params.runtime.storage_quota,
                    },
                );
                if o.ok {
                    self.runtime.commit(community, o.writes);
                }
                let mut output = o.output;
                output.truncate(runtime::MAX_RECEIPT_OUTPUT);
                self.runtime.push_receipt(Receipt {
                    tx,
                    community,
                    method,
                    ok: o.ok,
                    fuel_used: o.fuel_used,
                    output,
                    error: o.error,
                    height,
                });
            }
        }
        Ok(())
    }

    /// Consulta somente leitura ao módulo de uma Comunidade (interfaces).
    /// Nada é gravado.
    pub fn query_module(
        &self,
        community: &Hash32,
        method: &str,
        input: &[u8],
        caller: Address,
        height: u64,
        fuel: u64,
    ) -> Result<runtime::Outcome, &'static str> {
        runtime::check_method(method)?;
        let module = self
            .runtime
            .module_of(community)
            .ok_or("Comunidade sem módulo vinculado")?;
        let code = &self
            .runtime
            .modules
            .get(&module)
            .ok_or("módulo ausente")?
            .code;
        Ok(runtime::execute(
            &module,
            code,
            self.runtime.storage.get(community),
            &runtime::Call {
                community: *community,
                caller,
                height,
                method,
                input,
                fuel,
                quota: self.params.runtime.storage_quota,
            },
        ))
    }

    fn expire_defense(&mut self, height: u64) {
        let d = &self.defense;
        if d.mode != DefenseMode::Normal && height >= d.mode_expires_at {
            let to = d.mode.step_down();
            self.set_mode(to, height);
        }
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
