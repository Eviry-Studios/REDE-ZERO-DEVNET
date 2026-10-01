//! Orquestração do Node: conexões, handshake, propagação, sincronização e
//! produção de blocos (`spec/P2P.md`, `spec/CONSENSUS.md`).
//!
//! Modelo de concorrência simples e auditável: uma thread por conexão para
//! leitura, uma para escrita, e threads dedicadas para aceitar conexões,
//! discar pares e produzir blocos. O estado da cadeia fica atrás de um único
//! `Mutex`; nenhuma thread segura dois locks ao mesmo tempo.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand_core::{OsRng, RngCore};
use rz_chain::bft::{App, Bft, Output, Timeout};
use rz_chain::{Chain, ChainError, Mempool, MempoolError};
use rz_codec::Encode;
use rz_core::consensus::MAX_TIME_PARAM_MS;
use rz_core::state::ExecParams;
use rz_core::{
    Account, Block, BlockId, CommittedBlock, Genesis, Proposal, Transaction, TxBody, TxError, TxId,
    TxKind, Vote,
};
use rz_crypto::{Address, Hash32, SecretKey};
use rz_p2p::secure::{initiate, respond, SecureReader};
use rz_p2p::{
    check_hello, FrameError, Hello, Message, Offense, PeerAddr, PeerScore, TokenBucket,
    MAX_BLOCKS_PER_MSG, MAX_KEY_IMAGES_PER_MSG, MAX_OUTPUTS_PER_MSG, MAX_PEERS_PER_MSG,
    P2P_VERSION,
};

use crate::content::{ContentCtl, ContentStore, Download, MAX_DOWNLOADS};
use crate::store::BlockStore;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(3);
const BAN_DURATION: Duration = Duration::from_secs(600);
const SEND_QUEUE: usize = 1024;
/// Balde geral por conexão (todas as mensagens exceto as de consenso).
const GENERAL_BURST: u32 = 400;
const GENERAL_PER_SEC: u32 = 200;
/// Balde de pedidos de pedaços de conteúdo por conexão (1 MiB cada).
const CHUNK_BURST: u32 = 32;
const CHUNK_PER_SEC: u32 = 8;

/// Balde de consenso por conexão para `n` validadores: cada validador emite
/// até dois votos por rodada, repassados uma vez por conexão, mais a
/// retransmissão periódica. Retorna `(rajada, recarga por segundo)`.
fn consensus_rate(n: usize) -> (u32, u32) {
    let n = u32::try_from(n).unwrap_or(u32::MAX);
    (n.saturating_mul(32).max(200), n.saturating_mul(8).max(50))
}

/// Capacidade da fila de propostas e votos para a thread de consenso.
const CONSENSUS_QUEUE: usize = 8_192;
const MAX_BLOCKS_RESPONSE_BYTES: usize = 3 * 1024 * 1024;

// Dandelion++ (ADR-0010, spec/P2P.md §4).
/// Probabilidade de passar da haste à flor em cada salto: 1 em N.
const FLUFF_ONE_IN: u64 = 10;
/// Duração da época de escolha do relay da haste.
const STEM_EPOCH: Duration = Duration::from_secs(600);
/// Embargo: prazo para a transação aparecer em fase de flor.
const STEM_EMBARGO_MIN: Duration = Duration::from_secs(10);
const STEM_EMBARGO_JITTER_MS: u64 = 10_000;
const MAX_STEM_POOL: usize = 10_000;

/// Nível de registro local. Por padrão, endereços de pares não são
/// registrados (THR-PRIV-004).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Quiet,
    Info,
    Debug,
}

pub struct NodeConfig {
    pub genesis: Genesis,
    pub data_dir: PathBuf,
    /// Endereço de escuta. `None` = Node privado: só disca, nunca aceita
    /// conexões nem anuncia endereço (ADR-0011).
    pub listen: Option<SocketAddr>,
    pub bootstrap: Vec<PeerAddr>,
    pub validator_key: Option<SecretKey>,
    /// Chave de identidade do Node no canal cifrado. `None` = carrega ou
    /// cria `node.key` no diretório de dados.
    pub node_key: Option<SecretKey>,
    /// Proxy SOCKS5 para conexões de saída (ex.: Tor em 127.0.0.1:9050).
    pub proxy: Option<SocketAddr>,
    /// Endereço público anunciado aos pares (ex.: serviço onion).
    pub advertise: Option<PeerAddr>,
    pub max_inbound: usize,
    pub max_outbound: usize,
    pub max_inbound_per_ip: usize,
    pub mempool_capacity: usize,
    pub log: LogLevel,
    /// Pontos de verificação (altura, bloco) obtidos pelo operador de fontes
    /// em que confia (subjetividade fraca, THR-CON-001).
    pub checkpoints: Vec<(u64, BlockId)>,
    /// Cota local de conteúdo hospedado, em bytes (`spec/CONTENT.md`).
    pub content_quota: u64,
}

impl NodeConfig {
    pub fn new(genesis: Genesis, data_dir: PathBuf, listen: SocketAddr) -> Self {
        Self {
            genesis,
            data_dir,
            listen: Some(listen),
            bootstrap: Vec::new(),
            validator_key: None,
            node_key: None,
            proxy: None,
            advertise: None,
            max_inbound: 32,
            max_outbound: 8,
            max_inbound_per_ip: 8,
            mempool_capacity: 10_000,
            log: LogLevel::Info,
            checkpoints: Vec::new(),
            content_quota: 1 << 30,
        }
    }
}

/// Resumo do estado do Node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeStatus {
    pub height: u64,
    pub tip: BlockId,
    pub finalized_height: u64,
    pub peers: usize,
    pub mempool: usize,
}

/// Eventos para a thread de consenso.
enum ConsensusEvent {
    Proposal(Box<Proposal>, Option<u64>),
    Vote(Vote, Option<u64>),
    /// A cadeia avançou por sincronização.
    ChainAdvanced,
}

/// Identificadores de mensagens de consenso já vistas (evita reenvio em laço).
/// Instante daqui a `ms` milissegundos, sem estouro: valores acima de
/// `MAX_TIME_PARAM_MS` são limitados (defesa em profundidade além de
/// `ConsensusParams::validate`).
fn after_ms(ms: u64) -> Instant {
    let d = Duration::from_millis(ms.min(MAX_TIME_PARAM_MS));
    let now = Instant::now();
    now.checked_add(d).unwrap_or(now)
}

#[derive(Default)]
struct Seen {
    set: HashSet<Hash32>,
    order: std::collections::VecDeque<Hash32>,
}

impl Seen {
    const CAP: usize = 50_000;

    fn contains(&self, id: &Hash32) -> bool {
        self.set.contains(id)
    }

