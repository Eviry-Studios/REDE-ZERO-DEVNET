//! Exonet Runtime de ponta a ponta contra Nodes reais (ADR-0018): módulo
//! publicado, vinculado por aprovação da Comunidade, chamado pela Wallet e
//! pelo Navegador (com aprovação na interface), mesmo estado em Nodes
//! independentes.

#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use rz_browser::{Config, Origin, Server};
use rz_core::community::{approve, CommunityStatus, DecisionRule, NameKind};
use rz_core::governance::{Category, Choice};
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::{Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{
    call_module, community, connect, governance, module_info, query, send_account, Keys, NetOptions,
};

const NET: &str = "rede-zero-devnet-rt";

/// O módulo de exemplo do repositório: o teste garante que ele funciona.
const CONTADOR: &str = include_str!("../../../examples/runtime/contador.wat");

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([180 + i; 32]))
        .collect()
}

fn user() -> Keys {
    Keys::from_secret(SecretKey::from_seed([241; 32]))
}

fn genesis() -> Genesis {
    let mut allocations: Vec<Allocation> = validators()
        .iter()
        .map(|v| Allocation {
            address: v.public_key().address(),
            amount: 10_000_000,
        })
        .chain([Allocation {
            address: user().address(),
            amount: 1_000 * rz_core::UNITS_PER_ZERO,
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
        std::env::temp_dir().join(format!("rz-rt-{name}-{}-{}", std::process::id(), now_ms()));
    let mut cfg = NodeConfig::new(g.clone(), dir, "127.0.0.1:0".parse().unwrap());
    cfg.validator_key = Some(key);
    cfg.bootstrap = peers.into_iter().map(Into::into).collect();
    cfg.log = LogLevel::Quiet;
    Node::start(cfg).unwrap()
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn http(
    port: u16,
    method: &str,
    host: &str,
    path: &str,
    origin: Option<&str>,
    body: &str,
) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n");
    if let Some(o) = origin {
        req.push_str(&format!("Origin: {o}\r\n"));
    }
    req.push_str(&format!(
        "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ));
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    (head[9..12].parse().unwrap(), body.to_string())
}

fn field(json: &str, k: &str) -> String {
    json.split(&format!("\"{k}\":\""))
        .nth(1)
        .unwrap_or_else(|| panic!("campo {k} ausente em {json}"))
        .split('"')
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn runtime_end_to_end() {
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
    let u = user();
    let mut c = connect(n1.listen_addr(), &g).unwrap();
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();

    // Comunidade reconhecida pela governança (N-7).
    let h = n1.status().height;
    send_account(
        &mut c,
        &g,
        &u,
        TxKind::LockStake {
            amount: 100 * rz_core::UNITS_PER_ZERO,
            unlock_height: h + 400,
        },
        10,
    )
    .unwrap();
    let controllers: Vec<SecretKey> = (110..113).map(|i| SecretKey::from_seed([i; 32])).collect();
    assert!(wait_until(|| governance(&mut c, Some(u.address()))
        .map(|v| v.locks.len() == 1)
        .unwrap_or(false)));
    let cid = send_account(
        &mut c,
        &g,
        &u,
        TxKind::DeclareCommunity {
            name: "contadores".into(),
            manifest_hash: Hash32([0x6e; 32]),
            rule: DecisionRule {
                keys: controllers.iter().map(SecretKey::public_key).collect(),
                threshold: 2,
            },
        },
        10,
    )
    .unwrap()
    .0;
    assert!(wait_until(|| community(&mut c, "contadores")
        .map(|(_, c)| c.is_some())
        .unwrap_or(false)));
    let prop = send_account(
        &mut c,
        &g,
        &u,
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
        user(),
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
    assert!(wait_until(|| community(&mut c, "contadores")
        .map(|(_, c)| c.is_some_and(|c| c.status == CommunityStatus::Recognized))
        .unwrap_or(false)));

    // Publica e vincula o módulo (2 de 3 chaves da Comunidade).
    let code = wat::parse_str(CONTADOR).unwrap();
    let module = rz_core::runtime::module_id(&code);
    send_account(&mut c, &g, &u, TxKind::PublishModule { code }, 10).unwrap();
    let payload = rz_core::runtime::bind_payload(&cid, &Some(module), 0);
    let approvals = controllers[..2]
        .iter()
        .map(|k| approve(k, NET, &payload))
        .collect();
    // A vinculação só é aceita depois que o módulo estiver no estado.
    let mut bound = false;
    let bind = TxKind::BindModule {
        community: cid,
        module: Some(module),
        approvals,
    };
    assert!(wait_until(|| {
        if !bound {
            bound = send_account(&mut c, &g, &u, bind.clone(), 10).is_ok();
        }
        bound
    }));
    assert!(wait_until(|| module_info(&mut c2, cid)
        .map(|m| m.0 == Some(module))
        .unwrap_or(false)));

    // Chamada pela Wallet; o recibo e o estado aparecem no outro Node.
    let (_, r) = call_module(&mut c, &g, &u, cid, "incrementar", &[], 1_000_000).unwrap();
    assert!(r.ok, "{}", r.error);
    assert_eq!(r.output, 1u64.to_le_bytes());
    assert!(wait_until(|| query(&mut c2, cid, "ler", &[], None)
        .map(|v| v.ok && v.output == 1u64.to_le_bytes())
        .unwrap_or(false)));

    // Navegador: consulta da publicação da Comunidade e chamada aprovada.
    let mut nav = Server::start(Config {
        genesis: g.clone(),
        node: n2.listen_addr().into(),
        net: NetOptions::default(),
        key: Some(SecretKey::from_seed([241; 32])),
        data_dir: std::env::temp_dir().join(format!(
            "rz-rt-nav-{}-{}",
            std::process::id(),
            now_ms()
        )),
        listen: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let port = nav.port();
    let site = Origin::Named {
        name: "contadores".into(),
        kind: NameKind::Community,
    };
    let (host, url) = (site.host(port), site.url(port));
    let (ui, ui_url) = (Origin::Ui.host(port), Origin::Ui.url(port));
    let (st, body) = http(
        port,
        "POST",
        &host,
        "/.zero/consulta",
        Some(&url),
        "metodo=ler",
    );
    assert_eq!(st, 200, "{body}");
    assert_eq!(field(&body, "saida_hex"), "0100000000000000");
    // De outra origem: recusado.
    let (st, _) = http(
        port,
        "POST",
        &host,
        "/.zero/consulta",
        Some(&ui_url),
        "metodo=ler",
    );
    assert_eq!(st, 403);

    let (st, body) = http(
        port,
        "POST",
        &host,
        "/.zero/pedido",
        Some(&url),
        "tipo=chamada&metodo=incrementar&combustivel=500000",
    );
    assert_eq!(st, 200, "{body}");
    let pid = field(&body, "pedido");
    let (_, page) = http(port, "GET", &ui, &format!("/pedidos/{pid}"), None, "");
    assert!(page.contains("incrementar"));
    assert!(page.contains(&cid.to_hex()));
    let i = page.find("name=\"token\" value=\"").unwrap() + 20;
    let token = &page[i..i + 64];
    let (st, _) = http(
        port,
        "POST",
        &ui,
        &format!("/pedidos/{pid}/aprovar"),
        Some(&ui_url),
        &format!("token={token}"),
    );
    assert_eq!(st, 303);
    let (_, status) = http(
        port,
        "GET",
        &host,
        &format!("/.zero/pedido/{pid}"),
        None,
        "",
    );
    let txid = field(&status, "txid");
    let mut receipt = String::new();
    assert!(wait_until(|| {
        let (st, b) = http(
            port,
            "GET",
            &host,
            &format!("/.zero/recibo/{txid}"),
            None,
            "",
        );
        receipt = b;
        st == 200
    }));
    assert!(receipt.contains("\"ok\":true"), "{receipt}");
    assert_eq!(field(&receipt, "saida_hex"), "0200000000000000");

    // A página da Comunidade mostra o módulo vinculado.
    let (_, cpage) = http(port, "GET", &ui, "/comunidade?nome=contadores", None, "");
    assert!(cpage.contains(&module.to_hex()));
    nav.shutdown();
}
