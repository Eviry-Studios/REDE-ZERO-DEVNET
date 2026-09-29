//! Testes do Zero-BFT com rede simulada determinística (`SPEC §20`):
//! validadores honestos, offline, bizantinos e partições.

use std::collections::{BTreeMap, VecDeque};

use rz_core::{
    Allocation, Block, BlockId, Commit, ConsensusParams, Genesis, GenesisValidator,
    GovernanceParams, NetworkKind, Proposal, Vote, PROTOCOL_VERSION,
};
use rz_crypto::SecretKey;

use crate::bft::{App, Bft, Output, Timeout};
use crate::chain::Chain;

const NET: &str = "rede-zero-devnet-bft";

fn keys(n: u8) -> Vec<SecretKey> {
    (1..=n).map(|i| SecretKey::from_seed([i; 32])).collect()
}

fn genesis(ks: &[SecretKey]) -> Genesis {
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: NET.into(),
        consensus: ConsensusParams::fast(100),
        min_fee: 1,
        max_block_txs: 100,
        validators: ks
            .iter()
            .map(|k| GenesisValidator::new(k.public_key(), 100))
            .collect(),
        allocations: vec![Allocation {
            address: SecretKey::from_seed([200; 32]).public_key().address(),
            amount: 1_000,
        }],
        governance: GovernanceParams::default(),
    }
}

struct NodeApp<'a> {
    chain: &'a Chain,
    key: &'a SecretKey,
}

impl App for NodeApp<'_> {
    fn build_block(&mut self, _height: u64, round: u32) -> Option<Block> {
        let state = self.chain.state();
        Block::build(
            self.chain.genesis(),
            self.chain.tip(),
            self.chain.height(),
            &state,
            round as u64,
            vec![],
            self.key,
        )
        .ok()
        .map(|(b, _)| b)
    }

    fn validate_block(&mut self, block: &Block) -> bool {
        self.chain.check_next(block).is_ok()
    }
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)] // apenas no simulador de testes
enum Msg {
    Proposal(Proposal),
    Vote(Vote),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Behavior {
    Honest,
    Offline,
    /// Envia, junto com cada voto, um voto conflitante (dupla assinatura).
    DoubleVoter,
}

struct Node {
    bft: Bft,
    chain: Chain,
    key: SecretKey,
    behavior: Behavior,
    decisions: Vec<(u64, BlockId)>,
    vote_evidence: usize,
}

struct Sim {
    nodes: Vec<Node>,
    queue: VecDeque<(usize, Msg)>,
    timers: Vec<(u64, usize, Timeout)>,
    now: u64,
    /// `partition[i]` = grupo do Node `i`; mensagens só circulam no grupo.
    partition: Vec<u8>,
    /// Nodes que decidiram e devem iniciar a próxima altura.
    pending_start: Vec<usize>,
    target: u64,
    last_retransmit: u64,
}

impl Sim {
    fn new(n: u8, behaviors: &[(usize, Behavior)]) -> Self {
        let ks = keys(n);
        let g = genesis(&ks);
        let mut nodes: Vec<Node> = ks
            .iter()
            .map(|k| Node {
                bft: Bft::new(
                    NET,
                    Some(SecretKey::from_seed(k.seed())),
                    g.consensus.clone(),
                ),
                chain: Chain::new(g.clone()).unwrap(),
                key: SecretKey::from_seed(k.seed()),
                behavior: Behavior::Honest,
                decisions: vec![],
                vote_evidence: 0,
            })
            .collect();
        for (i, b) in behaviors {
            nodes[*i].behavior = *b;
        }
        let mut sim = Self {
            nodes,
            queue: VecDeque::new(),
            timers: vec![],
            now: 0,
            partition: vec![0; n as usize],
            pending_start: vec![],
            target: u64::MAX,
            last_retransmit: 0,
        };
        for i in 0..sim.nodes.len() {
            sim.start_height(i);
        }
        sim
    }

    fn start_height(&mut self, i: usize) {
        if self.nodes[i].behavior == Behavior::Offline {
            return;
        }
        let node = &mut self.nodes[i];
        let h = node.chain.height() + 1;
        let tip = node.chain.tip();
        let set = node.chain.state().validators().clone();
        let mut app = NodeApp {
            chain: &node.chain,
            key: &node.key,
        };
        let out = node.bft.start_height(h, tip, set, &mut app);
        self.handle(i, out);
    }

