//! Rotação de chave de conta de ponta a ponta contra Nodes reais
//! (ADR-0023): a conta mantém o endereço e a chave antiga para de assinar.

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::SecretKey;
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{
    account, account_address, connect, key_info, send_account, send_transparent, Keys,
};

const NET: &str = "rede-zero-devnet-chaves";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([210 + i; 32]))
        .collect()
}

fn faucet() -> Keys {
    Keys::from_secret(SecretKey::from_seed([231; 32]))
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
        "rz-chaves-{name}-{}-{}",
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
fn key_rotation_end_to_end() {
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
    let acc = f.address();
    let mut c = connect(n1.listen_addr(), &g).unwrap();
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();
    let new = Keys::from_secret(SecretKey::from_seed([232; 32]));
    let proof = new.transparent.sign(
        rz_core::rotation::KEY_ROTATION,
        &g.network_id,
        &rz_core::rotation::rotation_payload(&acc, &new.transparent.public_key()),
    );
    send_account(
        &mut c,
        &g,
        &f,
        TxKind::RotateKey {
            new_key: new.transparent.public_key(),
            proof,
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| account_address(&mut c2, &new)
        .map(|a| a == acc)
        .unwrap_or(false)));
    let old = key_info(&mut c2, f.transparent.public_key()).unwrap();
    assert_eq!(old.account, None);
    assert_eq!(old.current, Some(new.transparent.public_key()));

    // A chave antiga não assina mais; a nova move o saldo da mesma conta.
    let to = SecretKey::from_seed([233; 32]).public_key().address();
    assert!(send_transparent(&mut c, &g, &f, to, 1_000, 10).is_err());
    let (before, _, _) = account(&mut c, acc).unwrap();
    send_transparent(&mut c, &g, &new, to, 1_000, 10).unwrap();
    assert!(wait_until(|| account(&mut c2, to)
        .map(|(b, _, _)| b == 1_000)
        .unwrap_or(false)));
    assert_eq!(account(&mut c2, acc).unwrap().0, before - 1_010);
}
