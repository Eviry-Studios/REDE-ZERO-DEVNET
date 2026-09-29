//! Testes de governança sobre blocos reais (`spec/GOVERNANCE.md §8`).

use rz_crypto::{Hash32, SecretKey};

use crate::block::{Block, BlockId};
use crate::genesis::tests::sample;
use crate::governance::{lock_weight, Category, Choice, Lock, ParamChange, ProposalStatus};
use crate::state::{ExecParams, State};
use crate::tx::{Transaction, TxBody, TxError, TxKind, TX_VERSION};
use crate::Genesis;

fn validator() -> SecretKey {
    SecretKey::from_seed([1; 32])
}

fn rich() -> SecretKey {
    SecretKey::from_seed([2; 32])
}

fn poor() -> SecretKey {
    SecretKey::from_seed([3; 32])
}

/// Cadeia simulada com um único validador.
struct Sim {
    g: Genesis,
    state: State,
    parent: BlockId,
    height: u64,
}

impl Sim {
    fn new() -> Self {
        let g = sample();
        let state = State::from_genesis(&g).unwrap();
        let parent = BlockId(g.hash());
        Self {
            g,
            state,
            parent,
            height: 0,
        }
    }

    fn tx(&self, key: &SecretKey, kind: TxKind) -> Transaction {
        self.tx_nonce(key, kind, 0)
    }

    fn tx_nonce(&self, key: &SecretKey, kind: TxKind, offset: u64) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: key.public_key(),
            nonce: self.state.account(&key.public_key().address()).nonce + offset,
            fee: self.state.params().min_fee,
            kind,
        }
        .sign(key, &self.g.network_id)
        .unwrap()
    }

    /// Verifica a transação no contexto do próximo bloco.
    fn check(&self, tx: &Transaction) -> Result<(), TxError> {
        self.state
            .check_transaction(tx, &ExecParams::at(&self.g, self.height + 1))
    }

    fn block(&mut self, txs: Vec<Transaction>) {
        let (b, s) = Block::build(
            &self.g,
            self.parent,
            self.height,
            &self.state,
            self.height + 1,
            txs,
            &validator(),
        )
        .expect("bloco válido");
        self.parent = b.id();
        self.height += 1;
        self.state = s;
        self.state.check_supply().unwrap();
    }

    fn advance_to(&mut self, height: u64) {
        while self.height < height {
            self.block(vec![]);
        }
    }

    fn lock(&mut self, key: &SecretKey, amount: u64, duration: u64) {
        let unlock_height = self.height + 1 + duration;
        let tx = self.tx(
            key,
            TxKind::LockStake {
                amount,
                unlock_height,
            },
        );
        self.block(vec![tx]);
    }

    fn propose(&mut self, category: Category, params: Vec<ParamChange>) -> Hash32 {
        let tx = self.tx(
            &rich(),
            TxKind::Propose {
                category,
                content_hash: Hash32([7; 32]),
                params,
                release_id: None,
                deposit: self.state.params().governance.deposit,
            },
        );
        let id = tx.id().0;
        self.block(vec![tx]);
        id
    }

    fn vote(&self, key: &SecretKey, proposal: Hash32, choice: Choice) -> Transaction {
        self.tx(key, TxKind::Vote { proposal, choice })
    }
}

// AT-GOV-001 — ciclo completo: proposta aprovada nas duas câmaras e ativada.
#[test]
fn full_cycle_changes_parameter() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 100_000, 100);
    let balance_before = sim.state.account(&rich().public_key().address()).balance;
    let id = sim.propose(Category::Ordinary, vec![ParamChange::MinFee(7)]);
    let p = sim.state.proposal(&id).unwrap().clone();
    assert_eq!(p.status, ProposalStatus::Pending);
    assert_eq!(
        sim.state.account(&rich().public_key().address()).balance,
        balance_before - 1_000 - 1,
        "depósito e taxa debitados"
    );

    sim.advance_to(p.voting_start - 1);
    let v1 = sim.vote(&rich(), id, Choice::Yes);
    let v2 = sim.vote(&validator(), id, Choice::Yes);
    sim.block(vec![v1, v2]);

    sim.advance_to(p.voting_end);
    let p = sim.state.proposal(&id).unwrap();
    assert_eq!(p.status, ProposalStatus::Approved);
    let tally = p.tally.unwrap();
    assert!(tally.economic.yes > 0 && tally.contribution.yes > 0);
    assert_eq!(sim.state.params().min_fee, 1, "ainda não ativada");

    let activation = p.activation_height;
    sim.advance_to(activation);
    assert_eq!(
        sim.state.proposal(&id).unwrap().status,
        ProposalStatus::Activated
    );
    assert_eq!(sim.state.params().min_fee, 7);

    // A nova regra passa a valer: taxa antiga é rejeitada.
    let old_fee = TxBody {
        version: TX_VERSION,
        sender: rich().public_key(),
        nonce: sim.state.account(&rich().public_key().address()).nonce,
        fee: 1,
        kind: TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1,
        },
    }
    .sign(&rich(), &sim.g.network_id)
    .unwrap();
    assert!(matches!(
        sim.check(&old_fee),
        Err(TxError::FeeTooLow { .. })
    ));
}

