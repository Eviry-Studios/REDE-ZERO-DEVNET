//! Ponte e trocas atômicas sobre blocos reais (ADR-0019, `spec/BRIDGE.md`).
//!
//! A rede de origem é simulada com as regras de regtest do Bitcoin (alvo
//! máximo, sem reajuste): os cabeçalhos são minerados de verdade, só que com
//! dificuldade trivial.

use rz_crypto::{Address, Hash32, SecretKey};

use crate::block::{Block, BlockId};
use crate::bridge::{
    burn_script, sha256, sha256d, tests::regtest_rules, ChainConfig, Header, PowRules,
};
use crate::genesis::tests::sample;
use crate::market::AssetId;
use crate::state::{ExecParams, State};
use crate::tx::{Transaction, TxBody, TxError, TxKind, TX_VERSION};
use crate::{Genesis, NetworkKind};

const CHAIN: &str = "bitcoin-regtest";

fn validator() -> SecretKey {
    SecretKey::from_seed([1; 32])
}

fn rich() -> SecretKey {
    SecretKey::from_seed([2; 32])
}

fn bob() -> Address {
    Address(Hash32([0xb0; 32]))
}

/// Minera um cabeçalho sobre `prev` (nonce até a prova valer).
fn mine(prev: Hash32, merkle_root: Hash32, time: u32, rules: &PowRules) -> Header {
    let mut h = Header {
        version: 0x2000_0000,
        prev,
        merkle_root,
        time,
        bits: 0x207f_ffff,
        nonce: 0,
    };
    while crate::bridge::check_pow(&h, rules).is_err() {
        h.nonce += 1;
    }
    h
}

fn checkpoint() -> Header {
    mine(
        Hash32::ZERO,
        Hash32([0x11; 32]),
        1_700_000_000,
        &regtest_rules(),
    )
}

fn config() -> ChainConfig {
    ChainConfig {
        name: CHAIN.into(),
        asset_ref: "BTC".into(),
        decimals: 8,
        rules: regtest_rules(),
        checkpoint: checkpoint().serialize(),
        checkpoint_height: 0,
        confirmations: 3,
    }
}

fn genesis() -> Genesis {
    let mut g = sample();
    g.bridges = vec![config()];
    g
}

/// Transação de origem mínima (sem testemunhas) com as saídas dadas.
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

fn pair(a: &Hash32, b: &Hash32) -> Hash32 {
    let mut x = [0u8; 64];
    x[..32].copy_from_slice(&a.0);
    x[32..].copy_from_slice(&b.0);
    Hash32(sha256d(&x))
}

struct Sim {
    g: Genesis,
    state: State,
    parent: BlockId,
    height: u64,
    /// Ponta da cadeia de origem simulada.
    tip: Header,
    tip_time: u32,
}

impl Sim {
    fn new() -> Self {
        Self::with(genesis())
    }

    fn with(g: Genesis) -> Self {
        let state = State::from_genesis(&g).unwrap();
        let parent = BlockId(g.hash());
        let tip = checkpoint();
        Self {
            g,
            state,
            parent,
            height: 0,
            tip,
            tip_time: tip.time,
        }
    }

    fn tx(&self, key: &SecretKey, kind: TxKind) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: key.public_key(),
            nonce: self.state.account(&key.public_key().address()).nonce,
            fee: self.state.params().min_fee,
            kind,
        }
        .sign(key, &self.g.network_id)
        .unwrap()
    }

    fn check(&self, tx: &Transaction) -> Result<(), TxError> {
        self.state
            .check_transaction(tx, &ExecParams::at(&self.g, self.height + 1))
    }

    fn block(&mut self, txs: Vec<Transaction>) {
        let (b, s) = Block::build(
            &self.g,
            self.parent,
            self.height,
            &self.state,
            self.height + 1,
            txs,
            &validator(),
        )
        .expect("bloco válido");
        self.parent = b.id();
        self.height += 1;
        self.state = s;
        self.state.check_supply().unwrap();
    }

    fn apply(&mut self, kind: TxKind) {
        let tx = self.tx(&rich(), kind);
        self.check(&tx).unwrap();
        self.block(vec![tx]);
    }

    /// Minera `n` cabeçalhos na ponta; o primeiro com `merkle` (se houver).
    fn extend(&mut self, n: usize, first_merkle: Option<Hash32>) -> Vec<Header> {
        let mut out = Vec::new();
        for i in 0..n {
            self.tip_time += 600;
            let merkle = match (i, first_merkle) {
                (0, Some(m)) => m,
                _ => Hash32([i as u8 + 0x40; 32]),
            };
            let h = mine(self.tip.hash(), merkle, self.tip_time, &regtest_rules());
            self.tip = h;
            out.push(h);
        }
        out
    }

    fn submit(&mut self, headers: &[Header]) -> Result<(), TxError> {
        let tx = self.tx(
            &rich(),
            TxKind::BridgeHeaders {
                chain: CHAIN.into(),
                headers: headers.iter().map(Header::serialize).collect(),
            },
        );
        self.check(&tx)?;
        self.block(vec![tx]);
        Ok(())
    }

    fn asset(&self) -> AssetId {
        config().asset()
    }
}

