//! Testes da Defesa da Exonet sobre blocos reais (`spec/DEFENSE.md`):
//! AT-DEF-001..003, AT-CYBER-001..006, AT-INC-001..003, INV-009, INV-010.

use rz_crypto::{Hash32, SecretKey};

use crate::block::{Block, BlockId};
use crate::defense::{attest, payload, Attestation, DefenseMode, IncidentStatus, Scope};
use crate::genesis::tests::sample;
use crate::genesis::GenesisValidator;
use crate::state::{ExecParams, State};
use crate::tx::{Transaction, TxBody, TxError, TxKind, TX_VERSION};
use crate::Genesis;

fn validators() -> Vec<SecretKey> {
    (11..=14).map(|i| SecretKey::from_seed([i; 32])).collect()
}

/// Paga as taxas das decisões (qualquer conta pode enviá-las).
fn relayer() -> SecretKey {
    SecretKey::from_seed([2; 32])
}

fn outsider() -> SecretKey {
    SecretKey::from_seed([3; 32])
}

fn genesis() -> Genesis {
    let mut g = sample();
    g.validators = validators()
        .iter()
        .map(|k| GenesisValidator::new(k.public_key(), 100))
        .collect();
    // Validadores com saldo livre para as taxas das suas ações.
    for k in validators() {
        g.allocations.push(crate::genesis::Allocation {
            address: k.public_key().address(),
            amount: 1_000,
        });
    }
    g.allocations.sort_by_key(|a| a.address);
    g
}

const EV: Hash32 = Hash32([0xee; 32]);

struct Sim {
    g: Genesis,
    state: State,
    parent: BlockId,
    height: u64,
}

impl Sim {
    fn new() -> Self {
        let g = genesis();
        let mut state = State::from_genesis(&g).unwrap();
        let dp = &mut state.params_mut().defense;
        dp.vigilance_max_blocks = 20;
        dp.incident_max_blocks = 30;
        dp.war_max_blocks = 10;
        dp.war_persistence_blocks = 3;
        dp.credential_max_blocks = 15;
        Self {
            parent: BlockId(g.hash()),
            g,
            state,
            height: 0,
        }
    }

    fn tx(&self, key: &SecretKey, kind: TxKind) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: key.public_key(),
            nonce: self.state.account(&key.public_key().address()).nonce,
            fee: self.state.params().min_fee,
            kind,
        }
        .sign(key, &self.g.network_id)
        .unwrap()
    }

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
            0,
            txs,
            &validators()[0],
        )
        .expect("bloco válido");
        self.parent = b.id();
        self.height += 1;
        self.state = s;
        self.state.check_supply().unwrap();
    }

    fn advance_to(&mut self, h: u64) {
        while self.height < h {
            self.block(vec![]);
        }
    }

    fn seq(&self) -> u64 {
        self.state.defense().seq
    }

    fn attest_n(&self, n: usize, payload: &[u8]) -> Vec<Attestation> {
        validators()[..n]
            .iter()
            .map(|k| attest(k, &self.g.network_id, payload))
            .collect()
    }

    fn transition(&self, to: DefenseMode, n: usize) -> Transaction {
        let seq = self.seq();
        let p = payload::transition(seq, to, &EV);
        self.tx(
            &relayer(),
            TxKind::DefenseTransition {
                to,
                evidence: EV,
                seq,
                attestations: self.attest_n(n, &p),
            },
        )
    }

    fn apply(&mut self, tx: Transaction) {
        self.check(&tx).expect("decisão válida");
        self.block(vec![tx]);
    }

    fn incident(&self) -> Hash32 {
        self.state.defense().active_incident.expect("incidente")
    }

    fn update(&self, status: IncidentStatus, n: usize) -> Transaction {
        let seq = self.seq();
        let inc = self.incident();
        let p = payload::incident_update(seq, &inc, status, &EV);
        self.tx(
            &relayer(),
            TxKind::IncidentUpdate {
                incident: inc,
                status,
                evidence: EV,
                seq,
                attestations: self.attest_n(n, &p),
            },
        )
    }

    fn close(&self, n: usize) -> Transaction {
        let seq = self.seq();
        let inc = self.incident();
        let archive = Hash32([0xa5; 32]);
        let p = payload::close(seq, &inc, &archive);
        self.tx(
            &relayer(),
            TxKind::CloseIncident {
                incident: inc,
                archive,
                seq,
                attestations: self.attest_n(n, &p),
            },
        )
    }

    fn grant(&self, holder: &SecretKey, scopes: Vec<Scope>, lifetime: u64) -> Transaction {
        let seq = self.seq();
        let inc = self.incident();
        let expires_at = self.height + 1 + lifetime;
        let p = payload::grant(seq, &inc, &holder.public_key(), &scopes, expires_at);
        self.tx(
            &relayer(),
            TxKind::GrantCredential {
                incident: inc,
                holder: holder.public_key(),
                scopes,
                expires_at,
                seq,
                attestations: self.attest_n(3, &p),
            },
        )
    }

    fn action(&self, holder: &SecretKey, credential: Hash32, scope: Scope) -> Transaction {
        self.tx(
            holder,
            TxKind::DefenseAction {
                credential,
                scope,
                subject: Hash32([0x0d; 32]),
            },
        )
    }

    /// Abre um incidente e concede ao validador 1 uma credencial.
    fn incident_with_credential(&mut self, scopes: Vec<Scope>) -> Hash32 {
        let t = self.transition(DefenseMode::Incident, 3);
        self.apply(t);
        let g = self.grant(&validators()[1], scopes, 10);
        let id = g.id().0;
        self.apply(g);
        id
    }
}

