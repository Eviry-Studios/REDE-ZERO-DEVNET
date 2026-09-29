//! Fluxo privado completo contra Nodes reais (REQ-023, REQ-024).

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rz_codec::Encode;
use rz_core::{Allocation, Genesis, NetworkKind, Transaction, PROTOCOL_VERSION};
use rz_crypto::SecretKey;
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{account, connect, private_view, send_private, shield, Destination, Keys};

const NET: &str = "rede-zero-devnet-priv";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([150 + i; 32]))
        .collect()
}

fn faucet() -> Keys {
    Keys::from_secret(SecretKey::from_seed([210; 32]))
}

fn alice() -> Keys {
    Keys::from_secret(SecretKey::from_seed([211; 32]))
}

fn genesis() -> Genesis {
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: NET.into(),
        genesis_time_ms: now_ms(),
        slot_duration_ms: 250,
        finality_depth: 3,
        min_fee: 10,
        max_block_txs: 100,
        validators: validators().iter().map(SecretKey::public_key).collect(),
        allocations: vec![Allocation {
            address: faucet().address(),
            amount: 1_000_000,
        }],
    }
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "rz-priv-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let mut cfg = NodeConfig::new(g.clone(), dir, "127.0.0.1:0".parse().unwrap());
    cfg.validator_key = Some(key);
    cfg.bootstrap = peers;
    cfg.log = LogLevel::Quiet;
    Node::start(cfg).unwrap()
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn private_flow_end_to_end() {
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
        || n1.status().peers >= 1 && n2.status().height >= 2
    ));

    let f = faucet();
    let mut c1 = connect(n1.listen_addr(), &g).unwrap();

    // 1. Blindagens: cria notas (as do faucet também servem de disfarce).
    for (i, amount) in [50_000u64, 7_000, 3_000, 900].into_iter().enumerate() {
        shield(&mut c1, &g, &f, &f.shielded_address(), amount, 10).unwrap();
        assert!(
            wait_until(|| n1.account(&f.address()).nonce == i as u64 + 1),
            "blindagem {i} não incluída"
        );
    }
    let (_, notes) = private_view(&mut c1, &f).unwrap();
    assert_eq!(notes.iter().map(|n| n.note.amount).sum::<u64>(), 60_900);

    // 2. Envio privado faucet → Alice, através do node 1.
    let a = alice();
    let id = send_private(
        &mut c1,
        &g,
        &f,
        Destination::Shielded(a.shielded_address()),
        12_345,
        10,
    )
    .unwrap();

    // 3. Alice encontra a nota consultando o node 2.
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();
    assert!(wait_until(|| {
        private_view(&mut c2, &a)
            .map(|(_, n)| n.iter().map(|x| x.note.amount).sum::<u64>() == 12_345)
            .unwrap_or(false)
    }));

    // A transação publicada não contém o endereço da Alice, o do faucet,
    // nem o valor em claro.
    let (block_tx, tx_bytes) = (1..=n2.status().height)
        .filter_map(|h| n2.block_at(h))
        .flat_map(|b| b.txs)
        .find(|t| t.id() == id)
        .map(|t| {
            let bytes = t.to_canonical_bytes();
            (t, bytes)
        })
        .expect("transação privada em algum bloco");
    assert!(matches!(block_tx, Transaction::Private(_)));
    let contains = |needle: &[u8]| tx_bytes.windows(needle.len()).any(|w| w == needle);
    assert!(!contains(&a.shielded_address().to_bytes()[..32]));
    assert!(!contains(&a.shielded_address().to_bytes()[32..]));
    assert!(!contains(f.address().0.as_bytes()));
    assert!(!contains(f.transparent.public_key().as_bytes()));
    assert!(!contains(&12_345u64.to_be_bytes()));

    // 4. Alice retira parte para sua conta transparente (valor e destino
    //    públicos; origem oculta).
    send_private(
        &mut c2,
        &g,
        &a,
        Destination::Transparent(a.address()),
        2_000,
        10,
    )
    .unwrap();
    assert!(wait_until(|| n1.account(&a.address()).balance == 2_000));
    assert!(wait_until(|| {
        private_view(&mut c1, &a)
            .map(|(_, n)| n.iter().map(|x| x.note.amount).sum::<u64>() == 12_345 - 2_010)
            .unwrap_or(false)
    }));
    let (bal, _, _) = account(&mut c1, a.address()).unwrap();
    assert_eq!(bal, 2_000);
}