/// Bloco de origem com duas transações; a segunda queima `amount` para `to`.
fn burn_block(network: &str, to: &Address, amount: u64) -> (Vec<u8>, Hash32, Vec<Hash32>) {
    let other = origin_tx(&[(50, vec![0x51])], 1);
    let burn = origin_tx(
        &[(1_000, vec![0x51]), (amount, burn_script(network, to))],
        2,
    );
    let a = Hash32(sha256d(&other));
    let b = Hash32(sha256d(&burn));
    (burn, pair(&a, &b), vec![a])
}

fn deposit(block: &Header, tx: Vec<u8>, path: Vec<Hash32>, index: u32) -> TxKind {
    TxKind::BridgeDeposit {
        chain: CHAIN.into(),
        block: block.hash(),
        tx,
        path,
        index,
    }
}

#[test]
fn genesis_rules_for_bridges() {
    let mut g = genesis();
    g.validate().unwrap();
    g.bridges.push(config());
    assert!(g.validate().is_err(), "rede repetida");
    let mut m = genesis();
    m.kind = NetworkKind::Mainnet;
    m.network_id = "rede-zero-mainnet".into();
    assert!(m.validate().is_err(), "sem auditoria, nunca na MAINNET");
    let mut r = genesis();
    r.bridges[0].rules.retarget = true;
    r.bridges[0].checkpoint_height = 5;
    assert!(
        r.validate().is_err(),
        "partida fora da fronteira de reajuste"
    );
    let mut z = genesis();
    z.bridges[0].asset_ref = "ZERO".into();
    assert!(z.validate().is_err(), "não pode se passar por ZERO");
    // O ativo existe desde o Genesis, com oferta zero e verificação por
    // cliente leve.
    let s = State::from_genesis(&genesis()).unwrap();
    let info = &s.market().assets[&config().asset()];
    assert_eq!(info.supply, 0);
    assert_eq!(
        info.verification,
        crate::market::Verification::LightClientBurn
    );
}

#[test]
fn burn_proof_mints_once_after_confirmations() {
    let mut sim = Sim::new();
    let net = sim.g.network_id.clone();
    let (burn, root, path) = burn_block(&net, &bob(), 75_000);
    let hs = sim.extend(1, Some(root));
    let block = hs[0];
    sim.submit(&hs).unwrap();
    // Uma confirmação só: recusado.
    let early = sim.tx(&rich(), deposit(&block, burn.clone(), path.clone(), 1));
    assert!(matches!(sim.check(&early), Err(TxError::Bridge(_))));
    let more = sim.extend(2, None);
    sim.submit(&more).unwrap();
    // Caminho ou posição errados: recusado.
    let bad_path = sim.tx(
        &rich(),
        deposit(&block, burn.clone(), vec![Hash32([9; 32])], 1),
    );
    assert!(matches!(sim.check(&bad_path), Err(TxError::Bridge(_))));
    let bad_index = sim.tx(&rich(), deposit(&block, burn.clone(), path.clone(), 0));
    assert!(matches!(sim.check(&bad_index), Err(TxError::Bridge(_))));
    // Válido: cunha para o destinatário gravado na queima, não para quem envia.
    sim.apply(deposit(&block, burn.clone(), path.clone(), 1));
    let asset = sim.asset();
    assert_eq!(sim.state.market().balance(&bob(), &asset), 75_000);
    assert_eq!(sim.state.market().assets[&asset].supply, 75_000);
    assert_eq!(
        sim.state
            .market()
            .balance(&rich().public_key().address(), &asset),
        0
    );
    // A mesma queima nunca cunha duas vezes.
    let again = sim.tx(&rich(), deposit(&block, burn, path, 1));
    assert!(matches!(sim.check(&again), Err(TxError::Bridge(_))));
}