// AT-DEF-001 — anomalia sem evidência de ataque: VIGILÂNCIA com mais de
// 1/3, mas nada de incidente, credenciais ou punição.
#[test]
fn at_def_001_anomaly_only_vigilance() {
    let mut sim = Sim::new();
    // Um validador sozinho (1/4) não muda o modo.
    assert!(matches!(
        sim.check(&sim.transition(DefenseMode::Vigilance, 1)),
        Err(TxError::Defense(_))
    ));
    let t = sim.transition(DefenseMode::Vigilance, 2);
    sim.apply(t);
    assert_eq!(sim.state.defense().mode, DefenseMode::Vigilance);
    assert!(sim.state.defense().active_incident.is_none());
    // Incidente exige mais de 2/3; 2 de 4 não bastam.
    assert!(matches!(
        sim.check(&sim.transition(DefenseMode::Incident, 2)),
        Err(TxError::Defense(_))
    ));
    // Sem incidente não há credenciais nem bloqueios aplicados.
    let validators_before = sim.state.validators().clone();
    sim.block(vec![]);
    assert_eq!(sim.state.validators(), &validators_before);
    assert!(sim.state.defense().credentials.is_empty());
}

// AT-DEF-002 — incidente verificável: limitar, isolar, registrar e
// preservar evidências.
#[test]
fn at_def_002_incident_isolates_and_records() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::Isolate, Scope::PreserveEvidence]);
    let inc = sim.incident();
    assert_eq!(sim.state.defense().mode, DefenseMode::Incident);
    let a = sim.action(&validators()[1], cred, Scope::Isolate);
    sim.apply(a);
    let d = sim.state.defense();
    assert!(d.isolated(sim.height).contains(&Hash32([0x0d; 32])));
    let i = &d.incidents[&inc];
    assert_eq!(i.actions.len(), 1);
    assert_eq!(i.evidence, vec![EV]);
}

// AT-DEF-003 — evidência ou atestação falsificada é rejeitada, inclusive
// a repetição de atestações antigas.
#[test]
fn at_def_003_forged_or_replayed_rejected() {
    let mut sim = Sim::new();
    let mut forged = sim.transition(DefenseMode::Incident, 3);
    if let Transaction::Account(a) = &mut forged {
        if let TxKind::DefenseTransition { attestations, .. } = &mut a.body.kind {
            attestations[2].signature.0[5] ^= 1;
        }
        *a = match a.body.clone().sign(&relayer(), &sim.g.network_id).unwrap() {
            Transaction::Account(x) => x,
            Transaction::Private(_) => unreachable!(),
        };
    }
    assert_eq!(
        sim.check(&forged),
        Err(TxError::Defense("atestação inválida"))
    );
    // Evidência ausente.
    let seq = sim.seq();
    let p = payload::transition(seq, DefenseMode::Vigilance, &Hash32::ZERO);
    let none = sim.tx(
        &relayer(),
        TxKind::DefenseTransition {
            to: DefenseMode::Vigilance,
            evidence: Hash32::ZERO,
            seq,
            attestations: sim.attest_n(2, &p),
        },
    );
    assert_eq!(sim.check(&none), Err(TxError::Defense("evidência ausente")));
    // Repetição: as mesmas atestações depois que `seq` avançou.
    let first = sim.transition(DefenseMode::Vigilance, 2);
    let replay_kind = match &first {
        Transaction::Account(a) => a.body.kind.clone(),
        Transaction::Private(_) => unreachable!(),
    };
    sim.apply(first);
    let replay = sim.tx(&relayer(), replay_kind);
    assert_eq!(
        sim.check(&replay),
        Err(TxError::Defense("sequência de defesa desatualizada"))
    );
}

