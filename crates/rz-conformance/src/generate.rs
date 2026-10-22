//! Gerador dos vetores a partir da implementação de referência.
//!
//! Os vetores publicados são a referência; este gerador só é usado quando o
//! protocolo muda (e então a mudança entra numa nova versão, ADR-0021).
//! Provas privadas usam aleatoriedade, por isso duas gerações diferem nos
//! bytes; ambas verificam.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use rz_codec::Encode;
use rz_core::block::tx_root;
use rz_core::bridge::{
    self, burn_script, check_pow, sha256, sha256d, ChainConfig, Header, PowRules,
};
use rz_core::community::{approve, DecisionRule, NameKind};
use rz_core::defense::{attest, payload, DefenseMode};
use rz_core::genesis::GenesisAsset;
use rz_core::governance::{Category, Choice, GovernanceParams, ParamChange};
use rz_core::market::{AssetId, Side, PRICE_SCALE};
use rz_core::private::{build_private_tx, build_shield, SpendableNote};
use rz_core::release::{file_hash, tree_hash, Artifact, Compatibility, Release};
use rz_core::tx::TX_VERSION;
use rz_core::{
    Allocation, Block, BlockId, Genesis, GenesisValidator, NetworkKind, State, Transaction, TxBody,
    TxKind, PROTOCOL_VERSION,
};
use rz_crypto::{context, hash, Address, Hash32, SecretKey};
use rz_p2p::Message;
use rz_privacy::keys::ShieldedSecret;
use rz_privacy::note::scan_output;

use crate::format::{Case, Suite};
use crate::{SUITES, VECTOR_NETWORK};

type R<T> = Result<T, String>;

/// Gera todos os arquivos em `dir`.
pub fn generate(dir: &Path) -> R<()> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (file, title) in SUITES {
        let cases = match *file {
            "identidade.txt" => identity(),
            "hash.txt" => hashes(),
            "zero.txt" => zero(),
            "codificacao.txt" => encoding()?,
            "transacoes.txt" => transactions()?,
            "cadeia.txt" => chain()?,
            "p2p.txt" => p2p(),
            "governanca.txt" => governance(),
            "runtime.txt" => runtime()?,
            "ponte.txt" => bridge_vectors()?,
            "versoes.txt" => releases(),
            other => return Err(format!("conjunto desconhecido: {other}")),
        };
        let suite = Suite {
            title: format!("Rede Zero — vetores de conformidade v1 — {title}"),
            cases,
        };
        std::fs::write(dir.join(file), suite.render()).map_err(|e| format!("{file}: {e}"))?;
    }
    Ok(())
}

fn key(seed: u8) -> SecretKey {
    SecretKey::from_seed([seed; 32])
}

// ----------------------------------------------------------- identidade

fn identity() -> Vec<Case> {
    let mut out = Vec::new();
    let messages: [(&str, &str, &[u8]); 3] = [
        (context::TX_SIGNATURE, VECTOR_NETWORK, b"zero"),
        (context::BLOCK_SIGNATURE, VECTOR_NETWORK, b""),
        (context::TX_SIGNATURE, "rede-zero-testnet-1", b"zero"),
    ];
    for (i, seed) in [
        [0u8; 32],
        [1u8; 32],
        [0xffu8; 32],
        *b"rede-zero-identidade-vetor-00001",
    ]
    .into_iter()
    .enumerate()
    {
        let sk = SecretKey::from_seed(seed);
        let pk = sk.public_key();
        let (ctx, net, msg) = messages[i % messages.len()];
        let sig = sk.sign(ctx, net, msg);
        out.push(
            Case::new(format!("chave-{i}"))
                .hex("semente", &seed)
                .hex("chave_publica", pk.as_bytes())
                .hex("node_id", &pk.node_id().0 .0)
                .hex("endereco", &pk.address().0 .0)
                .set("contexto", ctx)
                .set("rede", net)
                .hex("mensagem", msg)
                .hex("assinatura", &sig.0)
                .set("valida", "sim"),
        );
    }
    // Assinatura alterada, de outra rede e de outro contexto: inválidas.
    let sk = key(1);
    let pk = sk.public_key();
    let mut bad = sk.sign(context::TX_SIGNATURE, VECTOR_NETWORK, b"zero");
    bad.0[0] ^= 1;
    let other_net = sk.sign(context::TX_SIGNATURE, "rede-zero-testnet-1", b"zero");
    let other_ctx = sk.sign(context::BLOCK_SIGNATURE, VECTOR_NETWORK, b"zero");
    for (name, sig) in [
        ("assinatura-alterada", bad),
        ("assinatura-de-outra-rede", other_net),
        ("assinatura-de-outro-contexto", other_ctx),
    ] {
        out.push(
            Case::new(name)
                .hex("chave_publica", pk.as_bytes())
                .set("contexto", context::TX_SIGNATURE)
                .set("rede", VECTOR_NETWORK)
                .hex("mensagem", b"zero")
                .hex("assinatura", &sig.0)
                .set("valida", "nao"),
        );
    }
    out
}

// ----------------------------------------------------------------- hash

fn hashes() -> Vec<Case> {
    let contexts = [
        context::TX_ID,
        context::BLOCK_ID,
        context::STATE_ROOT,
        context::GENESIS,
        rz_core::release::RELEASE_ID,
        rz_core::content::CONTENT_ID,
    ];
    let long: Vec<u8> = (0..2_000u32).map(|i| (i % 251) as u8).collect();
    let inputs: [&[u8]; 3] = [b"", b"zero", &long];
    let mut out = Vec::new();
    for (i, ctx) in contexts.iter().enumerate() {
        for (j, input) in inputs.iter().enumerate() {
            out.push(
                Case::new(format!("hash-{i}-{j}"))
                    .set("contexto", ctx)
                    .hex("entrada", input)
                    .hex("saida", &hash(ctx, input).0),
            );
        }
    }
    out.push(
        Case::new("sha256d")
            .set("contexto", "sha256d")
            .hex("entrada", b"zero")
            .hex("saida", &sha256d(b"zero")),
    );
    out
}

// ----------------------------------------------------------------- ZERO

