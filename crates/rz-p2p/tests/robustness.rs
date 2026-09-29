//! Robustez dos decodificadores e da verificação diante de entradas hostis
//! (THR-TX-004, THR-P2P-003, `docs/AUDIT.md §4`).
//!
//! Testes de mutação determinísticos, sem dependências externas: partem de
//! um corpus de mensagens válidas de todos os tipos e aplicam inversões de
//! bits, trocas, truncamentos, inserções e bytes aleatórios. Propriedades:
//!
//! 1. nenhuma entrada provoca pânico;
//! 2. **canonicidade**: se uma entrada é aceita, reencodificá-la reproduz
//!    exatamente os mesmos bytes (não existe segunda codificação válida);
//! 3. transações e blocos mutados que ainda decodificam não provocam pânico
//!    na verificação de estado (assinaturas, provas de faixa, anéis).
//!
//! A semente é fixa para reprodutibilidade; `RZ_FUZZ_SEED` a altera e
//! `RZ_FUZZ_ITERS` aumenta o número de mutações por item (padrão 200).

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rz_codec::{Decode, Encode};
use rz_core::consensus::{Commit, Proposal, Vote, VoteType};
use rz_core::governance::{Category, Choice, ParamChange};
use rz_core::private::{build_private_tx, build_shield, SpendableNote};
use rz_core::state::ExecParams;
use rz_core::{
    Allocation, Block, BlockId, CommittedBlock, ConsensusParams, Genesis, GenesisValidator,
    GovernanceParams, NetworkKind, State, Transaction, TxBody, TxKind, PROTOCOL_VERSION,
};
use rz_crypto::{Hash32, SecretKey};
use rz_p2p::{secure, Hello, Message, PeerAddr, P2P_VERSION};
use rz_privacy::keys::ShieldedSecret;
use rz_privacy::note::scan_output;

const NET: &str = "rede-zero-devnet-fuzz";

/// SplitMix64: gerador determinístico simples, suficiente para mutações.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

