//! Máquina de estados do consenso Zero-BFT (ADR-0012, `spec/CONSENSUS.md`).
//!
//! Implementa o algoritmo do Tendermint ("The latest gossip on BFT consensus",
//! Buchman, Kwon, Milosevic, 2018) sobre poder de voto proporcional ao ZERO
//! bloqueado:
//!
//! * **Segurança:** com menos de 1/3 do poder bizantino, dois blocos
//!   diferentes nunca são finalizados na mesma altura (regra de travamento).
//! * **Vivacidade:** após estabilização da rede, com mais de 2/3 do poder
//!   honesto e online, algum bloco é finalizado (temporizadores crescentes).
//! * **Finalidade imediata:** um bloco com pré-compromissos de mais de 2/3
//!   do poder é final; não há reorganização.
//!
//! A máquina não faz E/S: recebe eventos e devolve [`Output`]s. O Node
//! transporta mensagens, agenda temporizadores e fornece [`App`].

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rz_core::{
    Block, BlockId, Commit, ConsensusParams, Proposal, SignedProposal, ValidatorSet, Vote, VoteType,
};
use rz_crypto::{PublicKey, SecretKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    Propose,
    Prevote,
    Precommit,
    Commit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timeout {
    pub height: u64,
    pub round: u32,
    pub step: Step,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    BroadcastProposal(Box<Proposal>),
    BroadcastVote(Vote),
    /// Agendar `Timeout` após o atraso em milissegundos.
    ScheduleTimeout(Timeout, u64),
    /// Bloco finalizado com seu certificado.
    Decide(Box<Block>, Commit),
    /// Dupla assinatura de votos (evidência para `ReportDoubleVote`).
    VoteEvidence(Box<Vote>, Box<Vote>),
    /// Duas propostas distintas do mesmo proponente para a mesma rodada
    /// (evidência para `ReportEquivocation`).
    ProposalEvidence(Box<SignedProposal>, Box<SignedProposal>),
}

/// Serviços que o Node fornece à máquina de consenso.
pub trait App {
    /// Monta um bloco novo para a altura e rodada.
    fn build_block(&mut self, height: u64, round: u32) -> Option<Block>;
    /// O bloco é válido como próximo bloco da cadeia?
    fn validate_block(&mut self, block: &Block) -> bool;
}

/// Resultado de processar uma mensagem.
#[derive(Debug, Default)]
pub struct Reaction {
    /// A mensagem era nova e válida (deve ser repassada aos pares).
    pub relay: bool,
    pub outputs: Vec<Output>,
}

/// Mensagens para alturas futuras guardadas até a altura começar.
const MAX_FUTURE_MSGS: usize = 4_096;

/// Quantas rodadas à frente da atual uma proposta é guardada, e até que
/// rodada mensagens da altura seguinte são guardadas. Mensagens além disso
/// são descartadas sem repasse; a retransmissão periódica as entrega de novo
/// quando a rodada chegar. Limita a memória que um validador bizantino pode
/// ocupar assinando mensagens para rodadas arbitrárias.
pub const FUTURE_ROUND_WINDOW: u32 = 2;

/// Votos de um validador numa rodada e tipo: o primeiro recebido e, se
/// houver, um conflitante. Guardar o conflitante permite completar a prova
/// de > 2/3 para qualquer bloco em que um bizantino votou (ele conta para
/// cada lado). Isso não afeta a segurança: dois quóruns de > 2/3 se cruzam
/// em > 1/3 do poder, que sempre inclui um honesto, e honestos votam uma vez.
#[derive(Clone, Debug)]
struct VoteSlot {
    first: Vote,
    conflict: Option<Vote>,
}

impl VoteSlot {
    fn iter(&self) -> impl Iterator<Item = &Vote> {
        std::iter::once(&self.first).chain(self.conflict.as_ref())
    }

    fn voted_for(&self, block: Option<BlockId>) -> bool {
        self.iter().any(|v| v.block == block)
    }
}

enum Pending {
    Proposal(Box<Proposal>),
    Vote(Vote),
}

pub struct Bft {
    network_id: String,
    key: Option<SecretKey>,
    params: ConsensusParams,

    validators: ValidatorSet,
    prev_block: BlockId,
    height: u64,
    round: u32,
    step: Step,
    locked: Option<(u32, Block)>,
    valid: Option<(u32, Block)>,
    decided: bool,

    proposals: BTreeMap<u32, Proposal>,
    votes: HashMap<(u32, VoteType), BTreeMap<PublicKey, VoteSlot>>,
    prevote_timeout_set: BTreeSet<u32>,
    precommit_timeout_set: BTreeSet<u32>,
    polka_handled: BTreeSet<u32>,
    validity: HashMap<BlockId, bool>,
    future: Vec<Pending>,
}

impl Bft {
    pub fn new(network_id: &str, key: Option<SecretKey>, params: ConsensusParams) -> Self {
        Self {
            network_id: network_id.to_owned(),
            key,
            params,
            validators: ValidatorSet::default(),
            prev_block: BlockId::default(),
            height: 0,
            round: 0,
            step: Step::Commit,
            locked: None,
            valid: None,
            decided: true,
            proposals: BTreeMap::new(),
            votes: HashMap::new(),
            prevote_timeout_set: BTreeSet::new(),
            precommit_timeout_set: BTreeSet::new(),
            polka_handled: BTreeSet::new(),
            validity: HashMap::new(),
            future: Vec::new(),
        }
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn round(&self) -> u32 {
        self.round
    }

    pub fn step(&self) -> Step {
        self.step
    }

    pub fn validators(&self) -> &ValidatorSet {
        &self.validators
    }

    pub fn set_params(&mut self, params: ConsensusParams) {
        self.params = params;
    }

    fn me(&self) -> Option<PublicKey> {
        self.key
            .as_ref()
            .map(SecretKey::public_key)
            .filter(|k| self.validators.contains(k))
    }

    /// Inicia uma nova altura sobre `prev_block`, com o conjunto vigente.
    pub fn start_height(
        &mut self,
        height: u64,
        prev_block: BlockId,
        validators: ValidatorSet,
        app: &mut dyn App,
    ) -> Vec<Output> {
        self.height = height;
        self.prev_block = prev_block;
        self.validators = validators;
        self.round = 0;
        self.locked = None;
        self.valid = None;
        self.decided = false;
        self.proposals.clear();
        self.votes.clear();
        self.prevote_timeout_set.clear();
        self.precommit_timeout_set.clear();
        self.polka_handled.clear();
        self.validity.clear();

        let mut out = Vec::new();
        self.start_round(0, app, &mut out);
        // Reprocessa mensagens que chegaram antes desta altura.
        let pending = std::mem::take(&mut self.future);
        for m in pending {
            let r = match m {
                Pending::Proposal(p) => self.on_proposal(*p, app),
                Pending::Vote(v) => self.on_vote(v, app),
            };
            out.extend(r.outputs);
        }
        out
    }

    fn timeout_ms(&self, base: u64, round: u32) -> u64 {
        base.saturating_add(self.params.timeout_delta_ms.saturating_mul(round as u64))
    }

    fn start_round(&mut self, round: u32, app: &mut dyn App, out: &mut Vec<Output>) {
        self.round = round;
        self.step = Step::Propose;
        let proposer = self
            .validators
            .proposer(&self.prev_block, self.height, round)
            .copied();
        if proposer.is_some() && proposer == self.me() {
            let (block, pol_round) = match &self.valid {
                Some((vr, b)) => (Some(b.clone()), Some(*vr)),
                None => (app.build_block(self.height, round), None),
            };
            if let (Some(block), Some(key)) = (block, &self.key) {
                let p = Proposal::sign(self.height, round, pol_round, block, key, &self.network_id);
                out.push(Output::BroadcastProposal(Box::new(p.clone())));
                self.proposals.entry(round).or_insert(p);
            }
        }
        out.push(Output::ScheduleTimeout(
            Timeout {
                height: self.height,
                round,
                step: Step::Propose,
            },
            self.timeout_ms(self.params.timeout_propose_ms, round),
        ));
        self.process(app, out);
    }

    /// Proposta recebida da rede.
    pub fn on_proposal(&mut self, p: Proposal, app: &mut dyn App) -> Reaction {
        let mut r = Reaction::default();
        if p.height == self.height + 1 {
            // A altura seguinte ainda não tem conjunto conhecido: usa o atual
            // como filtro (quem entra na época seguinte é recuperado pela
            // retransmissão).
            if self.future.len() < MAX_FUTURE_MSGS
                && p.round <= FUTURE_ROUND_WINDOW
                && self.validators.contains(&p.proposer)
                && p.verify(&self.network_id)
            {
                self.future.push(Pending::Proposal(Box::new(p)));
                r.relay = true;
            }
            return r;
        }
        if p.height != self.height
            || self.decided
            || p.round > self.round.saturating_add(FUTURE_ROUND_WINDOW)
        {
            return r;
        }
        if !p.verify(&self.network_id)
            || self
                .validators
                .proposer(&self.prev_block, p.height, p.round)
                != Some(&p.proposer)
            || p.block.header.height != self.height
            || p.block.header.parent != self.prev_block
            || p.pol_round.is_some_and(|vr| vr >= p.round)
        {
            return r;
        }
        match self.proposals.get(&p.round) {
            Some(existing) if existing.signed() == p.signed() => return r,
            Some(existing) => {
                // Proponente assinou duas propostas diferentes (bloco ou
                // `pol_round`) para a mesma rodada.
                r.outputs.push(Output::ProposalEvidence(
                    Box::new(existing.signed()),
                    Box::new(p.signed()),
                ));
                return r;
            }
            None => {}
        }
        self.proposals.insert(p.round, p);
        r.relay = true;
        self.process(app, &mut r.outputs);
        r
    }

    /// Voto recebido da rede.
    pub fn on_vote(&mut self, v: Vote, app: &mut dyn App) -> Reaction {
        let mut r = Reaction::default();
        if v.height == self.height + 1 {
            if self.future.len() < MAX_FUTURE_MSGS
                && v.round <= FUTURE_ROUND_WINDOW
                && self.validators.contains(&v.validator)
                && v.verify(&self.network_id)
            {
                self.future.push(Pending::Vote(v));
                r.relay = true;
            }
            return r;
        }
        if v.height != self.height || !self.validators.contains(&v.validator) {
            return r;
        }
        if !v.verify(&self.network_id) {
            return r;
        }
        if v.round > self.round && !self.keep_future_vote(&v) {
            return r;
        }
        let set = self.votes.entry((v.round, v.kind)).or_default();
        match set.get_mut(&v.validator) {
            Some(slot) if slot.voted_for(v.block) => return r,
            Some(slot) if slot.conflict.is_none() => {
                // Dupla assinatura: evidência, e o voto conflitante também é
                // guardado e repassado (ver `VoteSlot`).
                r.outputs.push(Output::VoteEvidence(
                    Box::new(slot.first.clone()),
                    Box::new(v.clone()),
                ));
                slot.conflict = Some(v);
            }
            // Terceiro voto distinto: nada a acrescentar.
            Some(_) => return r,
            None => {
                set.insert(
                    v.validator,
                    VoteSlot {
                        first: v,
                        conflict: None,
                    },
                );
            }
        }
        r.relay = true;
        self.process(app, &mut r.outputs);
        r
    }

    /// Rodadas futuras: guarda só o voto de rodada mais alta de cada
    /// validador e tipo, o que basta para o salto de rodada (> 1/3 numa
    /// rodada à frente) e limita a memória a um voto futuro por validador.
    /// Retorna `false` se já há um voto futuro mais alto (descartar `v`).
    fn keep_future_vote(&mut self, v: &Vote) -> bool {
        let current = self.round;
        let older: Vec<u32> = self
            .votes
            .iter()
            .filter(|((rr, kind), set)| {
                *rr > current && *kind == v.kind && *rr != v.round && set.contains_key(&v.validator)
            })
            .map(|((rr, _), _)| *rr)
            .collect();
        if older.iter().any(|rr| *rr > v.round) {
            return false;
        }
        for rr in older {
            if let Some(set) = self.votes.get_mut(&(rr, v.kind)) {
                set.remove(&v.validator);
                if set.is_empty() {
                    self.votes.remove(&(rr, v.kind));
                }
            }
        }
        true
    }

    /// Resumo do estado interno (diagnóstico em testes).
    #[cfg(test)]
    pub(crate) fn debug_state(&self) -> String {
        let short = |b: &Block| format!("{}", b.id().0).chars().take(6).collect::<String>();
        let props: Vec<String> = self
            .proposals
            .iter()
            .map(|(r, p)| {
                format!(
                    "r{r}:{}{}",
                    short(&p.block),
                    p.pol_round.map(|v| format!("/pol{v}")).unwrap_or_default()
                )
            })
            .collect();
        let pv: Vec<String> = self
            .votes
            .iter()
            .filter(|((r, _), _)| *r + 3 >= self.round)
            .map(|((r, k), set)| {
                let blocks: Vec<String> = set
                    .values()
                    .flat_map(VoteSlot::iter)
                    .map(|v| {
                        v.block
                            .map(|b| format!("{}", b.0).chars().take(6).collect())
                            .unwrap_or("nil".into())
                    })
                    .collect();
                format!("r{r}{:?}:{}", k, blocks.join(","))
            })
            .collect();
        format!(
            "locked={:?} valid={:?} props=[{}] votes=[{}]",
            self.locked.as_ref().map(|(r, b)| (r, short(b))),
            self.valid.as_ref().map(|(r, b)| (r, short(b))),
            props.join(" "),
            pv.join(" ")
        )
    }

    /// Quantidade de votos guardados (diagnóstico e testes de limite).
    pub fn stored_votes(&self) -> usize {
        self.votes
            .values()
            .flat_map(BTreeMap::values)
            .map(|s| s.iter().count())
            .sum()
    }

    /// Quantidade de propostas guardadas na altura atual.
    pub fn stored_proposals(&self) -> usize {
        self.proposals.len()
    }

    /// Quantidade de mensagens guardadas para a altura seguinte.
    pub fn stored_future(&self) -> usize {
        self.future.len()
    }

    /// Mensagens da rodada atual para retransmissão periódica.
    ///
    /// O Tendermint assume entrega eventual de todas as mensagens após a
    /// estabilização da rede. Numa rede real, mensagens se perdem (partições,
    /// desconexões): retransmitir a proposta e os votos conhecidos da rodada
    /// atual garante que Nodes reconectados voltem a progredir.
    pub fn retransmit(&self) -> Vec<Output> {
        let mut out = Vec::new();
        if self.decided {
            return out;
        }
        if let Some(p) = self.proposals.get(&self.round) {
            out.push(Output::BroadcastProposal(Box::new(p.clone())));
        }
        // Os próprios votos da rodada atual: cada validador retransmite os seus.
        if let Some(me) = self.me() {
            for kind in [VoteType::Prevote, VoteType::Precommit] {
                if let Some(slot) = self.votes.get(&(self.round, kind)).and_then(|s| s.get(&me)) {
                    out.push(Output::BroadcastVote(slot.first.clone()));
                }
            }
        }
        // Prova do bloco válido: os pré-votos (> 2/3) da rodada em que ele foi
        // validado. Sem eles, quem os perdeu não aceita a re-proposta com
        // `pol_round` e Nodes travados nesse bloco votariam nulo para sempre
        // (gossip de votos da rodada de trava, como no Tendermint).
        if let Some((vr, block)) = &self.valid {
            if *vr != self.round {
                let id = block.id();
                if let Some(set) = self.votes.get(&(*vr, VoteType::Prevote)) {
                    out.extend(
                        set.values()
                            .flat_map(VoteSlot::iter)
                            .filter(|v| v.block == Some(id))
                            .map(|v| Output::BroadcastVote(v.clone())),
                    );
                }
            }
        }
        out
    }

    pub fn on_timeout(&mut self, t: Timeout, app: &mut dyn App) -> Vec<Output> {
        let mut out = Vec::new();
        if t.height != self.height || self.decided {
            return out;
        }
        match t.step {
            Step::Propose if t.round == self.round && self.step == Step::Propose => {
                self.cast(VoteType::Prevote, None, &mut out);
                self.step = Step::Prevote;
            }
            Step::Prevote if t.round == self.round && self.step == Step::Prevote => {
                self.cast(VoteType::Precommit, None, &mut out);
                self.step = Step::Precommit;
            }
            Step::Precommit if t.round == self.round => {
                self.start_round(self.round + 1, app, &mut out);
                return out;
            }
            _ => return out,
        }
        self.process(app, &mut out);
        out
    }

    fn cast(&mut self, kind: VoteType, block: Option<BlockId>, out: &mut Vec<Output>) {
        let (Some(key), Some(_)) = (&self.key, self.me()) else {
            return;
        };
        let v = Vote::sign(kind, self.height, self.round, block, key, &self.network_id);
        self.votes.entry((self.round, kind)).or_default().insert(
            v.validator,
            VoteSlot {
                first: v.clone(),
                conflict: None,
            },
        );
        out.push(Output::BroadcastVote(v));
    }

    fn power(&self, round: u32, kind: VoteType, block: Option<Option<BlockId>>) -> u128 {
        // Cada validador conta no máximo uma vez por bloco (e uma vez no
        // total, quando `block` é `None`).
        self.votes.get(&(round, kind)).map_or(0, |set| {
            set.iter()
                .filter(|(_, slot)| block.is_none_or(|b| slot.voted_for(b)))
                .map(|(k, _)| self.validators.power_of(k) as u128)
                .sum()
        })
    }

    fn polka(&self, round: u32, block: Option<BlockId>) -> bool {
        self.validators
            .is_supermajority(self.power(round, VoteType::Prevote, Some(block)))
    }

    fn valid_block(&mut self, block: &Block, app: &mut dyn App) -> bool {
        let id = block.id();
        if let Some(v) = self.validity.get(&id) {
            return *v;
        }
        let ok = app.validate_block(block);
        self.validity.insert(id, ok);
        ok
    }

    /// Avalia as regras "upon" até não haver mais mudanças.
    fn process(&mut self, app: &mut dyn App, out: &mut Vec<Output>) {
        for _ in 0..16 {
            if self.decided || !self.step_once(app, out) {
                return;
            }
        }
    }

    fn step_once(&mut self, app: &mut dyn App, out: &mut Vec<Output>) -> bool {
        let r = self.round;

        // Decisão: proposta com mais de 2/3 de pré-compromissos em qualquer rodada.
        let decisions: Vec<(u32, Block)> = self
            .proposals
            .iter()
            .filter(|(rr, p)| {
                self.validators.is_supermajority(self.power(
                    **rr,
                    VoteType::Precommit,
                    Some(Some(p.block.id())),
                ))
            })
            .map(|(rr, p)| (*rr, p.block.clone()))
            .collect();
        for (rr, block) in decisions {
            if self.valid_block(&block, app) {
                let id = block.id();
                let votes: Vec<&Vote> = self
                    .votes
                    .get(&(rr, VoteType::Precommit))
                    .map(|s| {
                        s.values()
                            .flat_map(VoteSlot::iter)
                            .filter(|v| v.block == Some(id))
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(commit) = Commit::from_votes(votes) {
                    self.decided = true;
                    self.step = Step::Commit;
                    out.push(Output::Decide(Box::new(block), commit));
                    return false;
                }
            }
        }

        // Avança de rodada ao ver mais de 1/3 do poder numa rodada futura.
        let future_round = self
            .votes
            .keys()
            .map(|(rr, _)| *rr)
            .filter(|rr| *rr > r)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .find(|rr| {
                let mut who: BTreeSet<PublicKey> = BTreeSet::new();
                for kind in [VoteType::Prevote, VoteType::Precommit] {
                    if let Some(s) = self.votes.get(&(*rr, kind)) {
                        who.extend(s.keys().copied());
                    }
                }
                let p: u128 = who
                    .iter()
                    .map(|k| self.validators.power_of(k) as u128)
                    .sum();
                self.validators.exceeds_one_third(p)
            });
        if let Some(rr) = future_round {
            self.start_round(rr, app, out);
            return true;
        }

        let proposal = self.proposals.get(&r).cloned();

        if self.step == Step::Propose {
            if let Some(p) = &proposal {
                let id = p.block.id();
                match p.pol_round {
                    None => {
                        let ok = self.valid_block(&p.block, app)
                            && self.locked.as_ref().is_none_or(|(_, b)| b.id() == id);
                        self.cast(VoteType::Prevote, ok.then_some(id), out);
                        self.step = Step::Prevote;
                        return true;
                    }
                    Some(vr) if vr < r && self.polka(vr, Some(id)) => {
                        let ok = self.valid_block(&p.block, app)
                            && self
                                .locked
                                .as_ref()
                                .is_none_or(|(lr, b)| *lr <= vr || b.id() == id);
                        self.cast(VoteType::Prevote, ok.then_some(id), out);
                        self.step = Step::Prevote;
                        return true;
                    }
                    Some(_) => {}
                }
            }
        }

        if self.step == Step::Prevote
            && !self.prevote_timeout_set.contains(&r)
            && self
                .validators
                .is_supermajority(self.power(r, VoteType::Prevote, None))
        {
            self.prevote_timeout_set.insert(r);
            out.push(Output::ScheduleTimeout(
                Timeout {
                    height: self.height,
                    round: r,
                    step: Step::Prevote,
                },
                self.timeout_ms(self.params.timeout_prevote_ms, r),
            ));
        }

        if let Some(p) = &proposal {
            let id = p.block.id();
            if self.step >= Step::Prevote
                && !self.polka_handled.contains(&r)
                && self.polka(r, Some(id))
                && self.valid_block(&p.block, app)
            {
                self.polka_handled.insert(r);
                if self.step == Step::Prevote {
                    self.locked = Some((r, p.block.clone()));
                    self.cast(VoteType::Precommit, Some(id), out);
                    self.step = Step::Precommit;
                }
                self.valid = Some((r, p.block.clone()));
                return true;
            }
        }

        if self.step == Step::Prevote && self.polka(r, None) {
            self.cast(VoteType::Precommit, None, out);
            self.step = Step::Precommit;
            return true;
        }

        if !self.precommit_timeout_set.contains(&r)
            && self
                .validators
                .is_supermajority(self.power(r, VoteType::Precommit, None))
        {
            self.precommit_timeout_set.insert(r);
            out.push(Output::ScheduleTimeout(
                Timeout {
                    height: self.height,
                    round: r,
                    step: Step::Precommit,
                },
                self.timeout_ms(self.params.timeout_precommit_ms, r),
            ));
        }
        false
    }
}