    fn broadcast(&mut self, from: usize, m: Msg) {
        for to in 0..self.nodes.len() {
            if to != from && self.partition[to] == self.partition[from] {
                self.queue.push_back((to, m.clone()));
            }
        }
    }

    fn handle(&mut self, i: usize, outputs: Vec<Output>) {
        for o in outputs {
            match o {
                Output::BroadcastProposal(p) => self.broadcast(i, Msg::Proposal(*p)),
                Output::BroadcastVote(v) => {
                    if self.nodes[i].behavior == Behavior::DoubleVoter {
                        let other = Vote::sign(
                            v.kind,
                            v.height,
                            v.round,
                            match v.block {
                                Some(_) => None,
                                None => Some(BlockId(rz_crypto::Hash32([0xee; 32]))),
                            },
                            &self.nodes[i].key,
                            NET,
                        );
                        self.broadcast(i, Msg::Vote(other));
                    }
                    self.broadcast(i, Msg::Vote(v));
                }
                Output::ScheduleTimeout(t, ms) => self.timers.push((self.now + ms, i, t)),
                Output::Decide(block, commit) => {
                    let node = &mut self.nodes[i];
                    let id = block.id();
                    node.chain
                        .commit(rz_core::CommittedBlock {
                            block: *block,
                            commit,
                        })
                        .expect("decisão deve ser um commit válido");
                    node.decisions.push((node.chain.height(), id));
                    self.pending_start.push(i);
                }
                Output::VoteEvidence(..) => self.nodes[i].vote_evidence += 1,
                Output::ProposalEvidence(..) => {}
            }
        }
    }

    fn deliver(&mut self, to: usize, m: Msg) {
        if self.nodes[to].behavior == Behavior::Offline {
            return;
        }
        let node = &mut self.nodes[to];
        let mut app = NodeApp {
            chain: &node.chain,
            key: &node.key,
        };
        let r = match m {
            Msg::Proposal(p) => node.bft.on_proposal(p, &mut app),
            Msg::Vote(v) => node.bft.on_vote(v, &mut app),
        };
        self.handle(to, r.outputs);
    }

    /// Executa até `max_time` (ms simulados) ou até todos os honestos
    /// atingirem `target` alturas.
    fn run(&mut self, target: u64, max_time: u64) {
        self.target = target;
        loop {
            while let Some((to, m)) = self.queue.pop_front() {
                self.deliver(to, m);
            }
            if !self.pending_start.is_empty() {
                for i in std::mem::take(&mut self.pending_start) {
                    if self.nodes[i].chain.height() < self.target {
                        self.start_height(i);
                    }
                }
                continue;
            }
            let done = self
                .nodes
                .iter()
                .filter(|n| n.behavior == Behavior::Honest)
                .all(|n| n.chain.height() >= target);
            if done || self.now > max_time {
                return;
            }
            // Retransmissão periódica (gossip): a cada segundo simulado.
            if self.now >= self.last_retransmit + 1_000 || self.timers.is_empty() {
                if self.timers.is_empty() {
                    self.now += 1_000;
                }
                self.last_retransmit = self.now;
                for i in 0..self.nodes.len() {
                    if self.nodes[i].behavior == Behavior::Offline {
                        continue;
                    }
                    let out = self.nodes[i].bft.retransmit();
                    for o in out {
                        match o {
                            Output::BroadcastProposal(p) => self.broadcast(i, Msg::Proposal(*p)),
                            Output::BroadcastVote(v) => self.broadcast(i, Msg::Vote(v)),
                            _ => {}
                        }
                    }
                }
                if !self.queue.is_empty() {
                    continue;
                }
            }
            if self.timers.is_empty() {
                continue;
            }
            self.timers
                .sort_by_key(|(t, i, _)| (std::cmp::Reverse(*t), std::cmp::Reverse(*i)));
            let (t, i, timeout) = self.timers.pop().unwrap();
            if t > max_time {
                return;
            }
            self.now = t;
            if self.nodes[i].behavior == Behavior::Offline {
                continue;
            }
            let node = &mut self.nodes[i];
            let mut app = NodeApp {
                chain: &node.chain,
                key: &node.key,
            };
            let out = node.bft.on_timeout(timeout, &mut app);
            self.handle(i, out);
        }
    }

    /// Segurança: nenhuma altura com dois blocos finalizados diferentes.
    fn assert_safety(&self) {
        let mut by_height: BTreeMap<u64, BlockId> = BTreeMap::new();
        for n in &self.nodes {
            for (h, id) in &n.decisions {
                if let Some(prev) = by_height.insert(*h, *id) {
                    assert_eq!(prev, *id, "dois blocos finalizados na altura {h}");
                }
            }
        }
    }