#[test]
fn burns_for_other_networks_or_malformed_rejected() {
    let mut sim = Sim::new();
    // Queima destinada a outra rede Zero.
    let (other, root, path) = burn_block("rede-zero-devnet-outra", &bob(), 5);
    let hs = sim.extend(3, Some(root));
    sim.submit(&hs).unwrap();
    let tx = sim.tx(&rich(), deposit(&hs[0], other, path, 1));
    assert!(matches!(sim.check(&tx), Err(TxError::Bridge(_))));
    // Transação de 64 bytes (ambiguidade com nós internos da árvore).
    let fake = vec![0u8; 64];
    let leaf = Hash32(sha256d(&fake));
    let hs = sim.extend(3, Some(leaf));
    sim.submit(&hs).unwrap();
    let tx = sim.tx(&rich(), deposit(&hs[0], fake, vec![], 0));
    assert!(matches!(sim.check(&tx), Err(TxError::Bridge(_))));
    // Bloco desconhecido.
    let tx = sim.tx(
        &rich(),
        TxKind::BridgeDeposit {
            chain: CHAIN.into(),
            block: Hash32([7; 32]),
            tx: vec![1, 2, 3],
            path: vec![],
            index: 0,
        },
    );
    assert!(sim.check(&tx).is_err());
}

#[test]
fn header_rules_enforced() {
    let mut sim = Sim::new();
    let rules = regtest_rules();
    let tip = sim.tip;
    // Não se conecta a nada conhecido.
    let orphan = mine(Hash32([5; 32]), Hash32::ZERO, tip.time + 600, &rules);
    assert!(sim.submit(&[orphan]).is_err());
    // Dificuldade diferente da exigida (sem reajuste, deve repetir a anterior).
    let mut wrong_bits = mine(tip.hash(), Hash32::ZERO, tip.time + 600, &rules);
    wrong_bits.bits = 0x1f7f_ffff;
    assert!(sim.submit(&[wrong_bits]).is_err());
    // Prova de trabalho inválida.
    let mut weak = mine(tip.hash(), Hash32::ZERO, tip.time + 600, &rules);
    while crate::bridge::check_pow(&weak, &rules).is_ok() {
        weak.nonce += 1;
    }
    assert!(sim.submit(&[weak]).is_err());
    // Horário que não passa da mediana dos anteriores.
    let stale = mine(tip.hash(), Hash32::ZERO, tip.time, &rules);
    assert!(sim.submit(&[stale]).is_err());
    // Repetido.
    let ok = sim.extend(1, None);
    sim.submit(&ok).unwrap();
    assert!(sim.submit(&ok).is_err());
    // Rede desconhecida.
    let tx = sim.tx(
        &rich(),
        TxKind::BridgeHeaders {
            chain: "outra".into(),
            headers: vec![ok[0].serialize()],
        },
    );
    assert!(sim.check(&tx).is_err());
}

#[test]
fn reorganization_follows_most_work() {
    let mut sim = Sim::new();
    let net = sim.g.network_id.clone();
    let base = sim.tip;
    let (burn, root, path) = burn_block(&net, &bob(), 9);
    let a = sim.extend(3, Some(root));
    sim.submit(&a).unwrap();
    // Ramo concorrente mais longo a partir do ponto de partida.
    sim.tip = base;
    sim.tip_time = base.time + 7;
    let b = sim.extend(5, None);
    sim.submit(&b).unwrap();
    let st = &sim.state.bridge().chains[CHAIN];
    assert_eq!(st.best, b[4].hash());
    assert_eq!(st.best_height(), 5);
    // O bloco da queima saiu da cadeia de maior trabalho.
    let tx = sim.tx(&rich(), deposit(&a[0], burn, path, 1));
    assert!(matches!(sim.check(&tx), Err(TxError::Bridge(_))));
}

