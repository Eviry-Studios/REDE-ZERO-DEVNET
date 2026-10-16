//! Governança de ponta a ponta contra Nodes reais (ADR-0008).

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_core::governance::{Category, Choice, ParamChange};
use rz_core::{
    Allocation, Genesis, GovernanceParams, NetworkKind, ProposalStatus, TxKind, PROTOCOL_VERSION,
};
use rz_crypto::{Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{account, connect, governance, send_account, Keys};

const NET: &str = "rede-zero-devnet-gov";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([170 + i; 32]))
        .collect()
}

fn faucet() -> Keys {
    Keys::from_secret(SecretKey::from_seed([220; 32]))
}

fn genesis() -> Genesis {
    let mut allocations: Vec<Allocation> = validators()
        .iter()
        .map(|v| Allocation {
            address: v.public_key().address(),
            amount: 10_000,
        })
        .chain([Allocation {
            address: faucet().address(),
            amount: 10_000_000,
        }])
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
        governance: GovernanceParams {
            deposit: 1_000,
            analysis_blocks: 3,
            voting_blocks: 25,
            ordinary_delay_blocks: 3,
            constitutional_delay_blocks: 10,
            lock_min_blocks: 5,
            lock_max_blocks: 500,
            contribution_half_life_blocks: 10_000,
        },
        assets: vec![],
        bridges: vec![],
    }
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let dir =
        std::env::temp_dir().join(format!("rz-gov-{name}-{}-{}", std::process::id(), now_ms()));
    let mut cfg = NodeConfig::new(g.clone(), dir, "127.0.0.1:0".parse().unwrap());
    cfg.validator_key = Some(key);
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

#[test]
fn governance_changes_min_fee() {
    let g = genesis();
    let vs = validators();
    let n1 = start(&g, "a", SecretKey::from_seed(vs[0].seed()), vec![]);
    let n2 = start(
        &g,
        "b",
        SecretKey::from_seed(vs[1].seed()),
        vec![n1.listen_addr()],
    );
    assert!(wait_until(
        || n1.status().peers >= 1 && n2.status().height >= 3
    ));

    let f = faucet();
    let mut c = connect(n1.listen_addr(), &g).unwrap();

    // 1. Câmara econômica: o faucet bloqueia ZERO por 300 blocos.
    let (_, _, h) = account(&mut c, f.address()).unwrap();
    send_account(
        &mut c,
        &g,
        &f,
        TxKind::LockStake {
            amount: 5_000_000,
            unlock_height: h + 300,
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| governance(&mut c, Some(f.address()))
        .map(|v| v.locks.len() == 1)
        .unwrap_or(false)));

    // 2. Proposta ordinária: taxa mínima 10 → 25.
    let id = send_account(
        &mut c,
        &g,
        &f,
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([1; 32]),
            params: vec![ParamChange::MinFee(25)],
            release_id: None,
            deposit: 1_000,
        },
        10,
    )
    .unwrap()
    .0;
    let mut proposal = None;
    assert!(wait_until(|| {
        proposal = governance(&mut c, None)
            .ok()
            .and_then(|v| v.proposals.into_iter().find(|p| p.id == id));
        proposal.is_some()
    }));
    let p = proposal.unwrap();

    // 3. Votos: faucet (econômica) e os dois validadores (contribuição).
    assert!(wait_until(|| n1.status().height + 1 >= p.voting_start));
    let voters = [
        f,
        Keys::from_secret(SecretKey::from_seed(vs[0].seed())),
        Keys::from_secret(SecretKey::from_seed(vs[1].seed())),
    ];
    for k in &voters {
        send_account(
            &mut c,
            &g,
            k,
            TxKind::Vote {
                proposal: id,
                choice: Choice::Yes,
            },
            10,
        )
        .unwrap();
    }

    // 4. Apuração e ativação: o parâmetro muda em todos os Nodes.
    assert!(
        wait_until(|| governance(&mut c, None)
            .map(|v| v.params.min_fee == 25)
            .unwrap_or(false)),
        "proposta não ativada"
    );
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();
    let v = governance(&mut c2, None).unwrap();
    assert_eq!(v.params.min_fee, 25);
    let p = v.proposals.iter().find(|p| p.id == id).unwrap();
    assert_eq!(p.status, ProposalStatus::Activated);
    let t = p.tally.unwrap();
    assert!(t.economic.yes > 0 && t.contribution.yes > 0);

    // A taxa antiga passa a ser rejeitada.
    let err = send_account(
        &mut c,
        &g,
        &voters[0],
        TxKind::Transfer {
            to: voters[1].address(),
            amount: 1,
        },
        10,
    )
    .unwrap_err();
    assert!(err.contains("taxa"), "{err}");
}
