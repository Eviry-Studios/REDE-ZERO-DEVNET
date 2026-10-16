//! Testes de arquitetura com Nodes reais (`ACCEPTANCE_TESTS.md §74, §77,
//! §79`, THR-P2P-001, ADV-03, ADR-0024): eclipse, Sybil e corrupção de dados.

#![allow(clippy::unwrap_used)]

use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rz_core::{Allocation, Genesis, NetworkKind, PROTOCOL_VERSION};
use rz_crypto::SecretKey;
use rz_node::addrbook::{netgroup, NetGroup, MAX_PER_GROUP, MAX_PER_SOURCE};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_p2p::{Client, Message, PeerAddr};

const NET: &str = "rede-zero-devnet-arquitetura";

fn validator() -> SecretKey {
    SecretKey::from_seed([150; 32])
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
            address: SecretKey::from_seed([151; 32]).public_key().address(),
            amount: 1_000_000,
        }],
        governance: rz_core::GovernanceParams::default(),
        assets: vec![],
        bridges: vec![],
    }
}

fn data_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rz-arq-{name}-{}-{}", std::process::id(), now_ms()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn config(
    dir: PathBuf,
    listen: SocketAddr,
    key: Option<SecretKey>,
    peers: &[SocketAddr],
) -> NodeConfig {
    let mut cfg = NodeConfig::new(genesis(), dir, listen);
    cfg.validator_key = key;
    cfg.bootstrap = peers.iter().copied().map(Into::into).collect();
    cfg.log = LogLevel::Quiet;
    cfg
}

fn start(dir: PathBuf, key: Option<SecretKey>, peers: &[SocketAddr]) -> Node {
    Node::start(config(dir, "127.0.0.1:0".parse().unwrap(), key, peers)).unwrap()
}

fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn client(node: &Node) -> Result<Client, rz_p2p::ClientError> {
    Client::connect(
        node.listen_addr(),
        NET,
        node.genesis().hash(),
        Duration::from_secs(3),
    )
}