#[test]
fn htlc_claim_and_refund_with_zero_and_external_asset() {
    let mut sim = Sim::new();
    let me = rich().public_key().address();
    let secret = [0x5e; 32].to_vec();
    let hashlock = Hash32(sha256(&secret));
    let before = sim.state.account(&me).balance;
    let lock = sim.tx(
        &rich(),
        TxKind::HtlcLock {
            asset: AssetId::ZERO,
            amount: 1_000,
            recipient: bob(),
            hashlock,
            timeout: sim.height + 10,
        },
    );
    let id = lock.id().0;
    sim.block(vec![lock]);
    assert_eq!(sim.state.account(&me).balance, before - 1_000 - 1);
    assert_eq!(sim.state.bridge().zero_escrow(), 1_000);
    // Antes do prazo: devolução recusada; pré-imagem errada recusada.
    assert!(sim
        .check(&sim.tx(&rich(), TxKind::HtlcRefund { id }))
        .is_err());
    assert!(sim
        .check(&sim.tx(
            &rich(),
            TxKind::HtlcClaim {
                id,
                preimage: b"errado".to_vec()
            }
        ))
        .is_err());
    // Pré-imagem de outro tamanho, mesmo com hash certo: recusada.
    let short = b"curto".to_vec();
    let lock2 = sim.tx(
        &rich(),
        TxKind::HtlcLock {
            asset: AssetId::ZERO,
            amount: 5,
            recipient: bob(),
            hashlock: Hash32(sha256(&short)),
            timeout: sim.height + 10,
        },
    );
    let id2 = lock2.id().0;
    sim.block(vec![lock2]);
    assert!(sim
        .check(&sim.tx(
            &rich(),
            TxKind::HtlcClaim {
                id: id2,
                preimage: short
            }
        ))
        .is_err());
    // Qualquer conta revela a pré-imagem; o valor vai para o destinatário.
    sim.apply(TxKind::HtlcClaim {
        id,
        preimage: secret.clone(),
    });
    assert_eq!(sim.state.account(&bob()).balance, 1_000);
    assert_eq!(sim.state.bridge().zero_escrow(), 5);
    let settled = sim.state.bridge().settled(&id).unwrap();
    assert_eq!(settled.preimage.as_deref(), Some(secret.as_slice()));
    assert!(sim
        .check(&sim.tx(
            &rich(),
            TxKind::HtlcClaim {
                id,
                preimage: secret
            }
        ))
        .is_err());

    // Ativo externo cunhado pela ponte, bloqueado e devolvido após o prazo.
    let net = sim.g.network_id.clone();
    let (burn, root, path) = burn_block(&net, &me, 40_000);
    let hs = sim.extend(3, Some(root));
    sim.submit(&hs).unwrap();
    sim.apply(deposit(&hs[0], burn, path, 1));
    let asset = sim.asset();
    let lock = sim.tx(
        &rich(),
        TxKind::HtlcLock {
            asset,
            amount: 30_000,
            recipient: bob(),
            hashlock: Hash32([1; 32]),
            timeout: sim.height + 3,
        },
    );
    let id = lock.id().0;
    sim.block(vec![lock]);
    assert_eq!(sim.state.market().balance(&me, &asset), 10_000);
    // Conservação do ativo com o valor retido (check_supply em cada bloco).
    while sim.height + 1 < sim.state.bridge().htlcs[&id].timeout {
        sim.block(vec![]);
    }
    assert!(sim
        .check(&sim.tx(&rich(), TxKind::HtlcRefund { id }))
        .is_ok());
    sim.apply(TxKind::HtlcRefund { id });
    assert_eq!(sim.state.market().balance(&me, &asset), 40_000);
    assert!(sim.state.bridge().settled(&id).unwrap().preimage.is_none());
    // Prazo já vencido na criação: recusado.
    let late = sim.tx(
        &rich(),
        TxKind::HtlcLock {
            asset: AssetId::ZERO,
            amount: 1,
            recipient: bob(),
            hashlock,
            timeout: sim.height + 1,
        },
    );
    assert!(sim.check(&late).is_err());
}