fn zero() -> Vec<Case> {
    let mut out = Vec::new();
    for (i, t) in [
        "0",
        "1",
        "1.5",
        "0.00000001",
        "21000000",
        "184467440737.09551615",
        "",
        "1.000000001",
        "-1",
        "1,5",
        "184467440737.09551616",
        " 1",
        ".5",
        "1.",
    ]
    .iter()
    .enumerate()
    {
        let c = Case::new(format!("ler-{i}"))
            .set("operacao", "ler")
            .set("texto", format!("\"{t}\""));
        out.push(match rz_core::parse_zero(t) {
            Some(u) => c.set("resultado", "ok").set("unidades", u),
            None => c.set("resultado", "rejeitado"),
        });
    }
    for (i, u) in [0u64, 1, 100_000_000, 150_000_000, 123_456_789, u64::MAX]
        .iter()
        .enumerate()
    {
        out.push(
            Case::new(format!("escrever-{i}"))
                .set("operacao", "escrever")
                .set("unidades", u)
                .set("texto", format!("\"{}\"", rz_core::format_zero(*u))),
        );
    }
    out
}

// ------------------------------------------------------- codificação

fn signed(k: &SecretKey, nonce: u64, fee: u64, kind: TxKind, net: &str) -> R<Transaction> {
    TxBody {
        version: TX_VERSION,
        sender: k.public_key(),
        nonce,
        fee,
        kind,
    }
    .sign(k, net)
    .map_err(|e| e.to_string())
}

fn encoding() -> R<Vec<Case>> {
    let tx = signed(
        &key(2),
        0,
        1,
        TxKind::Transfer {
            to: key(3).public_key().address(),
            amount: 5,
        },
        VECTOR_NETWORK,
    )?;
    let bytes = tx.to_canonical_bytes();
    // Posição da tag de TxKind: tag da transação, versão, remetente, nonce, taxa.
    let kind_at = 1 + 2 + 32 + 8 + 8;
    let mut out = vec![Case::new("transacao-valida")
        .set("tipo", "transacao")
        .hex("bytes", &bytes)
        .set("resultado", "ok")
        .hex("id", &tx.id().0 .0)];
    let mut push_bad = |name: &str, tipo: &str, b: Vec<u8>, motivo: &str| {
        out.push(
            Case::new(name)
                .set("tipo", tipo)
                .hex("bytes", &b)
                .set("resultado", "rejeitado")
                .set("motivo", motivo),
        );
    };
    let mut trailing = bytes.clone();
    trailing.push(0);
    push_bad("byte-sobrando", "transacao", trailing, "bytes após o fim");
    push_bad(
        "truncada",
        "transacao",
        bytes[..bytes.len() - 1].to_vec(),
        "fim inesperado",
    );
    push_bad("vazia", "transacao", vec![], "fim inesperado");
    let mut tag = bytes.clone();
    tag[0] = 0x07;
    push_bad(
        "tag-de-transacao-desconhecida",
        "transacao",
        tag,
        "tag inválida",
    );
    let mut kind = bytes.clone();
    kind[kind_at] = 0xfe;
    push_bad(
        "tag-de-operacao-desconhecida",
        "transacao",
        kind,
        "tag inválida",
    );
    // Option com tag 2: Propose com release_id = tag inválida.
    let prop = signed(
        &key(2),
        0,
        1,
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([1; 32]),
            params: vec![],
            release_id: None,
            deposit: 1_000,
        },
        VECTOR_NETWORK,
    )?;
    let pb = prop.to_canonical_bytes();
    // tag(1) body: versão, remetente, nonce, taxa, kind(0x12), categoria,
    // content_hash, params (u32 n = 0), release_id (option).
    let opt_at = kind_at + 1 + 1 + 32 + 4;
    let mut bad_opt = pb.clone();
    if bad_opt.get(opt_at) == Some(&0) {
        bad_opt[opt_at] = 2;
    }
    out.push(
        Case::new("proposta-valida")
            .set("tipo", "transacao")
            .hex("bytes", &pb)
            .set("resultado", "ok")
            .hex("id", &prop.id().0 .0),
    );
    out.push(
        Case::new("option-com-tag-2")
            .set("tipo", "transacao")
            .hex("bytes", &bad_opt)
            .set("resultado", "rejeitado")
            .set("motivo", "tag de option inválida"),
    );
    let g = genesis();
    let gb = g.to_canonical_bytes();
    out.push(
        Case::new("genesis-valido")
            .set("tipo", "genesis")
            .hex("bytes", &gb)
            .set("resultado", "ok")
            .hex("id", &g.hash().0),
    );
    let mut gt = gb.clone();
    gt.push(1);
    out.push(
        Case::new("genesis-com-byte-sobrando")
            .set("tipo", "genesis")
            .hex("bytes", &gt)
            .set("resultado", "rejeitado")
            .set("motivo", "bytes após o fim"),
    );
    let r = sample_release();
    let mut unsorted = r.clone();
    unsorted.artifacts.reverse();
    out.push(
        Case::new("manifesto-valido")
            .set("tipo", "versao")
            .hex("bytes", &r.to_canonical_bytes())
            .set("resultado", "ok")
            .hex("id", &r.id().0),
    );
    out.push(
        Case::new("manifesto-com-artefatos-fora-de-ordem")
            .set("tipo", "versao")
            .hex("bytes", &unsorted.to_canonical_bytes())
            .set("resultado", "rejeitado")
            .set("motivo", "artefatos fora de ordem"),
    );
    Ok(out)
}

// ----------------------------------------------------------- transações