// AT-GOV-002 — categoria incompatível com os parâmetros.
#[test]
fn category_mismatch_rejected() {
    let sim = Sim::new();
    let tx = sim.tx(
        &rich(),
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([1; 32]),
            params: vec![ParamChange::GovDeposit(1)],
            release_id: None,
            deposit: 1_000,
        },
    );
    assert!(matches!(sim.check(&tx), Err(TxError::Governance(_))));
}

// AT-GOV-003 — depósito insuficiente.
#[test]
fn deposit_too_low() {
    let sim = Sim::new();
    let tx = sim.tx(
        &rich(),
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([1; 32]),
            params: vec![ParamChange::MinFee(2)],
            release_id: None,
            deposit: 999,
        },
    );
    assert_eq!(
        sim.check(&tx),
        Err(TxError::DepositTooLow {
            deposit: 999,
            min: 1_000
        })
    );
    // Conta sem saldo para o depósito.
    let tx = sim.tx(
        &poor(),
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([1; 32]),
            params: vec![ParamChange::MinFee(2)],
            release_id: None,
            deposit: 1_000,
        },
    );
    assert!(matches!(
        sim.check(&tx),
        Err(TxError::InsufficientBalance { .. })
    ));
}

// AT-GOV-004 — voto fora do período.
#[test]
fn vote_outside_window() {
    let mut sim = Sim::new();
    let id = sim.propose(Category::Ordinary, vec![ParamChange::MinFee(2)]);
    let p = sim.state.proposal(&id).unwrap().clone();
    // Ainda em análise (próximo bloco < voting_start).
    assert!(sim.height + 1 < p.voting_start);
    assert!(matches!(
        sim.check(&sim.vote(&rich(), id, Choice::Yes)),
        Err(TxError::Governance(_))
    ));
    sim.advance_to(p.voting_end);
    assert!(matches!(
        sim.check(&sim.vote(&rich(), id, Choice::Yes)),
        Err(TxError::Governance(_))
    ));
    // Proposta inexistente.
    assert!(matches!(
        sim.check(&sim.vote(&rich(), Hash32([9; 32]), Choice::Yes)),
        Err(TxError::Governance(_))
    ));
}

// AT-GOV-005 — voto repetido substitui, nunca soma.
#[test]
fn vote_is_replaced() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 100_000, 100);
    let id = sim.propose(Category::Ordinary, vec![ParamChange::MinFee(2)]);
    let p = sim.state.proposal(&id).unwrap().clone();
    sim.advance_to(p.voting_start - 1);
    let yes = sim.vote(&rich(), id, Choice::Yes);
    let v = sim.vote(&validator(), id, Choice::Yes);
    sim.block(vec![yes, v]);
    let no = sim.vote(&rich(), id, Choice::No);
    sim.block(vec![no]);
    assert_eq!(sim.state.proposal(&id).unwrap().votes.len(), 2);
    sim.advance_to(p.voting_end);
    let p = sim.state.proposal(&id).unwrap();
    let t = p.tally.unwrap();
    assert_eq!(t.economic.yes, 0);
    assert!(t.economic.no > 0);
    assert_eq!(p.status, ProposalStatus::Rejected);
}

// Sem quórum: depósito queimado (oferta total diminui).
#[test]
fn no_quorum_burns_deposit() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 100_000, 100);
    let supply = sim.state.total_supply();
    let id = sim.propose(Category::Ordinary, vec![ParamChange::MinFee(2)]);
    let end = sim.state.proposal(&id).unwrap().voting_end;
    sim.advance_to(end);
    assert_eq!(
        sim.state.proposal(&id).unwrap().status,
        ProposalStatus::NoQuorum
    );
    assert_eq!(sim.state.total_supply(), supply - 1_000);
}

