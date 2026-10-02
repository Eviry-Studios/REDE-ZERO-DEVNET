//! Grande Mercado e Pool permanente de ponta a ponta contra Nodes reais
//! (ADR-0014): ordens pela Wallet, leilão por bloco, depósito no Pool.

#![allow(clippy::unwrap_used)]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use rz_core::genesis::GenesisAsset;
use rz_core::market::{AssetId, Side, PRICE_SCALE};
use rz_core::{Allocation, Genesis, GovernanceParams, NetworkKind, TxKind, PROTOCOL_VERSION};
use rz_crypto::SecretKey;
use rz_node::{now_ms, LogLevel, Node, NodeConfig};
use rz_wallet::{account, assets, connect, market, send_account, Keys};

const NET: &str = "rede-zero-devnet-mkt";

fn validators() -> Vec<SecretKey> {
    (0..2)
        .map(|i| SecretKey::from_seed([180 + i; 32]))
        .collect()
}

/// Vende o ativo de teste.
fn seller() -> Keys {
    Keys::from_secret(SecretKey::from_seed([230; 32]))
}

/// Compra com ZERO.
fn buyer() -> Keys {
    Keys::from_secret(SecretKey::from_seed([231; 32]))
}

fn asset() -> AssetId {
    AssetId::external("testnet-externa", "ATV")
}

fn genesis() -> Genesis {
    let mut allocations = vec![
        Allocation {
            address: seller().address(),
            amount: 1_000_000,
        },
        Allocation {
            address: buyer().address(),
            amount: 10_000_000,
        },
    ];
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
        assets: vec![GenesisAsset {
            network: "testnet-externa".into(),
            asset_ref: "ATV".into(),
            decimals: 8,
            allocations: vec![Allocation {
                address: seller().address(),
                amount: 50_000,
            }],
        }],
        bridges: vec![],
    }
}

fn start(g: &Genesis, name: &str, key: SecretKey, peers: Vec<SocketAddr>) -> Node {
    let dir =
        std::env::temp_dir().join(format!("rz-mkt-{name}-{}-{}", std::process::id(), now_ms()));
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
fn market_orders_settle_and_pool_accumulates() {
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
    let mut c1 = connect(n1.listen_addr(), &g).unwrap();
    let mut c2 = connect(n2.listen_addr(), &g).unwrap();

    // O ativo de teste aparece com sua origem e o saldo do vendedor.
    let v = assets(&mut c1, Some(seller().address())).unwrap();
    assert_eq!(v.assets.len(), 1);
    assert_eq!(v.assets[0].id, asset());
    assert_eq!(v.assets[0].balance, Some(50_000));

    let price = 3 * PRICE_SCALE as u64; // 3 unidades de ZERO por unidade
    let (_, _, h) = account(&mut c1, seller().address()).unwrap();
    let sell = send_account(
        &mut c1,
        &g,
        &seller(),
        TxKind::PlaceOrder {
            asset: asset(),
            side: Side::Sell,
            amount: 10_000,
            price,
            expires_at: h + 500,
        },
        10,
    )
    .unwrap();
    // A venda fica no livro (visível em outro Node).
    assert!(wait_until(|| market(
        &mut c2,
        asset(),
        Some(seller().address())
    )
    .map(|m| m.asks.len() == 1 && m.own.iter().any(|o| o.id == sell.0))
    .unwrap_or(false)));

    // Compra acima do limite do vendedor: executa ao preço de equilíbrio.
    let (_, _, h) = account(&mut c2, buyer().address()).unwrap();
    send_account(
        &mut c2,
        &g,
        &buyer(),
        TxKind::PlaceOrder {
            asset: asset(),
            side: Side::Buy,
            amount: 4_000,
            price: 5 * PRICE_SCALE as u64,
            expires_at: h + 500,
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| assets(&mut c1, Some(buyer().address()))
        .map(|v| v.assets[0].balance == Some(4_000))
        .unwrap_or(false)));
    let m = market(&mut c1, asset(), None).unwrap();
    // Candidatos 3 e 5 empatam em volume (4 000) e desequilíbrio (6 000):
    // a regra da mediana inferior escolhe 3.
    assert_eq!(m.last_price, Some(price));
    assert_eq!(m.asks[0].amount, 6_000);
    assert!(m.bids.is_empty());
    // O comprador reservou 4 000 × 5 e pagou 4 000 × 3: a diferença voltou.
    let (balance, _, _) = account(&mut c1, buyer().address()).unwrap();
    assert_eq!(balance, 10_000_000 - 10 - 12_000);
    let (balance, _, _) = account(&mut c1, seller().address()).unwrap();
    assert_eq!(balance, 1_000_000 - 10 + 12_000);

    // Depósito permanente no Pool: contabilizado em todos os Nodes.
    send_account(
        &mut c2,
        &g,
        &buyer(),
        TxKind::PoolDeposit {
            asset: AssetId::ZERO,
            amount: 12_345,
        },
        10,
    )
    .unwrap();
    assert!(wait_until(|| assets(&mut c1, None)
        .map(|v| v.pool_zero == 12_345)
        .unwrap_or(false)));
    assert!(wait_until(|| assets(&mut c2, None)
        .map(|v| v.pool_zero == 12_345)
        .unwrap_or(false)));
}