    fn honest_heights(&self) -> Vec<u64> {
        self.nodes
            .iter()
            .filter(|n| n.behavior == Behavior::Honest)
            .map(|n| n.chain.height())
            .collect()
    }
}

// AT-CON-001 / AT-CON-003 — validadores honestos finalizam o mesmo bloco.
#[test]
fn honest_validators_agree() {
    let mut sim = Sim::new(4, &[]);
    sim.run(5, 60_000);
    assert_eq!(sim.honest_heights(), vec![5, 5, 5, 5]);
    sim.assert_safety();
    let tip = sim.nodes[0].chain.tip();
    assert!(sim.nodes.iter().all(|n| n.chain.tip() == tip));
}

// Participante offline (ARCHITECTURE §12): com 3 de 4, a rede continua.
#[test]
fn tolerates_one_offline_validator() {
    // Desliga o proponente da altura 1, rodada 0: exige troca de rodada.
    let probe = Sim::new(4, &[]);
    let set = probe.nodes[0].chain.state().validators().clone();
    let genesis_id = probe.nodes[0].chain.tip();
    let proposer = *set.proposer(&genesis_id, 1, 0).unwrap();
    let offline = probe
        .nodes
        .iter()
        .position(|n| n.key.public_key() == proposer)
        .unwrap();
    drop(probe);

    let mut sim = Sim::new(4, &[(offline, Behavior::Offline)]);
    sim.run(3, 120_000);
    assert!(
        sim.honest_heights().iter().all(|h| *h >= 3),
        "{:?}",
        sim.honest_heights()
    );
    sim.assert_safety();
}

// Nodes maliciosos (SPEC §20): dupla assinatura não impede a decisão, e os
// honestos detectam a infração como evidência.
#[test]
fn byzantine_double_voter_detected_and_tolerated() {
    let mut sim = Sim::new(4, &[(0, Behavior::DoubleVoter)]);
    sim.run(3, 120_000);
    assert!(sim.honest_heights().iter().all(|h| *h >= 3));
    sim.assert_safety();
    assert!(
        sim.nodes[1..].iter().any(|n| n.vote_evidence > 0),
        "dupla assinatura deveria gerar evidência"
    );
}

// Partições (THR-CON-007): nenhum lado finaliza sozinho — segurança antes
// de disponibilidade; após a reconexão, a rede volta a progredir.
#[test]
fn partition_halts_then_recovers() {
    let mut sim = Sim::new(4, &[]);
    sim.partition = vec![0, 0, 1, 1];
    sim.run(1, 20_000);
    assert_eq!(
        sim.honest_heights(),
        vec![0, 0, 0, 0],
        "metades não podem finalizar"
    );

    sim.partition = vec![0, 0, 0, 0];
    // Retransmissão após a reconexão: os temporizadores seguem disparando e
    // novas rodadas reúnem os quatro validadores.
    sim.run(2, 400_000);
    assert!(
        sim.honest_heights().iter().all(|h| *h >= 2),
        "{:?}",
        sim.honest_heights()
    );
    sim.assert_safety();
}

// Sem mais de 2/3 do poder online, nada é finalizado.
#[test]
fn two_of_four_offline_no_progress() {
    let mut sim = Sim::new(4, &[(0, Behavior::Offline), (1, Behavior::Offline)]);
    sim.run(1, 30_000);
    assert_eq!(sim.nodes[2].chain.height(), 0);
    assert_eq!(sim.nodes[3].chain.height(), 0);
}

// Um validador sozinho (1 de 1) finaliza — rede local mínima.
#[test]
fn single_validator() {
    let mut sim = Sim::new(1, &[]);
    sim.run(3, 10_000);
    assert_eq!(sim.nodes[0].chain.height(), 3);
}

#[test]
fn decision_carries_valid_commit() {
    let mut sim = Sim::new(4, &[]);
    sim.run(1, 60_000);
    let cb = sim.nodes[0].chain.committed_at(1).unwrap();
    let c: &Commit = &cb.commit;
    let set = rz_core::State::from_genesis(sim.nodes[0].chain.genesis())
        .unwrap()
        .validators()
        .clone();
    c.verify(1, &cb.block.id(), &set, NET).unwrap();
}

// ------------------------------------------------------ limites de memória
// Revisão de segurança (`docs/AUDIT.md`): mensagens hostis não crescem a
// memória nem são repassadas.

/// Máquina de consenso de um validador honesto na altura 1, sem proposta.
fn observer(ks: &[SecretKey]) -> (Chain, Bft) {
    let g = genesis(ks);
    let chain = Chain::new(g.clone()).expect("genesis");
    // Observador sem chave: não propõe nem vota, só armazena mensagens.
    let bft = Bft::new(NET, None, g.consensus.clone());
    (chain, bft)
}

#[test]
fn next_height_messages_from_non_validators_ignored() {
    let ks = keys(4);
    let (chain, mut bft) = observer(&ks);
    let mut app = NodeApp {
        chain: &chain,
        key: &ks[0],
    };
    bft.start_height(1, chain.tip(), chain.state().validators().clone(), &mut app);
    for i in 0..500u32 {
        let outsider = SecretKey::from_seed([100 + (i % 100) as u8; 32]);
        let v = Vote::sign(
            rz_core::VoteType::Prevote,
            2,
            i,
            Some(BlockId(rz_crypto::Hash32([i as u8; 32]))),
            &outsider,
            NET,
        );
        assert!(!bft.on_vote(v, &mut app).relay);
    }
    assert_eq!(bft.stored_future(), 0);
    // Validador legítimo, rodada inicial da altura seguinte: guardado.
    let v = Vote::sign(rz_core::VoteType::Prevote, 2, 0, None, &ks[1], NET);
    assert!(bft.on_vote(v, &mut app).relay);
    // Mesmo validador, rodada distante da altura seguinte: descartado.
    let v = Vote::sign(rz_core::VoteType::Prevote, 2, 1_000, None, &ks[1], NET);
    assert!(!bft.on_vote(v, &mut app).relay);
    assert_eq!(bft.stored_future(), 1);
}

#[test]
fn byzantine_validator_cannot_grow_future_round_votes() {
    let ks = keys(4);
    let (chain, mut bft) = observer(&ks);
    let mut app = NodeApp {
        chain: &chain,
        key: &ks[0],
    };
    bft.start_height(1, chain.tip(), chain.state().validators().clone(), &mut app);
    // Um validador (1/4 do poder, abaixo de 1/3) assina votos para milhares
    // de rodadas futuras.
    for r in 1..5_000u32 {
        for kind in [rz_core::VoteType::Prevote, rz_core::VoteType::Precommit] {
            let v = Vote::sign(kind, 1, r, None, &ks[3], NET);
            bft.on_vote(v, &mut app);
        }
    }
    // Só o voto mais alto de cada tipo fica guardado, e sem salto de rodada.
    assert_eq!(bft.stored_votes(), 2);
    assert_eq!(bft.round(), 0);
    // Voto mais antigo que o guardado é descartado sem repasse.
    let v = Vote::sign(rz_core::VoteType::Prevote, 1, 10, None, &ks[3], NET);
    assert!(!bft.on_vote(v, &mut app).relay);

    // Dois validadores (> 1/3) na mesma rodada futura: o salto acontece.
    let v = Vote::sign(rz_core::VoteType::Prevote, 1, 4_999, None, &ks[2], NET);
    bft.on_vote(v, &mut app);
    assert_eq!(bft.round(), 4_999);
}

#[test]
fn far_future_proposals_dropped() {
    let ks = keys(4);
    let (chain, mut bft) = observer(&ks);
    let mut app = NodeApp {
        chain: &chain,
        key: &ks[0],
    };
    bft.start_height(1, chain.tip(), chain.state().validators().clone(), &mut app);
    let validators = chain.state().validators().clone();
    let mut accepted = 0;
    for r in 0..200u32 {
        let proposer = *validators.proposer(&chain.tip(), 1, r).expect("proponente");
        let key = ks
            .iter()
            .find(|k| k.public_key() == proposer)
            .expect("chave");
        let block = Block::build(
            chain.genesis(),
            chain.tip(),
            0,
            &chain.state(),
            r as u64,
            vec![],
            key,
        )
        .expect("bloco")
        .0;
        let p = Proposal::sign(1, r, None, block, key, NET);
        if bft.on_proposal(p, &mut app).relay {
            accepted += 1;
        }
    }
    // Rodadas 0..=FUTURE_ROUND_WINDOW a partir da rodada atual (0).
    assert_eq!(accepted, crate::bft::FUTURE_ROUND_WINDOW as usize + 1);
    assert_eq!(bft.stored_proposals(), accepted);
}