/// Gerador para um teste: semente fixa, alterável por `RZ_FUZZ_SEED`.
fn rng(test: u64) -> Rng {
    let extra = std::env::var("RZ_FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    Rng(test ^ extra.wrapping_mul(0x2545_f491_4f6c_dd1d))
}

fn iters() -> usize {
    std::env::var("RZ_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200)
}

/// Uma mutação aleatória de `input`.
fn mutate(rng: &mut Rng, input: &[u8]) -> Vec<u8> {
    let mut v = input.to_vec();
    let rounds = 1 + rng.below(3);
    for _ in 0..rounds {
        match rng.below(8) {
            // Inverte um bit.
            0 if !v.is_empty() => {
                let i = rng.below(v.len());
                v[i] ^= 1 << rng.below(8);
            }
            // Substitui um byte por um valor extremo ou aleatório.
            1 if !v.is_empty() => {
                let i = rng.below(v.len());
                v[i] = [0x00, 0x01, 0x7f, 0x80, 0xff, rng.byte()][rng.below(6)];
            }
            // Trunca.
            2 if !v.is_empty() => {
                let n = rng.below(v.len());
                v.truncate(n);
            }
            // Insere bytes.
            3 => {
                let i = rng.below(v.len() + 1);
                let n = 1 + rng.below(8);
                for _ in 0..n {
                    v.insert(i, rng.byte());
                }
            }
            // Remove um trecho.
            4 if v.len() > 1 => {
                let i = rng.below(v.len());
                let n = 1 + rng.below((v.len() - i).min(16));
                v.drain(i..i + n);
            }
            // Sobrescreve 4 bytes com um comprimento exagerado (prefixos u32).
            5 if v.len() >= 4 => {
                let i = rng.below(v.len() - 3);
                let big = [u32::MAX, 0x0100_0000, 0x0000_ffff, 65][rng.below(4)];
                v[i..i + 4].copy_from_slice(&big.to_be_bytes());
            }
            // Acrescenta bytes ao final.
            6 => {
                let n = 1 + rng.below(8);
                for _ in 0..n {
                    v.push(rng.byte());
                }
            }
            // Duplica um trecho (confunde contagens de listas).
            _ if !v.is_empty() => {
                let i = rng.below(v.len());
                let n = 1 + rng.below((v.len() - i).min(64));
                let chunk = v[i..i + n].to_vec();
                let j = rng.below(v.len() + 1);
                v.splice(j..j, chunk);
            }
            _ => {}
        }
    }
    v
}

/// Decodifica sem pânico e verifica a canonicidade do que for aceito.
fn check_decode<T: Decode + Encode>(label: &str, bytes: &[u8]) -> Option<T> {
    let r = catch_unwind(AssertUnwindSafe(|| T::from_canonical_bytes(bytes)))
        .unwrap_or_else(|_| panic!("{label}: pânico ao decodificar {}", hex(bytes)));
    let v = r.ok()?;
    assert_eq!(
        v.to_canonical_bytes(),
        bytes,
        "{label}: codificação não canônica aceita: {}",
        hex(bytes)
    );
    Some(v)
}

fn hex(b: &[u8]) -> String {
    let shown: String = b.iter().take(96).map(|x| format!("{x:02x}")).collect();
    if b.len() > 96 {
        format!("{shown}… ({} bytes)", b.len())
    } else {
        shown
    }
}

// ------------------------------------------------------------------ corpus

fn validator() -> SecretKey {
    SecretKey::from_seed([1; 32])
}

fn rich() -> SecretKey {
    SecretKey::from_seed([2; 32])
}

fn genesis() -> Genesis {
    Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id: NET.into(),
        consensus: ConsensusParams::fast(200),
        min_fee: 1,
        max_block_txs: 100,
        validators: vec![GenesisValidator::new(validator().public_key(), 10_000)],
        allocations: vec![Allocation {
            address: rich().public_key().address(),
            amount: 10_000_000,
        }],
        governance: GovernanceParams::default(),
    }
}

fn account_tx(nonce: u64, kind: TxKind) -> Transaction {
    TxBody {
        version: 1,
        sender: rich().public_key(),
        nonce,
        fee: 1,
        kind,
    }
    .sign(&rich(), NET)
    .expect("assinatura")
}

struct Corpus {
    genesis: Genesis,
    /// Estado com blindagens suficientes para formar anéis.
    state: State,
    txs: Vec<Transaction>,
    blocks: Vec<CommittedBlock>,
    messages: Vec<Message>,
}

fn corpus() -> Corpus {
    let g = genesis();
    let p = ExecParams::from_genesis(&g);
    let mut state = State::from_genesis(&g).expect("genesis");
    let alice = ShieldedSecret::from_seed(&[10; 32]);
    let decoy = ShieldedSecret::from_seed(&[99; 32]).address();
    let mut shields = Vec::new();
    for nonce in 0..12 {
        let (to, amount) = if nonce == 5 {
            (alice.address(), 1_000)
        } else {
            (decoy, 10 + nonce)
        };
        let tx = build_shield(&rich(), NET, nonce, 1, &to, amount).expect("blindagem");
        state.apply_transaction(&tx, &p).expect("aplica blindagem");
        shields.push(tx);
    }
    let notes: Vec<SpendableNote> = state
        .outputs()
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            scan_output(&alice, o).map(|note| SpendableNote {
                global_index: i as u64,
                data: o.clone(),
                note,
            })
        })
        .collect();
    let private = build_private_tx(
        NET,
        &notes,
        state.outputs(),
        &[(ShieldedSecret::from_seed(&[11; 32]).address(), 400)],
        &alice.address(),
        None,
        5,
    )
    .expect("transação privada");

    let vote = |kind, block: Option<BlockId>| Vote::sign(kind, 1, 0, block, &validator(), NET);
    let v1 = vote(VoteType::Prevote, Some(BlockId(Hash32([1; 32]))));
    let v2 = vote(VoteType::Prevote, Some(BlockId(Hash32([2; 32]))));

    let n = 12;
    let mut txs = vec![
        shields[0].clone(),
        Transaction::Private(private),
        account_tx(
            n,
            TxKind::Transfer {
                to: validator().public_key().address(),
                amount: 5,
            },
        ),
        account_tx(
            n,
            TxKind::LockStake {
                amount: 10,
                unlock_height: 1_000,
            },
        ),
        account_tx(n, TxKind::Unlock { lock_id: 3 }),
        account_tx(
            n,
            TxKind::Propose {
                category: Category::Constitutional,
                content_hash: Hash32([7; 32]),
                params: vec![ParamChange::SlashBps(100), ParamChange::MinBond(5)],
                release_id: Some(Hash32([8; 32])),
                deposit: 1_000,
            },
        ),
        account_tx(
            n,
            TxKind::Vote {
                proposal: Hash32([9; 32]),
                choice: Choice::Yes,
            },
        ),
        account_tx(n, TxKind::Bond { amount: 5_000 }),
        account_tx(n, TxKind::Unbond { amount: 5_000 }),
        account_tx(
            n,
            TxKind::ReportDoubleVote {
                first: Box::new(v1.clone()),
                second: Box::new(v2.clone()),
            },
        ),
    ];

    // Bloco com transações e certificado.
    let parent = BlockId(g.hash());
    let (block, _) = Block::build(
        &g,
        parent,
        0,
        &State::from_genesis(&g).expect("genesis"),
        0,
        vec![account_tx(
            0,
            TxKind::Transfer {
                to: validator().public_key().address(),
                amount: 5,
            },
        )],
        &validator(),
    )
    .expect("bloco");
    let other = Block::build(
        &g,
        parent,
        0,
        &State::from_genesis(&g).expect("genesis"),
        0,
        vec![],
        &validator(),
    )
    .expect("bloco")
    .0;
    txs.push(account_tx(
        n,
        TxKind::ReportEquivocation {
            first: Box::new(block.signed_header()),
            second: Box::new(other.signed_header()),
        },
    ));
    let precommit = Vote::sign(
        VoteType::Precommit,
        1,
        0,
        Some(block.id()),
        &validator(),
        NET,
    );
    let committed = CommittedBlock {
        commit: Commit::from_votes(&[precommit]).expect("certificado"),
        block: block.clone(),
    };
    let proposal = Proposal::sign(1, 0, None, block.clone(), &validator(), NET);

    let mut messages = vec![
        Message::Hello(Hello {
            p2p_version: P2P_VERSION,
            network_id: NET.into(),
            genesis: g.hash(),
            height: 7,
            listen_port: 7100,
            relay: true,
            advertise: Some(PeerAddr::Host {
                host: "exemplo.onion".into(),
                port: 7100,
            }),
        }),
        Message::Ping(42),
        Message::Peers(vec![
            PeerAddr::Ip("127.0.0.1:7100".parse().expect("ip")),
            PeerAddr::Ip("[::1]:7101".parse().expect("ip")),
            PeerAddr::Host {
                host: "abc.onion".into(),
                port: 1,
            },
        ]),
        Message::GetBlocks {
            from_height: 1,
            max: 64,
        },
        Message::Block(Box::new(committed.clone())),
        Message::Blocks(vec![committed.clone()]),
        Message::Account {
            address: rich().public_key().address(),
            balance: 1,
            nonce: 2,
            height: 3,
        },
        Message::TxResult {
            id: txs[0].id(),
            accepted: false,
            reason: "motivo com acentuação".into(),
        },
        Message::Status {
            height: 1,
            tip: block.id(),
            finalized_height: 1,
            peers: 2,
            mempool: 3,
        },
        Message::Reject("rede errada".into()),
        Message::Outputs {
            start: 0,
            outputs: state.outputs()[..3].to_vec(),
        },
        Message::KeyImages {
            start: 0,
            images: vec![[3; 32], [4; 32]],
        },
        Message::GetGovernance {
            address: Some(rich().public_key().address()),
        },
        Message::Governance {
            height: 1,
            params: state.params().clone(),
            proposals: vec![],
            locks: vec![],
        },
        Message::ConsensusProposal(Box::new(proposal)),
        Message::ConsensusVote(v1),
    ];
    for tx in &txs {
        messages.push(Message::Transaction(tx.clone()));
    }
    messages.push(Message::StemTransaction(txs[1].clone()));
    txs.extend(shields.into_iter().skip(1).take(2));

    Corpus {
        genesis: g,
        state,
        txs,
        blocks: vec![committed],
        messages,
    }
}

