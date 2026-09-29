//! Testes do Grande Mercado e do Pool permanente sobre blocos reais
//! (`spec/MARKET.md`, AT-MKT-001..004, AT-POOL-001..004).

use rz_codec::{Decode, Encode};
use rz_crypto::{Hash32, SecretKey};

use crate::block::{Block, BlockId};
use crate::genesis::tests::sample;
use crate::genesis::{Allocation, GenesisAsset};
use crate::market::{AssetId, Side, PRICE_SCALE};
use crate::state::{ExecParams, State};
use crate::tx::{Transaction, TxBody, TxError, TxKind, TX_VERSION};
use crate::Genesis;

const P: u64 = PRICE_SCALE as u64; // 1 unidade de ZERO por unidade do ativo

fn validator() -> SecretKey {
    SecretKey::from_seed([1; 32])
}

/// Tem ZERO (1 000 000) e o ativo de teste.
fn alice() -> SecretKey {
    SecretKey::from_seed([2; 32])
}

/// Tem ZERO (500) e o ativo de teste.
fn bob() -> SecretKey {
    SecretKey::from_seed([3; 32])
}

fn carol() -> SecretKey {
    SecretKey::from_seed([4; 32])
}

fn asset() -> AssetId {
    AssetId::external("testnet-externa", "ATV")
}

fn genesis() -> Genesis {
    let mut g = sample();
    let mut allocations = vec![
        Allocation {
            address: alice().public_key().address(),
            amount: 10_000,
        },
        Allocation {
            address: bob().public_key().address(),
            amount: 5_000,
        },
    ];
    allocations.sort_by_key(|a| a.address);
    g.assets = vec![GenesisAsset {
        network: "testnet-externa".into(),
        asset_ref: "ATV".into(),
        decimals: 8,
        allocations,
    }];
    // Carol recebe ZERO para comprar.
    g.allocations.push(Allocation {
        address: carol().public_key().address(),
        amount: 100_000,
    });
    g.allocations.sort_by_key(|a| a.address);
    g
}

struct Sim {
    g: Genesis,
    state: State,
    parent: BlockId,
    height: u64,
}

impl Sim {
    fn new() -> Self {
        let g = genesis();
        let state = State::from_genesis(&g).unwrap();
        state.check_supply().unwrap();
        Self {
            parent: BlockId(g.hash()),
            g,
            state,
            height: 0,
        }
    }

    fn tx_nonce(&self, key: &SecretKey, kind: TxKind, offset: u64) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: key.public_key(),
            nonce: self.state.account(&key.public_key().address()).nonce + offset,
            fee: self.state.params().min_fee,
            kind,
        }
        .sign(key, &self.g.network_id)
        .unwrap()
    }

    fn tx(&self, key: &SecretKey, kind: TxKind) -> Transaction {
        self.tx_nonce(key, kind, 0)
    }

    fn check(&self, tx: &Transaction) -> Result<(), TxError> {
        self.state
            .check_transaction(tx, &ExecParams::at(&self.g, self.height + 1))
    }

    fn build(&self, txs: Vec<Transaction>) -> (Block, State) {
        Block::build(
            &self.g,
            self.parent,
            self.height,
            &self.state,
            0,
            txs,
            &validator(),
        )
        .expect("bloco válido")
    }

    fn block(&mut self, txs: Vec<Transaction>) {
        let (b, s) = self.build(txs);
        self.parent = b.id();
        self.height += 1;
        self.state = s;
        self.state.check_supply().unwrap();
    }

    fn order(&self, key: &SecretKey, side: Side, amount: u64, price: u64) -> Transaction {
        self.tx(
            key,
            TxKind::PlaceOrder {
                asset: asset(),
                side,
                amount,
                price,
                expires_at: self.height + 100,
            },
        )
    }

    fn zero(&self, key: &SecretKey) -> u64 {
        self.state.account(&key.public_key().address()).balance
    }

    fn asset_balance(&self, key: &SecretKey) -> u64 {
        self.state
            .market()
            .balance(&key.public_key().address(), &asset())
    }
}