// spec/BRIDGE.md §2.1 — nova rede de origem depois do Genesis: publicada por
// qualquer conta, adotada só por proposta constitucional.
#[test]
fn chain_added_after_genesis_by_constitutional_proposal() {
    use crate::governance::{Category, Choice, ParamChange, ProposalStatus};
    let mut sim = Sim::with(sample());
    assert!(sim.state.bridge().chains.is_empty());
    let cfg = config();
    let id = crate::bridge::chain_config_id(&cfg);

    // Proposta antes da publicação: recusada.
    let propose = |sim: &Sim, category| {
        sim.tx(
            &rich(),
            TxKind::Propose {
                category,
                content_hash: Hash32([4; 32]),
                params: vec![ParamChange::AddBridgeChain(id)],
                release_id: None,
                deposit: sim.state.params().governance.deposit,
            },
        )
    };
    assert!(matches!(
        sim.check(&propose(&sim, Category::Constitutional)),
        Err(TxError::Bridge(_))
    ));
    let supply = sim.state.total_supply();
    sim.apply(TxKind::PublishBridgeChain {
        config: cfg.clone(),
    });
    assert_eq!(
        sim.state.total_supply(),
        supply - sim.state.params().governance.deposit,
        "custo de publicação queimado"
    );
    assert!(sim.state.bridge().chains.is_empty(), "publicar não adota");
    let again = sim.tx(
        &rich(),
        TxKind::PublishBridgeChain {
            config: cfg.clone(),
        },
    );
    assert!(matches!(sim.check(&again), Err(TxError::Bridge(_))));
    // Só constitucional.
    assert!(matches!(
        sim.check(&propose(&sim, Category::Ordinary)),
        Err(TxError::Governance(_))
    ));

    sim.apply(TxKind::LockStake {
        amount: 100_000,
        unlock_height: sim.height + 101,
    });
    let tx = propose(&sim, Category::Constitutional);
    let pid = tx.id().0;
    sim.check(&tx).unwrap();
    sim.block(vec![tx]);
    let p = sim.state.proposal(&pid).unwrap().clone();
    while sim.height + 1 < p.voting_start {
        sim.block(vec![]);
    }
    let votes = vec![
        sim.tx(
            &rich(),
            TxKind::Vote {
                proposal: pid,
                choice: Choice::Yes,
            },
        ),
        sim.tx(
            &validator(),
            TxKind::Vote {
                proposal: pid,
                choice: Choice::Yes,
            },
        ),
    ];
    sim.block(votes);
    while sim.height < p.activation_height {
        sim.block(vec![]);
    }
    assert_eq!(
        sim.state.proposal(&pid).unwrap().status,
        ProposalStatus::Activated
    );
    assert!(sim.state.bridge().chains.contains_key(CHAIN));
    assert!(sim.state.bridge().proposed.is_empty());
    let asset = AssetId::external(CHAIN, "BTC");
    assert_eq!(sim.state.market().assets[&asset].supply, 0);
    // A rede adotada funciona: cabeçalhos são aceitos.
    let hs = sim.extend(1, None);
    sim.apply(TxKind::BridgeHeaders {
        chain: CHAIN.into(),
        headers: hs.iter().map(Header::serialize).collect(),
    });
    // Republicar a mesma rede agora é recusado (já existe).
    let dup = sim.tx(&rich(), TxKind::PublishBridgeChain { config: cfg });
    assert!(matches!(sim.check(&dup), Err(TxError::Bridge(_))));
}

// Fora de DEVNET e TESTNET a ponte não cresce, nem por governança.
#[test]
fn chain_publication_refused_outside_test_networks() {
    let mut g = sample();
    g.kind = NetworkKind::Staging;
    g.network_id = "rede-zero-staging-teste".into();
    let sim = Sim::with(g);
    let tx = sim.tx(&rich(), TxKind::PublishBridgeChain { config: config() });
    assert_eq!(
        sim.check(&tx),
        Err(TxError::Bridge(
            "ponte ainda sem auditoria: só DEVNET e TESTNET"
        ))
    );
}

// spec/BRIDGE.md §4.1 — taxa de retransmissão escolhida por quem queima:
// vai para quem traz a prova, o resto para o destinatário; nada é emitido.
#[test]
fn relay_fee_split_between_relayer_and_recipient() {
    let mut sim = Sim::new();
    let net = sim.g.network_id.clone();
    let script = crate::bridge::burn_script_with_fee(&net, &bob(), 1_500);
    let make = |script: Vec<u8>, salt: u8| {
        let other = origin_tx(&[(50, vec![0x51])], salt);
        let burn = origin_tx(&[(1_000, vec![0x51]), (75_000, script)], salt + 1);
        let a = Hash32(sha256d(&other));
        let b = Hash32(sha256d(&burn));
        (burn, pair(&a, &b), vec![a])
    };
    let (burn, root, path) = make(script, 10);
    let hs = sim.extend(3, Some(root));
    sim.submit(&hs).unwrap();
    sim.apply(deposit(&hs[0], burn, path, 1));
    let asset = sim.asset();
    let relayer = rich().public_key().address();
    assert_eq!(sim.state.market().balance(&bob(), &asset), 73_500);
    assert_eq!(sim.state.market().balance(&relayer, &asset), 1_500);
    assert_eq!(
        sim.state.market().assets[&asset].supply,
        75_000,
        "a oferta é exatamente o que foi queimado"
    );
    // Taxa maior que a queima: recusada.
    let (bad, root, path) = make(
        crate::bridge::burn_script_with_fee(&net, &bob(), 75_001),
        20,
    );
    let hs = sim.extend(3, Some(root));
    sim.submit(&hs).unwrap();
    let tx = sim.tx(&rich(), deposit(&hs[0], bad, path, 1));
    assert_eq!(
        sim.check(&tx),
        Err(TxError::Bridge("taxa de retransmissão maior que a queima"))
    );
}