// Aprovação só na câmara econômica não basta.
#[test]
fn economic_chamber_alone_cannot_approve() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 900_000, 100);
    let id = sim.propose(Category::Ordinary, vec![ParamChange::MinFee(2)]);
    let p = sim.state.proposal(&id).unwrap().clone();
    sim.advance_to(p.voting_start - 1);
    let a = sim.vote(&rich(), id, Choice::Yes);
    let b = sim.vote(&validator(), id, Choice::No);
    sim.block(vec![a, b]);
    sim.advance_to(p.voting_end);
    let p = sim.state.proposal(&id).unwrap();
    assert_eq!(p.status, ProposalStatus::Rejected);
    // Mesmo rejeitada, atingiu quórum: depósito devolvido.
    assert!(p.tally.unwrap().economic.eligible > 0);
}

// GOV-INV-4 — dividir o bloqueio entre identidades não aumenta o peso.
#[test]
fn economic_weight_is_sybil_neutral() {
    let sim = Sim::new();
    let g = &sim.state.params().governance;
    let mut p = sim_proposal();
    p.submitted_at = 10;
    p.voting_end = 20;
    let one = Lock {
        owner: rich().public_key().address(),
        amount: 1_000,
        locked_at: 5,
        unlock_height: 60,
    };
    let split: Vec<Lock> = (0..10u8)
        .map(|i| Lock {
            owner: SecretKey::from_seed([100 + i; 32]).public_key().address(),
            amount: 100,
            ..one.clone()
        })
        .collect();
    let w_one = lock_weight(&one, &p, g);
    let w_split: u128 = split.iter().map(|l| lock_weight(l, &p, g)).sum();
    assert_eq!(w_one, w_split);
    assert!(w_one > 0);

    // Bloqueio criado depois da submissão não conta (anti compra-de-última-hora).
    let late = Lock {
        locked_at: 10,
        ..one.clone()
    };
    assert_eq!(lock_weight(&late, &p, g), 0);
    // Bloqueio que vence antes do fim da votação não conta.
    let short = Lock {
        unlock_height: 19,
        ..one
    };
    assert_eq!(lock_weight(&short, &p, g), 0);
}

fn sim_proposal() -> crate::governance::Proposal {
    crate::governance::Proposal {
        id: Hash32::ZERO,
        proposer: rich().public_key().address(),
        category: Category::Ordinary,
        content_hash: Hash32::ZERO,
        params: vec![],
        release_id: None,
        deposit: 0,
        submitted_at: 0,
        voting_start: 0,
        voting_end: 0,
        activation_height: 0,
        status: ProposalStatus::Pending,
        votes: Default::default(),
        tally: None,
    }
}

#[test]
fn unlock_respects_expiry() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 50_000, 6);
    let (id, lock) = sim
        .state
        .locks()
        .next()
        .map(|(i, l)| (*i, l.clone()))
        .unwrap();
    let unlock = sim.tx(&rich(), TxKind::Unlock { lock_id: id });
    assert!(matches!(sim.check(&unlock), Err(TxError::Governance(_))));
    // Outra conta não pode desbloquear.
    let steal = sim.tx(&poor(), TxKind::Unlock { lock_id: id });
    assert!(matches!(sim.check(&steal), Err(TxError::Governance(_))));

    sim.advance_to(lock.unlock_height - 1);
    let before = sim.state.account(&rich().public_key().address()).balance;
    let unlock = sim.tx(&rich(), TxKind::Unlock { lock_id: id });
    sim.block(vec![unlock]);
    assert_eq!(
        sim.state.account(&rich().public_key().address()).balance,
        before + 50_000 - 1
    );
    assert_eq!(sim.state.locks().count(), 0);
}

#[test]
fn lock_duration_bounds() {
    let sim = Sim::new();
    let g = sim.state.params().governance.clone();
    let too_short = sim.tx(
        &rich(),
        TxKind::LockStake {
            amount: 1,
            unlock_height: 1 + g.lock_min_blocks - 1,
        },
    );
    assert!(matches!(sim.check(&too_short), Err(TxError::Governance(_))));
    let too_long = sim.tx(
        &rich(),
        TxKind::LockStake {
            amount: 1,
            unlock_height: 1 + g.lock_max_blocks + 1,
        },
    );
    assert!(matches!(sim.check(&too_long), Err(TxError::Governance(_))));
}