// AT-CYBER-001 e AT-CYBER-002 — guerra só a partir de incidente aberto e
// persistente, com mais de 2/3.
#[test]
fn at_cyber_001_002_war_activation_rules() {
    let mut sim = Sim::new();
    assert!(sim
        .check(&sim.transition(DefenseMode::CyberWar, 4))
        .is_err());
    let t = sim.transition(DefenseMode::Incident, 3);
    sim.apply(t);
    // Ainda não persistente.
    assert!(sim
        .check(&sim.transition(DefenseMode::CyberWar, 4))
        .is_err());
    sim.advance_to(sim.height + 3);
    // Persistente, mas sem atestações suficientes.
    assert!(sim
        .check(&sim.transition(DefenseMode::CyberWar, 2))
        .is_err());
    let t = sim.transition(DefenseMode::CyberWar, 3);
    sim.apply(t);
    assert_eq!(sim.state.defense().mode, DefenseMode::CyberWar);
}

// AT-CYBER-003 — a credencial só autoriza operações do seu escopo, e só
// para o seu portador.
#[test]
fn at_cyber_003_credential_scope_limited() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::Diagnose]);
    assert!(sim
        .check(&sim.action(&validators()[1], cred, Scope::Diagnose))
        .is_ok());
    assert!(matches!(
        sim.check(&sim.action(&validators()[1], cred, Scope::Isolate)),
        Err(TxError::Defense(_))
    ));
    assert!(matches!(
        sim.check(&sim.action(&validators()[2], cred, Scope::Diagnose)),
        Err(TxError::Defense(_))
    ));
}

// AT-CYBER-004 — credencial vencida não autoriza mais.
#[test]
fn at_cyber_004_credential_expires() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::Recover]);
    let expires = sim.state.defense().credentials[&cred].expires_at;
    // `check` valida para o próximo bloco (altura + 1).
    sim.advance_to(expires - 2);
    assert!(sim
        .check(&sim.action(&validators()[1], cred, Scope::Recover))
        .is_ok());
    sim.advance_to(expires - 1);
    assert!(sim
        .check(&sim.action(&validators()[1], cred, Scope::Recover))
        .is_err());
}

// AT-CYBER-005 — revogação por mais de 1/3 encerra a autorização.
#[test]
fn at_cyber_005_credential_revoked() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::Coordinate]);
    let seq = sim.seq();
    let p = payload::revoke(seq, &cred);
    let revoke = sim.tx(
        &relayer(),
        TxKind::RevokeCredential {
            credential: cred,
            seq,
            attestations: sim.attest_n(2, &p),
        },
    );
    sim.apply(revoke);
    assert!(sim
        .check(&sim.action(&validators()[1], cred, Scope::Coordinate))
        .is_err());
}

// Credenciais só para quem tem histórico verificável (validador ou pontos
// de contribuição) — mais reputação não é mais soberania.
#[test]
fn credential_requires_verifiable_history() {
    let mut sim = Sim::new();
    let t = sim.transition(DefenseMode::Incident, 3);
    sim.apply(t);
    assert_eq!(
        sim.check(&sim.grant(&outsider(), vec![Scope::Diagnose], 5)),
        Err(TxError::Defense("portador sem histórico verificável"))
    );
    // Validade acima do máximo.
    assert!(sim
        .check(&sim.grant(&validators()[1], vec![Scope::Diagnose], 1_000))
        .is_err());
}