// AT-MKT-001 — ordem válida: aceita, com o valor reservado.
#[test]
fn at_mkt_001_valid_order_reserves_value() {
    let mut sim = Sim::new();
    let before = sim.zero(&carol());
    let buy = sim.order(&carol(), Side::Buy, 100, 2 * P);
    let id = buy.id().0;
    sim.block(vec![buy]);
    let o = sim.state.market().orders.get(&id).expect("ordem no livro");
    assert_eq!(o.escrow, 200);
    assert_eq!(sim.zero(&carol()), before - 200 - sim.g.min_fee);

    let sell = sim.order(&alice(), Side::Sell, 50, 3 * P);
    sim.block(vec![sell]);
    assert_eq!(sim.asset_balance(&alice()), 10_000 - 50);
    assert_eq!(sim.state.market().orders.len(), 2);
}

// AT-MKT-002 — ordens que violam as regras são rejeitadas.
#[test]
fn at_mkt_002_invalid_orders_rejected() {
    let sim = Sim::new();
    let h = sim.height;
    let place = |key: &SecretKey, asset, side, amount, price, expires_at| {
        sim.tx(
            key,
            TxKind::PlaceOrder {
                asset,
                side,
                amount,
                price,
                expires_at,
            },
        )
    };
    let cases = [
        // Ativo não registrado.
        place(
            &carol(),
            AssetId::external("x", "Y"),
            Side::Buy,
            10,
            P,
            h + 5,
        ),
        // ZERO não é ativo-base (todo par é cotado em ZERO).
        place(&carol(), AssetId::ZERO, Side::Buy, 10, P, h + 5),
        // Poeira: valor nulo no limite.
        place(&carol(), asset(), Side::Buy, 1, 1, h + 5),
        // Validade já vencida ou além do máximo.
        place(&carol(), asset(), Side::Buy, 10, P, h),
        place(&carol(), asset(), Side::Buy, 10, P, h + 10_000_000),
        // Vender sem ter o ativo; comprar sem ZERO suficiente.
        place(&carol(), asset(), Side::Sell, 10, P, h + 5),
        place(&bob(), asset(), Side::Buy, 1_000, P, h + 5),
    ];
    for tx in &cases {
        assert!(sim.check(tx).is_err(), "deveria rejeitar {tx:?}");
    }
    // Preço nulo é rejeitado antes do estado.
    let zero_price = place(&carol(), asset(), Side::Buy, 10, 0, h + 5);
    assert_eq!(sim.check(&zero_price), Err(TxError::Market("preço nulo")));
}

// AT-MKT-003 e AT-MKT-004 — só o autor cancela; o valor reservado volta.
#[test]
fn at_mkt_003_004_cancel_only_by_owner() {
    let mut sim = Sim::new();
    let buy = sim.order(&carol(), Side::Buy, 100, 2 * P);
    let id = buy.id().0;
    sim.block(vec![buy]);
    let zero_after_order = sim.zero(&carol());

    let steal = sim.tx(&bob(), TxKind::CancelOrder { order: id });
    assert_eq!(
        sim.check(&steal),
        Err(TxError::Market("ordem pertence a outra conta"))
    );
    let missing = sim.tx(
        &carol(),
        TxKind::CancelOrder {
            order: Hash32([9; 32]),
        },
    );
    assert!(sim.check(&missing).is_err());

    let cancel = sim.tx(&carol(), TxKind::CancelOrder { order: id });
    sim.block(vec![cancel]);
    assert!(!sim.state.market().orders.contains_key(&id));
    assert_eq!(sim.zero(&carol()), zero_after_order + 200 - sim.g.min_fee);
}

