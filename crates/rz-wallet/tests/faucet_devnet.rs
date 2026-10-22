//! Faucet de ponta a ponta contra Nodes reais (`docs/TESTNET.md`): prova
//! de trabalho, pagamento e limite por endereço.

#![allow(clippy::unwrap_used)]

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, PROTOCOL_VERSION};
use rz_crypto::SecretKey;
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::faucet::{request, serve, FaucetConfig};
use rz_wallet::{account, connect, Keys};

const NET: &str = "rede-zero-devnet-faucet";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([160 + i; 32]))
        .collect()
}

fn faucet() -> Keys {
    Keys::from_secret(SecretKey::from_seed([170; 32]))
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
        "rz-faucet-{name}-{}-{}",
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
fn faucet_pays_once_per_address() {
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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let faucet_addr = listener.local_addr().unwrap();
    let (gs, client) = (g.clone(), connect(n1.listen_addr(), &g).unwrap());
    std::thread::spawn(move || {
        serve(
            listener,
            gs,
            faucet(),
            client,
            FaucetConfig {
                amount: 5_000,
                cooldown: Duration::from_secs(3_600),
                bits: 10,
                max_per_hour: 10,
                fee: 10,
            },
        )
    });
    let ask = |a| request(TcpStream::connect(faucet_addr).unwrap(), &g.network_id, &a);
    let a = SecretKey::from_seed([171; 32]).public_key().address();
    let b = SecretKey::from_seed([172; 32]).public_key().address();
    ask(a).unwrap();
    ask(b).unwrap();
    assert!(ask(a).unwrap_err().contains("já atendido"));
    // Rede errada: recusada antes de pagar.
    assert!(request(TcpStream::connect(faucet_addr).unwrap(), "outra-rede", &a).is_err());
    let mut c = connect(n2.listen_addr(), &g).unwrap();
    assert!(wait_until(
        || account(&mut c, a).unwrap().0 == 5_000 && account(&mut c, b).unwrap().0 == 5_000
    ));
}