// SPEC §49 — denúncia de equivocação com evidência verificável zera pontos.
#[test]
fn equivocation_report_slashes_contribution() {
    let mut sim = Sim::new();
    sim.advance_to(3);
    let vk = validator().public_key();
    assert!(sim.state.contribution(&vk, sim.height) > 0);

    // Dois blocos distintos do mesmo validador para o mesmo slot.
    let (a, _) = Block::build(
        &sim.g,
        sim.parent,
        sim.height,
        &sim.state,
        50,
        vec![],
        &validator(),
    )
    .unwrap();
    let t = sim.tx(
        &rich(),
        TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1,
        },
    );
    let (b, _) = Block::build(
        &sim.g,
        sim.parent,
        sim.height,
        &sim.state,
        50,
        vec![t],
        &validator(),
    )
    .unwrap();

    // Evidência inválida: mesmo cabeçalho duas vezes.
    let bad = sim.tx(
        &poor(),
        TxKind::ReportEquivocation {
            first: Box::new(a.signed_header()),
            second: Box::new(a.signed_header()),
        },
    );
    assert!(matches!(sim.check(&bad), Err(TxError::InvalidEvidence(_))));
    // Evidência com assinatura adulterada.
    let mut forged = b.signed_header();
    forged.signature.0[0] ^= 1;
    let bad = sim.tx(
        &poor(),
        TxKind::ReportEquivocation {
            first: Box::new(a.signed_header()),
            second: Box::new(forged),
        },
    );
    assert!(matches!(sim.check(&bad), Err(TxError::InvalidEvidence(_))));

    let report = sim.tx(
        &poor(),
        TxKind::ReportEquivocation {
            first: Box::new(a.signed_header()),
            second: Box::new(b.signed_header()),
        },
    );
    sim.block(vec![report]);
    // Zerado pela denúncia; volta a acumular apenas com novos blocos (REQ-048).
    assert_eq!(sim.state.contribution(&vk, sim.height), 1_000_000);
}

#[test]
fn contribution_accrues_and_decays() {
    let mut sim = Sim::new();
    sim.advance_to(10);
    let vk = validator().public_key();
    let now = sim.state.contribution(&vk, 10);
    // Reproduz a regra: a cada bloco, decai o acumulado e soma 1 ponto.
    let mut expected = 0u64;
    for _ in 0..10 {
        expected = crate::governance::decay(expected, 1, 16) + 1_000_000;
    }
    assert_eq!(now, expected);
    assert!(now > 8_000_000 && now < 9_000_000);
    let later = sim.state.contribution(&vk, 10 + 16);
    assert_eq!(later, now / 2);
}

#[test]
fn community_proposal_records_content() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 100_000, 100);
    let id = sim.propose(Category::Community, vec![]);
    let p = sim.state.proposal(&id).unwrap().clone();
    assert_eq!(p.activation_height, p.voting_end);
    sim.advance_to(p.voting_start - 1);
    let a = sim.vote(&rich(), id, Choice::Yes);
    let b = sim.vote(&validator(), id, Choice::Yes);
    sim.block(vec![a, b]);
    sim.advance_to(p.voting_end);
    assert_eq!(
        sim.state.proposal(&id).unwrap().status,
        ProposalStatus::Activated
    );
    assert!(sim
        .state
        .approved_communities()
        .any(|c| *c == Hash32([7; 32])));
}

// ------------------------------------------------------------ participação
// Zero-BFT (ADR-0012): vínculo, desvinculação, épocas e punição.

fn bond(sim: &mut Sim, key: &SecretKey, amount: u64) {
    let tx = sim.tx(key, TxKind::Bond { amount });
    sim.block(vec![tx]);
}

#[test]
fn bond_enters_validator_set_at_next_epoch() {
    let mut sim = Sim::new();
    let rk = rich().public_key();
    let before = sim.state.account(&rk.address()).balance;
    bond(&mut sim, &rich(), 5_000);
    assert_eq!(sim.state.bond_of(&rk), 5_000);
    assert_eq!(
        sim.state.account(&rk.address()).balance,
        before - 5_000 - sim.g.min_fee
    );
    // Ainda fora do conjunto até o fim da época.
    assert!(!sim.state.validators().contains(&rk));
    let epoch = sim.state.params().consensus.epoch_blocks;
    sim.advance_to(epoch - 1);
    assert!(!sim.state.validators().contains(&rk));
    sim.advance_to(epoch);
    assert!(sim.state.validators().contains(&rk));
    assert_eq!(sim.state.validators().power_of(&rk), 5_000);
    assert_eq!(sim.state.validators().total_power(), 15_000);
}