// Leilão de preço uniforme: todos executam ao mesmo preço; a sobra da
// reserva de quem aceitava pagar mais volta ao comprador.
#[test]
fn crossing_orders_settle_at_uniform_price() {
    let mut sim = Sim::new();
    let carol_zero = sim.zero(&carol());
    let alice_zero = sim.zero(&alice());
    let txs = vec![
        sim.order(&carol(), Side::Buy, 100, 3 * P),
        sim.order(&alice(), Side::Sell, 60, P),
        sim.order(&bob(), Side::Sell, 60, 2 * P),
    ];
    sim.block(txs);
    let price = *sim.state.market().last_price.get(&asset()).expect("preço");
    // Demanda 100 a qualquer preço ≤ 3; oferta 60 a 1 e 120 a 2:
    // volume máximo 100 a 2.
    assert_eq!(price, 2 * P);
    assert_eq!(sim.asset_balance(&carol()), 100);
    // Carol paga 200 (não 300): a diferença da reserva voltou.
    assert_eq!(sim.zero(&carol()), carol_zero - 200 - sim.g.min_fee);
    // Alice vendeu 60 a 2 (e não a 1, seu limite).
    assert_eq!(sim.zero(&alice()), alice_zero + 120 - sim.g.min_fee);
    // Bob vendeu os 40 restantes; 20 continuam no livro.
    let rest: Vec<_> = sim.state.market().orders.values().collect();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].remaining, 20);
    assert_eq!(rest[0].owner, bob().public_key().address());
}

// THR-MKT-001 — reordenar as transações do bloco não muda o resultado.
#[test]
fn block_order_does_not_change_market_outcome() {
    let sim = Sim::new();
    let txs = vec![
        sim.order(&carol(), Side::Buy, 70, 3 * P),
        sim.order(&alice(), Side::Sell, 50, P),
        sim.order(&bob(), Side::Sell, 50, 2 * P),
    ];
    let (_, a) = sim.build(txs.clone());
    let mut reversed = txs;
    reversed.reverse();
    let (_, b) = sim.build(reversed);
    assert_eq!(a.market().last_price, b.market().last_price);
    assert_eq!(a.market().balances, b.market().balances);
    for addr in [alice(), bob(), carol()].map(|k| k.public_key().address()) {
        assert_eq!(a.account(&addr), b.account(&addr));
    }
}

// Ordens vencidas saem do livro com reembolso.
#[test]
fn expired_orders_refunded() {
    let mut sim = Sim::new();
    let sell = sim.tx(
        &alice(),
        TxKind::PlaceOrder {
            asset: asset(),
            side: Side::Sell,
            amount: 30,
            price: 5 * P,
            expires_at: sim.height + 3,
        },
    );
    sim.block(vec![sell]);
    assert_eq!(sim.asset_balance(&alice()), 10_000 - 30);
    sim.block(vec![]);
    sim.block(vec![]);
    assert!(sim.state.market().orders.is_empty());
    assert_eq!(sim.asset_balance(&alice()), 10_000);
}

// Tarifa do mercado (padrão 0): quando aprovada, vai para o Pool.
#[test]
fn market_fee_goes_to_pool() {
    let mut sim = Sim::new();
    sim.state.params_mut().market.fee_bps = 100; // 1%
    let alice_zero = sim.zero(&alice());
    let txs = vec![
        sim.order(&carol(), Side::Buy, 1_000, P),
        sim.order(&alice(), Side::Sell, 1_000, P),
    ];
    sim.block(txs);
    assert_eq!(sim.state.market().pool_balance(&AssetId::ZERO), 10);
    assert_eq!(sim.zero(&alice()), alice_zero + 990 - sim.g.min_fee);
}

// AT-POOL-001 — depósito válido é contabilizado no Pool (ZERO e externo).
#[test]
fn at_pool_001_deposits_counted() {
    let mut sim = Sim::new();
    let zero_before = sim.zero(&carol());
    let d1 = sim.tx(
        &carol(),
        TxKind::PoolDeposit {
            asset: AssetId::ZERO,
            amount: 500,
        },
    );
    let d2 = sim.tx(
        &alice(),
        TxKind::PoolDeposit {
            asset: asset(),
            amount: 700,
        },
    );
    sim.block(vec![d1, d2]);
    let m = sim.state.market();
    assert_eq!(m.pool_balance(&AssetId::ZERO), 500);
    assert_eq!(m.pool_balance(&asset()), 700);
    assert_eq!(sim.zero(&carol()), zero_before - 500 - sim.g.min_fee);
    assert_eq!(sim.asset_balance(&alice()), 10_000 - 700);
    // Depósito acima do saldo é rejeitado.
    let too_much = sim.tx(
        &bob(),
        TxKind::PoolDeposit {
            asset: asset(),
            amount: 5_001,
        },
    );
    assert!(sim.check(&too_much).is_err());
}

