//! Defesa da Exonet de ponta a ponta contra Nodes reais (ADR-0016,
//! AT-DEF-002): incidente atestado por validadores, credencial temporária de
//! isolamento e Nodes que deixam de se conectar ao Node isolado.

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_core::defense::{attest, payload, Attestation, DefenseMode, Scope};
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::{Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{connect, defense, send_account, Keys};

const NET: &str = "rede-zero-devnet-def";

fn validators() -> Vec<SecretKey> {
    (0..3)
        .map(|i| SecretKey::from_seed([160 + i; 32]))
        .collect()
}

fn genesis() -> Genesis {
    let mut allocations: Vec<Allocation> = validators()
        .iter()
        .map(|v| Allocation {
            address: v.public_key().address(),
            amount: 1_000_000,
        })
        .collect();
    allocations.sort_by_key(|a| a.address);
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: NET.into(),
        consensus: rz_core::ConsensusParams::fast(200),
        min_fee: 10,
        max_block_txs: 100,
        validators: validators()
            .iter()
            .map(|k| rz_core::GenesisValidator::new(k.public_key(), 100))
            .collect(),
        allocations,
        governance: GovernanceParams::default(),
        assets: vec![],
        bridges: vec![],
    }
}

fn start(g: &Genesis, name: &str, key: Option<SecretKey>, peers: Vec<SocketAddr>) -> Node {
    let dir =
        std::env::temp_dir().join(format!("rz-def-{name}-{}-{}", std::process::id(), now_ms()));
    let mut cfg = NodeConfig::new(g.clone(), dir, "127.0.0.1:0".parse().unwrap());
    cfg.validator_key = key;
    cfg.bootstrap = peers.into_iter().map(Into::into).collect();
    cfg.log = LogLevel::Quiet;
    Node::start(cfg).unwrap()
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn attest_all(p: &[u8]) -> Vec<Attestation> {
    validators().iter().map(|k| attest(k, NET, p)).collect()
}

#[test]
fn attested_isolation_disconnects_node() {
    let g = genesis();
    let vs = validators();
    // Node alvo, não validador; o primeiro validador disca para ele.
    let target = start(&g, "alvo", None, vec![]);
    let n1 = start(
        &g,
        "a",
        Some(SecretKey::from_seed(vs[0].seed())),
        vec![target.listen_addr()],
    );
    let n2 = start(
        &g,
        "b",
        Some(SecretKey::from_seed(vs[1].seed())),
        vec![n1.listen_addr()],
    );
    let n3 = start(
        &g,
        "c",
        Some(SecretKey::from_seed(vs[2].seed())),
        vec![n1.listen_addr()],
    );
    let target_id = target.node_id().0;
    assert!(wait_until(
        || n1.outbound_node_ids().contains(&target_id) && n3.status().height >= 2
    ));

    let payer = Keys::from_secret(SecretKey::from_seed(vs[2].seed()));
    let mut c = connect(n2.listen_addr(), &g).unwrap();

    // 1. Incidente com evidência e atestação de mais de 2/3 (aqui, todos).
    let evidence = Hash32([0xe1; 32]);
    let seq = defense(&mut c).unwrap().seq;
    let p = payload::transition(seq, DefenseMode::Incident, &evidence);
    send_account(
        &mut c,
        &g,
        &payer,
        TxKind::DefenseTransition {
            to: DefenseMode::Incident,
            evidence,
            seq,
            attestations: attest_all(&p),
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| defense(&mut c)
        .map(|d| d.mode == DefenseMode::Incident && d.incident.is_some())
        .unwrap_or(false)));

    // 2. Credencial de isolamento para o validador 0.
    let d = defense(&mut c).unwrap();
    let incident = d.incident.unwrap().id;
    let holder = vs[0].public_key();
    let expires_at = d.height + 200;
    let scopes = vec![Scope::Isolate];
    let p = payload::grant(d.seq, &incident, &holder, &scopes, expires_at);
    let cred = send_account(
        &mut c,
        &g,
        &payer,
        TxKind::GrantCredential {
            incident,
            holder,
            scopes,
            expires_at,
            seq: d.seq,
            attestations: attest_all(&p),
        },
        10,
    )
    .unwrap()
    .0;
    assert!(wait_until(|| defense(&mut c)
        .map(|d| d.credentials.iter().any(|c| c.id == cred))
        .unwrap_or(false)));

    // 3. O portador isola o Node alvo pela identidade criptográfica.
    let holder_keys = Keys::from_secret(SecretKey::from_seed(vs[0].seed()));
    send_account(
        &mut c,
        &g,
        &holder_keys,
        TxKind::DefenseAction {
            credential: cred,
            scope: Scope::Isolate,
            subject: target_id,
        },
        10,
    )
    .unwrap();

    // 4. Finalizada a ação, o validador 0 corta a conexão e não volta a
    // discar; o alvo continua funcionando (defesa, não retaliação).
    assert!(
        wait_until(|| !n1.outbound_node_ids().contains(&target_id)),
        "conexão com o Node isolado não foi encerrada"
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(!n1.outbound_node_ids().contains(&target_id));
    for n in [&n1, &n2, &n3] {
        assert!(!n.outbound_node_ids().contains(&target_id));
    }
    let _ = target.status();
}
