//! Ponte e trocas atômicas de ponta a ponta contra Nodes reais (ADR-0019):
//! cabeçalhos de uma rede de origem simulada (regras de regtest), prova de
//! queima extraída de um `merkleblock`, cunhagem verificada nos dois Nodes e
//! troca atômica resgatada pela pré-imagem.

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_core::bridge::{burn_script, check_pow, sha256, sha256d, ChainConfig, Header, PowRules};
use rz_core::market::AssetId;
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::{Address, Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{account, assets, bridge_status, connect, htlc, send_account, Keys};

const NET: &str = "rede-zero-devnet-ponte";
const CHAIN: &str = "bitcoin-regtest";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([190 + i; 32]))
        .collect()
}

fn relayer() -> Keys {
    Keys::from_secret(SecretKey::from_seed([251; 32]))
}

fn alice() -> Keys {
    Keys::from_secret(SecretKey::from_seed([252; 32]))
}

fn rules() -> PowRules {
    let mut limit = [0xffu8; 32];
    limit[0] = 0x7f;
    PowRules {
        pow_limit: Hash32(limit),
        retarget: false,
        interval: 2016,
        target_timespan: 1_209_600,
    }
}

fn mine(prev: Hash32, merkle_root: Hash32, time: u32) -> Header {
    let mut h = Header {
        version: 0x2000_0000,
        prev,
        merkle_root,
        time,
        bits: 0x207f_ffff,
        nonce: 0,
    };
    while check_pow(&h, &rules()).is_err() {
        h.nonce += 1;
    }
    h
}

fn checkpoint() -> Header {
    mine(Hash32::ZERO, Hash32([0x22; 32]), 1_700_000_000)
}

fn genesis() -> Genesis {
    let mut allocations: Vec<Allocation> = validators()
        .iter()
        .map(|v| Allocation {
            address: v.public_key().address(),
            amount: 10_000,
        })
        .chain([relayer(), alice()].iter().map(|k| Allocation {
            address: k.address(),
            amount: 10 * rz_core::UNITS_PER_ZERO,
        }))
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
        bridges: vec![ChainConfig {
            name: CHAIN.into(),
            asset_ref: "BTC".into(),
            decimals: 8,
            rules: rules(),
            checkpoint: checkpoint().serialize(),
            checkpoint_height: 0,
            confirmations: 2,
        }],
        monetary: Default::default(),
    }
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let dir = std::env::temp_dir().join(format!(
        "rz-ponte-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ));
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

/// `merkleblock` (formato de `gettxoutproof`) de duas transações, provando a
/// segunda.
fn merkleblock(header: &Header, txids: &[Hash32; 2]) -> Vec<u8> {
    let mut out = header.serialize().to_vec();
    out.extend(2u32.to_le_bytes());
    out.push(2);
    out.extend(txids[0].0);
    out.extend(txids[1].0);
    // Bits: raiz (pai da prova), folha 0 (não), folha 1 (sim) = 0b101.
    out.push(1);
    out.push(0b101);
    out
}

fn origin_tx(outputs: &[(u64, Vec<u8>)], salt: u8) -> Vec<u8> {
    let mut t = vec![2, 0, 0, 0, 1];
    t.extend([salt; 32]);
    t.extend([0, 0, 0, 0]);
    t.extend([0, 0xff, 0xff, 0xff, 0xff]);
    t.push(outputs.len() as u8);
    for (v, s) in outputs {
        t.extend(v.to_le_bytes());
        t.push(s.len() as u8);
        t.extend(s);
    }
    t.extend([0, 0, 0, 0]);
    t
}

#[test]
fn bridge_and_atomic_swap_end_to_end() {
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
    let mut c1 = connect(n1.listen_addr(), &g).unwrap();
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();
    let r = relayer();
    let a = alice();

    // Na origem: alguém queima 0,5 BTC para a Alice na Rede Zero.
    let other = origin_tx(&[(1_000, vec![0x51])], 1);
    let burn = origin_tx(&[(50_000_000, burn_script(NET, &a.address()))], 2);
    let txids = [Hash32(sha256d(&other)), Hash32(sha256d(&burn))];
    let mut b = [0u8; 64];
    b[..32].copy_from_slice(&txids[0].0);
    b[32..].copy_from_slice(&txids[1].0);
    let root = Hash32(sha256d(&b));
    let cp = checkpoint();
    let h1 = mine(cp.hash(), root, cp.time + 600);
    let h2 = mine(h1.hash(), Hash32([3; 32]), cp.time + 1200);

    // Qualquer conta retransmite os cabeçalhos; os dois Nodes veem a ponta.
    send_account(
        &mut c1,
        &g,
        &r,
        TxKind::BridgeHeaders {
            chain: CHAIN.into(),
            headers: vec![h1.serialize(), h2.serialize()],
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| bridge_status(&mut c2, CHAIN)
        .map(|v| v.best == Some((h2.hash(), 2)))
        .unwrap_or(false)));

    // Prova extraída do merkleblock, como a Wallet faz com gettxoutproof.
    let mb = merkleblock(&h1, &txids);
    let (header, path, index) = rz_core::bridge::path_from_merkleblock(&mb, &txids[1]).unwrap();
    assert_eq!(header, h1.serialize());
    send_account(
        &mut c1,
        &g,
        &r,
        TxKind::BridgeDeposit {
            chain: CHAIN.into(),
            block: h1.hash(),
            tx: burn.clone(),
            path: path.clone(),
            index,
        },
        10,
    )
    .unwrap();
    let asset = AssetId::external(CHAIN, "BTC");
    let balance = |c: &mut rz_p2p::Client, who: Address| {
        assets(c, Some(who))
            .ok()
            .and_then(|v| v.assets.into_iter().find(|x| x.id == asset))
            .and_then(|x| x.balance)
            .unwrap_or(0)
    };
    assert!(wait_until(|| balance(&mut c2, a.address()) == 50_000_000));
    // A mesma queima de novo: recusada.
    assert!(send_account(
        &mut c1,
        &g,
        &r,
        TxKind::BridgeDeposit {
            chain: CHAIN.into(),
            block: h1.hash(),
            tx: burn,
            path,
            index,
        },
        10,
    )
    .is_err());

    // Troca atômica: Alice bloqueia 0,2 BTC representado para o relayer, que
    // resgata revelando o segredo (como faria do outro lado de uma troca).
    let secret = [0x3c; 32].to_vec();
    let (_, _, height) = account(&mut c1, a.address()).unwrap();
    let lock = send_account(
        &mut c1,
        &g,
        &a,
        TxKind::HtlcLock {
            asset,
            amount: 20_000_000,
            recipient: r.address(),
            hashlock: Hash32(sha256(&secret)),
            timeout: height + 200,
        },
        10,
    )
    .unwrap()
    .0;
    assert!(wait_until(|| htlc(&mut c2, lock)
        .map(|h| h.0.is_some())
        .unwrap_or(false)));
    send_account(
        &mut c2,
        &g,
        &r,
        TxKind::HtlcClaim {
            id: lock,
            preimage: secret.clone(),
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| balance(&mut c1, r.address()) == 20_000_000));
    assert_eq!(balance(&mut c1, a.address()), 30_000_000);
    // A pré-imagem fica registrada: a outra parte a usa na rede de origem.
    let (_, settled) = htlc(&mut c1, lock).unwrap();
    assert_eq!(settled.unwrap().preimage, Some(secret));
}