fn transactions() -> R<Vec<Case>> {
    let k = key(2);
    let to = key(3).public_key().address();
    let mut out = Vec::new();
    let mut add = |name: &str, tx: &Transaction, net: &str, min_fee: u64, ok: bool| {
        out.push(
            Case::new(name)
                .hex("bytes", &tx.to_canonical_bytes())
                .hex("id", &tx.id().0 .0)
                .set("rede", net)
                .set("taxa_minima", min_fee)
                .set("resultado", if ok { "ok" } else { "rejeitado" }),
        );
    };
    let t = signed(&k, 0, 1, TxKind::Transfer { to, amount: 5 }, VECTOR_NETWORK)?;
    add("transferencia", &t, VECTOR_NETWORK, 1, true);
    add(
        "transferencia-outra-rede",
        &t,
        "rede-zero-testnet-1",
        1,
        false,
    );
    add("taxa-abaixo-do-minimo", &t, VECTOR_NETWORK, 2, false);
    let zero = signed(&k, 0, 1, TxKind::Transfer { to, amount: 0 }, VECTOR_NETWORK)?;
    add("valor-zero", &zero, VECTOR_NETWORK, 1, false);
    let mut other_version = TxBody {
        version: TX_VERSION + 1,
        sender: k.public_key(),
        nonce: 0,
        fee: 1,
        kind: TxKind::Transfer { to, amount: 5 },
    }
    .sign(&k, VECTOR_NETWORK)
    .map_err(|e| e.to_string())?;
    add(
        "versao-desconhecida",
        &other_version,
        VECTOR_NETWORK,
        1,
        false,
    );
    if let Transaction::Account(a) = &mut other_version {
        a.body.version = TX_VERSION;
    }
    add(
        "assinatura-de-outro-corpo",
        &other_version,
        VECTOR_NETWORK,
        1,
        false,
    );
    let lock = signed(
        &k,
        3,
        1,
        TxKind::LockStake {
            amount: 1_000,
            unlock_height: 50,
        },
        VECTOR_NETWORK,
    )?;
    add("bloqueio", &lock, VECTOR_NETWORK, 1, true);
    let name = signed(
        &k,
        4,
        1,
        TxKind::RegisterName {
            name: "Nome Invalido".into(),
            kind: NameKind::App,
            target: Hash32([1; 32]),
        },
        VECTOR_NETWORK,
    )?;
    add("nome-invalido", &name, VECTOR_NETWORK, 1, false);
    let rel = signed(
        &k,
        5,
        1,
        TxKind::PublishRelease {
            release: sample_release(),
        },
        VECTOR_NETWORK,
    )?;
    add("publicacao-de-versao", &rel, VECTOR_NETWORK, 1, true);
    let new = key(9).public_key();
    let proof = key(9).sign(
        rz_core::rotation::KEY_ROTATION,
        VECTOR_NETWORK,
        &rz_core::rotation::rotation_payload(&k.public_key().address(), &new),
    );
    let rot = signed(
        &k,
        6,
        1,
        TxKind::RotateKey {
            new_key: new,
            proof,
        },
        VECTOR_NETWORK,
    )?;
    add("rotacao-de-chave", &rot, VECTOR_NETWORK, 1, true);
    let to_self = signed(
        &k,
        6,
        1,
        TxKind::RotateKey {
            new_key: k.public_key(),
            proof,
        },
        VECTOR_NETWORK,
    )?;
    add(
        "rotacao-para-a-mesma-chave",
        &to_self,
        VECTOR_NETWORK,
        1,
        false,
    );
    for (name, delay, ok) in [
        (
            "recuperacao-prazo-minimo",
            rz_core::rotation::MIN_RECOVERY_DELAY,
            true,
        ),
        (
            "recuperacao-prazo-curto",
            rz_core::rotation::MIN_RECOVERY_DELAY - 1,
            false,
        ),
        (
            "recuperacao-prazo-longo",
            rz_core::rotation::MAX_RECOVERY_DELAY + 1,
            false,
        ),
    ] {
        let t = signed(
            &k,
            7,
            1,
            TxKind::SetRecovery {
                recovery: Some(rz_core::rotation::Recovery::single(new, delay)),
            },
            VECTOR_NETWORK,
        )?;
        add(name, &t, VECTOR_NETWORK, 1, ok);
    }
    Ok(out)
}

// ----------------------------------------------------------------- cadeia

pub(crate) fn rules() -> PowRules {
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

const ORIGIN: &str = "bitcoin-regtest";

/// Validador único, conta rica, conta pobre, compradora e relayer.
fn validator() -> SecretKey {
    key(1)
}
fn rich() -> SecretKey {
    key(2)
}
fn poor() -> SecretKey {
    key(3)
}
fn carol() -> SecretKey {
    key(4)
}
fn alice_shielded() -> ShieldedSecret {
    ShieldedSecret::from_seed(&[10; 32])
}
fn bob_shielded() -> ShieldedSecret {
    ShieldedSecret::from_seed(&[11; 32])
}
fn controllers() -> Vec<SecretKey> {
    (40..43).map(key).collect()
}
fn ext_asset() -> AssetId {
    AssetId::external("testnet-externa", "ATV")
}

pub(crate) fn genesis() -> Genesis {
    let mut allocations = vec![
        Allocation {
            address: rich().public_key().address(),
            amount: 1_000 * rz_core::UNITS_PER_ZERO,
        },
        Allocation {
            address: poor().public_key().address(),
            amount: 500,
        },
        Allocation {
            address: carol().public_key().address(),
            amount: 100_000,
        },
    ];
    allocations.sort_by_key(|a| a.address);
    let mut asset_alloc = vec![Allocation {
        address: rich().public_key().address(),
        amount: 10_000,
    }];
    asset_alloc.sort_by_key(|a| a.address);
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: VECTOR_NETWORK.into(),
        consensus: rz_core::ConsensusParams::fast(200),
        min_fee: 1,
        max_block_txs: 100,
        validators: vec![GenesisValidator::new(validator().public_key(), 10_000)],
        allocations,
        governance: GovernanceParams {
            deposit: 1_000,
            analysis_blocks: 2,
            voting_blocks: 3,
            ordinary_delay_blocks: 2,
            constitutional_delay_blocks: 4,
            lock_min_blocks: 5,
            lock_max_blocks: 105,
            contribution_half_life_blocks: 16,
        },
        assets: vec![GenesisAsset {
            network: "testnet-externa".into(),
            asset_ref: "ATV".into(),
            decimals: 8,
            allocations: asset_alloc,
        }],
        bridges: vec![ChainConfig {
            name: ORIGIN.into(),
            asset_ref: "BTC".into(),
            decimals: 8,
            rules: rules(),
            checkpoint: checkpoint().serialize(),
            checkpoint_height: 0,
            confirmations: 2,
        }],
        monetary: rz_core::monetary::MonetaryParams::TESTNET,
    }
}

struct Chain {
    g: Genesis,
    state: State,
    parent: BlockId,
    height: u64,
    pending: HashMap<Address, u64>,
    cases: Vec<Case>,
}

impl Chain {
    fn new() -> R<Self> {
        let g = genesis();
        let state = State::from_genesis(&g).map_err(|e| e.to_string())?;
        let cases = vec![Case::new("genesis")
            .hex("bytes", &g.to_canonical_bytes())
            .hex("id", &g.hash().0)
            .hex("raiz_do_estado", &state.root().0)];
        Ok(Self {
            parent: BlockId(g.hash()),
            g,
            state,
            height: 0,
            pending: HashMap::new(),
            cases,
        })
    }

    fn fee(&self) -> u64 {
        self.state.params().min_fee
    }

