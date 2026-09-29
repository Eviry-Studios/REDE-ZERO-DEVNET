//! Proteção do IP do usuário (ADR-0011) contra Nodes reais.

#![allow(clippy::unwrap_used)]

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, PROTOCOL_VERSION};
use rz_crypto::SecretKey;
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_p2p::{Message, PeerAddr};
use rz_wallet::{connect, connect_to, NetOptions, DIRECT_REFUSED};

const NET: &str = "rede-zero-devnet-net";

fn validator() -> SecretKey {
    SecretKey::from_seed([180; 32])
}

fn genesis() -> Genesis {
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: NET.into(),
        consensus: rz_core::ConsensusParams::fast(200),
        min_fee: 10,
        max_block_txs: 100,
        validators: vec![rz_core::GenesisValidator::new(
            validator().public_key(),
            100,
        )],
        allocations: vec![Allocation {
            address: SecretKey::from_seed([181; 32]).public_key().address(),
            amount: 1_000,
        }],
        governance: GovernanceParams::default(),
    }
}

fn config(g: &Genesis, name: &str) -> NodeConfig {
    let dir =
        std::env::temp_dir().join(format!("rz-net-{name}-{}-{}", std::process::id(), now_ms()));
    let mut cfg = NodeConfig::new(g.clone(), dir, "127.0.0.1:0".parse().unwrap());
    cfg.log = LogLevel::Quiet;
    cfg
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Proxy SOCKS5 mínimo: registra o destino pedido e encaminha para
/// `127.0.0.1:porta`, simulando o Tor alcançando um serviço onion.
fn socks_proxy() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            let log = log.clone();
            thread::spawn(move || {
                let mut g = [0u8; 3];
                c.read_exact(&mut g).unwrap();
                c.write_all(&[5, 0]).unwrap();
                let mut h = [0u8; 4];
                c.read_exact(&mut h).unwrap();
                assert_eq!(h[3], 3, "o destino deve chegar como NOME ao proxy");
                let mut l = [0u8; 1];
                c.read_exact(&mut l).unwrap();
                let mut name = vec![0u8; l[0] as usize];
                c.read_exact(&mut name).unwrap();
                let mut p = [0u8; 2];
                c.read_exact(&mut p).unwrap();
                let port = u16::from_be_bytes(p);
                log.lock()
                    .unwrap()
                    .push(format!("{}:{port}", String::from_utf8(name).unwrap()));
                let up = TcpStream::connect(("127.0.0.1", port)).unwrap();
                c.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
                let (mut a, mut b) = (c.try_clone().unwrap(), up.try_clone().unwrap());
                let (mut c2, mut up2) = (c, up);
                thread::spawn(move || {
                    let _ = io::copy(&mut a, &mut up2);
                });
                let _ = io::copy(&mut b, &mut c2);
            });
        }
    });
    (addr, seen)
}

fn status_height(c: &mut rz_p2p::Client) -> u64 {
    c.request(&Message::GetStatus, |m| match m {
        Message::Status { height, .. } => Some(height),
        _ => None,
    })
    .unwrap()
}

// Wallet → proxy SOCKS5 (Tor) → serviço onion: o node não recebe o IP do
// usuário, e nenhum DNS é resolvido localmente.
#[test]
fn wallet_through_tor_style_proxy() {
    let g = genesis();
    let mut cfg = config(&g, "tor");
    cfg.validator_key = Some(validator());
    let n = start_node(cfg);
    let (proxy, seen) = socks_proxy();
    let onion: PeerAddr = format!("rz3exemplo.onion:{}", n.listen_addr().port())
        .parse()
        .unwrap();
    let net = NetOptions {
        proxy: Some(proxy),
        node_id: Some(n.node_id()),
        allow_direct: false,
    };
    let mut c = connect_to(&onion, &g, &net).unwrap();
    assert!(wait_until(|| status_height(&mut c) >= 1));
    assert_eq!(seen.lock().unwrap()[0], onion.to_string());
}

fn start_node(cfg: NodeConfig) -> Node {
    Node::start(cfg).unwrap()
}

// Privacidade por padrão: sem proxy, a Wallet recusa nodes remotos.
#[test]
fn direct_remote_connection_refused_by_default() {
    let g = genesis();
    let remote: PeerAddr = "203.0.113.7:7100".parse().unwrap();
    let err = connect_to(&remote, &g, &NetOptions::default())
        .err()
        .unwrap();
    assert_eq!(err, DIRECT_REFUSED);
}

// Identidade fixada: impede que um intermediário se passe pelo node.
#[test]
fn pinned_node_identity() {
    let g = genesis();
    let n = start_node(config(&g, "pin"));
    let addr: PeerAddr = n.listen_addr().into();
    let wrong = NetOptions {
        node_id: Some(SecretKey::from_seed([1; 32]).public_key().node_id()),
        ..NetOptions::default()
    };
    assert!(connect_to(&addr, &g, &wrong).is_err());
    let right = NetOptions {
        node_id: Some(n.node_id()),
        ..NetOptions::default()
    };
    assert!(connect_to(&addr, &g, &right).is_ok());
}

// Node privado (--no-listen): acompanha a cadeia por propagação, mas nunca
// é anunciado a outros pares.
#[test]
fn private_node_syncs_without_being_announced() {
    let g = genesis();
    let mut c1 = config(&g, "pub");
    c1.validator_key = Some(validator());
    let public = start_node(c1);

    let mut c2 = config(&g, "priv");
    c2.listen = None;
    c2.bootstrap = vec![public.listen_addr().into()];
    let private = start_node(c2);

    assert!(wait_until(
        || public.status().peers >= 1 && private.status().height >= 3
    ));
    // Continua recebendo blocos novos (propagação para Nodes privados).
    let h = private.status().height;
    assert!(wait_until(|| private.status().height >= h + 3));

    let mut c = connect(public.listen_addr(), &g).unwrap();
    let peers = c
        .request(&Message::GetPeers, |m| match m {
            Message::Peers(p) => Some(p),
            _ => None,
        })
        .unwrap();
    assert!(
        peers.is_empty(),
        "node privado não deve ser anunciado: {peers:?}"
    );
}

// Um Node atrás de um serviço onion anuncia o endereço onion, não o IP.
#[test]
fn onion_address_is_advertised_instead_of_ip() {
    let g = genesis();
    let mut c1 = config(&g, "hub");
    c1.validator_key = Some(validator());
    let hub = start_node(c1);

    let mut c2 = config(&g, "onion");
    c2.advertise = Some("rz3servico.onion:7100".parse().unwrap());
    c2.bootstrap = vec![hub.listen_addr().into()];
    let _onion_node = start_node(c2);
    assert!(wait_until(|| hub.status().peers >= 1));

    let mut c = connect(hub.listen_addr(), &g).unwrap();
    let peers = c
        .request(&Message::GetPeers, |m| match m {
            Message::Peers(p) => Some(p),
            _ => None,
        })
        .unwrap();
    assert!(
        peers
            .iter()
            .any(|p| p.to_string() == "rz3servico.onion:7100"),
        "{peers:?}"
    );
    assert!(!peers.iter().any(|p| matches!(p, PeerAddr::Ip(_))));
}