// ------------------------------------------------------------------- testes

#[test]
fn corpus_roundtrips() {
    let c = corpus();
    for m in &c.messages {
        let bytes = m.to_canonical_bytes();
        let back = check_decode::<Message>("Message", &bytes).expect("mensagem válida");
        assert_eq!(&back, m);
    }
    for tx in &c.txs {
        check_decode::<Transaction>("Transaction", &tx.to_canonical_bytes()).expect("tx válida");
    }
    check_decode::<Genesis>("Genesis", &c.genesis.to_canonical_bytes()).expect("genesis válido");
}

#[test]
fn mutated_messages_never_panic_and_stay_canonical() {
    let c = corpus();
    let mut rng = rng(0x5eed_0001);
    let n = iters();
    for m in &c.messages {
        let bytes = m.to_canonical_bytes();
        for _ in 0..n {
            let mutated = mutate(&mut rng, &bytes);
            check_decode::<Message>("Message", &mutated);
        }
    }
}

#[test]
fn mutated_core_types_never_panic_and_stay_canonical() {
    let c = corpus();
    let mut rng = rng(0x5eed_0002);
    let n = iters();
    let g = c.genesis.to_canonical_bytes();
    for _ in 0..n * 4 {
        check_decode::<Genesis>("Genesis", &mutate(&mut rng, &g));
    }
    for cb in &c.blocks {
        let b = cb.to_canonical_bytes();
        for _ in 0..n * 2 {
            check_decode::<CommittedBlock>("CommittedBlock", &mutate(&mut rng, &b));
        }
        let b = cb.block.to_canonical_bytes();
        for _ in 0..n * 2 {
            check_decode::<Block>("Block", &mutate(&mut rng, &b));
        }
        let b = cb.commit.to_canonical_bytes();
        for _ in 0..n * 2 {
            check_decode::<Commit>("Commit", &mutate(&mut rng, &b));
        }
    }
}