    fn tx_fee(&mut self, k: &SecretKey, kind: TxKind, fee: u64) -> R<Transaction> {
        let a = k.public_key().address();
        let off = self.pending.entry(a).or_default();
        let nonce = self.state.account(&a).nonce + *off;
        *off += 1;
        signed(k, nonce, fee, kind, &self.g.network_id)
    }

    fn tx(&mut self, k: &SecretKey, kind: TxKind) -> R<Transaction> {
        let fee = self.fee();
        self.tx_fee(k, kind, fee)
    }

    fn block(&mut self, label: &str, txs: Vec<Transaction>) -> R<()> {
        self.pending.clear();
        let (b, s) = Block::build(
            &self.g,
            self.parent,
            self.height,
            &self.state,
            0,
            txs,
            &validator(),
        )
        .map_err(|e| format!("{label}: {e}"))?;
        self.cases.push(
            Case::new(format!("bloco-{}-{label}", b.header.height))
                .hex("bytes", &b.to_canonical_bytes())
                .set("resultado", "ok")
                .hex("id", &b.id().0 .0)
                .hex("raiz_do_estado", &s.root().0),
        );
        self.parent = b.id();
        self.height += 1;
        self.state = s;
        Ok(())
    }

    fn advance_to(&mut self, height: u64) -> R<()> {
        while self.height < height {
            self.block("vazio", vec![])?;
        }
        Ok(())
    }

    /// Bloco inválido sobre a ponta atual (não avança a cadeia). `motivo`
    /// precisa aparecer no erro da implementação de referência, para que o
    /// vetor teste a regra pretendida e não outra.
    fn reject(&mut self, label: &str, b: &Block, motivo: &str) -> R<()> {
        self.pending.clear();
        let err = match rz_core::apply_block(&self.g, self.parent, self.height, &self.state, b) {
            Ok(_) => return Err(format!("{label}: bloco inválido aceito")),
            Err(e) => e.to_string(),
        };
        if !err.contains(motivo) {
            return Err(format!(
                "{label}: rejeitado por '{err}', esperado '{motivo}'"
            ));
        }
        self.cases.push(
            Case::new(format!("invalido-{label}"))
                .hex("bytes", &b.to_canonical_bytes())
                .set("resultado", "rejeitado")
                .set("motivo", format!("\"{err}\"")),
        );
        Ok(())
    }

    /// Assina um bloco montado à mão.
    fn forge(&self, txs: Vec<Transaction>, state_root: Hash32) -> Block {
        let header = rz_core::BlockHeader {
            version: rz_core::block::BLOCK_VERSION,
            height: self.height + 1,
            parent: self.parent,
            round: 0,
            proposer: validator().public_key(),
            tx_root: tx_root(&txs),
            state_root,
        };
        let signature = validator().sign(
            context::BLOCK_SIGNATURE,
            &self.g.network_id,
            &header.to_canonical_bytes(),
        );
        Block {
            header,
            txs,
            signature,
        }
    }

    fn notes(&self, who: &ShieldedSecret) -> Vec<SpendableNote> {
        self.state
            .outputs()
            .iter()
            .enumerate()
            .filter_map(|(i, o)| {
                scan_output(who, o).map(|note| SpendableNote {
                    global_index: i as u64,
                    data: o.clone(),
                    note,
                })
            })
            .filter(|n| !self.state.is_spent(&n.key_image()))
            .collect()
    }

    fn vote_through(&mut self, proposal: Hash32) -> R<()> {
        let p = self
            .state
            .proposal(&proposal)
            .ok_or("proposta ausente")?
            .clone();
        self.advance_to(p.voting_start - 1)?;
        let a = self.tx(
            &rich(),
            TxKind::Vote {
                proposal,
                choice: Choice::Yes,
            },
        )?;
        let b = self.tx(
            &validator(),
            TxKind::Vote {
                proposal,
                choice: Choice::Yes,
            },
        )?;
        self.block("votos", vec![a, b])?;
        self.advance_to(p.activation_height)
    }
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

fn chain() -> R<Vec<Case>> {
    let mut c = Chain::new()?;
    let net = c.g.network_id.clone();

    // 1. Transferências.
    let t1 = c.tx(
        &rich(),
        TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1_000,
        },
    )?;
    let t2 = c.tx(
        &rich(),
        TxKind::Transfer {
            to: carol().public_key().address(),
            amount: 2_000,
        },
    )?;
    c.block("transferencias", vec![t1, t2])?;