// AT-POOL-002 — não existe operação de retirada do Pool: nenhuma tag de
// transação decodificável corresponde a uma retirada, e o conjunto de
// operações é exatamente o documentado.
#[test]
fn at_pool_002_no_withdrawal_operation_exists() {
    let documented = [
        0x00u8, 0x01, 0x10, 0x11, 0x12, 0x13, 0x14, 0x20, 0x21, 0x22, 0x23, 0x30, 0x31, 0x32, 0x33,
    ];
    for tag in 0..=255u8 {
        // Um corpo longo de zeros basta para ultrapassar a tag.
        let mut bytes = vec![tag];
        bytes.extend(std::iter::repeat_n(0u8, 4_096));
        let decoded = {
            let mut d = rz_codec::Decoder::new(&bytes);
            TxKind::decode(&mut d)
        };
        let known = !matches!(decoded, Err(rz_codec::DecodeError::InvalidTag(t)) if t == tag);
        assert_eq!(
            known,
            documented.contains(&tag),
            "tag {tag:#04x} fora do conjunto documentado"
        );
    }
}

// AT-POOL-003 — o Pool persiste através de muitas transições: nenhuma
// operação o reduz.
#[test]
fn at_pool_003_pool_never_decreases() {
    let mut sim = Sim::new();
    let d = sim.tx(
        &alice(),
        TxKind::PoolDeposit {
            asset: asset(),
            amount: 1_000,
        },
    );
    let z = sim.tx(
        &carol(),
        TxKind::PoolDeposit {
            asset: AssetId::ZERO,
            amount: 2_000,
        },
    );
    sim.block(vec![d, z]);
    let snapshot = sim.state.market().pool.clone();
    for round in 0..10u64 {
        let txs = vec![
            sim.order(&carol(), Side::Buy, 10 + round, 2 * P),
            sim.order(&alice(), Side::Sell, 10, P),
            sim.tx(
                &bob(),
                TxKind::TransferAsset {
                    asset: asset(),
                    to: carol().public_key().address(),
                    amount: 1,
                },
            ),
        ];
        sim.block(txs);
        for (a, v) in &snapshot {
            assert!(sim.state.market().pool_balance(a) >= *v);
        }
    }
    assert_eq!(sim.state.market().pool, snapshot);
}

// AT-POOL-004 — ativo externo não pode ser registrado nem usado como ZERO.
#[test]
fn at_pool_004_external_asset_never_zero() {
    let mut g = genesis();
    g.assets[0].asset_ref = "ZERO".into();
    assert!(g.validate().is_err());
    let mut g = genesis();
    g.assets[0].network = "rede-zero-devnet-1".into();
    assert!(g.validate().is_err());
    // Fora da DEVNET, sem ponte verificável, nenhum ativo externo.
    let mut g = genesis();
    g.kind = crate::NetworkKind::Testnet;
    g.network_id = "rede-zero-testnet-1".into();
    assert!(g.validate().is_err());

    let sim = Sim::new();
    let as_zero = sim.tx(
        &alice(),
        TxKind::TransferAsset {
            asset: AssetId::ZERO,
            to: bob().public_key().address(),
            amount: 1,
        },
    );
    assert_eq!(
        sim.check(&as_zero),
        Err(TxError::Market("ativo externo não registrado"))
    );
}

#[test]
fn market_transactions_roundtrip() {
    let sim = Sim::new();
    for kind in [
        TxKind::TransferAsset {
            asset: asset(),
            to: bob().public_key().address(),
            amount: 3,
        },
        TxKind::PoolDeposit {
            asset: AssetId::ZERO,
            amount: 4,
        },
        TxKind::CancelOrder {
            order: Hash32([5; 32]),
        },
    ] {
        let tx = sim.tx(&alice(), kind);
        assert_eq!(
            Transaction::from_canonical_bytes(&tx.to_canonical_bytes()).unwrap(),
            tx
        );
    }
    let o = sim.order(&alice(), Side::Sell, 1, P);
    assert_eq!(
        Transaction::from_canonical_bytes(&o.to_canonical_bytes()).unwrap(),
        o
    );
}

