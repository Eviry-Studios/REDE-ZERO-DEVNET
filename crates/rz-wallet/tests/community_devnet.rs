//! Comunidades e nomes de ponta a ponta contra Nodes reais (ADR-0015):
//! declaração, reconhecimento por governança, posição, nomes, e o mesmo
//! estado em Nodes independentes (AT-COM-004).

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_codec::Encode;
use rz_core::community::{approve, position_payload, CommunityStatus, DecisionRule, NameKind};
use rz_core::governance::{Category, Choice};
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::{Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{
    community, connect, fetch_content, fetch_manifest, governance, publish_content,
    publish_manifest, resolve, send_account, Keys,
};

fn a_manifest(bytes: &[u8]) -> Hash32 {
    rz_core::content::Manifest::hash_of(bytes)
}

const NET: &str = "rede-zero-devnet-com";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([150 + i; 32]))
        .collect()
}

fn faucet() -> Keys {
    Keys::from_secret(SecretKey::from_seed([221; 32]))
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
            amount: 100 * rz_core::UNITS_PER_ZERO,
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
    }
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let dir =
        std::env::temp_dir().join(format!("rz-com-{name}-{}-{}", std::process::id(), now_ms()));
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
fn community_recognized_and_replicated() {
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
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();

    // Câmara econômica para a votação de reconhecimento.
    let h = n1.status().height;
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

    // 1. Declaração: nome, manifesto e regra de decisão 2 de 3.
    let controllers: Vec<SecretKey> = (90..93).map(|i| SecretKey::from_seed([i; 32])).collect();
    let rule = DecisionRule {
        keys: controllers.iter().map(SecretKey::public_key).collect(),
        threshold: 2,
    };
    assert!(wait_until(|| governance(&mut c, Some(f.address()))
        .map(|v| v.locks.len() == 1)
        .unwrap_or(false)));
    // Interface e manifesto estruturado (spec/CONTENT.md).
    let site = rz_core::content::Bundle::new(vec![rz_core::content::BundleFile {
        path: "index.html".into(),
        mime: "text/html; charset=utf-8".into(),
        data: b"<h1>Cientistas</h1>".to_vec(),
    }])
    .unwrap()
    .to_canonical_bytes();
    let frontend = rz_core::content::describe(&site).unwrap().id();
    let manifest = rz_core::content::Manifest {
        name: "cientistas".into(),
        version: 1,
        description: "Comunidade de ciência aberta".into(),
        frontend: Some(frontend),
        module: None,
    }
    .to_bytes();
    let cid = send_account(
        &mut c,
        &g,
        &f,
        TxKind::DeclareCommunity {
            name: "cientistas".into(),
            manifest_hash: rz_core::content::Manifest::hash_of(&manifest),
            rule,
        },
        10,
    )
    .unwrap()
    .0;
    assert!(wait_until(|| community(&mut c2, "cientistas")
        .map(|(_, c)| c.is_some_and(|c| c.status == CommunityStatus::Declared))
        .unwrap_or(false)));
    // O manifesto de uma Comunidade declarada já pode ser hospedado; a
    // interface, só depois do reconhecimento.
    publish_manifest(&mut c, &manifest).unwrap();
    assert!(publish_content(&mut c, &site).is_err());

    // 2. Reconhecimento por proposta da categoria Comunidade (N-7).
    let prop = send_account(
        &mut c,
        &g,
        &f,
        TxKind::Propose {
            category: Category::Community,
            content_hash: cid,
            params: vec![],
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
            .and_then(|v| v.proposals.into_iter().find(|p| p.id == prop));
        proposal.is_some()
    }));
    let p = proposal.unwrap();
    assert!(wait_until(|| n1.status().height + 1 >= p.voting_start));
    for k in [
        faucet(),
        Keys::from_secret(SecretKey::from_seed(vs[0].seed())),
        Keys::from_secret(SecretKey::from_seed(vs[1].seed())),
    ] {
        send_account(
            &mut c,
            &g,
            &k,
            TxKind::Vote {
                proposal: prop,
                choice: Choice::Yes,
            },
            10,
        )
        .unwrap();
    }
    assert!(
        wait_until(|| community(&mut c, "cientistas")
            .map(|(_, c)| c.is_some_and(|c| c.status == CommunityStatus::Recognized))
            .unwrap_or(false)),
        "Comunidade não reconhecida"
    );

    // 3. zero://cientistas.comunidade resolve para o id em ambos os Nodes.
    assert_eq!(
        resolve(&mut c, "cientistas", NameKind::Community)
            .unwrap()
            .0,
        Some(cid)
    );
    assert_eq!(
        resolve(&mut c2, "cientistas", NameKind::Community)
            .unwrap()
            .0,
        Some(cid)
    );

    // 4. Nome de aplicação registrado num Node, visível no outro.
    send_account(
        &mut c,
        &g,
        &f,
        TxKind::RegisterName {
            name: "laboratorio".into(),
            kind: NameKind::App,
            target: Hash32([0x1a; 32]),
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| resolve(
        &mut c2,
        "laboratorio",
        NameKind::App
    )
    .map(|(t, _)| t == Some(Hash32([0x1a; 32])))
    .unwrap_or(false)));

    // Interface publicada num Node e obtida pelo outro, verificada contra o
    // manifesto registrado (replicação sob demanda).
    assert_eq!(publish_content(&mut c, &site).unwrap(), frontend);
    let m = fetch_manifest(&mut c2, &a_manifest(&manifest), Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(m, manifest);
    assert_eq!(
        fetch_content(&mut c2, &frontend, Duration::from_secs(10)).unwrap(),
        site
    );

    // AT-COM-004 — o registro da Comunidade é idêntico nos dois Nodes.
    let a = community(&mut c, "cientistas").unwrap().1.unwrap();
    let b = community(&mut c2, "cientistas").unwrap().1.unwrap();
    assert_eq!(a, b);

    // 5. Posição da Comunidade sobre outra proposta, em votação: 1 de 3
    // aprovações é recusada; 2 de 3 é aceita (N-6). Qualquer conta pode
    // retransmitir (aqui, o faucet, que paga a taxa).
    let other = send_account(
        &mut c,
        &g,
        &f,
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([0x77; 32]),
            params: vec![],
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
            .and_then(|v| v.proposals.into_iter().find(|p| p.id == other));
        proposal.is_some()
    }));
    let p = proposal.unwrap();
    assert!(wait_until(|| n1.status().height > p.voting_start));
    let payload = position_payload(&cid, &other, Choice::No);
    let position = |signers: &[SecretKey]| TxKind::CommunityPosition {
        community: cid,
        proposal: other,
        choice: Choice::No,
        approvals: signers.iter().map(|k| approve(k, NET, &payload)).collect(),
    };
    let err = send_account(&mut c, &g, &f, position(&controllers[..1]), 10).unwrap_err();
    assert!(err.contains("comunidade"), "{err}");
    send_account(&mut c, &g, &f, position(&controllers[1..]), 10).unwrap();
}