    // Inválidos sobre a ponta: nonce repetido, saldo, raiz, assinatura,
    // pai, altura e raiz de transações.
    let stale = signed(
        &rich(),
        0,
        1,
        TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1,
        },
        &net,
    )?;
    let b = c.forge(vec![stale], Hash32::ZERO);
    c.reject("nonce-repetido", &b, "nonce inválido")?;
    let broke = c.tx(
        &poor(),
        TxKind::Transfer {
            to: rich().public_key().address(),
            amount: 1_000_000,
        },
    )?;
    let b = c.forge(vec![broke], Hash32::ZERO);
    c.reject("saldo-insuficiente", &b, "saldo insuficiente")?;
    let (good, _) = Block::build(&c.g, c.parent, c.height, &c.state, 0, vec![], &validator())
        .map_err(|e| e.to_string())?;
    let mut wrong_root = good.header.state_root;
    wrong_root.0[0] ^= 1;
    let b = c.forge(vec![], wrong_root);
    c.reject("raiz-de-estado", &b, "state_root não confere")?;
    let mut bad_sig = good.clone();
    bad_sig.signature.0[5] ^= 1;
    c.reject(
        "assinatura-do-bloco",
        &bad_sig,
        "assinatura do bloco inválida",
    )?;
    let mut b = c.forge(vec![], good.header.state_root);
    b.header.parent = BlockId(Hash32([9; 32]));
    let b = resign(&c, b);
    c.reject("pai-errado", &b, "Previous Block ID incorreto")?;
    let mut b = c.forge(vec![], good.header.state_root);
    b.header.height += 1;
    let b = resign(&c, b);
    c.reject("altura-errada", &b, "altura inconsistente")?;
    let mut b = c.forge(vec![], good.header.state_root);
    b.header.tx_root = Hash32([3; 32]);
    let b = resign(&c, b);
    c.reject("raiz-de-transacoes", &b, "tx_root não confere")?;
    let mut b = c.forge(vec![], good.header.state_root);
    b.header.proposer = rich().public_key();
    c.reject(
        "produtor-diferente-do-assinante",
        &b,
        "assinatura do bloco inválida",
    )?;

    // 2. Blindagens (formam anéis) e transferência privada.
    let decoy = ShieldedSecret::from_seed(&[99; 32]).address();
    let mut shields = Vec::new();
    for i in 0..12u64 {
        let (to, amount) = if i == 5 {
            (alice_shielded().address(), 1_000)
        } else {
            (decoy, 10 + i)
        };
        let a = rich().public_key().address();
        let off = c.pending.entry(a).or_default();
        let nonce = c.state.account(&a).nonce + *off;
        *off += 1;
        shields
            .push(build_shield(&rich(), &net, nonce, 1, &to, amount).map_err(|e| e.to_string())?);
    }
    c.block("blindagens", shields)?;
    let notes = c.notes(&alice_shielded());
    let ptx = Transaction::Private(
        build_private_tx(
            &net,
            &notes,
            c.state.outputs(),
            &[(bob_shielded().address(), 400)],
            &alice_shielded().address(),
            None,
            5,
        )
        .map_err(|e| format!("{e:?}"))?,
    );
    c.block("transferencia-privada", vec![ptx.clone()])?;
    let b = c.forge(vec![ptx], Hash32::ZERO);
    c.reject("gasto-duplo-privado", &b, "nota já gasta")?;

    // 3. Governança e Comunidade reconhecida.
    let lock = c.tx(
        &rich(),
        TxKind::LockStake {
            amount: 100_000,
            unlock_height: c.height + 1 + 100,
        },
    )?;
    let rule = DecisionRule {
        keys: controllers().iter().map(SecretKey::public_key).collect(),
        threshold: 2,
    };
    let declare = c.tx(
        &rich(),
        TxKind::DeclareCommunity {
            name: "conformidade".into(),
            manifest_hash: Hash32([0x3a; 32]),
            rule,
        },
    )?;
    let community = declare.id().0;
    c.block("bloqueio-e-declaracao", vec![lock, declare])?;
    let deposit = c.state.params().governance.deposit;
    let prop = c.tx(
        &rich(),
        TxKind::Propose {
            category: Category::Community,
            content_hash: community,
            params: vec![],
            release_id: None,
            deposit,
        },
    )?;
    let pid = prop.id().0;
    let fee_change = c.tx(
        &rich(),
        TxKind::Propose {
            category: Category::Ordinary,
            content_hash: Hash32([7; 32]),
            params: vec![ParamChange::MaxBlockTxs(90)],
            release_id: None,
            deposit,
        },
    )?;
    let fid = fee_change.id().0;
    c.block("propostas", vec![prop, fee_change])?;
    {
        let p = c.state.proposal(&pid).ok_or("proposta")?.clone();
        c.advance_to(p.voting_start - 1)?;
        let mut votes = Vec::new();
        for id in [pid, fid] {
            votes.push(c.tx(
                &rich(),
                TxKind::Vote {
                    proposal: id,
                    choice: Choice::Yes,
                },
            )?);
            votes.push(c.tx(
                &validator(),
                TxKind::Vote {
                    proposal: id,
                    choice: Choice::Yes,
                },
            )?);
        }
        c.block("votos", votes)?;
        let f = c.state.proposal(&fid).ok_or("proposta")?.clone();
        c.advance_to(p.activation_height.max(f.activation_height))?;
    }

    // 4. Nome, módulo do Runtime vinculado e chamado.
    let name = c.tx(
        &rich(),
        TxKind::RegisterName {
            name: "conformidade".into(),
            kind: NameKind::App,
            target: Hash32([0xab; 32]),
        },
    )?;
    let code = modulo()?;
    let module = rz_core::runtime::module_id(&code);
    let publish = c.tx(&rich(), TxKind::PublishModule { code })?;
    c.block("nome-e-modulo", vec![name, publish])?;
    let payload = rz_core::runtime::bind_payload(&community, &Some(module), 0);
    let approvals = controllers()[..2]
        .iter()
        .map(|k| approve(k, &net, &payload))
        .collect();
    let bind = c.tx(
        &poor(),
        TxKind::BindModule {
            community,
            module: Some(module),
            approvals,
        },
    )?;
    c.block("vinculacao", vec![bind])?;
    let fuel = 500_000;
    let call_fee = c.fee()
        + c.state
            .params()
            .runtime
            .fuel_fee(fuel)
            .ok_or("combustível")?;
    let call = c.tx_fee(
        &rich(),
        TxKind::CallModule {
            community,
            method: "incrementar".into(),
            args: vec![],
            fuel,
        },
        call_fee,
    )?;
    let fail = c.tx_fee(
        &carol(),
        TxKind::CallModule {
            community,
            method: "girar".into(),
            args: vec![],
            fuel: 70_000,
        },
        c.fee()
            + c.state
                .params()
                .runtime
                .fuel_fee(70_000)
                .ok_or("combustível")?,
    )?;
    c.block("chamadas", vec![call, fail])?;

    // 5. Grande Mercado: venda e compra cruzam no leilão do bloco.
    let expires = c.height + 20;
    let sell = c.tx(
        &rich(),
        TxKind::PlaceOrder {
            asset: ext_asset(),
            side: Side::Sell,
            amount: 1_000,
            price: PRICE_SCALE as u64,
            expires_at: expires,
        },
    )?;
    let buy = c.tx(
        &carol(),
        TxKind::PlaceOrder {
            asset: ext_asset(),
            side: Side::Buy,
            amount: 600,
            price: 2 * PRICE_SCALE as u64,
            expires_at: expires,
        },
    )?;
    let pool = c.tx(
        &rich(),
        TxKind::PoolDeposit {
            asset: AssetId::ZERO,
            amount: 777,
        },
    )?;
    c.block("mercado-e-pool", vec![sell, buy, pool])?;

    // 6. Ponte: cabeçalhos, queima comprovada e troca atômica.
    let other = origin_tx(&[(1_000, vec![0x51])], 1);
    let burn = origin_tx(
        &[(
            50_000_000,
            burn_script(&net, &carol().public_key().address()),
        )],
        2,
    );
    let txids = [Hash32(sha256d(&other)), Hash32(sha256d(&burn))];
    let mut pair = [0u8; 64];
    pair[..32].copy_from_slice(&txids[0].0);
    pair[32..].copy_from_slice(&txids[1].0);
    let cp = checkpoint();
    let h1 = mine(cp.hash(), Hash32(sha256d(&pair)), cp.time + 600);
    let h2 = mine(h1.hash(), Hash32([3; 32]), cp.time + 1200);
    let headers = c.tx(
        &poor(),
        TxKind::BridgeHeaders {
            chain: ORIGIN.into(),
            headers: vec![h1.serialize(), h2.serialize()],
        },
    )?;
    c.block("ponte-cabecalhos", vec![headers])?;
    let deposit_tx = c.tx(
        &poor(),
        TxKind::BridgeDeposit {
            chain: ORIGIN.into(),
            block: h1.hash(),
            tx: burn.clone(),
            path: vec![txids[0]],
            index: 1,
        },
    )?;
    let secret = [0x3c; 32].to_vec();
    let lock_htlc = c.tx(
        &rich(),
        TxKind::HtlcLock {
            asset: AssetId::ZERO,
            amount: 3_000,
            recipient: poor().public_key().address(),
            hashlock: Hash32(sha256(&secret)),
            timeout: c.height + 50,
        },
    )?;
    let htlc_id = lock_htlc.id().0;
    c.block("ponte-deposito-e-htlc", vec![deposit_tx.clone(), lock_htlc])?;
    let again = signed(
        &poor(),
        c.state.account(&poor().public_key().address()).nonce,
        c.fee(),
        match deposit_tx {
            Transaction::Account(a) => a.body.kind,
            Transaction::Private(_) => return Err("depósito".into()),
        },
        &net,
    )?;
    let b = c.forge(vec![again], Hash32::ZERO);
    c.reject("queima-reaproveitada", &b, "queima já usada")?;
    let claim = c.tx(
        &poor(),
        TxKind::HtlcClaim {
            id: htlc_id,
            preimage: secret,
        },
    )?;
    c.block("htlc-resgate", vec![claim])?;

    // 7. Defesa: vigilância atestada pelo validador.
    let seq = c.state.defense().seq;
    let ev = Hash32([0xe1; 32]);
    let p = payload::transition(seq, DefenseMode::Vigilance, &ev);
    let att = attest(&validator(), &net, &p);
    let vig = c.tx(
        &poor(),
        TxKind::DefenseTransition {
            to: DefenseMode::Vigilance,
            evidence: ev,
            seq,
            attestations: vec![att],
        },
    )?;
    c.block("defesa-vigilancia", vec![vig])?;

    // 8. Versão publicada e adotada por proposta constitucional.
    let release = sample_release();
    let rid = release.id();
    let pubr = c.tx(&rich(), TxKind::PublishRelease { release })?;
    c.block("versao-publicada", vec![pubr])?;
    let deposit = c.state.params().governance.deposit;
    let adopt = c.tx(
        &rich(),
        TxKind::Propose {
            category: Category::Constitutional,
            content_hash: Hash32([8; 32]),
            params: vec![],
            release_id: Some(rid),
            deposit,
        },
    )?;
    let aid = adopt.id().0;
    c.block("proposta-de-versao", vec![adopt])?;
    c.vote_through(aid)?;
    // 9. Rotação de chave: a conta da Carol passa para uma chave nova; a
    // antiga deixa de assinar. Recuperação registrada para a conta rica.
    let carol_acc = carol().public_key().address();
    let new_carol = key(70);
    let proof = new_carol.sign(
        rz_core::rotation::KEY_ROTATION,
        &net,
        &rz_core::rotation::rotation_payload(&carol_acc, &new_carol.public_key()),
    );
    let rot = c.tx(
        &carol(),
        TxKind::RotateKey {
            new_key: new_carol.public_key(),
            proof,
        },
    )?;
    let rec = c.tx(
        &rich(),
        TxKind::SetRecovery {
            recovery: Some(rz_core::rotation::Recovery {
                guardians: vec![
                    key(71).public_key(),
                    key(72).public_key(),
                    key(73).public_key(),
                ],
                threshold: 2,
                delay: rz_core::rotation::MIN_RECOVERY_DELAY,
            }),
        },
    )?;
    c.block("rotacao-e-recuperacao", vec![rot, rec])?;
    let old_key_tx = c.tx(
        &carol(),
        TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1,
        },
    )?;
    let b = c.forge(vec![old_key_tx], Hash32::ZERO);
    c.reject("chave-substituida", &b, "chave substituída")?;
    let new_tx = signed(
        &new_carol,
        c.state.account(&carol_acc).nonce,
        c.fee(),
        TxKind::Transfer {
            to: poor().public_key().address(),
            amount: 1,
        },
        &net,
    )?;
    c.block("nova-chave-assina", vec![new_tx])?;

    // O cenário precisa ter exercitado cada parte; senão o vetor engana.
    let st = &c.state;
    let carol_addr = carol().public_key().address();
    let checks = [
        (st.releases().current == Some(rid), "versão adotada"),
        (
            st.communities()
                .communities
                .get(&community)
                .map(|x| x.status)
                == Some(rz_core::community::CommunityStatus::Recognized),
            "Comunidade reconhecida",
        ),
        (
            st.params().max_block_txs == 90,
            "parâmetro ordinário ativado",
        ),
        (
            st.runtime()
                .storage
                .get(&community)
                .and_then(|m| m.get(b"n".as_slice()))
                == Some(&1u64.to_le_bytes().to_vec()),
            "contador incrementado uma vez",
        ),
        (
            st.market().balance(&carol_addr, &ext_asset()) == 600,
            "negócio no leilão",
        ),
        (
            st.market()
                .balance(&carol_addr, &AssetId::external(ORIGIN, "BTC"))
                == 50_000_000,
            "cunhagem pela ponte",
        ),
        (st.bridge().htlcs.is_empty(), "troca atômica liquidada"),
        (
            st.defense().mode == DefenseMode::Vigilance,
            "modo de vigilância",
        ),
        (
            !c.notes(&bob_shielded()).is_empty(),
            "nota privada recebida",
        ),
    ];
    for (ok, what) in checks {
        if !ok {
            return Err(format!("cenário não exercitou: {what}"));
        }
    }
    Ok(c.cases)
}