    fn insert(&mut self, id: Hash32) -> bool {
        if !self.set.insert(id) {
            return false;
        }
        self.order.push_back(id);
        if self.order.len() > Self::CAP {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }
}

struct Core {
    chain: Chain,
    mempool: Mempool,
    store: BlockStore,
}

struct Peer {
    stream: TcpStream,
    tx: SyncSender<Message>,
    outbound_target: Option<PeerAddr>,
    listen: Option<PeerAddr>,
    /// Nodes (inclusive privados) recebem propagação; clientes (Wallets) não.
    relay: bool,
    /// Identidade comprovada no canal cifrado (só conexões de saída: quem
    /// conecta a este Node é anônimo, ADR-0011).
    node_id: Option<Hash32>,
}

#[derive(Default)]
struct Peers {
    conns: HashMap<u64, Peer>,
    next_id: u64,
    banned: HashMap<IpAddr, Instant>,
    known: BTreeSet<PeerAddr>,
    dialing: HashSet<PeerAddr>,
}

struct Shared {
    genesis: Genesis,
    genesis_hash: Hash32,
    key: Option<SecretKey>,
    node_key: SecretKey,
    listen_addr: Option<SocketAddr>,
    bootstrap: Vec<PeerAddr>,
    proxy: Option<SocketAddr>,
    advertise: Option<PeerAddr>,
    max_inbound: usize,
    max_outbound: usize,
    max_inbound_per_ip: usize,
    log_level: LogLevel,
    core: Mutex<Core>,
    peers: Mutex<Peers>,
    shutdown: AtomicBool,
    stem: Mutex<Stem>,
    consensus_tx: mpsc::SyncSender<ConsensusEvent>,
    seen: Mutex<Seen>,
    /// Tamanho do conjunto de validadores vigente (dimensiona o limite de
    /// taxa de mensagens de consenso por conexão).
    validator_count: AtomicUsize,
    /// Conteúdo hospedado e transferências (`spec/CONTENT.md`).
    content: Mutex<ContentCtl>,
}

/// Estado local do Dandelion++.
struct Stem {
    relay: Option<u64>,
    chosen_at: Instant,
    /// Transações em haste, com prazo de embargo.
    pool: HashMap<TxId, (Transaction, Instant)>,
}

/// Node em execução. Encerra suas threads ao ser descartado.
pub struct Node {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // Um pânico em outra thread não deve impedir o encerramento ordenado.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Node {
    pub fn start(cfg: NodeConfig) -> io::Result<Node> {
        let genesis_hash = cfg.genesis.hash();
        let mut chain = Chain::new(cfg.genesis.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        // Antes do reprocessamento: blocos gravados que divergem de um ponto
        // de verificação são descartados.
        chain
            .set_checkpoints(cfg.checkpoints.iter().copied())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Reprocessa blocos persistidos, verificando cada certificado e cada
        // transição de estado: o disco também é fonte não confiável.
        let (store, blocks) = BlockStore::open(&cfg.data_dir)?;
        let content = ContentStore::open(&cfg.data_dir.join("content"), cfg.content_quota)?;
        let total = blocks.len();
        let mut rejected = 0usize;
        for cb in blocks {
            if chain.commit(cb).is_err() {
                rejected += 1;
            }
        }

        let node_key = match cfg.node_key {
            Some(k) => k,
            None => load_or_create_node_key(&cfg.data_dir)?,
        };

        let listener = match cfg.listen {
            Some(addr) => {
                let l = TcpListener::bind(addr)?;
                l.set_nonblocking(true)?;
                Some(l)
            }
            None => None,
        };
        let listen_addr = match &listener {
            Some(l) => Some(l.local_addr()?),
            None => None,
        };

        // Fila limitada: sob inundação, mensagens excedentes são descartadas
        // (a retransmissão periódica as recupera).
        let (consensus_tx, consensus_rx) = mpsc::sync_channel::<ConsensusEvent>(CONSENSUS_QUEUE);
        let validator_count = chain.state().validators().validators().len();
        let shared = Arc::new(Shared {
            genesis_hash,
            key: cfg.validator_key,
            node_key,
            listen_addr,
            bootstrap: cfg.bootstrap,
            proxy: cfg.proxy,
            advertise: cfg.advertise,
            max_inbound: cfg.max_inbound,
            max_outbound: cfg.max_outbound,
            max_inbound_per_ip: cfg.max_inbound_per_ip,
            log_level: cfg.log,
            core: Mutex::new(Core {
                chain,
                mempool: Mempool::new(cfg.mempool_capacity),
                store,
            }),
            peers: Mutex::new(Peers::default()),
            shutdown: AtomicBool::new(false),
            stem: Mutex::new(Stem {
                relay: None,
                chosen_at: Instant::now(),
                pool: HashMap::new(),
            }),
            consensus_tx,
            seen: Mutex::new(Seen::default()),
            validator_count: AtomicUsize::new(validator_count),
            content: Mutex::new(ContentCtl::new(content)),
            genesis: cfg.genesis,
        });

        shared.info(format!(
            "rede {} | genesis {} | {} | {} blocos no disco ({} descartados) | altura {}",
            shared.genesis.network_id,
            genesis_hash,
            match listen_addr {
                Some(a) => format!("escutando {a}"),
                None => "node privado (sem escuta)".into(),
            },
            total,
            rejected,
            lock(&shared.core).chain.height()
        ));
        shared.info(format!(
            "identidade do node {}",
            shared.node_key.public_key().node_id()
        ));
        if let Some(p) = &shared.proxy {
            shared.info(format!("conexões de saída via proxy SOCKS5 {p}"));
        }
        if let Some(a) = &shared.advertise {
            shared.info(format!("endereço anunciado {a}"));
        }
        if let Some(k) = &shared.key {
            shared.info(format!("validador {}", k.public_key()));
        }

        let mut threads = Vec::new();
        if let Some(listener) = listener {
            let s = shared.clone();
            threads.push(thread::spawn(move || s.accept_loop(listener)));
        }
        {
            let s = shared.clone();
            threads.push(thread::spawn(move || s.dial_loop()));
        }
        {
            let s = shared.clone();
            threads.push(thread::spawn(move || s.consensus_loop(consensus_rx)));
        }
        Ok(Node { shared, threads })
    }

    /// Endereço de escuta.
    ///
    /// # Panics
    /// Em um Node privado (sem escuta).
    pub fn listen_addr(&self) -> SocketAddr {
        self.shared
            .listen_addr
            .expect("node privado não possui endereço de escuta")
    }

    /// Identidade do Node no canal cifrado.
    pub fn node_id(&self) -> rz_crypto::NodeId {
        self.shared.node_key.public_key().node_id()
    }

    /// Identidades dos Nodes com que há conexão de saída ativa.
    pub fn outbound_node_ids(&self) -> Vec<Hash32> {
        lock(&self.shared.peers)
            .conns
            .values()
            .filter_map(|c| c.node_id)
            .collect()
    }

    pub fn genesis(&self) -> &Genesis {
        &self.shared.genesis
    }

    pub fn status(&self) -> NodeStatus {
        self.shared.status()
    }

    pub fn account(&self, address: &Address) -> Account {
        lock(&self.shared.core).chain.state().account(address)
    }

    pub fn block_at(&self, height: u64) -> Option<Block> {
        lock(&self.shared.core).chain.block_at(height).cloned()
    }

    pub fn is_banned(&self, ip: IpAddr) -> bool {
        lock(&self.shared.peers)
            .banned
            .get(&ip)
            .is_some_and(|t| *t > Instant::now())
    }

    pub fn shutdown(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        for p in lock(&self.shared.peers).conns.values() {
            let _ = p.stream.shutdown(Shutdown::Both);
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum Flow {
    Continue,
    Penalize(Offense),
    Disconnect,
}

/// Estado local de uma conexão.
struct ConnCtx {
    id: u64,
    relay: bool,
    score: PeerScore,
    bucket: TokenBucket,
    /// Balde próprio para propostas e votos, proporcional ao número de
    /// validadores (RZ-IR-06). Excedente é descartado sem penalidade.
    consensus_bucket: TokenBucket,
    /// Tamanho do conjunto usado para dimensionar `consensus_bucket`.
    consensus_sized_for: usize,
    /// Pedidos de pedaços de conteúdo.
    chunk_bucket: TokenBucket,
    peer_height: u64,
    last_request_from: Option<u64>,
}

impl Shared {
    fn running(&self) -> bool {
        !self.shutdown.load(Ordering::SeqCst)
    }

    fn log(&self, level: LogLevel, msg: impl AsRef<str>) {
        if self.log_level >= level && level > LogLevel::Quiet {
            let t = now_ms();
            eprintln!(
                "[{:02}:{:02}:{:02}.{:03} {}] {}",
                (t / 3_600_000) % 24,
                (t / 60_000) % 60,
                (t / 1000) % 60,
                t % 1000,
                self.listen_addr.map_or(0, |a| a.port()),
                msg.as_ref()
            );
        }
    }

    fn info(&self, msg: impl AsRef<str>) {
        self.log(LogLevel::Info, msg);
    }

    fn debug(&self, msg: impl AsRef<str>) {
        self.log(LogLevel::Debug, msg);
    }

    fn status(&self) -> NodeStatus {
        let (height, tip, finalized_height, mempool) = {
            let c = lock(&self.core);
            (
                c.chain.height(),
                c.chain.tip(),
                c.chain.height(),
                c.mempool.len(),
            )
        };
        let peers = lock(&self.peers).conns.len();
        NodeStatus {
            height,
            tip,
            finalized_height,
            peers,
            mempool,
        }
    }

    fn hello(&self) -> Hello {
        Hello {
            p2p_version: P2P_VERSION,
            network_id: self.genesis.network_id.clone(),
            genesis: self.genesis_hash,
            height: lock(&self.core).chain.height(),
            listen_port: self.listen_addr.map_or(0, |a| a.port()),
            relay: true,
            advertise: self.advertise.clone(),
        }
    }

    // ---------------------------------------------------------------- cadeia

    /// Acrescenta um bloco finalizado (do consenso local ou da rede).
    /// Retorna `true` se era novo.
    fn commit_block(&self, cb: CommittedBlock) -> Result<bool, ChainError> {
        let mut guard = lock(&self.core);
        let Core {
            chain,
            mempool,
            store,
        } = &mut *guard;
        if cb.block.header.height <= chain.height() {
            return Ok(false);
        }
        chain.commit(cb.clone())?;
        if let Err(e) = store.append(&cb) {
            self.info(format!("falha ao gravar bloco: {e}"));
        }
        let state = chain.state();
        mempool.prune(&state, &ExecParams::at(chain.genesis(), chain.height() + 1));
        self.validator_count
            .store(state.validators().validators().len(), Ordering::Relaxed);
        // Ações de isolamento passam a valer assim que finalizadas.
        let isolated = state.defense().isolated(chain.height() + 1);
        drop(guard);
        if !isolated.is_empty() {
            let p = lock(&self.peers);
            for c in p.conns.values() {
                if c.node_id.is_some_and(|id| isolated.contains(&id)) {
                    let _ = c.stream.shutdown(Shutdown::Both);
                }
            }
        }
        Ok(true)
    }

    /// Node IDs isolados por ações de defesa vigentes.
    fn isolated(&self) -> BTreeSet<Hash32> {
        let c = lock(&self.core);
        c.chain.state().defense().isolated(c.chain.height() + 1)
    }

    // ------------------------------------------------------------- consenso

    /// Thread do consenso Zero-BFT (ADR-0012): alimenta a máquina de estados
    /// com propostas, votos e temporizadores, e executa suas saídas.
    fn consensus_loop(self: Arc<Self>, rx: mpsc::Receiver<ConsensusEvent>) {
        let key = self.key.as_ref().map(|k| SecretKey::from_seed(k.seed()));
        let params = lock(&self.core).chain.state().params().consensus.clone();
        let mut bft = Bft::new(&self.genesis.network_id, key, params);
        let mut timers: Vec<(Instant, Timeout)> = Vec::new();
        let mut next_height_at: Option<Instant> = Some(Instant::now());
        let retransmit_every = Duration::from_millis(
            lock(&self.core)
                .chain
                .state()
                .params()
                .consensus
                .timeout_propose_ms
                .clamp(200, MAX_TIME_PARAM_MS),
        );
        let mut next_retransmit = after_ms(retransmit_every.as_millis() as u64);

        while self.running() {
            let now = Instant::now();
            if next_height_at.is_some_and(|t| now >= t) {
                next_height_at = None;
                timers.clear();
                let outs = {
                    let guard = lock(&self.core);
                    let state = guard.chain.state();
                    bft.set_params(state.params().consensus.clone());
                    let mut app = NodeApp::new(&guard, &self.genesis, self.key.as_ref());
                    bft.start_height(
                        guard.chain.height() + 1,
                        guard.chain.tip(),
                        state.validators().clone(),
                        &mut app,
                    )
                };
                self.apply_outputs(outs, &mut timers, &mut next_height_at, &bft);
            }

            let mut wait = Duration::from_millis(100);
            for t in timers
                .iter()
                .map(|(t, _)| *t)
                .chain(next_height_at)
                .chain([next_retransmit])
            {
                wait = wait.min(t.saturating_duration_since(now));
            }

            // Blocos importados pela sincronização também avançam a altura,
            // mesmo que o aviso ChainAdvanced tenha se perdido na fila cheia.
            if next_height_at.is_none() && lock(&self.core).chain.height() >= bft.height() {
                next_height_at = Some(Instant::now());
            }

            match rx.recv_timeout(wait) {
                Ok(ConsensusEvent::Proposal(p, from)) => {
                    let r = {
                        let guard = lock(&self.core);
                        let mut app = NodeApp::new(&guard, &self.genesis, self.key.as_ref());
                        bft.on_proposal((*p).clone(), &mut app)
                    };
                    if r.relay && lock(&self.seen).insert(p.id()) {
                        self.broadcast(&Message::ConsensusProposal(p), from);
                    }
                    self.apply_outputs(r.outputs, &mut timers, &mut next_height_at, &bft);
                }
                Ok(ConsensusEvent::Vote(v, from)) => {
                    let r = {
                        let guard = lock(&self.core);
                        let mut app = NodeApp::new(&guard, &self.genesis, self.key.as_ref());
                        bft.on_vote(v.clone(), &mut app)
                    };
                    if r.relay && lock(&self.seen).insert(v.id()) {
                        self.broadcast(&Message::ConsensusVote(v), from);
                    }
                    self.apply_outputs(r.outputs, &mut timers, &mut next_height_at, &bft);
                }
                Ok(ConsensusEvent::ChainAdvanced) => {
                    let h = lock(&self.core).chain.height();
                    if h >= bft.height() && next_height_at.is_none() {
                        next_height_at = Some(Instant::now());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }

            let now = Instant::now();
            let due: Vec<Timeout> = timers
                .iter()
                .filter(|(t, _)| *t <= now)
                .map(|(_, t)| *t)
                .collect();
            timers.retain(|(t, _)| *t > now);
            for t in due {
                let outs = {
                    let guard = lock(&self.core);
                    let mut app = NodeApp::new(&guard, &self.genesis, self.key.as_ref());
                    bft.on_timeout(t, &mut app)
                };
                self.apply_outputs(outs, &mut timers, &mut next_height_at, &bft);
            }
            if now >= next_retransmit {
                next_retransmit = now + retransmit_every;
                let outs = bft.retransmit();
                self.apply_outputs(outs, &mut timers, &mut next_height_at, &bft);
            }
        }
    }

    fn apply_outputs(
        &self,
        outs: Vec<Output>,
        timers: &mut Vec<(Instant, Timeout)>,
        next_height_at: &mut Option<Instant>,
        bft: &Bft,
    ) {
        for o in outs {
            match o {
                Output::BroadcastProposal(p) => {
                    lock(&self.seen).insert(p.id());
                    self.broadcast(&Message::ConsensusProposal(p), None);
                }
                Output::BroadcastVote(v) => {
                    lock(&self.seen).insert(v.id());
                    self.broadcast(&Message::ConsensusVote(v), None);
                }
                Output::ScheduleTimeout(t, ms) => {
                    timers.push((after_ms(ms), t));
                }
                Output::Decide(block, commit) => {
                    let cb = CommittedBlock {
                        block: *block,
                        commit,
                    };
                    let (h, n, id, round) = (
                        cb.block.header.height,
                        cb.block.txs.len(),
                        cb.block.id(),
                        cb.commit.round,
                    );
                    match self.commit_block(cb.clone()) {
                        Ok(true) => {
                            self.info(format!(
                                "bloco finalizado: altura {h}, rodada {round}, {n} transações, id {id}"
                            ));
                            self.broadcast(&Message::Block(Box::new(cb)), None);
                        }
                        Ok(false) => {}
                        Err(e) => self.info(format!("decisão não aplicada: {e}")),
                    }
                    let interval = lock(&self.core)
                        .chain
                        .state()
                        .params()
                        .consensus
                        .block_interval_ms;
                    *next_height_at = Some(after_ms(interval));
                }
                Output::VoteEvidence(a, b) => {
                    self.info(format!(
                        "dupla assinatura detectada: validador {} na altura {}",
                        a.validator, a.height
                    ));
                    self.report(TxKind::ReportDoubleVote {
                        first: a,
                        second: b,
                    });
                }
                Output::ProposalEvidence(a, b) => {
                    self.info("propostas conflitantes detectadas");
                    self.report(TxKind::ReportDoubleProposal {
                        first: a,
                        second: b,
                    });
                }
            }
        }
        let _ = bft;
    }

    /// Transforma evidência em transação de denúncia, assinada pela chave do
    /// validador local (que paga a taxa).
    fn report(&self, kind: TxKind) {
        let Some(key) = &self.key else {
            return;
        };
        let tx = {
            let c = lock(&self.core);
            let state = c.chain.state();
            TxBody {
                version: rz_core::tx::TX_VERSION,
                sender: key.public_key(),
                nonce: state.account(&key.public_key().address()).nonce,
                fee: state.params().min_fee,
                kind,
            }
            .sign(key, &self.genesis.network_id)
        };
        if let Ok(tx) = tx {
            let _ = self.fluff(tx, None);
        }
    }

    // ----------------------------------------------------------------- pares

    fn send_to(&self, id: u64, msg: Message) {
        let tx = lock(&self.peers).conns.get(&id).map(|p| p.tx.clone());
        if let Some(tx) = tx {
            if let Err(TrySendError::Full(_)) = tx.try_send(msg) {
                self.drop_peer(id);
            }
        }
    }

    fn broadcast(&self, msg: &Message, except: Option<u64>) {
        let targets: Vec<(u64, SyncSender<Message>)> = lock(&self.peers)
            .conns
            .iter()
            .filter(|(id, p)| p.relay && Some(**id) != except)
            .map(|(id, p)| (*id, p.tx.clone()))
            .collect();
        for (id, tx) in targets {
            if let Err(TrySendError::Full(_)) = tx.try_send(msg.clone()) {
                // Par lento demais: desconecta em vez de acumular memória.
                self.drop_peer(id);
            }
        }
    }

    fn drop_peer(&self, id: u64) {
        if let Some(p) = lock(&self.peers).conns.get(&id) {
            let _ = p.stream.shutdown(Shutdown::Both);
        }
    }

    fn accept_loop(self: Arc<Self>, listener: TcpListener) {
        while self.running() {
            match listener.accept() {
                Ok((stream, addr)) => {
                    if !self.admit_inbound(addr.ip()) {
                        let _ = stream.shutdown(Shutdown::Both);
                        continue;
                    }
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    let s = self.clone();
                    thread::spawn(move || s.run_connection(stream, None));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    fn admit_inbound(&self, ip: IpAddr) -> bool {
        let mut p = lock(&self.peers);
        let now = Instant::now();
        p.banned.retain(|_, until| *until > now);
        if p.banned.contains_key(&ip) {
            return false;
        }
        let inbound: Vec<&Peer> = p
            .conns
            .values()
            .filter(|c| c.outbound_target.is_none())
            .collect();
        let same_ip = inbound
            .iter()
            .filter(|c| c.stream.peer_addr().map(|a| a.ip()).ok() == Some(ip))
            .count();
        inbound.len() < self.max_inbound && same_ip < self.max_inbound_per_ip
    }

    fn dial_loop(self: Arc<Self>) {
        while self.running() {
            self.check_embargo();
            let candidates: Vec<PeerAddr> = {
                let mut p = lock(&self.peers);
                let now = Instant::now();
                p.banned.retain(|_, until| *until > now);
                let connected: HashSet<PeerAddr> = p
                    .conns
                    .values()
                    .flat_map(|c| [c.outbound_target.clone(), c.listen.clone()])
                    .flatten()
                    .collect();
                let outbound = p
                    .conns
                    .values()
                    .filter(|c| c.outbound_target.is_some())
                    .count()
                    + p.dialing.len();
                let slots = self.max_outbound.saturating_sub(outbound);
                let pool: Vec<PeerAddr> = self
                    .bootstrap
                    .iter()
                    .chain(p.known.iter())
                    .filter(|a| {
                        !self.is_self(a)
                            && !connected.contains(*a)
                            && !p.dialing.contains(*a)
                            && !matches!(a, PeerAddr::Ip(ip) if p.banned.contains_key(&ip.ip()))
                            // Endereços onion só são alcançáveis via proxy Tor.
                            && (self.proxy.is_some() || !a.is_onion())
                    })
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .take(slots)
                    .collect();
                for a in &pool {
                    p.dialing.insert(a.clone());
                }
                pool
            };
            for addr in candidates {
                let s = self.clone();
                thread::spawn(move || {
                    if let Ok(stream) = rz_p2p::socks::connect(&addr, s.proxy, DIAL_TIMEOUT) {
                        s.clone().run_connection(stream, Some(addr));
                    } else {
                        lock(&s.peers).dialing.remove(&addr);
                    }
                });
            }
            for _ in 0..20 {
                if !self.running() {
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    fn is_self(&self, a: &PeerAddr) -> bool {
        if self.advertise.as_ref() == Some(a) {
            return true;
        }
        match (a, self.listen_addr) {
            (PeerAddr::Ip(a), Some(l)) => {
                a.port() == l.port()
                    && (a.ip() == l.ip() || a.ip().is_loopback() || l.ip().is_unspecified())
            }
            _ => false,
        }
    }

    fn run_connection(self: Arc<Self>, stream: TcpStream, outbound: Option<PeerAddr>) {
        let result = self.clone().connection(stream, outbound.clone());
        if let Some(addr) = outbound {
            lock(&self.peers).dialing.remove(&addr);
        }
        if let Err(e) = result {
            self.debug(format!("conexão encerrada: {e}"));
        }
    }

    fn connection(
        self: Arc<Self>,
        stream: TcpStream,
        outbound: Option<PeerAddr>,
    ) -> io::Result<()> {
        let remote = stream.peer_addr()?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;

        // Canal cifrado (ADR-0011): quem disca inicia; quem aceita prova sua
        // identidade de Node.
        let net = &self.genesis.network_id;
        let mut node_id = None;
        let (mut reader, mut writer) = if outbound.is_some() {
            let (r, w, node) =
                initiate(stream.try_clone()?, stream.try_clone()?, net, None).map_err(frame_io)?;
            let id = node.node_id().0;
            // Node isolado por ação de defesa atestada (ADR-0016).
            if self.isolated().contains(&id) {
                return Err(io::Error::other("par isolado por ação de defesa"));
            }
            node_id = Some(id);
            (r, w)
        } else {
            respond(
                stream.try_clone()?,
                stream.try_clone()?,
                net,
                &self.node_key,
            )
            .map_err(frame_io)?
        };

        // Handshake de protocolo (spec/P2P.md §3).
        writer
            .write_message(&Message::Hello(self.hello()))
            .map_err(frame_io)?;
        let hello = match reader.read_message() {
            Ok(Message::Hello(h)) => h,
            Ok(Message::Reject(r)) => {
                return Err(io::Error::other(format!("rejeitado pelo par: {r}")))
            }
            Ok(_) => {
                let _ = writer.write_message(&Message::Reject("esperado HELLO".into()));
                return Err(io::Error::other("mensagem antes do HELLO"));
            }
            Err(e) => return Err(frame_io(e)),
        };
        if let Err(e) = check_hello(&hello, net, &self.genesis_hash) {
            let _ = writer.write_message(&Message::Reject(e.to_string()));
            return Err(io::Error::other(format!("handshake: {e}")));
        }
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;

        // Endereço de escuta do par: o discado; um nome anunciado (ex.:
        // onion); ou o IP observado com a porta declarada. Um IP anunciado
        // nunca substitui o observado (evita envenenar a lista de pares).
        let listen = match (&outbound, &hello.advertise) {
            (Some(a), _) => Some(a.clone()),
            (None, Some(a @ PeerAddr::Host { .. })) => Some(a.clone()),
            (None, _) if hello.listen_port != 0 => Some(PeerAddr::Ip(SocketAddr::new(
                remote.ip(),
                hello.listen_port,
            ))),
            (None, _) => None,
        };
        let relay = outbound.is_some() || hello.relay;

        let (tx, rx) = mpsc::sync_channel::<Message>(SEND_QUEUE);
        let id = {
            let mut p = lock(&self.peers);
            let id = p.next_id;
            p.next_id += 1;
            p.conns.insert(
                id,
                Peer {
                    stream: stream.try_clone()?,
                    tx: tx.clone(),
                    outbound_target: outbound.clone(),
                    listen: listen.clone(),
                    relay,
                    node_id,
                },
            );
            if let Some(l) = listen {
                if !self.is_self(&l) {
                    p.known.insert(l);
                }
            }
            id
        };
        // Endereços de clientes nunca são registrados (THR-PRIV-004).
        match &outbound {
            Some(a) => self.debug(format!("conectado a {a}")),
            None if relay => self.debug("par conectado"),
            None => self.debug("cliente conectado"),
        }

        let shutdown_handle = stream.try_clone()?;
        let writer_thread = thread::spawn(move || {
            for msg in rx {
                if writer.write_message(&msg).is_err() {
                    break;
                }
            }
            let _ = shutdown_handle.shutdown(Shutdown::Both);
        });

        let mut ctx = ConnCtx {
            id,
            relay,
            score: PeerScore::default(),
            bucket: TokenBucket::new(GENERAL_BURST, GENERAL_PER_SEC),
            consensus_bucket: {
                let (burst, rate) = consensus_rate(0);
                TokenBucket::new(burst, rate)
            },
            consensus_sized_for: 0,
            chunk_bucket: TokenBucket::new(CHUNK_BURST, CHUNK_PER_SEC),
            peer_height: hello.height,
            last_request_from: None,
        };
        if relay {
            let _ = tx.try_send(Message::GetPeers);
        }
        self.maybe_sync(&mut ctx);

        let banned = self.read_loop(&mut reader, &mut ctx);

        {
            let mut p = lock(&self.peers);
            p.conns.remove(&id);
            // Atrás de um serviço onion, todas as conexões chegam do proxy
            // local: banir o IP de loopback isolaria todos os pares onion.
            let behind_onion = remote.ip().is_loopback() && self.advertise.is_some();
            if banned && !behind_onion {
                p.banned.insert(remote.ip(), Instant::now() + BAN_DURATION);
            }
        }
        if banned {
            self.info("par desconectado e colocado em quarentena por mau comportamento");
        }
        let _ = stream.shutdown(Shutdown::Both);
        drop(tx);
        let _ = writer_thread.join();
        Ok(())
    }

    /// Lê mensagens até o fim da conexão. Retorna `true` se o par foi banido.
    fn read_loop(&self, reader: &mut SecureReader<TcpStream>, ctx: &mut ConnCtx) -> bool {
        while self.running() {
            let flow = match reader.read_message() {
                Ok(msg @ (Message::ConsensusProposal(_) | Message::ConsensusVote(_))) => {
                    let n = self.validator_count.load(Ordering::Relaxed);
                    if n != ctx.consensus_sized_for {
                        let (burst, rate) = consensus_rate(n);
                        ctx.consensus_bucket.set_rate(burst, rate);
                        ctx.consensus_sized_for = n;
                    }
                    // Acima do orçamento: descarta antes de qualquer
                    // verificação, sem penalizar (rajadas honestas existem e
                    // a retransmissão recupera o que se perder).
                    if ctx.consensus_bucket.try_take(1) {
                        self.handle(ctx, msg)
                    } else {
                        Flow::Continue
                    }
                }
                Ok(msg) => {
                    if !ctx.bucket.try_take(1) {
                        Flow::Penalize(Offense::RateLimited)
                    } else {
                        self.handle(ctx, msg)
                    }
                }
                Err(FrameError::Decode(_)) => Flow::Penalize(Offense::MalformedMessage),
                Err(FrameError::TooLarge(_)) => {
                    // O enquadramento foi perdido: encerra e penaliza.
                    ctx.score.penalize(Offense::MalformedMessage);
                    return ctx.score.penalize(Offense::ProtocolViolation);
                }
                Err(FrameError::Crypto | FrameError::Handshake(_)) => {
                    // Canal adulterado ou dessincronizado: encerra.
                    return ctx.score.penalize(Offense::ProtocolViolation);
                }
                Err(FrameError::Io(_)) => return false,
            };
            match flow {
                Flow::Continue => {}
                Flow::Disconnect => return false,
                Flow::Penalize(o) => {
                    self.debug(format!("infração: {o:?}"));
                    if ctx.score.penalize(o) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn maybe_sync(&self, ctx: &mut ConnCtx) {
        let height = lock(&self.core).chain.height();
        if ctx.peer_height > height {
            self.request_blocks(ctx, height + 1);
        }
    }

    fn request_blocks(&self, ctx: &mut ConnCtx, from: u64) {
        ctx.last_request_from = Some(from);
        self.send_to(
            ctx.id,
            Message::GetBlocks {
                from_height: from,
                max: MAX_BLOCKS_PER_MSG as u32,
            },
        );
    }

    fn handle(&self, ctx: &mut ConnCtx, msg: Message) -> Flow {
        match msg {
            Message::Hello(_) => Flow::Penalize(Offense::ProtocolViolation),
            Message::Ping(n) => {
                self.send_to(ctx.id, Message::Pong(n));
                Flow::Continue
            }
            Message::Pong(_) => Flow::Continue,
            Message::GetPeers => {
                let peers: Vec<PeerAddr> = lock(&self.peers)
                    .conns
                    .iter()
                    .filter(|(id, _)| **id != ctx.id)
                    .filter_map(|(_, p)| p.listen.clone())
                    .take(MAX_PEERS_PER_MSG)
                    .collect();
                self.send_to(ctx.id, Message::Peers(peers));
                Flow::Continue
            }
            Message::Peers(list) => {
                let mut p = lock(&self.peers);
                for a in list {
                    if a.port() != 0 && !self.is_self(&a) && p.known.len() < 1024 {
                        p.known.insert(a);
                    }
                }
                Flow::Continue
            }
            Message::Transaction(tx) => self.handle_tx(ctx, tx),
            Message::Block(b) => self.handle_block(ctx, *b),
            Message::ConsensusProposal(p) => {
                // Marcada como vista só quando o consenso a aceita: uma
                // mensagem descartada (rodada adiantada, fila cheia) volta a
                // ser considerada quando retransmitida.
                if !lock(&self.seen).contains(&p.id()) {
                    let _ = self
                        .consensus_tx
                        .try_send(ConsensusEvent::Proposal(p, Some(ctx.id)));
                }
                Flow::Continue
            }
            Message::ConsensusVote(v) => {
                if !lock(&self.seen).contains(&v.id()) {
                    let _ = self
                        .consensus_tx
                        .try_send(ConsensusEvent::Vote(v, Some(ctx.id)));
                }
                Flow::Continue
            }
            Message::GetBlocks { from_height, max } => {
                let blocks = self.blocks_from(from_height, max);
                self.send_to(ctx.id, Message::Blocks(blocks));
                Flow::Continue
            }
            Message::Blocks(blocks) => self.handle_blocks(ctx, blocks),
            Message::GetAccount(address) => {
                let (acc, height) = {
                    let c = lock(&self.core);
                    (c.chain.state().account(&address), c.chain.height())
                };
                self.send_to(
                    ctx.id,
                    Message::Account {
                        address,
                        balance: acc.balance,
                        nonce: acc.nonce,
                        height,
                    },
                );
                Flow::Continue
            }
            Message::GetStatus => {
                let s = self.status();
                self.send_to(
                    ctx.id,
                    Message::Status {
                        height: s.height,
                        tip: s.tip,
                        finalized_height: s.finalized_height,
                        peers: s.peers as u32,
                        mempool: s.mempool as u32,
                    },
                );
                Flow::Continue
            }
            Message::GetOutputs { from, max } => {
                let outputs = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let all = state.outputs();
                    let start = usize::try_from(from).unwrap_or(usize::MAX).min(all.len());
                    let end = start
                        .saturating_add((max as usize).min(MAX_OUTPUTS_PER_MSG))
                        .min(all.len());
                    all[start..end].to_vec()
                };
                self.send_to(
                    ctx.id,
                    Message::Outputs {
                        start: from,
                        outputs,
                    },
                );
                Flow::Continue
            }
            Message::GetKeyImages { from, max } => {
                let images = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let all = state.key_image_log();
                    let start = usize::try_from(from).unwrap_or(usize::MAX).min(all.len());
                    let end = start
                        .saturating_add((max as usize).min(MAX_KEY_IMAGES_PER_MSG))
                        .min(all.len());
                    all[start..end].to_vec()
                };
                self.send_to(
                    ctx.id,
                    Message::KeyImages {
                        start: from,
                        images,
                    },
                );
                Flow::Continue
            }
            Message::StemTransaction(tx) => self.handle_stem(ctx, tx),
            Message::GetGovernance { address } => {
                let msg = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let mut proposals: Vec<_> = state.proposals().collect();
                    proposals.sort_by_key(|p| std::cmp::Reverse(p.submitted_at));
                    Message::Governance {
                        height: c.chain.height(),
                        params: state.params().clone(),
                        proposals: proposals
                            .into_iter()
                            .take(rz_p2p::MAX_PROPOSALS_PER_MSG)
                            .map(|p| rz_core::governance::ProposalSummary(p.clone()))
                            .collect(),
                        locks: state
                            .locks()
                            .filter(|(_, l)| Some(l.owner) == address)
                            .take(rz_p2p::MAX_LOCKS_PER_MSG)
                            .map(|(id, l)| rz_core::governance::LockEntry {
                                id: *id,
                                lock: l.clone(),
                            })
                            .collect(),
                    }
                };
                self.send_to(ctx.id, msg);
                Flow::Continue
            }
            Message::GetAssets { address } => {
                let msg = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let m = state.market();
                    Message::Assets {
                        height: c.chain.height(),
                        pool_zero: m.pool_balance(&rz_core::market::AssetId::ZERO),
                        assets: m
                            .assets
                            .iter()
                            .take(rz_core::market::MAX_ASSETS)
                            .map(|(id, info)| rz_core::market::AssetView {
                                id: *id,
                                info: info.clone(),
                                pool: m.pool_balance(id),
                                balance: address.map(|a| m.balance(&a, id)),
                                last_price: m.last_price.get(id).copied(),
                            })
                            .collect(),
                    }
                };
                self.send_to(ctx.id, msg);
                Flow::Continue
            }
            Message::GetMarket { asset, owner } => {
                let msg = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let m = state.market();
                    let (bids, asks) = m.book(&asset, rz_p2p::MAX_BOOK_LEVELS);
                    Message::Market {
                        height: c.chain.height(),
                        asset,
                        last_price: m.last_price.get(&asset).copied(),
                        bids,
                        asks,
                        own: m
                            .orders
                            .values()
                            .filter(|o| o.asset == asset && Some(o.owner) == owner)
                            .take(rz_p2p::MAX_OWN_ORDERS)
                            .cloned()
                            .collect(),
                    }
                };
                self.send_to(ctx.id, msg);
                Flow::Continue
            }
            Message::GetCommunity { name } => {
                let msg = {
                    let c = lock(&self.core);
                    Message::Community {
                        height: c.chain.height(),
                        community: c
                            .chain
                            .state()
                            .communities()
                            .by_name(&name)
                            .cloned()
                            .map(Box::new),
                    }
                };
                self.send_to(ctx.id, msg);
                Flow::Continue
            }
            Message::Resolve { name, kind } => {
                let msg = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let cs = state.communities();
                    Message::Resolved {
                        height: c.chain.height(),
                        target: cs.resolve(&name, kind),
                        owner: cs.names.get(&(kind, name.clone())).map(|r| r.owner),
                        name,
                        kind,
                    }
                };
                self.send_to(ctx.id, msg);
                Flow::Continue
            }
            Message::GetDefense => {
                let msg = {
                    let c = lock(&self.core);
                    let state = c.chain.state();
                    let d = state.defense();
                    let incident = d.active_incident.and_then(|id| d.incidents.get(&id));
                    Message::Defense {
                        height: c.chain.height(),
                        mode: d.mode,
                        mode_since: d.mode_since,
                        mode_expires_at: d.mode_expires_at,
                        seq: d.seq,
                        credentials: incident
                            .map(|i| {
                                d.credentials
                                    .values()
                                    .filter(|cr| cr.incident == i.id)
                                    .take(rz_p2p::MAX_CREDENTIALS_PER_MSG)
                                    .cloned()
                                    .collect()
                            })
                            .unwrap_or_default(),
                        incident: incident.cloned().map(Box::new),
                    }
                };
                self.send_to(ctx.id, msg);
                Flow::Continue
            }
            Message::Reject(_) => Flow::Disconnect,
            // Respostas não solicitadas são ignoradas.
            Message::Account { .. }
            | Message::TxResult { .. }
            | Message::Status { .. }
            | Message::Outputs { .. }
            | Message::KeyImages { .. }
            | Message::Governance { .. }
            | Message::Assets { .. }
            | Message::Market { .. }
            | Message::Community { .. }
            | Message::Resolved { .. }
            | Message::Defense { .. } => Flow::Continue,
            Message::GetManifest(h) => self.handle_get_manifest(ctx, h),
            Message::Manifest { hash, bytes } => self.handle_manifest(ctx, hash, bytes),
            Message::GetContent(id) => self.handle_get_content(ctx, id),
            Message::ContentInfo { id, info } => self.handle_content_info(ctx, id, info),
            Message::GetChunk { id, index } => {
                let data = if ctx.chunk_bucket.try_take(1) {
                    lock(&self.content).store.chunk(&id, index as usize)
                } else {
                    None
                };
                self.send_to(ctx.id, Message::Chunk { id, index, data });
                Flow::Continue
            }
            Message::Chunk { id, index, data } => self.handle_chunk(ctx, id, index, data),
            Message::HaveContent(id) => {
                if ctx.relay && self.content_wanted_from(ctx.id, id) {
                    self.send_to(ctx.id, Message::GetContent(id));
                }
                Flow::Continue
            }
        }
    }

    // ------------------------------------------------- conteúdo (Exonet)

    /// Manifestos referenciados por Comunidades no estado (qualquer versão do
    /// histórico) e os das Comunidades reconhecidas.
    fn referenced_manifests(&self) -> (BTreeSet<Hash32>, Vec<Hash32>) {
        let c = lock(&self.core);
        let state = c.chain.state();
        let cs = state.communities();
        let mut all = BTreeSet::new();
        let mut recognized = Vec::new();
        for com in cs.communities.values() {
            let hashes = std::iter::once(com.manifest_hash).chain(com.history.iter().map(|h| h.1));
            for h in hashes {
                all.insert(h);
                if com.status == rz_core::community::CommunityStatus::Recognized {
                    recognized.push(h);
                }
            }
        }
        (all, recognized)
    }

    /// O objeto é alvo de um nome ou interface de uma Comunidade reconhecida.
    fn content_referenced(&self, id: &Hash32) -> bool {
        let named = {
            let c = lock(&self.core);
            let state = c.chain.state();
            state.communities().names.values().any(|r| r.target == *id)
        };
        if named {
            return true;
        }
        let (_, recognized) = self.referenced_manifests();
        lock(&self.content).frontends_of(&recognized).contains(id)
    }

    /// Deve pedir este objeto à conexão `from`?
    fn content_wanted_from(&self, _from: u64, id: Hash32) -> bool {
        {
            let mut ct = lock(&self.content);
            ct.expire();
            if ct.store.has(&id)
                || ct.downloads.contains_key(&id)
                || ct.wanted.contains_key(&id)
                || ct.downloads.len() >= MAX_DOWNLOADS
            {
                return false;
            }
        }
        if !self.content_referenced(&id) {
            return false;
        }
        lock(&self.content).wanted.insert(id, Instant::now());
        true
    }

    fn handle_get_manifest(&self, ctx: &mut ConnCtx, h: Hash32) -> Flow {
        let bytes = lock(&self.content).store.manifest(&h);
        let missing = bytes.is_none();
        self.send_to(ctx.id, Message::Manifest { hash: h, bytes });
        if missing && self.referenced_manifests().0.contains(&h) {
            let ask = {
                let mut ct = lock(&self.content);
                ct.expire();
                ct.wanted_manifests.insert(h, Instant::now()).is_none()
            };
            if ask {
                self.broadcast(&Message::GetManifest(h), Some(ctx.id));
            }
        }
        Flow::Continue
    }

    fn handle_manifest(&self, ctx: &mut ConnCtx, hash: Hash32, bytes: Option<Vec<u8>>) -> Flow {
        let Some(bytes) = bytes else {
            return Flow::Continue;
        };
        if rz_core::content::Manifest::hash_of(&bytes) != hash {
            return Flow::Penalize(Offense::ProtocolViolation);
        }
        if lock(&self.content).store.has_manifest(&hash) {
            return Flow::Continue;
        }
        if !self.referenced_manifests().0.contains(&hash) {
            return Flow::Continue;
        }
        let stored = {
            let mut ct = lock(&self.content);
            ct.wanted_manifests.remove(&hash);
            ct.store.put_manifest(hash, &bytes)
        };
        match stored {
            Ok(true) => self.debug(format!("manifesto {hash} hospedado")),
            Ok(false) => self.debug("manifesto recusado (cota)"),
            Err(e) => self.info(format!("falha ao gravar manifesto: {e}")),
        }
        let _ = ctx;
        Flow::Continue
    }

    fn handle_get_content(&self, ctx: &mut ConnCtx, id: Hash32) -> Flow {
        let info = lock(&self.content).store.info(&id);
        let missing = info.is_none();
        self.send_to(ctx.id, Message::ContentInfo { id, info });
        // Não hospedado ainda: procura nos pares (replicação sob demanda).
        if missing && self.content_wanted_from(ctx.id, id) {
            self.broadcast(&Message::GetContent(id), Some(ctx.id));
        }
        Flow::Continue
    }

    /// Descrição recebida: resposta a um pedido nosso (pede os pedaços) ou
    /// início de um envio para hospedagem (aguarda os pedaços).
    fn handle_content_info(
        &self,
        ctx: &mut ConnCtx,
        id: Hash32,
        info: Option<rz_core::content::ContentInfo>,
    ) -> Flow {
        let Some(info) = info else {
            return Flow::Continue;
        };
        if info.id() != id {
            return Flow::Penalize(Offense::ProtocolViolation);
        }
        let requested = {
            let mut ct = lock(&self.content);
            ct.expire();
            if ct.store.has(&id) || ct.downloads.contains_key(&id) {
                return Flow::Continue;
            }
            ct.wanted.remove(&id).is_some()
        };
        if !requested && !self.content_referenced(&id) {
            self.send_to(ctx.id, Message::ContentInfo { id, info: None });
            return Flow::Continue;
        }
        let accepted = {
            let mut ct = lock(&self.content);
            let room = ct.store.fits(info.len.saturating_add(ct.reserved()));
            if room
                && ct.downloads.len() < MAX_DOWNLOADS
                && ct.downloads_by(ctx.id) == 0
                && !ct.downloads.contains_key(&id)
            {
                ct.downloads.insert(id, Download::new(info.clone(), ctx.id));
                true
            } else {
                false
            }
        };
        if !accepted {
            self.send_to(ctx.id, Message::ContentInfo { id, info: None });
            return Flow::Continue;
        }
        if requested {
            for index in 0..info.chunks.len() as u32 {
                self.send_to(ctx.id, Message::GetChunk { id, index });
            }
        }
        Flow::Continue
    }

    fn handle_chunk(
        &self,
        ctx: &mut ConnCtx,
        id: Hash32,
        index: u32,
        data: Option<Vec<u8>>,
    ) -> Flow {
        let finished = {
            let mut ct = lock(&self.content);
            let Some(d) = ct.downloads.get_mut(&id) else {
                return Flow::Continue;
            };
            if d.source != ctx.id {
                return Flow::Continue;
            }
            let Some(data) = data else {
                // A origem não tem o pedaço: abandona o download.
                ct.downloads.remove(&id);
                return Flow::Continue;
            };
            let i = index as usize;
            if !d.info.check_chunk(i, &data) {
                ct.downloads.remove(&id);
                return Flow::Penalize(Offense::ProtocolViolation);
            }
            d.chunks[i] = Some(data);
            d.last_progress = Instant::now();
            if !d.complete() {
                return Flow::Continue;
            }
            let Some(d) = ct.downloads.remove(&id) else {
                return Flow::Continue;
            };
            let bytes = d.assemble();
            ct.store.put_object(d.info, &bytes)
        };
        match finished {
            Ok(true) => {
                self.debug(format!("conteúdo {id} hospedado"));
                self.send_to(ctx.id, Message::HaveContent(id));
                self.broadcast(&Message::HaveContent(id), Some(ctx.id));
            }
            Ok(false) => self.send_to(ctx.id, Message::ContentInfo { id, info: None }),
            Err(e) => self.info(format!("falha ao gravar conteúdo: {e}")),
        }
        Flow::Continue
    }

    // ------------------------------------------------ transações (Dandelion++)

    /// Transação recebida pela mensagem `TRANSACTION`.
    ///
    /// * De um cliente (Wallet): entra na **haste** (Dandelion++), para que a
    ///   origem não seja o primeiro Node a difundi-la (REQ-026).
    /// * De um par: fase de **flor** — mempool e difusão.
    fn handle_tx(&self, ctx: &mut ConnCtx, tx: Transaction) -> Flow {
        if !ctx.relay {
            return self.originate_stem(ctx, tx);
        }
        lock(&self.stem).pool.remove(&tx.id());
        match self.fluff(tx, Some(ctx.id)) {
            Ok(()) => Flow::Continue,
            Err(e) => penalty(&e),
        }
    }

    fn originate_stem(&self, ctx: &mut ConnCtx, tx: Transaction) -> Flow {
        let id = tx.id();
        match self.check_executable(&tx) {
            Ok(()) => {
                self.send_to(
                    ctx.id,
                    Message::TxResult {
                        id,
                        accepted: true,
                        reason: String::new(),
                    },
                );
                self.debug(format!("transação {id} entrou na haste"));
                self.stem_forward(tx, None);
                Flow::Continue
            }
            Err(e) => {
                self.send_to(
                    ctx.id,
                    Message::TxResult {
                        id,
                        accepted: false,
                        reason: e.to_string(),
                    },
                );
                penalty(&e)
            }
        }
    }

    /// Transação recebida na fase de haste.
    fn handle_stem(&self, ctx: &mut ConnCtx, tx: Transaction) -> Flow {
        let id = tx.id();
        if lock(&self.core).mempool.contains(&id) {
            return Flow::Continue;
        }
        // Ciclo na haste: difunde imediatamente em vez de esperar o embargo.
        if lock(&self.stem).pool.remove(&id).is_some() {
            let _ = self.fluff(tx, None);
            return Flow::Continue;
        }
        if let Err(e) = self.check_executable(&tx) {
            return penalty(&e);
        }
        if random_below(FLUFF_ONE_IN) == 0 {
            let _ = self.fluff(tx, None);
        } else {
            self.stem_forward(tx, Some(ctx.id));
        }
        Flow::Continue
    }

    /// A transação é executável agora sobre a ponta (sem inseri-la).
    fn check_executable(&self, tx: &Transaction) -> Result<(), MempoolError> {
        let c = lock(&self.core);
        if c.mempool.contains(&tx.id()) {
            return Err(MempoolError::Duplicate);
        }
        c.chain
            .state()
            .check_transaction(tx, &ExecParams::at(&self.genesis, c.chain.height() + 1))
            .map_err(MempoolError::Invalid)
    }

    /// Encaminha pela haste ao relay da época; sem relay disponível, difunde.
    fn stem_forward(&self, tx: Transaction, from: Option<u64>) {
        match self.stem_relay(from) {
            Some(relay) => {
                let deadline = Instant::now()
                    + STEM_EMBARGO_MIN
                    + Duration::from_millis(random_below(STEM_EMBARGO_JITTER_MS));
                {
                    let mut st = lock(&self.stem);
                    if st.pool.len() >= MAX_STEM_POOL {
                        return;
                    }
                    st.pool.insert(tx.id(), (tx.clone(), deadline));
                }
                self.send_to(relay, Message::StemTransaction(tx));
            }
            None => {
                let _ = self.fluff(tx, from);
            }
        }
    }

    /// Relay da haste: um par escolhido aleatoriamente e mantido durante a
    /// época, o que dificulta inferir a origem por observação repetida.
    fn stem_relay(&self, exclude: Option<u64>) -> Option<u64> {
        let relays: Vec<u64> = {
            let p = lock(&self.peers);
            let mut v: Vec<u64> = p
                .conns
                .iter()
                .filter(|(id, c)| c.relay && Some(**id) != exclude)
                .map(|(id, _)| *id)
                .collect();
            v.sort_unstable();
            v
        };
        if relays.is_empty() {
            return None;
        }
        let mut st = lock(&self.stem);
        let expired = st.chosen_at.elapsed() >= STEM_EPOCH;
        match st.relay {
            Some(r) if !expired && relays.contains(&r) => Some(r),
            _ => {
                let r = relays[random_below(relays.len() as u64) as usize];
                st.relay = Some(r);
                st.chosen_at = Instant::now();
                Some(r)
            }
        }
    }

    /// Fase de flor: insere no mempool e difunde a todos os pares.
    fn fluff(&self, tx: Transaction, except: Option<u64>) -> Result<(), MempoolError> {
        let id = tx.id();
        {
            let mut guard = lock(&self.core);
            let Core { chain, mempool, .. } = &mut *guard;
            let state = chain.state();
            let params = ExecParams::at(&self.genesis, chain.height() + 1);
            mempool.insert(tx.clone(), &state, &params)?;
        }
        self.debug(format!("transação {id} difundida"));
        self.broadcast(&Message::Transaction(tx), except);
        Ok(())
    }

    /// Difunde transações cuja haste não terminou dentro do embargo — garante
    /// entrega mesmo que um Node da haste descarte a transação.
    fn check_embargo(&self) {
        let now = Instant::now();
        let expired: Vec<Transaction> = {
            let mut st = lock(&self.stem);
            let ids: Vec<TxId> = st
                .pool
                .iter()
                .filter(|(_, (_, deadline))| *deadline <= now)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| st.pool.remove(&id).map(|(tx, _)| tx))
                .collect()
        };
        for tx in expired {
            let _ = self.fluff(tx, None);
        }
    }

    fn handle_block(&self, ctx: &mut ConnCtx, cb: CommittedBlock) -> Flow {
        ctx.peer_height = ctx.peer_height.max(cb.block.header.height);
        match self.commit_block(cb.clone()) {
            Ok(true) => {
                self.broadcast(&Message::Block(Box::new(cb)), Some(ctx.id));
                let _ = self.consensus_tx.try_send(ConsensusEvent::ChainAdvanced);
                Flow::Continue
            }
            Ok(false) => Flow::Continue,
            Err(ChainError::NotNext { expected, got }) if got > expected => {
                self.request_blocks(ctx, expected);
                Flow::Continue
            }
            Err(ChainError::NotNext { .. }) => Flow::Continue,
            Err(e) => {
                self.debug(format!("bloco inválido recebido: {e}"));
                Flow::Penalize(Offense::InvalidBlock)
            }
        }
    }

    fn handle_blocks(&self, ctx: &mut ConnCtx, blocks: Vec<CommittedBlock>) -> Flow {
        let mut imported = false;
        for cb in blocks {
            ctx.peer_height = ctx.peer_height.max(cb.block.header.height);
            match self.commit_block(cb) {
                Ok(new) => imported |= new,
                Err(ChainError::NotNext { .. }) => {}
                Err(e) => {
                    self.debug(format!("bloco inválido na sincronização: {e}"));
                    return Flow::Penalize(Offense::InvalidBlock);
                }
            }
        }
        let height = lock(&self.core).chain.height();
        if imported {
            let _ = self.consensus_tx.try_send(ConsensusEvent::ChainAdvanced);
            if ctx.peer_height > height {
                self.request_blocks(ctx, height + 1);
            }
        }
        Flow::Continue
    }

    fn blocks_from(&self, from: u64, max: u32) -> Vec<CommittedBlock> {
        let c = lock(&self.core);
        let max = (max as usize).min(MAX_BLOCKS_PER_MSG);
        let mut out = Vec::new();
        let mut bytes = 0usize;
        let mut h = from.max(1);
        while out.len() < max {
            let Some(b) = c.chain.committed_at(h) else {
                break;
            };
            bytes += b.to_canonical_bytes().len();
            if bytes > MAX_BLOCKS_RESPONSE_BYTES && !out.is_empty() {
                break;
            }
            out.push(b.clone());
            h += 1;
        }
        out
    }
}

/// Serviços do Node para a máquina de consenso.
struct NodeApp<'a> {
    core: &'a Core,
    genesis: &'a Genesis,
    key: Option<&'a SecretKey>,
}

impl<'a> NodeApp<'a> {
    fn new(core: &'a Core, genesis: &'a Genesis, key: Option<&'a SecretKey>) -> Self {
        Self { core, genesis, key }
    }
}

impl App for NodeApp<'_> {
    fn build_block(&mut self, height: u64, round: u32) -> Option<Block> {
        let key = self.key?;
        let chain = &self.core.chain;
        let state = chain.state();
        let params = ExecParams::at(self.genesis, height);
        let txs = self
            .core
            .mempool
            .select(&state, &params, state.params().max_block_txs as usize);
        Block::build(
            self.genesis,
            chain.tip(),
            chain.height(),
            &state,
            u64::from(round),
            txs,
            key,
        )
        .ok()
        .map(|(b, _)| b)
    }

    fn validate_block(&mut self, block: &Block) -> bool {
        self.core.chain.check_next(block).is_ok()
    }
}

/// Penalidade para rejeições que um par honesto nunca produz.
fn penalty(e: &MempoolError) -> Flow {
    match e {
        MempoolError::Invalid(
            TxError::Signature
            | TxError::UnsupportedVersion(_)
            | TxError::ZeroAmount
            | TxError::RingSignature
            | TxError::RangeProof
            | TxError::Excess
            | TxError::Balance,
        ) => Flow::Penalize(Offense::InvalidTransaction),
        _ => Flow::Continue,
    }
}

/// Inteiro uniforme em `[0, n)` a partir da entropia do sistema.
fn random_below(n: u64) -> u64 {
    let n = n.max(1);
    let zone = u64::MAX - (u64::MAX % n);
    loop {
        let v = OsRng.next_u64();
        if v < zone {
            return v % n;
        }
    }
}

/// Carrega a chave de identidade do Node, ou cria uma nova.
///
/// A identidade só é revelada a quem se conecta a este Node (lado que aceita
/// conexões); um Node privado (`--no-listen`) nunca a expõe.
fn load_or_create_node_key(dir: &std::path::Path) -> io::Result<SecretKey> {
    let path = dir.join("node.key");
    match rz_crypto::keyfile::load(&path) {
        Ok(k) => Ok(k),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let k = SecretKey::generate();
            rz_crypto::keyfile::save(&path, &k)?;
            Ok(k)
        }
        Err(e) => Err(e),
    }
}

fn frame_io(e: FrameError) -> io::Error {
    match e {
        FrameError::Io(e) => e,
        other => io::Error::other(other.to_string()),
    }
}