// AT-INC-001, AT-INC-002 e AT-INC-003 — encerramento só após contenção e
// recuperação; evidências preservadas; retorno gradual.
#[test]
fn at_inc_001_002_003_closing_rules() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::PreserveEvidence]);
    let inc = sim.incident();
    let a = sim.action(&validators()[1], cred, Scope::PreserveEvidence);
    sim.apply(a);
    // Encerramento prematuro e estados fora de ordem: rejeitados.
    assert!(matches!(sim.check(&sim.close(4)), Err(TxError::Defense(_))));
    assert!(sim
        .check(&sim.update(IncidentStatus::Recovered, 3))
        .is_err());
    let u = sim.update(IncidentStatus::Contained, 3);
    sim.apply(u);
    assert!(sim.check(&sim.close(4)).is_err());
    let u = sim.update(IncidentStatus::Recovered, 3);
    sim.apply(u);
    // Encerramento exige mais de 2/3.
    assert!(sim.check(&sim.close(2)).is_err());
    let c = sim.close(3);
    sim.apply(c);

    let d = sim.state.defense();
    let i = &d.incidents[&inc];
    assert_eq!(i.status, IncidentStatus::Closed);
    // Evidências e ações preservadas: abertura, contenção, recuperação,
    // pacote final.
    assert_eq!(i.evidence.len(), 4);
    assert_eq!(i.evidence[3], Hash32([0xa5; 32]));
    assert_eq!(i.actions.len(), 1);
    // Credenciais do incidente revogadas; rede volta em VIGILÂNCIA.
    assert!(d.credentials[&cred].revoked);
    assert_eq!(d.mode, DefenseMode::Vigilance);
    assert!(d.active_incident.is_none());
    // E desce para NORMAL com mais de 1/3.
    let t = sim.transition(DefenseMode::Normal, 2);
    sim.apply(t);
    assert_eq!(sim.state.defense().mode, DefenseMode::Normal);
}

// THR-DEF-001, INV-010 — sem renovação, todo modo vence e desce um nível;
// um incidente esquecido sai de vigor e revoga suas credenciais.
#[test]
fn modes_expire_without_renewal() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::Diagnose]);
    let inc = sim.incident();
    sim.advance_to(sim.height + 3);
    let t = sim.transition(DefenseMode::CyberWar, 3);
    sim.apply(t);
    let war_end = sim.state.defense().mode_expires_at;
    sim.advance_to(war_end);
    assert_eq!(sim.state.defense().mode, DefenseMode::Incident);
    // Renovar exige a mesma atestação de entrada.
    assert!(sim
        .check(&sim.transition(DefenseMode::Incident, 2))
        .is_err());
    let incident_end = sim.state.defense().mode_expires_at;
    sim.advance_to(incident_end);
    let d = sim.state.defense();
    assert_eq!(d.mode, DefenseMode::Vigilance);
    assert_eq!(d.incidents[&inc].status, IncidentStatus::Lapsed);
    assert!(d.credentials[&cred].revoked);
    let vigilance_end = d.mode_expires_at;
    sim.advance_to(vigilance_end);
    assert_eq!(sim.state.defense().mode, DefenseMode::Normal);
}

// SPEC §57 — participação não gera registro; só contribuição atestada após
// o encerramento.
#[test]
fn contribution_record_distinct_from_participation() {
    let mut sim = Sim::new();
    let cred = sim.incident_with_credential(vec![Scope::Recover]);
    let inc = sim.incident();
    let a = sim.action(&validators()[1], cred, Scope::Recover);
    sim.apply(a);
    let holder = validators()[1].public_key();
    assert!(!sim.state.defense().records.contains_key(&holder));

    let contribution = |sim: &Sim| {
        let seq = sim.seq();
        let p = payload::contribution(seq, &inc, &holder, Scope::Recover, &EV);
        sim.tx(
            &relayer(),
            TxKind::AttestContribution {
                incident: inc,
                node: holder,
                role: Scope::Recover,
                evidence: EV,
                seq,
                attestations: sim.attest_n(3, &p),
            },
        )
    };
    // Antes do encerramento: rejeitada.
    assert!(sim.check(&contribution(&sim)).is_err());
    for s in [IncidentStatus::Contained, IncidentStatus::Recovered] {
        let u = sim.update(s, 3);
        sim.apply(u);
    }
    let c = sim.close(3);
    sim.apply(c);
    let t = contribution(&sim);
    sim.apply(t);
    let records = &sim.state.defense().records[&holder];
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].incident, inc);
}