fn resign(c: &Chain, mut b: Block) -> Block {
    b.signature = validator().sign(
        context::BLOCK_SIGNATURE,
        &c.g.network_id,
        &b.header.to_canonical_bytes(),
    );
    b
}

/// Módulo de teste do Runtime (o mesmo dos testes de `rz-core`).
const MODULO: &str = r#"
(module
  (import "zero" "storage_get" (func $get (param i32 i32 i32 i32) (result i32)))
  (import "zero" "storage_set" (func $set (param i32 i32 i32 i32)))
  (import "zero" "output" (func $out (param i32 i32)))
  (import "zero" "abort" (func $abort (param i32 i32)))
  (import "zero" "caller" (func $caller (param i32)))
  (import "zero" "input_len" (func $ilen (result i32)))
  (import "zero" "input_read" (func $iread (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "n")
  (data (i32.const 16) "falhou de proposito")
  (func (export "incrementar")
    (if (i32.eq (call $get (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8)) (i32.const -1))
      (then (i64.store (i32.const 8) (i64.const 0))))
    (i64.store (i32.const 8) (i64.add (i64.load (i32.const 8)) (i64.const 1)))
    (call $set (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8))
    (call $out (i32.const 8) (i32.const 8)))
  (func (export "ler")
    (drop (call $get (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8)))
    (call $out (i32.const 8) (i32.const 8)))
  (func (export "incrementar_e_abortar")
    (call $set (i32.const 0) (i32.const 1) (i32.const 16) (i32.const 8))
    (call $abort (i32.const 16) (i32.const 19)))
  (func (export "girar") (loop $l (br $l)))
  (func (export "quem") (call $caller (i32.const 64)) (call $out (i32.const 64) (i32.const 32)))
  (func (export "eco") (call $iread (i32.const 128)) (call $out (i32.const 128) (call $ilen)))
  (func (export "encher") (local $i i32)
    (loop $l
      (i32.store16 (i32.const 1000) (local.get $i))
      (call $set (i32.const 1000) (i32.const 2) (i32.const 2048) (i32.const 4096))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 300)))))
  (func (export "fora") (call $out (i32.const 65530) (i32.const 100)))
  (func (export "crescer") (result) (drop (memory.grow (i32.const 1000))))
)"#;