/// Gerador determinístico (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

// Propriedades sob operações aleatórias em muitos blocos:
// conservação de ZERO e do ativo (verificada a cada bloco por
// `check_supply`), Pool monotônico e livro não cruzado após o leilão.
#[test]
fn randomized_market_invariants() {
    let seeds: u64 = std::env::var("RZ_MARKET_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    for seed in 0..seeds {
        let mut sim = Sim::new();
        let mut rng = Rng(0xa11ce ^ seed);
        let traders = [alice(), bob(), carol()];
        let mut pool_prev = sim.state.market().pool.clone();
        for _ in 0..60 {
            let mut txs = Vec::new();
            for (i, k) in traders.iter().enumerate() {
                if rng.below(3) == 0 {
                    continue;
                }
                let kind = match rng.below(10) {
                    0 => TxKind::PoolDeposit {
                        asset: if rng.below(2) == 0 {
                            AssetId::ZERO
                        } else {
                            asset()
                        },
                        amount: 1 + rng.below(20),
                    },
                    1 => {
                        // Cancela uma ordem própria, se houver.
                        let own: Vec<Hash32> = sim
                            .state
                            .market()
                            .orders
                            .values()
                            .filter(|o| o.owner == k.public_key().address())
                            .map(|o| o.id)
                            .collect();
                        match own.first() {
                            Some(id) => TxKind::CancelOrder { order: *id },
                            None => continue,
                        }
                    }
                    2 => TxKind::TransferAsset {
                        asset: asset(),
                        to: traders[(i + 1) % 3].public_key().address(),
                        amount: 1 + rng.below(50),
                    },
                    _ => TxKind::PlaceOrder {
                        asset: asset(),
                        side: if rng.below(2) == 0 {
                            Side::Buy
                        } else {
                            Side::Sell
                        },
                        amount: 1 + rng.below(200),
                        // Preços de 0,5 a 3 unidades de ZERO.
                        price: P / 2 + rng.below(5 * P / 2),
                        expires_at: sim.height + 2 + rng.below(20),
                    },
                };
                let tx = sim.tx(k, kind);
                // Só inclui transações válidas (como o mempool faria).
                if sim.check(&tx).is_ok() {
                    txs.push(tx);
                }
            }
            if rng.below(8) == 0 {
                sim.state.params_mut().market.fee_bps = rng.below(200) as u32;
            }
            sim.block(txs); // check_supply a cada bloco

            let m = sim.state.market();
            for (a, v) in &pool_prev {
                assert!(m.pool_balance(a) >= *v, "semente {seed}: Pool diminuiu");
            }
            pool_prev = m.pool.clone();
            let best_bid = m
                .orders
                .values()
                .filter(|o| o.side == Side::Buy)
                .map(|o| o.price)
                .max();
            let best_ask = m
                .orders
                .values()
                .filter(|o| o.side == Side::Sell)
                .map(|o| o.price)
                .min();
            if let (Some(b), Some(a)) = (best_bid, best_ask) {
                // Cruzamento só pode sobrar por poeira (valor nulo) — com
                // estas quantidades e preços, nunca.
                if b >= a {
                    let mut os: Vec<_> = m.orders.values().collect();
                    os.sort_by_key(|o| (o.side, o.price));
                    for o in os {
                        eprintln!(
                            "{:?} p={} rem={} esc={} h={} exp={} owner={}",
                            o.side,
                            o.price,
                            o.remaining,
                            o.escrow,
                            o.placed_at,
                            o.expires_at,
                            &format!("{}", o.owner)[..6]
                        );
                    }
                    eprintln!("altura {} dirty={:?}", sim.height, m.dirty);
                }
                assert!(b < a, "semente {seed}: livro cruzado ({b} ≥ {a})");
            }
        }
    }
}
