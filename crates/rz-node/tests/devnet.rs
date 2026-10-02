//! Testes de integração com Nodes reais comunicando por TCP local.
//!
//! Cobrem AT-CON-001, AT-SYNC-001..003, AT-GEN-002, AT-P2P-004..005 e o fluxo
//! completo de uma transação (`ARCHITECTURE.md §48`).

#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rz_codec::Encode;
use rz_core::{
    Allocation, Block, BlockHeader, Commit, CommittedBlock, Genesis, NetworkKind, TxBody, TxKind,
    Vote, VoteType, PROTOCOL_VERSION,
};
use rz_crypto::{context, Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_p2p::{Client, ClientError, Message};

const NET: &str = "rede-zero-devnet-it";

fn faucet() -> SecretKey {
    SecretKey::from_seed([200; 32])
}

fn validators(n: u8) -> Vec<SecretKey> {
    (0..n)
        .map(|i| SecretKey::from_seed([100 + i; 32]))
        .collect()
}

fn genesis(n: u8) -> Genesis {
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: NET.into(),
        consensus: rz_core::ConsensusParams::fast(200),
        min_fee: 10,
        max_block_txs: 500,
        validators: validators(n)
            .iter()
            .map(|k| rz_core::GenesisValidator::new(k.public_key(), 100))
            .collect(),
        allocations: vec![Allocation {
            address: faucet().public_key().address(),
            amount: 1_000_000_000,
        }],
        governance: rz_core::GovernanceParams::default(),
        assets: vec![],
        bridges: vec![],
    }
}

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rz-it-{}-{}-{}",
        name,
        std::process::id(),
        now_ms()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn start(g: &Genesis, dir: PathBuf, key: Option<SecretKey>, peers: Vec<SocketAddr>) -> Node {
    let mut cfg = NodeConfig::new(g.clone(), dir, "127.0.0.1:0".parse().unwrap());
    cfg.validator_key = key;
    cfg.bootstrap = peers.into_iter().map(Into::into).collect();
    cfg.log = LogLevel::Quiet;
    Node::start(cfg).expect("node deve iniciar")
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

fn client(node: &Node) -> Client {
    Client::connect(
        node.listen_addr(),
        NET,
        node.genesis().hash(),
        Duration::from_secs(5),
    )
    .expect("cliente deve conectar")
}

#[test]
fn devnet_end_to_end() {
    let g = genesis(3);
    let vs = validators(3);
    let n1 = start(
        &g,
        data_dir("n1"),
        Some(SecretKey::from_seed(vs[0].seed())),
        vec![],
    );
    let n2 = start(
        &g,
        data_dir("n2"),
        Some(SecretKey::from_seed(vs[1].seed())),
        vec![n1.listen_addr()],
    );
    let n3 = start(
        &g,
        data_dir("n3"),
        Some(SecretKey::from_seed(vs[2].seed())),
        vec![n1.listen_addr()],
    );
    let nodes = [&n1, &n2, &n3];

    // AT-CON-001 — os três validadores produzem e convergem.
    assert!(
        wait_until(Duration::from_secs(30), || nodes
            .iter()
            .all(|n| n.status().finalized_height >= 4)),
        "nodes não finalizaram blocos: {:?}",
        nodes.iter().map(|n| n.status()).collect::<Vec<_>>()
    );
    let fin = nodes
        .iter()
        .map(|n| n.status().finalized_height)
        .min()
        .unwrap();
    let reference = n1.block_at(fin).unwrap().id();
    for n in nodes {
        assert_eq!(
            n.block_at(fin).unwrap().id(),
            reference,
            "divergência na altura finalizada"
        );
    }
    // Todos os validadores produzem blocos: o proponente é sorteado com peso
    // igual, então em algumas alturas todos aparecem.
    assert!(
        wait_until(Duration::from_secs(60), || {
            let producers: std::collections::HashSet<_> = (1..=n1.status().height)
                .filter_map(|h| n1.block_at(h))
                .map(|b| b.header.proposer)
                .collect();
            producers.len() == 3
        }),
        "algum validador nunca propôs"
    );

    // Fluxo de transação: Wallet assina localmente e envia a um Node.
    let bob = SecretKey::from_seed([201; 32]).public_key().address();
    let mut c = client(&n2);
    let nonce = c
        .request(
            &Message::GetAccount(faucet().public_key().address()),
            |m| match m {
                Message::Account { nonce, .. } => Some(nonce),
                _ => None,
            },
        )
        .unwrap();
    let tx = TxBody {
        version: 1,
        sender: faucet().public_key(),
        nonce,
        fee: 10,
        kind: TxKind::Transfer {
            to: bob,
            amount: 12_345,
        },
    }
    .sign(&faucet(), NET)
    .unwrap();
    let (accepted, reason) = c
        .request(&Message::Transaction(tx.clone()), |m| match m {
            Message::TxResult {
                accepted, reason, ..
            } => Some((accepted, reason)),
            _ => None,
        })
        .unwrap();
    assert!(accepted, "transação rejeitada: {reason}");

    // AT-ZERO-001 — transferência refletida em todos os Nodes.
    assert!(wait_until(Duration::from_secs(20), || nodes
        .iter()
        .all(|n| n.account(&bob).balance == 12_345)));

    // Replay da mesma transação é rejeitado (AT-TX-007).
    assert!(wait_until(Duration::from_secs(10), || n2.status().mempool == 0));
    let (accepted, _) = c
        .request(&Message::Transaction(tx), |m| match m {
            Message::TxResult {
                accepted, reason, ..
            } => Some((accepted, reason)),
            _ => None,
        })
        .unwrap();
    assert!(!accepted);

    // AT-SYNC-001 — Node novo sincroniza a partir de um par.
    let dir4 = data_dir("n4");
    let mut n4 = start(&g, dir4.clone(), None, vec![n3.listen_addr()]);
    let target = n1.status().finalized_height;
    assert!(wait_until(Duration::from_secs(30), || n4.status().height >= target));
    assert_eq!(
        n4.block_at(target).unwrap().id(),
        n1.block_at(target).unwrap().id()
    );
    assert_eq!(n4.account(&bob).balance, 12_345);

    // Persistência: reinicia sem pares e recupera a cadeia do disco,
    // reverificando cada bloco.
    let h4 = n4.status().height;
    n4.shutdown();
    drop(n4);
    let n4 = start(&g, dir4, None, vec![]);
    assert!(n4.status().height >= h4.saturating_sub(1));
    assert_eq!(n4.account(&bob).balance, 12_345);
}

// AT-GEN-002 — Genesis de rede diferente é rejeitado no handshake.
#[test]
fn wrong_genesis_rejected() {
    let g = genesis(1);
    let n = start(&g, data_dir("gen"), None, vec![]);
    let err = Client::connect(
        n.listen_addr(),
        NET,
        Hash32([9; 32]),
        Duration::from_secs(5),
    )
    .err()
    .expect("handshake deveria falhar");
    assert!(
        matches!(err, ClientError::Rejected(_) | ClientError::Handshake(_)),
        "{err}"
    );

    let err = Client::connect(
        n.listen_addr(),
        "rede-zero-devnet-outra",
        g.hash(),
        Duration::from_secs(5),
    )
    .err()
    .expect("handshake deveria falhar");
    assert!(
        // Rede diferente: a assinatura do canal cifrado já é vinculada à rede.
        matches!(
            err,
            ClientError::Rejected(_)
                | ClientError::Handshake(_)
                | ClientError::Frame(rz_p2p::FrameError::Handshake(_))
        ),
        "{err}"
    );
}

// AT-SYNC-002 / AT-SYNC-003 — par que envia blocos inválidos é isolado e
// não altera o estado.
#[test]
fn malicious_blocks_rejected_and_peer_banned() {
    let g = genesis(1);
    let v = validators(1).remove(0);
    let n = start(&g, data_dir("mal"), Some(v), vec![]);
    assert!(wait_until(Duration::from_secs(10), || n.status().height >= 2));

    let mut c = client(&n);
    let attacker = SecretKey::from_seed([66; 32]);
    // Bloco forjado com um certificado assinado por quem não é validador.
    let forge = |round: u64| {
        let s = n.status();
        let height = s.height + 50;
        let header = BlockHeader {
            version: 1,
            height,
            parent: s.tip,
            round,
            proposer: attacker.public_key(),
            tx_root: Hash32([0; 32]),
            state_root: Hash32([0; 32]),
        };
        let signature = attacker.sign(context::BLOCK_SIGNATURE, NET, &header.to_canonical_bytes());
        let block = Block {
            header,
            txs: vec![],
            signature,
        };
        let vote = Vote::sign(
            VoteType::Precommit,
            height,
            0,
            Some(block.id()),
            &attacker,
            NET,
        );
        CommittedBlock {
            commit: Commit::from_votes(&[vote]).expect("voto"),
            block,
        }
    };
    let root_before = n.account(&faucet().public_key().address());
    c.send(&Message::Blocks(vec![forge(1_000)])).unwrap();
    c.send(&Message::Block(Box::new(forge(1_001)))).unwrap();

    assert!(wait_until(Duration::from_secs(5), || n
        .is_banned("127.0.0.1".parse().unwrap())));
    assert_eq!(n.account(&faucet().public_key().address()), root_before);
    // Blocos forjados nunca entram na cadeia.
    for h in 1..=n.status().height {
        assert_ne!(
            n.block_at(h).unwrap().header.proposer,
            attacker.public_key()
        );
    }
}

// AT-P2P-005 — flooding leva à desconexão e quarentena.
#[test]
fn flooding_peer_banned() {
    let g = genesis(1);
    let n = start(&g, data_dir("flood"), None, vec![]);
    let mut c = client(&n);
    for i in 0..2_000u64 {
        if c.send(&Message::Ping(i)).is_err() {
            break;
        }
    }
    assert!(wait_until(Duration::from_secs(5), || n
        .is_banned("127.0.0.1".parse().unwrap())));
}

// RZ-IR-06 — rajadas de mensagens de consenso não banem o par (o excedente
// é descartado sem verificação) e não atrapalham o consenso local.
#[test]
fn consensus_flood_dropped_without_ban() {
    let g = genesis(1);
    let v = validators(1).remove(0);
    let n = start(&g, data_dir("cflood"), Some(v), vec![]);
    assert!(wait_until(Duration::from_secs(10), || n.status().height >= 1));
    let mut c = client(&n);
    let outsider = SecretKey::from_seed([77; 32]);
    let start_height = n.status().height;
    for i in 0..5_000u32 {
        let vote = Vote::sign(VoteType::Prevote, 1_000_000, i, None, &outsider, NET);
        if c.send(&Message::ConsensusVote(vote)).is_err() {
            break;
        }
    }
    // Depois da rajada, a mesma conexão continua útil para mensagens gerais.
    c.send(&Message::Ping(1)).unwrap();
    assert!(wait_until(Duration::from_secs(10), || n.status().height
        >= start_height + 3));
    assert!(!n.is_banned("127.0.0.1".parse().unwrap()));
}

// AT-P2P-004 — mensagem acima do limite encerra a conexão.
#[test]
fn oversized_frame_disconnects() {
    let g = genesis(1);
    let n = start(&g, data_dir("big"), None, vec![]);
    let mut c = client(&n);
    c.stream_mut().write_all(&u32::MAX.to_be_bytes()).unwrap();
    assert!(wait_until(Duration::from_secs(5), || n.status().peers == 0));
}

// REQ-026 / ADR-0010 — Dandelion++: a transação de um cliente sai primeiro em
// fase de haste para um único relay; se o relay a segurar, o embargo garante
// a difusão.
#[test]
fn dandelion_stem_then_embargo_fluff() {
    let g = genesis(1);
    let n = start(&g, data_dir("dandelion"), None, vec![]);

    // Espião: único par (relay) do node.
    let mut spy =
        Client::connect_with_port(n.listen_addr(), NET, g.hash(), Duration::from_secs(30), 9)
            .unwrap();
    assert!(wait_until(Duration::from_secs(5), || n.status().peers == 1));

    let mut wallet = client(&n);
    let tx = TxBody {
        version: 1,
        sender: faucet().public_key(),
        nonce: 0,
        fee: 10,
        kind: TxKind::Transfer {
            to: SecretKey::from_seed([202; 32]).public_key().address(),
            amount: 1,
        },
    }
    .sign(&faucet(), NET)
    .unwrap();
    let id = tx.id();
    let accepted = wallet
        .request(&Message::Transaction(tx), |m| match m {
            Message::TxResult { accepted, .. } => Some(accepted),
            _ => None,
        })
        .unwrap();
    assert!(accepted);

    // 1. Haste: o espião recebe STEM_TRANSACTION, nunca TRANSACTION antes.
    let first = spy
        .request(&Message::Ping(0), |m| match m {
            Message::StemTransaction(t) if t.id() == id => Some("stem"),
            Message::Transaction(t) if t.id() == id => Some("fluff"),
            _ => None,
        })
        .unwrap();
    assert_eq!(first, "stem");
    // Durante a haste, a transação não está no mempool (não é difundida).
    assert_eq!(n.status().mempool, 0);

    // 2. O espião não repassa; o embargo (10–20 s) difunde a transação.
    let fluffed = spy
        .request(&Message::Ping(1), |m| match m {
            Message::Transaction(t) if t.id() == id => Some(true),
            _ => None,
        })
        .unwrap();
    assert!(fluffed);
    assert_eq!(n.status().mempool, 1);
}
