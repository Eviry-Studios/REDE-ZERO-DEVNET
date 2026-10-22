//! Atualização verificável de ponta a ponta contra Nodes reais (ADR-0021):
//! manifesto montado a partir de arquivos, publicado, adotado por proposta
//! constitucional e verificado; os Nodes passam a avisar que a versão em
//! execução difere da adotada.

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_core::governance::{Category, Choice};
use rz_core::release::{file_hash, Artifact, Compatibility, Release, UpgradeNotice};
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::{Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{account, connect, governance, releases, send_account, tree_hash_of, Keys};

const NET: &str = "rede-zero-devnet-versoes";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([200 + i; 32]))
        .collect()
}

fn faucet() -> Keys {
    Keys::from_secret(SecretKey::from_seed([230; 32]))
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
            constitutional_delay_blocks: 5,
            lock_min_blocks: 5,
            lock_max_blocks: 500,
            contribution_half_life_blocks: 10_000,
        },
        assets: vec![],
        bridges: vec![],
        monetary: Default::default(),
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "rz-versoes-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ))
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let mut cfg = NodeConfig::new(g.clone(), temp_dir(name), "127.0.0.1:0".parse().unwrap());
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
fn release_published_adopted_and_verified() {
    // Árvore de origem: o hash não depende da ordem de leitura e ignora
    // arquivos ocultos.
    let src = temp_dir("fonte");
    std::fs::create_dir_all(src.join("spec/sub")).unwrap();
    std::fs::write(src.join("spec/A.md"), b"regra A").unwrap();
    std::fs::write(src.join("spec/sub/B.md"), b"regra B").unwrap();
    std::fs::write(src.join("spec/.rascunho"), b"ignorado").unwrap();
    let spec = tree_hash_of(&src, &["spec"]).unwrap();
    std::fs::write(src.join("spec/.rascunho"), b"mudou").unwrap();
    assert_eq!(tree_hash_of(&src, &["spec/"]).unwrap(), spec);

    let binary = b"binario reproduzivel do node".to_vec();
    let release = Release {
        version: "9.9.9".into(),
        protocol_version: PROTOCOL_VERSION + 1,
        p2p_version: rz_p2p::P2P_VERSION,
        compatibility: Compatibility::Incompatible,
        previous: None,
        specification: spec,
        implementation: Hash32([2; 32]),
        tests: Hash32([3; 32]),
        artifacts: vec![Artifact {
            name: "rede-zero-node".into(),
            hash: file_hash(&binary),
        }],
        notes: Hash32::ZERO,
    };
    let rid = release.id();

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
    assert_eq!(n1.status().upgrade, UpgradeNotice::UpToDate);
    let f = faucet();
    let mut c = connect(n1.listen_addr(), &g).unwrap();
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();

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
    send_account(
        &mut c,
        &g,
        &f,
        TxKind::PublishRelease {
            release: release.clone(),
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| releases(&mut c2)
        .map(|(_, cur, list)| cur.is_none() && list.iter().any(|r| r.release == release))
        .unwrap_or(false)));

    let pid = send_account(
        &mut c,
        &g,
        &f,
        TxKind::Propose {
            category: Category::Constitutional,
            content_hash: Hash32([5; 32]),
            params: vec![],
            release_id: Some(rid),
            deposit: 1_000,
        },
        10,
    )
    .unwrap()
    .0;
    let mut start_at = None;
    assert!(wait_until(|| {
        start_at = governance(&mut c, None)
            .ok()
            .and_then(|v| v.proposals.into_iter().find(|p| p.id == pid))
            .map(|p| p.voting_start);
        start_at.is_some()
    }));
    assert!(wait_until(|| n1.status().height + 1 >= start_at.unwrap()));
    for k in [
        f,
        Keys::from_secret(SecretKey::from_seed(vs[0].seed())),
        Keys::from_secret(SecretKey::from_seed(vs[1].seed())),
    ] {
        send_account(
            &mut c,
            &g,
            &k,
            TxKind::Vote {
                proposal: pid,
                choice: Choice::Yes,
            },
            10,
        )
        .unwrap();
    }

    // Adotada nos dois Nodes; ambos avisam que precisam de atualização.
    assert!(
        wait_until(|| releases(&mut c2)
            .map(|(_, cur, _)| cur == Some(rid))
            .unwrap_or(false)),
        "versão não adotada"
    );
    for n in [&n1, &n2] {
        assert!(wait_until(|| matches!(
            n.status().upgrade,
            UpgradeNotice::Required { ref version, .. } if version == "9.9.9"
        )));
    }

    // Verificação do binário baixado contra o manifesto adotado.
    let (_, _, list) = releases(&mut c).unwrap();
    let adopted = list.into_iter().find(|r| r.release.id() == rid).unwrap();
    assert!(adopted.adopted_at.is_some());
    let a = adopted.release.artifact("rede-zero-node").unwrap();
    assert_eq!(a.hash, file_hash(&binary));
    assert_ne!(a.hash, file_hash(b"binario adulterado"));
    assert_eq!(
        adopted.release.specification,
        tree_hash_of(&src, &["spec"]).unwrap()
    );
}