#[test]
fn unbond_releases_after_period_and_leaves_set() {
    let mut sim = Sim::new();
    let rk = rich().public_key();
    let epoch = sim.state.params().consensus.epoch_blocks;
    bond(&mut sim, &rich(), 5_000);
    sim.advance_to(epoch);
    assert!(sim.state.validators().contains(&rk));

    let too_much = sim.tx(&rich(), TxKind::Unbond { amount: 5_001 });
    assert!(matches!(sim.check(&too_much), Err(TxError::Staking(_))));

    let balance = sim.state.account(&rk.address()).balance;
    let tx = sim.tx(&rich(), TxKind::Unbond { amount: 5_000 });
    sim.block(vec![tx]);
    let unbond_height = sim.height;
    assert_eq!(sim.state.bond_of(&rk), 0);
    assert_eq!(sim.state.unbonding().len(), 1);
    let release = sim.state.unbonding()[0].release_height;
    assert_eq!(
        release,
        unbond_height + sim.state.params().consensus.unbonding_blocks
    );

    // Sai do conjunto na próxima época; o valor fica retido até a liberação.
    sim.advance_to(2 * epoch);
    assert!(!sim.state.validators().contains(&rk));
    sim.advance_to(release - 1);
    assert_eq!(
        sim.state.account(&rk.address()).balance,
        balance - sim.g.min_fee
    );
    sim.advance_to(release);
    assert!(sim.state.unbonding().is_empty());
    assert_eq!(
        sim.state.account(&rk.address()).balance,
        balance - sim.g.min_fee + 5_000
    );
}

// SPEC §49 — voto duplo comprovado: queima, exclusão imediata e bloqueio
// de novo vínculo.
#[test]
fn double_vote_slashes_bond_and_jails() {
    let mut sim = Sim::new();
    let rk = rich().public_key();
    let epoch = sim.state.params().consensus.epoch_blocks;
    bond(&mut sim, &rich(), 5_000);
    sim.advance_to(epoch);
    assert!(sim.state.validators().contains(&rk));

    let net = sim.g.network_id.clone();
    let h = sim.height + 1;
    let a = crate::Vote::sign(
        crate::VoteType::Prevote,
        h,
        0,
        Some(BlockId(Hash32([1; 32]))),
        &rich(),
        &net,
    );
    let b = crate::Vote::sign(
        crate::VoteType::Prevote,
        h,
        0,
        Some(BlockId(Hash32([2; 32]))),
        &rich(),
        &net,
    );
    // Evidência não conflitante (o mesmo voto duas vezes).
    let bad = sim.tx(
        &poor(),
        TxKind::ReportDoubleVote {
            first: Box::new(a.clone()),
            second: Box::new(a.clone()),
        },
    );
    assert!(matches!(sim.check(&bad), Err(TxError::InvalidEvidence(_))));
    // Assinatura adulterada.
    let mut forged = b.clone();
    forged.signature.0[0] ^= 1;
    let bad = sim.tx(
        &poor(),
        TxKind::ReportDoubleVote {
            first: Box::new(a.clone()),
            second: Box::new(forged),
        },
    );
    assert!(matches!(sim.check(&bad), Err(TxError::InvalidEvidence(_))));

    let supply = sim.state.total_supply();
    let report = sim.tx(
        &poor(),
        TxKind::ReportDoubleVote {
            first: Box::new(a.clone()),
            second: Box::new(b.clone()),
        },
    );
    sim.block(vec![report]);
    let burned = 5_000 * u64::from(sim.state.params().consensus.slash_bps) / 10_000;
    assert_eq!(sim.state.bond_of(&rk), 5_000 - burned);
    assert_eq!(sim.state.total_supply(), supply - burned);
    assert!(sim.state.is_jailed(&rk));
    assert!(!sim.state.validators().contains(&rk));

    // A mesma infração não é punida duas vezes.
    let again = sim.tx(
        &poor(),
        TxKind::ReportDoubleVote {
            first: Box::new(b),
            second: Box::new(a),
        },
    );
    assert!(matches!(
        sim.check(&again),
        Err(TxError::InvalidEvidence(_))
    ));
    // Excluído não volta a vincular.
    let rebond = sim.tx(&rich(), TxKind::Bond { amount: 1_000 });
    assert!(matches!(sim.check(&rebond), Err(TxError::Staking(_))));
    // Nem retorna ao conjunto na época seguinte.
    sim.advance_to(3 * epoch);
    assert!(!sim.state.validators().contains(&rk));
}