/// Mutantes que ainda decodificam passam pela verificação completa do
/// estado: nenhum pânico, e nenhum é aceito sem ser idêntico ao original
/// (qualquer alteração invalida assinatura, prova ou anel).
#[test]
fn mutated_transactions_rejected_by_state_without_panic() {
    let c = corpus();
    let p = ExecParams::at(&c.genesis, 2);
    let mut rng = rng(0x5eed_0003);
    let n = iters();
    let mut verified = 0usize;
    for tx in &c.txs {
        let bytes = tx.to_canonical_bytes();
        for _ in 0..n {
            let mutated = mutate(&mut rng, &bytes);
            let Some(m) = check_decode::<Transaction>("Transaction", &mutated) else {
                continue;
            };
            verified += 1;
            let r = catch_unwind(AssertUnwindSafe(|| c.state.check_transaction(&m, &p)))
                .unwrap_or_else(|_| panic!("pânico na verificação de {}", hex(&mutated)));
            if mutated != bytes {
                assert!(r.is_err(), "transação adulterada aceita: {}", hex(&mutated));
            }
        }
    }
    // Garante que o teste exercitou a verificação, não só o decodificador.
    assert!(verified > 0);
}

#[test]
fn mutated_blocks_rejected_without_panic() {
    let c = corpus();
    let g = &c.genesis;
    let base = State::from_genesis(g).expect("genesis");
    let parent = BlockId(g.hash());
    let mut rng = rng(0x5eed_0004);
    let good = c.blocks[0].block.to_canonical_bytes();
    for _ in 0..iters() * 2 {
        let mutated = mutate(&mut rng, &good);
        let Some(b) = check_decode::<Block>("Block", &mutated) else {
            continue;
        };
        let r = catch_unwind(AssertUnwindSafe(|| {
            rz_core::apply_block(g, parent, 0, &base, &b)
        }))
        .unwrap_or_else(|_| panic!("pânico ao aplicar {}", hex(&mutated)));
        if mutated != good {
            assert!(r.is_err(), "bloco adulterado aceito: {}", hex(&mutated));
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = rng(0x5eed_0005);
    for _ in 0..iters() * 50 {
        let len = rng.below(512);
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
        // Metade começa com uma tag válida, para ir além do primeiro byte.
        if !bytes.is_empty() && rng.below(2) == 0 {
            bytes[0] = rng.below(0x18) as u8;
        }
        check_decode::<Message>("Message", &bytes);
        check_decode::<Transaction>("Transaction", &bytes);
        check_decode::<Block>("Block", &bytes);
        check_decode::<Vote>("Vote", &bytes);
        check_decode::<Proposal>("Proposal", &bytes);
        check_decode::<Genesis>("Genesis", &bytes);
    }
}

/// Escritor que repassa ao socket durante o handshake e, depois, apenas
/// captura os quadros cifrados (para adulterá-los ou repeti-los).
struct Capture {
    stream: TcpStream,
    captured: Arc<Mutex<Vec<u8>>>,
    passthrough: Arc<AtomicBool>,
}

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.passthrough.load(Ordering::SeqCst) {
            self.stream.write(buf)
        } else {
            self.captured.lock().expect("trava").extend_from_slice(buf);
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

/// Canal cifrado: quadros aleatórios, adulterados ou repetidos depois do
/// handshake são rejeitados com erro, sem pânico, e encerram a leitura.
#[test]
fn secure_channel_rejects_garbage_frames() {
    let mut rng = rng(0x5eed_0006);
    for round in 0..iters().min(64) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let key = SecretKey::from_seed([42; 32]);
            let (s, _) = listener.accept().expect("accept");
            let r = s.try_clone().expect("clone");
            let (mut reader, _w) = secure::respond(r, s, NET, &key).expect("handshake");
            let mut results = Vec::new();
            loop {
                let res = catch_unwind(AssertUnwindSafe(|| reader.read_message()))
                    .expect("pânico no canal");
                results.push(res.is_ok());
                if res.is_err() || results.len() >= 4 {
                    break;
                }
            }
            results
        });
        let s = TcpStream::connect(addr).expect("connect");
        let mut raw = s.try_clone().expect("clone");
        let captured = Arc::new(Mutex::new(Vec::new()));
        let passthrough = Arc::new(AtomicBool::new(true));
        let w = Capture {
            stream: s.try_clone().expect("clone"),
            captured: captured.clone(),
            passthrough: passthrough.clone(),
        };
        let (_reader, mut writer, _) = secure::initiate(s, w, NET, None).expect("handshake");
        passthrough.store(false, Ordering::SeqCst);
        writer
            .write_message(&Message::Ping(round as u64))
            .expect("cifra");
        let mut frame = captured.lock().expect("trava").clone();
        assert!(frame.len() > 4);

        match round % 4 {
            // Legítimo, depois lixo com comprimento aleatório.
            0 => {
                raw.write_all(&frame).expect("envia");
                let len = rng.below(200) as u32;
                raw.write_all(&len.to_be_bytes()).expect("envia");
                let junk: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
                raw.write_all(&junk).expect("envia");
            }
            // Quadro adulterado (qualquer bit após o comprimento).
            1 => {
                let i = 4 + rng.below(frame.len() - 4);
                frame[i] ^= 1 << rng.below(8);
                raw.write_all(&frame).expect("envia");
            }
            // Repetição: o mesmo quadro duas vezes (nonce já consumido).
            2 => {
                raw.write_all(&frame).expect("envia");
                raw.write_all(&frame).expect("envia");
            }
            // Comprimento declarado gigante.
            _ => {
                raw.write_all(&u32::MAX.to_be_bytes()).expect("envia");
            }
        }
        let _ = raw.shutdown(std::net::Shutdown::Write);
        let results = server.join().expect("thread");
        match round % 4 {
            0 | 2 => assert_eq!(results, vec![true, false], "rodada {round}"),
            _ => assert_eq!(results, vec![false], "rodada {round}"),
        }
    }
}