fn modulo() -> R<Vec<u8>> {
    wat::parse_str(MODULO).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ P2P

fn p2p() -> Vec<Case> {
    let pk = key(1).public_key();
    let messages = vec![
        ("ping", Message::Ping(7)),
        ("get-releases", Message::GetReleases),
        (
            "releases",
            Message::Releases {
                height: 9,
                current: Some(sample_release().id()),
                releases: vec![rz_core::release::ReleaseRecord {
                    release: sample_release(),
                    published_at: 2,
                    adopted_at: Some(8),
                }],
            },
        ),
        ("get-reputation", Message::GetReputation(pk)),
        ("get-htlc", Message::GetHtlc(Hash32([5; 32]))),
        ("get-bridge", Message::GetBridge(ORIGIN.into())),
    ];
    let mut out = Vec::new();
    for (name, m) in messages {
        out.push(
            Case::new(name)
                .hex("bytes", &m.to_canonical_bytes())
                .set("resultado", "ok")
                .set("nome", m.kind()),
        );
    }
    for (name, b) in [
        ("tag-desconhecida", vec![0xffu8]),
        ("vazia", vec![]),
        (
            "ping-truncado",
            vec![Message::Ping(7).to_canonical_bytes()[0], 1],
        ),
    ] {
        out.push(
            Case::new(name)
                .hex("bytes", &b)
                .set("resultado", "rejeitado"),
        );
    }
    out
}

// ------------------------------------------------------------ governança

fn governance() -> Vec<Case> {
    let mut out = Vec::new();
    for (i, (v, dt, hl)) in [
        (1_000_000u64, 0u64, 16u64),
        (1_000_000, 16, 16),
        (1_000_000, 8, 16),
        (1_000_000, 1, 16),
        (51_000_000, 25, 16),
        (u64::MAX, 3, 7),
        (123_456_789, 1_000, 16),
        (5, 0, 1),
    ]
    .iter()
    .enumerate()
    {
        out.push(
            Case::new(format!("decaimento-{i}"))
                .set("operacao", "decaimento")
                .set("valor", v)
                .set("idade", dt)
                .set("meia_vida", hl)
                .set("resultado", rz_core::governance::decay(*v, *dt, *hl)),
        );
    }
    out
}

// --------------------------------------------------------------- Runtime

fn runtime() -> R<Vec<Case>> {
    use rz_core::runtime::{execute, module_id, validate_module, Call};
    let mut out = Vec::new();
    let code = modulo()?;
    let float = wat::parse_str("(module (func (export \"f\") (result f32) f32.const 1.5))")
        .map_err(|e| e.to_string())?;
    let import = wat::parse_str("(module (import \"env\" \"relogio\" (func (result i64))))")
        .map_err(|e| e.to_string())?;
    for (name, m, ok) in [
        ("modulo-de-teste", code.clone(), true),
        ("modulo-com-ponto-flutuante", float, false),
        ("modulo-com-importacao-desconhecida", import, false),
        ("modulo-invalido", b"\0asm\x02\0\0\0".to_vec(), false),
    ] {
        out.push(
            Case::new(name)
                .set("operacao", "validar")
                .hex("modulo", &m)
                .hex("id", &module_id(&m).0)
                .set(
                    "resultado",
                    if ok && validate_module(&m).is_ok() {
                        "ok"
                    } else {
                        "rejeitado"
                    },
                ),
        );
    }
    let id = module_id(&code);
    let mut storage = BTreeMap::new();
    storage.insert(b"n".to_vec(), 41u64.to_le_bytes().to_vec());
    for (name, method, input, fuel, base) in [
        (
            "incrementar-vazio",
            "incrementar",
            &b""[..],
            500_000u64,
            None,
        ),
        (
            "incrementar-existente",
            "incrementar",
            b"",
            500_000,
            Some(&storage),
        ),
        ("ler", "ler", b"", 500_000, Some(&storage)),
        ("sem-combustivel", "incrementar", b"", 10, Some(&storage)),
        ("metodo-inexistente", "nao_existe", b"", 500_000, None),
        ("laco-infinito", "girar", b"", 70_000, None),
        (
            "abortar-descarta-escritas",
            "incrementar_e_abortar",
            b"",
            500_000,
            None,
        ),
        ("chamador", "quem", b"", 500_000, None),
        ("eco", "eco", b"entrada de teste", 500_000, None),
        ("cota-excedida", "encher", b"", 5_000_000, None),
        ("memoria-fora-dos-limites", "fora", b"", 500_000, None),
        ("crescer-memoria", "crescer", b"", 500_000, None),
    ] {
        let call = Call {
            community: Hash32([1; 32]),
            caller: Address(Hash32([0xca; 32])),
            height: 7,
            method,
            input,
            fuel,
            quota: 4096,
        };
        let o = execute(&id, &code, base, &call);
        let mut c = Case::new(name)
            .set("operacao", "executar")
            .set("modulo", "modulo-de-teste")
            .hex("comunidade", &call.community.0)
            .hex("chamador", &call.caller.0 .0)
            .set("altura", call.height)
            .set("metodo", method)
            .hex("entrada", call.input)
            .set("combustivel", fuel)
            .set("cota", call.quota);
        if let Some(s) = base {
            for (k, v) in s {
                c = c.set(
                    "armazenamento",
                    format!(
                        "{}:{}",
                        rz_crypto::hex::encode(k),
                        rz_crypto::hex::encode(v)
                    ),
                );
            }
        }
        c = c
            .set("resultado", if o.ok { "ok" } else { "falha" })
            .set("combustivel_usado", o.fuel_used)
            .hex("saida", &o.output);
        if !o.ok {
            c = c.set("erro", format!("\"{}\"", o.error));
        }
        for (k, v) in &o.writes {
            c = c.set(
                "escrita",
                format!(
                    "{}:{}",
                    rz_crypto::hex::encode(k),
                    v.as_ref().map_or("-".into(), |v| rz_crypto::hex::encode(v))
                ),
            );
        }
        out.push(c);
    }
    Ok(out)
}

// ------------------------------------------------------------------ ponte

fn bridge_vectors() -> R<Vec<Case>> {
    let mut out = Vec::new();
    for (i, bits) in [
        0x1d00_ffffu32,
        0x207f_ffff,
        0x1b04_04cb,
        0x0300_0001,
        0x0480_0000,
        0xff12_3456,
        0x0000_0000,
        0x0100_3456,
    ]
    .iter()
    .enumerate()
    {
        let c = Case::new(format!("compacto-{i}"))
            .set("operacao", "compacto")
            .set("bits", format!("{bits:08x}"));
        out.push(match bridge::U256::from_compact(*bits) {
            Some(t) => c.set("resultado", "ok").hex("alvo", &t.to_be()),
            None => c.set("resultado", "rejeitado"),
        });
    }
    let cp = checkpoint();
    let mut weak = cp;
    weak.nonce = weak.nonce.wrapping_add(1);
    while check_pow(&weak, &rules()).is_ok() {
        weak.nonce = weak.nonce.wrapping_add(1);
    }
    for (name, h) in [("cabecalho-minerado", cp), ("cabecalho-sem-trabalho", weak)] {
        out.push(
            Case::new(name)
                .set("operacao", "cabecalho")
                .hex("bytes", &h.serialize())
                .set("limite", rz_crypto::hex::encode(&rules().pow_limit.0))
                .hex("hash", &h.hash().0)
                .set(
                    "resultado",
                    if check_pow(&h, &rules()).is_ok() {
                        "ok"
                    } else {
                        "rejeitado"
                    },
                ),
        );
    }
    let addr = key(4).public_key().address();
    out.push(
        Case::new("script-de-queima")
            .set("operacao", "queima")
            .set("rede", VECTOR_NETWORK)
            .hex("destinatario", &addr.0 .0)
            .hex("script", &burn_script(VECTOR_NETWORK, &addr)),
    );
    out.push(
        Case::new("script-de-queima-com-taxa")
            .set("operacao", "queima")
            .set("rede", VECTOR_NETWORK)
            .hex("destinatario", &addr.0 .0)
            .set("taxa", 1_500)
            .hex(
                "script",
                &bridge::burn_script_with_fee(VECTOR_NETWORK, &addr, 1_500),
            ),
    );
    let leaves: Vec<Hash32> = (0..3u8).map(|i| Hash32(sha256d(&[i]))).collect();
    let pair = |a: &Hash32, b: &Hash32| {
        let mut x = [0u8; 64];
        x[..32].copy_from_slice(&a.0);
        x[32..].copy_from_slice(&b.0);
        Hash32(sha256d(&x))
    };
    let n01 = pair(&leaves[0], &leaves[1]);
    let n22 = pair(&leaves[2], &leaves[2]);
    let root = pair(&n01, &n22);
    for (i, (leaf, path, idx)) in [
        (leaves[0], vec![leaves[1], n22], 0u32),
        (leaves[2], vec![leaves[2], n01], 2),
    ]
    .into_iter()
    .enumerate()
    {
        let got = bridge::merkle_root(&leaf, &path, idx).ok_or("merkle")?;
        if got != root {
            return Err("raiz de merkle divergente no gerador".into());
        }
        let mut c = Case::new(format!("merkle-{i}"))
            .set("operacao", "merkle")
            .hex("folha", &leaf.0)
            .set("indice", idx);
        for p in &path {
            c = c.hex("caminho", &p.0);
        }
        out.push(c.hex("raiz", &got.0));
    }
    Ok(out)
}

// ---------------------------------------------------------------- versões

pub(crate) fn sample_release() -> Release {
    Release {
        version: "0.2.0".into(),
        protocol_version: PROTOCOL_VERSION,
        p2p_version: rz_p2p::P2P_VERSION,
        compatibility: Compatibility::Incompatible,
        previous: None,
        specification: Hash32([1; 32]),
        implementation: Hash32([2; 32]),
        tests: Hash32([3; 32]),
        artifacts: vec![
            Artifact {
                name: "rede-zero-node-linux-x86_64".into(),
                hash: file_hash(b"binario do node"),
            },
            Artifact {
                name: "zero-wallet-linux-x86_64".into(),
                hash: file_hash(b"binario da wallet"),
            },
        ],
        notes: Hash32::ZERO,
    }
}

fn releases() -> Vec<Case> {
    let r = sample_release();
    let mut files = BTreeMap::new();
    files.insert("spec/A.md".to_string(), b"regra A".to_vec());
    files.insert("spec/sub/B.md".to_string(), b"regra B\n".to_vec());
    files.insert("Cargo.toml".to_string(), Vec::new());
    let mut tree = Case::new("arvore").set("operacao", "arvore");
    for (p, b) in &files {
        tree = tree.set("arquivo", format!("{p}:{}", rz_crypto::hex::encode(b)));
    }
    vec![
        Case::new("manifesto")
            .set("operacao", "manifesto")
            .hex("bytes", &r.to_canonical_bytes())
            .hex("id", &r.id().0),
        Case::new("arquivo")
            .set("operacao", "arquivo")
            .hex("bytes", b"binario do node")
            .hex("hash", &file_hash(b"binario do node").0),
        tree.hex("hash", &tree_hash(&files).0),
    ]
}
