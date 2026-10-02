//! Navegador Zero contra Nodes reais (ADR-0017): publicação verificada e
//! replicada, origens isoladas, política sem servidores externos, pedidos de
//! assinatura aprovados só na interface e identidade por site
//! (AT-BRW-001..003).

#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use rz_browser::{Config, Origin, Server};
use rz_codec::Encode;
use rz_core::community::NameKind;
use rz_core::content::{Bundle, BundleFile};
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::{Address, Hash32, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{account, connect, publish_content, resolve, send_account, Keys, NetOptions};

const NET: &str = "rede-zero-devnet-nav";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([170 + i; 32]))
        .collect()
}

fn user() -> Keys {
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
            address: user().address(),
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
        governance: GovernanceParams::default(),
        assets: vec![],
        bridges: vec![],
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("rz-nav-{name}-{}-{}", std::process::id(), now_ms()))
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let mut cfg = NodeConfig::new(g.clone(), tmp(name), "127.0.0.1:0".parse().unwrap());
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

struct Resp {
    status: u16,
    headers: String,
    body: String,
}

fn http(port: u16, method: &str, host: &str, path: &str, origin: Option<&str>, body: &str) -> Resp {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n");
    if let Some(o) = origin {
        req.push_str(&format!("Origin: {o}\r\n"));
    }
    if !body.is_empty() {
        req.push_str(&format!(
            "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    let status = head[9..12].parse().unwrap();
    Resp {
        status,
        headers: head.to_ascii_lowercase(),
        body: body.to_string(),
    }
}

fn site_bundle() -> Bundle {
    Bundle::new(vec![
        BundleFile {
            path: "index.html".into(),
            mime: "text/html; charset=utf-8".into(),
            data: b"<h1>Laboratorio na Exonet</h1><script src=\"/app.js\"></script>".to_vec(),
        },
        BundleFile {
            path: "app.js".into(),
            mime: "text/javascript; charset=utf-8".into(),
            data: b"console.log('ok')".to_vec(),
        },
    ])
    .unwrap()
}

/// Valor de `name="token" value="..."` numa página da interface.
fn token_from(page: &str) -> String {
    let i = page.find("name=\"token\" value=\"").unwrap() + "name=\"token\" value=\"".len();
    page[i..i + 64].to_string()
}

#[test]
fn browser_end_to_end() {
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
    let mut c1 = connect(n1.listen_addr(), &g).unwrap();

    // 1. Publicar: empacotar, registrar o nome com o id, enviar ao Node.
    let bytes = site_bundle().to_canonical_bytes();
    let id = rz_core::content::describe(&bytes).unwrap().id();
    // Antes do registro, o Node recusa hospedar (conteúdo não referenciado).
    assert!(publish_content(&mut c1, &bytes).is_err());
    send_account(
        &mut c1,
        &g,
        &u,
        TxKind::RegisterName {
            name: "laboratorio".into(),
            kind: NameKind::App,
            target: id,
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| resolve(
        &mut c1,
        "laboratorio",
        NameKind::App
    )
    .map(|r| r.0 == Some(id))
    .unwrap_or(false)));
    assert_eq!(publish_content(&mut c1, &bytes).unwrap(), id);

    // 2. Navegador ligado ao OUTRO Node: o conteúdo é replicado sob demanda e
    //    verificado pedaço a pedaço.
    let mut nav = Server::start(Config {
        genesis: g.clone(),
        node: n2.listen_addr().into(),
        net: NetOptions::default(),
        key: Some(SecretKey::from_seed([231; 32])),
        data_dir: tmp("nav"),
        listen: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let port = nav.port();
    let site = Origin::Named {
        name: "laboratorio".into(),
        kind: NameKind::App,
    };
    let host = site.host(port);
    let ui = Origin::Ui.host(port);
    let ui_url = Origin::Ui.url(port);
    let site_url = site.url(port);

    // AT-BRW-001: conexão válida e conteúdo verificado.
    let r = http(port, "GET", &host, "/", None, "");
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("Laboratorio na Exonet"));
    // Sem servidores externos, sem quadros, sem envio para fora.
    assert!(r
        .headers
        .contains("content-security-policy: default-src 'self'"));
    assert!(r.headers.contains("connect-src 'self'"));
    assert!(r.headers.contains("frame-ancestors 'none'"));
    assert!(r.headers.contains("x-content-type-options: nosniff"));
    assert!(r.headers.contains("referrer-policy: no-referrer"));
    let js = http(port, "GET", &host, "/app.js", None, "");
    assert_eq!(js.status, 200);
    assert!(js.headers.contains("content-type: text/javascript"));
    assert_eq!(http(port, "GET", &host, "/nada", None, "").status, 404);
    // O Node 2 agora hospeda uma cópia (replicação, REQ-070).
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();
    assert_eq!(
        rz_wallet::fetch_content(&mut c2, &id, Duration::from_secs(5)).unwrap(),
        bytes
    );
    // Publicação sem nome, pelo identificador.
    let by_id = Origin::Id(id).host(port);
    assert!(http(port, "GET", &by_id, "/", None, "")
        .body
        .contains("Laboratorio"));

    // Hosts estranhos são recusados (DNS rebinding).
    assert_eq!(
        http(port, "GET", &format!("evil.example:{port}"), "/", None, "").status,
        421
    );

    // 3. A interface não tem scripts e o identificador ficou fixado.
    let home = http(port, "GET", &ui, "/", None, "");
    assert_eq!(home.status, 200);
    assert!(home
        .headers
        .contains("content-security-policy: default-src 'none'"));
    assert!(home.body.contains("zero://laboratorio.app"));

    // 4. Pedido de assinatura pela publicação.
    let dest = Address(Hash32([0x77; 32]));
    let form = format!("tipo=transferencia&para={}&valor=2", dest.to_hex());
    // De outra origem: recusado (uma publicação não pede em nome de outra).
    let evil = Origin::Named {
        name: "golpe".into(),
        kind: NameKind::App,
    }
    .url(port);
    assert_eq!(
        http(port, "POST", &host, "/.zero/pedido", Some(&evil), &form).status,
        403
    );
    let r = http(port, "POST", &host, "/.zero/pedido", Some(&site_url), &form);
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("\"estado\":\"pendente\""));
    let pid = r.body.split("\"pedido\":\"").nth(1).unwrap()[..32].to_string();
    // Outra publicação não consulta pedidos alheios.
    let other = Origin::Id(Hash32([1; 32])).host(port);
    assert_eq!(
        http(
            port,
            "GET",
            &other,
            &format!("/.zero/pedido/{pid}"),
            None,
            ""
        )
        .status,
        404
    );
    // A publicação não consegue aprovar: a rota não existe na sua origem, e
    // na interface exige a origem e o token da interface.
    assert_eq!(
        http(
            port,
            "POST",
            &host,
            &format!("/pedidos/{pid}/aprovar"),
            Some(&site_url),
            ""
        )
        .status,
        405
    );
    assert_eq!(
        http(
            port,
            "POST",
            &ui,
            &format!("/pedidos/{pid}/aprovar"),
            Some(&site_url),
            "token=x"
        )
        .status,
        403
    );
    let page = http(port, "GET", &ui, &format!("/pedidos/{pid}"), None, "");
    assert!(page.body.contains(&dest.to_hex()));
    assert!(page.body.contains("zero://laboratorio.app"));
    let token = token_from(&page.body);
    assert_eq!(
        http(
            port,
            "POST",
            &ui,
            &format!("/pedidos/{pid}/aprovar"),
            Some(&site_url),
            &format!("token={token}")
        )
        .status,
        403,
        "token certo com origem errada continua recusado"
    );
    let ok = http(
        port,
        "POST",
        &ui,
        &format!("/pedidos/{pid}/aprovar"),
        Some(&ui_url),
        &format!("token={token}"),
    );
    assert_eq!(ok.status, 303);
    let st = http(
        port,
        "GET",
        &host,
        &format!("/.zero/pedido/{pid}"),
        None,
        "",
    );
    assert!(st.body.contains("\"estado\":\"aprovado\""), "{}", st.body);
    assert!(st.body.contains("\"txid\""));
    assert!(wait_until(|| account(&mut c1, dest)
        .map(|a| a.0 == 2 * rz_core::UNITS_PER_ZERO)
        .unwrap_or(false)));

    // 5. Identidade por site: verificável e diferente entre sites, sem
    //    revelar a Wallet.
    let sign = |origin_url: &str, h: &str| {
        let r = http(
            port,
            "POST",
            h,
            "/.zero/pedido",
            Some(origin_url),
            "tipo=assinatura&mensagem=desafio-123",
        );
        let pid = r.body.split("\"pedido\":\"").nth(1).unwrap()[..32].to_string();
        let page = http(port, "GET", &ui, &format!("/pedidos/{pid}"), None, "");
        let token = token_from(&page.body);
        http(
            port,
            "POST",
            &ui,
            &format!("/pedidos/{pid}/aprovar"),
            Some(&ui_url),
            &format!("token={token}"),
        );
        let st = http(port, "GET", h, &format!("/.zero/pedido/{pid}"), None, "").body;
        let field = |k: &str| {
            st.split(&format!("\"{k}\":\""))
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        (field("chave"), field("assinatura"))
    };
    let (k1, s1) = sign(&site_url, &host);
    let id_origin = Origin::Id(id);
    let (k2, _) = sign(&id_origin.url(port), &id_origin.host(port));
    assert_ne!(k1, k2, "sites diferentes veem identidades diferentes");
    assert_ne!(k1, u.transparent.public_key().to_hex());
    let pk = rz_crypto::PublicKey::from_hex(&k1).unwrap();
    let mut sig = [0u8; 64];
    for (i, b) in sig.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s1[2 * i..2 * i + 2], 16).unwrap();
    }
    assert!(rz_core::content::verify_login(
        &pk,
        NET,
        "zero://laboratorio.app",
        "desafio-123",
        &rz_crypto::Signature(sig)
    ));

    // 6. Troca do alvo do nome: o Navegador não abre o novo conteúdo sem
    //    revisão (identificador fixado).
    send_account(
        &mut c1,
        &g,
        &u,
        TxKind::UpdateName {
            name: "laboratorio".into(),
            kind: NameKind::App,
            target: Hash32([0xee; 32]),
            new_owner: None,
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| resolve(
        &mut c2,
        "laboratorio",
        NameKind::App
    )
    .map(|r| r.0 == Some(Hash32([0xee; 32])))
    .unwrap_or(false)));
    let changed = http(port, "GET", &host, "/", None, "");
    assert_eq!(changed.status, 409);
    assert!(!changed.body.contains("Laboratorio na Exonet"));

    // AT-BRW-003: uma interface alternativa (a biblioteca da Wallet) acessa o
    // mesmo conteúdo pelo mesmo protocolo.
    assert_eq!(
        rz_wallet::fetch_content(&mut c1, &id, Duration::from_secs(5)).unwrap(),
        bytes
    );
    nav.shutdown();
}

/// AT-BRW-002: uma interface que não implementa o protocolo corretamente
/// (rede ou Genesis diferentes) não estabelece sessão válida.
#[test]
fn incompatible_interface_rejected() {
    let g = genesis();
    let vs = validators();
    let n = start(&g, "c", SecretKey::from_seed(vs[0].seed()), vec![]);
    let mut other = g.clone();
    other.network_id = "outra-rede".into();
    assert!(connect(n.listen_addr(), &other).is_err());
    let mut wrong_genesis = g.clone();
    wrong_genesis.min_fee += 1;
    assert!(connect(n.listen_addr(), &wrong_genesis).is_err());
    // O Navegador com Genesis errado sobe, mas não obtém conteúdo nem estado.
    let nav = Server::start(Config {
        genesis: other,
        node: n.listen_addr().into(),
        net: NetOptions::default(),
        key: None,
        data_dir: tmp("nav-bad"),
        listen: "127.0.0.1:0".parse().unwrap(),
    })
    .unwrap();
    let ui = Origin::Ui.host(nav.port());
    let home = http(nav.port(), "GET", &ui, "/", None, "");
    assert!(home.body.contains("sem conexão com o Node"));
    // E o Navegador só escuta em loopback.
    assert!(Server::start(Config {
        genesis: g,
        node: n.listen_addr().into(),
        net: NetOptions::default(),
        key: None,
        data_dir: tmp("nav-bad2"),
        listen: "0.0.0.0:0".parse().unwrap(),
    })
    .is_err());
}

/// Um Node que entrega um pedaço adulterado é detectado: a verificação
/// acontece no cliente, não depende do Node ser honesto.
#[test]
fn tampered_chunk_detected_by_client() {
    let data = site_bundle().to_canonical_bytes();
    let info = rz_core::content::describe(&data).unwrap();
    let mut bad = data.clone();
    bad[3] ^= 0xff;
    assert!(!info.check_chunk(0, &bad));
    assert!(!rz_core::content::verify(&info.id(), &bad));
}
