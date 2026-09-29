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
        self.propose_hash(category, Hash32([7; 32]), params)
    }

    fn propose_hash(
        &mut self,
        category: Category,
        content_hash: Hash32,
        params: Vec<ParamChange>,
    ) -> Hash32 {
        let tx = self.tx(
            &rich(),
            TxKind::Propose {
                category,
                content_hash,
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

// ------------------------------------------------------------ Comunidades
// ADR-0015, spec/COMMUNITIES.md, AT-COM-001..003, N-1..N-7.

fn controllers() -> Vec<SecretKey> {
    (40..43).map(|i| SecretKey::from_seed([i; 32])).collect()
}

fn rule_of(keys: &[SecretKey], threshold: u8) -> crate::community::DecisionRule {
    crate::community::DecisionRule {
        keys: keys.iter().map(SecretKey::public_key).collect(),
        threshold,
    }
}

fn declare(sim: &mut Sim, name: &str, keys: &[SecretKey]) -> Hash32 {
    let tx = sim.tx(
        &rich(),
        TxKind::DeclareCommunity {
            name: name.into(),
            manifest_hash: Hash32([0x3a; 32]),
            rule: rule_of(keys, 2),
        },
    );
    let id = tx.id().0;
    sim.block(vec![tx]);
    id
}

/// Declara e reconhece por proposta aprovada (N-7).
fn recognize(sim: &mut Sim, name: &str, keys: &[SecretKey]) -> Hash32 {
    let id = declare(sim, name, keys);
    if sim.state.locks().next().is_none() {
        sim.lock(&rich(), 100_000, 100);
    }
    let prop = sim.propose_hash(Category::Community, id, vec![]);
    let p = sim.state.proposal(&prop).unwrap().clone();
    sim.advance_to(p.voting_start - 1);
    let a = sim.vote(&rich(), prop, Choice::Yes);
    let b = sim.vote(&validator(), prop, Choice::Yes);
    sim.block(vec![a, b]);
    sim.advance_to(p.activation_height);
    assert_eq!(
        sim.state.proposal(&prop).unwrap().status,
        ProposalStatus::Activated
    );
    id
}

// AT-COM-001 — Comunidade válida: declarada, reconhecida e resolvível.
#[test]
fn at_com_001_declare_and_recognize() {
    let mut sim = Sim::new();
    let id = recognize(&mut sim, "desenvolvedores", &controllers());
    let c = &sim.state.communities().communities[&id];
    assert_eq!(c.status, crate::community::CommunityStatus::Recognized);
    assert_eq!(c.version, 1);
    assert_eq!(
        sim.state
            .communities()
            .resolve("desenvolvedores", crate::community::NameKind::Community),
        Some(id)
    );
    assert!(sim.state.approved_communities().any(|c| *c == id));
}

// AT-COM-002 — Comunidade malformada é rejeitada; reconhecimento exige
// declaração pendente (validação técnica, N-7).
#[test]
fn at_com_002_malformed_rejected() {
    let mut sim = Sim::new();
    sim.lock(&rich(), 100_000, 100);
    let no_declaration = sim.tx(
        &rich(),
        TxKind::Propose {
            category: Category::Community,
            content_hash: Hash32([7; 32]),
            params: vec![],
            release_id: None,
            deposit: sim.state.params().governance.deposit,
        },
    );
    assert!(matches!(
        sim.check(&no_declaration),
        Err(TxError::Community(_))
    ));

    let ks = controllers();
    let declare_tx = |sim: &Sim, name: &str, rule| {
        sim.tx(
            &rich(),
            TxKind::DeclareCommunity {
                name: name.into(),
                manifest_hash: Hash32([1; 32]),
                rule,
            },
        )
    };
    for bad in [
        declare_tx(&sim, "ab", rule_of(&ks, 2)),
        declare_tx(&sim, "Maiusculas", rule_of(&ks, 2)),
        declare_tx(&sim, "rede-zero", rule_of(&ks, 2)),
        declare_tx(&sim, "valida", rule_of(&ks, 0)),
        declare_tx(&sim, "valida", rule_of(&ks, 4)),
        declare_tx(&sim, "valida", rule_of(&[], 1)),
    ] {
        assert!(matches!(sim.check(&bad), Err(TxError::Community(_))));
    }
    let dup_keys = crate::community::DecisionRule {
        keys: vec![ks[0].public_key(), ks[0].public_key()],
        threshold: 1,
    };
    assert!(sim.check(&declare_tx(&sim, "valida", dup_keys)).is_err());

    // Nome já usado, ou confundível, por outra Comunidade.
    declare(&mut sim, "ciencia", &ks);
    assert!(sim
        .check(&declare_tx(&sim, "ciencia", rule_of(&ks, 2)))
        .is_err());
    assert!(sim
        .check(&declare_tx(&sim, "c1enc1a", rule_of(&ks, 2)))
        .is_err());
    assert!(sim
        .check(&declare_tx(&sim, "ciencias", rule_of(&ks, 2)))
        .is_ok());
}

// N-1 e N-6 — posição comunitária: aceita só com a aprovação declarada;
// registrada, sem alterar a apuração oficial.
#[test]
fn community_position_verifiable_and_non_binding() {
    let ks = controllers();
    let run = |with_position: bool| {
        let mut sim = Sim::new();
        let cid = recognize(&mut sim, "privacidade", &ks);
        let prop = sim.propose(Category::Ordinary, vec![ParamChange::MinFee(3)]);
        let p = sim.state.proposal(&prop).unwrap().clone();
        sim.advance_to(p.voting_start - 1);
        let payload = crate::community::position_payload(&cid, &prop, Choice::No);
        let net = sim.g.network_id.clone();
        let one = vec![crate::community::approve(&ks[0], &net, &payload)];
        let insufficient = sim.tx(
            &poor(),
            TxKind::CommunityPosition {
                community: cid,
                proposal: prop,
                choice: Choice::No,
                approvals: one,
            },
        );
        assert!(matches!(
            sim.check(&insufficient),
            Err(TxError::Community(_))
        ));
        let mut txs = vec![sim.vote(&rich(), prop, Choice::Yes)];
        if with_position {
            let two = vec![
                crate::community::approve(&ks[0], &net, &payload),
                crate::community::approve(&ks[2], &net, &payload),
            ];
            // Qualquer conta pode retransmitir a posição (paga a taxa).
            txs.push(sim.tx(
                &poor(),
                TxKind::CommunityPosition {
                    community: cid,
                    proposal: prop,
                    choice: Choice::No,
                    approvals: two,
                },
            ));
        }
        sim.block(txs);
        sim.advance_to(p.voting_end);
        let positions = sim.state.communities().positions_on(&prop);
        (sim.state.proposal(&prop).unwrap().tally, positions)
    };
    let (tally_without, none) = run(false);
    let (tally_with, some) = run(true);
    assert!(none.is_empty());
    assert_eq!(some.len(), 1);
    assert_eq!(some[0].1, Choice::No);
    // A posição NÃO altera o resultado oficial (N-1).
    assert_eq!(tally_with, tally_without);
}

// AT-COM-003 — Comunidade "comprometida" (chaves de controle nas mãos de
// um atacante) não altera outras Comunidades, saldos, consenso nem regras.
#[test]
fn at_com_003_compromised_community_isolated() {
    let mut sim = Sim::new();
    let attacker_keys: Vec<SecretKey> = (60..63).map(|i| SecretKey::from_seed([i; 32])).collect();
    let victim_keys = controllers();
    let bad = recognize(&mut sim, "atacante", &attacker_keys);
    let victim = recognize(&mut sim, "vitima", &victim_keys);
    let net = sim.g.network_id.clone();

    // Tenta atualizar a Comunidade vítima com as próprias chaves.
    let payload = crate::community::update_payload(&victim, &Hash32([9; 32]), 2, &None);
    let approvals: Vec<_> = attacker_keys
        .iter()
        .map(|k| crate::community::approve(k, &net, &payload))
        .collect();
    let hijack = sim.tx(
        &poor(),
        TxKind::UpdateCommunity {
            community: victim,
            manifest_hash: Hash32([9; 32]),
            version: 2,
            rule: None,
            approvals,
        },
    );
    assert!(matches!(sim.check(&hijack), Err(TxError::Community(_))));

    // O que a própria Comunidade pode fazer só a afeta: nova versão com
    // nova regra. Saldos de terceiros, consenso e parâmetros não mudam.
    let others: Vec<_> = [validator(), rich()]
        .iter()
        .map(|k| sim.state.account(&k.public_key().address()))
        .collect();
    let validators = sim.state.validators().clone();
    let params = sim.state.params().clone();
    let new_rule = rule_of(&attacker_keys[..1], 1);
    let payload =
        crate::community::update_payload(&bad, &Hash32([8; 32]), 2, &Some(new_rule.clone()));
    let approvals: Vec<_> = attacker_keys[..2]
        .iter()
        .map(|k| crate::community::approve(k, &net, &payload))
        .collect();
    let own = sim.tx(
        &poor(),
        TxKind::UpdateCommunity {
            community: bad,
            manifest_hash: Hash32([8; 32]),
            version: 2,
            rule: Some(new_rule),
            approvals,
        },
    );
    sim.block(vec![own]);
    let c = &sim.state.communities().communities[&bad];
    assert_eq!(c.version, 2);
    assert_eq!(c.history.len(), 2);
    assert_eq!(sim.state.communities().communities[&victim].version, 1);
    let after: Vec<_> = [validator(), rich()]
        .iter()
        .map(|k| sim.state.account(&k.public_key().address()))
        .collect();
    // O validador recebeu a taxa do bloco; o saldo de `rich` não muda.
    assert_eq!(after[1], others[1]);
    assert_eq!(sim.state.validators(), &validators);
    assert_eq!(sim.state.params(), &params);
}

// Atualização versionada: a versão só aumenta, e a regra antiga deixa de
// aprovar depois de substituída (THR-COM-002).
#[test]
fn community_update_versioned_and_rule_rotates() {
    let mut sim = Sim::new();
    let ks = controllers();
    let cid = recognize(&mut sim, "jornal", &ks);
    let net = sim.g.network_id.clone();
    let update = |sim: &Sim,
                  version,
                  rule: Option<crate::community::DecisionRule>,
                  signers: &[SecretKey]| {
        let payload = crate::community::update_payload(&cid, &Hash32([5; 32]), version, &rule);
        sim.tx(
            &poor(),
            TxKind::UpdateCommunity {
                community: cid,
                manifest_hash: Hash32([5; 32]),
                version,
                rule,
                approvals: signers
                    .iter()
                    .map(|k| crate::community::approve(k, &net, &payload))
                    .collect(),
            },
        )
    };
    // Versão igual à atual: rejeitada.
    assert!(sim.check(&update(&sim, 1, None, &ks[..2])).is_err());
    let fresh: Vec<SecretKey> = (70..72).map(|i| SecretKey::from_seed([i; 32])).collect();
    let tx = update(&sim, 2, Some(rule_of(&fresh, 2)), &ks[..2]);
    sim.block(vec![tx]);
    // As chaves antigas já não aprovam; as novas sim.
    assert!(sim.check(&update(&sim, 3, None, &ks[..2])).is_err());
    assert!(sim.check(&update(&sim, 3, None, &fresh)).is_ok());
}

// Nomes zero://nome.tipo (N-3, THR-BRW-002).
#[test]
fn names_registered_resolved_and_protected() {
    use crate::community::NameKind;
    let mut sim = Sim::new();
    sim.state.params_mut().communities.name_fee = 50;
    let register = |sim: &Sim, key: &SecretKey, name: &str, kind| {
        sim.tx(
            key,
            TxKind::RegisterName {
                name: name.into(),
                kind,
                target: Hash32([0xab; 32]),
            },
        )
    };
    let pool_before = sim
        .state
        .market()
        .pool_balance(&crate::market::AssetId::ZERO);
    let tx = register(&sim, &rich(), "editorzero", NameKind::App);
    sim.block(vec![tx]);
    let fee = sim.state.params().communities.name_fee;
    assert_eq!(
        sim.state
            .market()
            .pool_balance(&crate::market::AssetId::ZERO),
        pool_before + fee,
        "a taxa de nome vai para o Pool"
    );
    assert_eq!(
        sim.state.communities().resolve("editorzero", NameKind::App),
        Some(Hash32([0xab; 32]))
    );
    // Mesmo nome, ou confundível, no mesmo tipo: rejeitado.
    for n in ["editorzero", "edit0rzer0", "editor-zero"] {
        assert!(sim
            .check(&register(&sim, &rich(), n, NameKind::App))
            .is_err());
    }
    // O mesmo nome em outro tipo é outro endereço (zero://editorzero.blog).
    assert!(sim
        .check(&register(&sim, &rich(), "editorzero", NameKind::Blog))
        .is_ok());
    // .comunidade só vem do reconhecimento.
    assert!(matches!(
        sim.check(&register(&sim, &rich(), "outra", NameKind::Community)),
        Err(TxError::Community(_))
    ));
    // Só o dono altera ou transfere.
    let steal = sim.tx(
        &poor(),
        TxKind::UpdateName {
            name: "editorzero".into(),
            kind: NameKind::App,
            target: Hash32([1; 32]),
            new_owner: Some(poor().public_key().address()),
        },
    );
    assert!(matches!(sim.check(&steal), Err(TxError::Community(_))));
    let transfer = sim.tx(
        &rich(),
        TxKind::UpdateName {
            name: "editorzero".into(),
            kind: NameKind::App,
            target: Hash32([2; 32]),
            new_owner: Some(poor().public_key().address()),
        },
    );
    sim.block(vec![transfer]);
    let r = &sim.state.communities().names[&(NameKind::App, "editorzero".to_string())];
    assert_eq!(r.owner, poor().public_key().address());
    assert_eq!(r.target, Hash32([2; 32]));
}

// Declaração não reconhecida expira e libera o nome.
#[test]
fn unrecognized_declaration_expires() {
    let mut sim = Sim::new();
    sim.state.params_mut().communities.declaration_ttl_blocks = 3;
    let id = declare(&mut sim, "efemera", &controllers());
    assert!(sim.state.communities().communities.contains_key(&id));
    sim.advance_to(sim.height + 3);
    assert!(!sim.state.communities().communities.contains_key(&id));
    // O nome volta a estar disponível.
    declare(&mut sim, "efemera", &controllers());
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

// RZ-IR-07 — propostas conflitantes do mesmo proponente de rodada, inclusive
// numa re-proposta (bloco assinado por outro validador).
#[test]
fn double_proposal_slashes_even_for_reproposal() {
    let mut sim = Sim::new();
    let rk = rich().public_key();
    let epoch = sim.state.params().consensus.epoch_blocks;
    bond(&mut sim, &rich(), 5_000);
    sim.advance_to(epoch);
    let net = sim.g.network_id.clone();
    let h = sim.height + 1;

    // Bloco montado por outro validador (rodada 0), reproposto por `rich`
    // na rodada 3 com dois `pol_round` diferentes.
    let (block, _) = Block::build(
        &sim.g,
        sim.parent,
        sim.height,
        &sim.state,
        0,
        vec![],
        &validator(),
    )
    .unwrap();
    let a = crate::Proposal::sign(h, 3, Some(0), block.clone(), &rich(), &net).signed();
    let b = crate::Proposal::sign(h, 3, Some(1), block.clone(), &rich(), &net).signed();
    assert_eq!(a.block, b.block);

    let same = sim.tx(
        &poor(),
        TxKind::ReportDoubleProposal {
            first: Box::new(a.clone()),
            second: Box::new(a.clone()),
        },
    );
    assert!(matches!(sim.check(&same), Err(TxError::InvalidEvidence(_))));
    let mut forged = b.clone();
    forged.signature.0[3] ^= 1;
    let bad = sim.tx(
        &poor(),
        TxKind::ReportDoubleProposal {
            first: Box::new(a.clone()),
            second: Box::new(forged),
        },
    );
    assert!(matches!(sim.check(&bad), Err(TxError::InvalidEvidence(_))));

    let report = sim.tx(
        &poor(),
        TxKind::ReportDoubleProposal {
            first: Box::new(a),
            second: Box::new(b),
        },
    );
    sim.block(vec![report]);
    assert!(sim.state.is_jailed(&rk));
    assert!(sim.state.bond_of(&rk) < 5_000);

    // A mesma infração (proponente, altura, rodada) denunciada por
    // cabeçalhos não é punida de novo.
    let (x, _) = Block::build(&sim.g, sim.parent, h - 1, &sim.state, 3, vec![], &rich()).unwrap();
    let t = sim.tx(
        &rich(),
        TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1,
        },
    );
    let (y, _) = Block::build(&sim.g, sim.parent, h - 1, &sim.state, 3, vec![t], &rich()).unwrap();
    assert_eq!(x.header.height, h);
    let again = sim.tx(
        &poor(),
        TxKind::ReportEquivocation {
            first: Box::new(x.signed_header()),
            second: Box::new(y.signed_header()),
        },
    );
    assert_eq!(
        sim.check(&again),
        Err(TxError::InvalidEvidence("infração já punida"))
    );
}