// THR-P2P-001 — eclipse por inundação de endereços: um atacante anuncia
// milhares de endereços. O livro guarda poucos dele, a conexão honesta
// continua e as conexões de saída não se concentram numa faixa.
#[test]
fn address_flood_does_not_eclipse() {
    let a = start(data_dir("eclipse-a"), Some(validator()), &[]);
    let t = start(data_dir("eclipse-t"), None, &[a.listen_addr()]);
    assert!(wait_until(Duration::from_secs(20), || t.status().height >= 2));

    let mut attacker = client(&t).unwrap();
    for round in 0..20u32 {
        let list: Vec<PeerAddr> = (0..100u32)
            .map(|i| {
                let n = round * 100 + i;
                // Metade numa só faixa; o resto espalhado por muitas.
                let ip = if n % 2 == 0 {
                    format!("198.51.{}.{}:7000", (n / 250) % 250, n % 250)
                } else {
                    format!("{}.{}.7.7:7000", 11 + (n % 200), n / 200)
                };
                PeerAddr::Ip(ip.parse().unwrap())
            })
            .collect();
        if attacker.send(&Message::Peers(list)).is_err() {
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(500));
    let book = t.known_addresses();
    let in_group = book
        .iter()
        .filter(|x| netgroup(x) == NetGroup::V4([198, 51]))
        .count();
    assert!(in_group <= MAX_PER_GROUP, "faixa única: {in_group}");
    let from_attacker = book
        .iter()
        .filter(|x| matches!(x, PeerAddr::Ip(s) if !s.ip().is_loopback()))
        .count();
    assert!(
        from_attacker <= MAX_PER_SOURCE,
        "uma origem: {from_attacker}"
    );
    // A conexão com o par honesto (âncora de partida) segue de pé e a
    // cadeia continua avançando.
    let h = t.status().height;
    assert!(wait_until(Duration::from_secs(20), || t.status().height >= h + 3));
    assert!(t
        .outbound_targets()
        .contains(&PeerAddr::Ip(a.listen_addr())));
    let groups: std::collections::BTreeSet<NetGroup> =
        t.outbound_targets().iter().map(netgroup).collect();
    assert_eq!(
        groups.len(),
        t.outbound_targets().len(),
        "uma saída por faixa"
    );
}

// THR-P2P-001 — estagnação: um Node preso a um par que não entrega blocos
// troca de par e volta a acompanhar a cadeia.
#[test]
fn stalled_peer_is_rotated() {
    // Par "parado": mesmo Genesis, sem validador e sem pares.
    let stuck = start(data_dir("stall-s"), None, &[]);
    // Endereço reservado para o par honesto, que só sobe depois.
    let honest_addr = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let mut cfg = config(
        data_dir("stall-t"),
        "127.0.0.1:0".parse().unwrap(),
        None,
        &[stuck.listen_addr(), honest_addr],
    );
    cfg.max_outbound = 1;
    cfg.stall_window = Duration::from_secs(2);
    let t = Node::start(cfg).unwrap();
    assert!(wait_until(Duration::from_secs(10), || t
        .outbound_targets()
        .contains(&PeerAddr::Ip(stuck.listen_addr()))));
    let a = Node::start(config(
        data_dir("stall-a"),
        honest_addr,
        Some(validator()),
        &[],
    ))
    .unwrap();
    assert!(wait_until(Duration::from_secs(10), || a.status().height >= 3));
    // Sem blocos novos pela janela, T troca o par parado pelo honesto.
    assert!(
        wait_until(Duration::from_secs(40), || t.status().height >= 3),
        "não saiu do par parado"
    );
    assert!(t.outbound_targets().contains(&PeerAddr::Ip(honest_addr)));
}

// ADV-03 — Sybil por conexões: muitas conexões do mesmo endereço não ocupam
// todas as entradas.
#[test]
fn sybil_connections_limited_per_address() {
    let n = start(data_dir("sybil"), None, &[]);
    let mut ok = Vec::new();
    for _ in 0..20 {
        if let Ok(c) = client(&n) {
            ok.push(c);
        }
    }
    // Limite padrão de 8 por IP; conexões recusadas não passam do handshake.
    assert!(ok.len() <= 8, "{} conexões aceitas", ok.len());
    assert!(!ok.is_empty());
}

// AT §77, §79 — corrupção do armazenamento local: o Node detecta, descarta o
// trecho inválido e reconstrói a cadeia a partir de um par, verificada.
#[test]
fn corrupted_block_log_is_cut_and_rebuilt() {
    let a = start(data_dir("corr-a"), Some(validator()), &[]);
    let dir = data_dir("corr-b");
    let mut b = start(dir.clone(), None, &[a.listen_addr()]);
    assert!(wait_until(Duration::from_secs(30), || b.status().height >= 12));
    b.shutdown();
    drop(b);
    let h = a.status().height;

    // Altera um byte no meio do arquivo de blocos.
    let log = dir.join("blocks.log");
    let mut bytes = std::fs::read(&log).unwrap();
    let full = bytes.len();
    let pos = full / 2;
    bytes[pos] ^= 0x55;
    std::fs::write(&log, &bytes).unwrap();

    // Sem pares: só o trecho anterior à corrupção é aceito, e o arquivo é
    // cortado ali.
    let mut offline = start(dir.clone(), None, &[]);
    let kept = offline.status().height;
    assert!(kept >= 1 && kept < h, "mantidos {kept} de {h}");
    offline.shutdown();
    drop(offline);
    let cut = std::fs::metadata(&log).unwrap().len() as usize;
    assert!(cut <= pos, "o arquivo foi cortado antes do byte alterado");

    // Com o par: a cadeia é reconstruída e coincide com a dele.
    let mut b = start(dir.clone(), None, &[a.listen_addr()]);
    assert!(wait_until(Duration::from_secs(30), || b.status().height >= h));
    assert_eq!(b.block_at(h).unwrap().id(), a.block_at(h).unwrap().id());
    b.shutdown();
    drop(b);
    // E o arquivo reconstruído reabre íntegro.
    let again = start(dir, None, &[]);
    assert!(again.status().height >= h);
}