/// Transações **corretamente assinadas** com valores extremos: a assinatura
/// não protege contra quem assina a própria transação. Nenhuma pode causar
/// pânico (os builds de release têm `overflow-checks`), e as aceitas devem
/// preservar a invariante monetária.
#[test]
fn extreme_signed_values_never_panic() {
    let c = corpus();
    let extremes = [0, 1, 2, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    let mut rng = rng(0x5eed_0007);
    let mut state = c.state.clone();
    let mut height = 2;
    for i in 0..iters() * 5 {
        let mut pick = || extremes[rng.below(extremes.len())];
        let (a, b, fee) = (pick(), pick(), pick());
        let kind = match i % 9 {
            0 => TxKind::Transfer {
                to: validator().public_key().address(),
                amount: a,
            },
            1 => TxKind::LockStake {
                amount: a,
                unlock_height: b,
            },
            2 => TxKind::Unlock { lock_id: a },
            3 => TxKind::Propose {
                category: Category::Ordinary,
                content_hash: Hash32([1; 32]),
                params: vec![ParamChange::MinFee(b)],
                release_id: None,
                deposit: a,
            },
            4 => TxKind::Bond { amount: a },
            5 => TxKind::Unbond { amount: a },
            6 => TxKind::Propose {
                category: Category::Constitutional,
                content_hash: Hash32([2; 32]),
                params: vec![
                    ParamChange::EpochBlocks(a),
                    ParamChange::UnbondingBlocks(b),
                    ParamChange::TimeoutDelta(a),
                ],
                release_id: None,
                deposit: b,
            },
            7 => TxKind::Vote {
                proposal: Hash32([3; 32]),
                choice: Choice::No,
            },
            _ => TxKind::Transfer {
                to: rich().public_key().address(),
                amount: a,
            },
        };
        let nonce = state.account(&rich().public_key().address()).nonce;
        let tx = TxBody {
            version: 1,
            sender: rich().public_key(),
            nonce: if i % 7 == 0 { pick() } else { nonce },
            fee,
            kind,
        }
        .sign(&rich(), NET)
        .expect("assinatura");
        let p = ExecParams::at(&c.genesis, height);
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mut s = state.clone();
            s.apply_transaction(&tx, &p).map(|_| s)
        }))
        .unwrap_or_else(|_| panic!("pânico com transação assinada {:?}", tx));
        if let Ok(s) = r {
            state = s;
            height += 1;
        }
    }
    // Fora de blocos, as taxas pagas ainda não foram creditadas; a oferta
    // total nunca pode ser excedida.
    match state.check_supply() {
        Ok(()) => {}
        Err(rz_core::StateError::SupplyMismatch { expected, actual }) => {
            assert!(actual <= expected as u128)
        }
        Err(e) => panic!("{e}"),
    }
}
