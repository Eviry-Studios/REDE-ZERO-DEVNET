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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rz_chain::{
    Chain, ChainError, ConsensusError, ImportOutcome, Mempool, MempoolError, RoundRobin,
};
use rz_codec::Encode;
use rz_core::state::ExecParams;
use rz_core::{Account, Block, BlockId, Genesis, TxError};
use rz_crypto::{Address, Hash32, SecretKey};
use rz_p2p::{
    check_hello, read_frame, write_frame, FrameError, Hello, Message, Offense, PeerAddr, PeerScore,
    TokenBucket, MAX_BLOCKS_PER_MSG, MAX_KEY_IMAGES_PER_MSG, MAX_OUTPUTS_PER_MSG,
    MAX_PEERS_PER_MSG, P2P_VERSION,
};

use crate::store::BlockStore;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const DIAL_TIMEOUT: Duration = Duration::from_secs(3);
const BAN_DURATION: Duration = Duration::from_secs(600);
const SEND_QUEUE: usize = 1024;
const MAX_BLOCKS_RESPONSE_BYTES: usize = 3 * 1024 * 1024;

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
    pub listen: SocketAddr,
    pub bootstrap: Vec<SocketAddr>,
    pub validator_key: Option<SecretKey>,
    pub max_inbound: usize,
    pub max_outbound: usize,
    pub max_inbound_per_ip: usize,
    pub mempool_capacity: usize,
    pub log: LogLevel,
}

impl NodeConfig {
    pub fn new(genesis: Genesis, data_dir: PathBuf, listen: SocketAddr) -> Self {
        Self {
            genesis,
            data_dir,
            listen,
            bootstrap: Vec::new(),
            validator_key: None,
            max_inbound: 32,
            max_outbound: 8,
            max_inbound_per_ip: 8,
            mempool_capacity: 10_000,
            log: LogLevel::Info,
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

struct Core {
    chain: Chain<RoundRobin>,
    mempool: Mempool,
    store: BlockStore,
}

struct Peer {
    stream: TcpStream,
    tx: SyncSender<Message>,
    outbound_target: Option<SocketAddr>,
    listen: Option<SocketAddr>,
    /// Pares que aceitam conexões recebem propagação; clientes (Wallets) não.
    relay: bool,
}

#[derive(Default)]
struct Peers {
    conns: HashMap<u64, Peer>,
    next_id: u64,
    banned: HashMap<IpAddr, Instant>,
    known: BTreeSet<SocketAddr>,
    dialing: HashSet<SocketAddr>,
}

struct Shared {
    genesis: Genesis,
    genesis_hash: Hash32,
    key: Option<SecretKey>,
    listen_addr: SocketAddr,
    bootstrap: Vec<SocketAddr>,
    max_inbound: usize,
    max_outbound: usize,
    max_inbound_per_ip: usize,
    log_level: LogLevel,
    core: Mutex<Core>,
    peers: Mutex<Peers>,
    shutdown: AtomicBool,
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
        let mut chain = Chain::new(cfg.genesis.clone(), RoundRobin::default())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Reprocessa blocos persistidos, verificando cada um.
        let (store, blocks) = BlockStore::open(&cfg.data_dir)?;
        let total = blocks.len();
        let mut rejected = 0usize;
        for b in blocks {
            if chain.import(b, None).is_err() {
                rejected += 1;
            }
        }

        let listener = TcpListener::bind(cfg.listen)?;
        listener.set_nonblocking(true)?;
        let listen_addr = listener.local_addr()?;

        let shared = Arc::new(Shared {
            genesis_hash,
            key: cfg.validator_key,
            listen_addr,
            bootstrap: cfg.bootstrap,
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
            genesis: cfg.genesis,
        });

        shared.info(format!(
            "rede {} | genesis {} | escutando {} | {} blocos no disco ({} descartados) | altura {}",
            shared.genesis.network_id,
            genesis_hash,
            listen_addr,
            total,
            rejected,
            lock(&shared.core).chain.height()
        ));
        if let Some(k) = &shared.key {
            shared.info(format!("validador {}", k.public_key()));
        }

        let mut threads = Vec::new();
        {
            let s = shared.clone();
            threads.push(thread::spawn(move || s.accept_loop(listener)));
        }
        {
            let s = shared.clone();
            threads.push(thread::spawn(move || s.dial_loop()));
        }
        if shared.key.is_some() {
            let s = shared.clone();
            threads.push(thread::spawn(move || s.produce_loop()));
        }
        Ok(Node { shared, threads })
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.shared.listen_addr
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
                self.listen_addr.port(),
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
                c.chain.finalized_height(),
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
            listen_port: self.listen_addr.port(),
        }
    }

    // ---------------------------------------------------------------- cadeia

    /// Importa um bloco; retorna `true` se ele era novo.
    fn process_block(&self, block: Block, now: Option<u64>) -> Result<bool, ChainError> {
        let mut guard = lock(&self.core);
        let Core {
            chain,
            mempool,
            store,
        } = &mut *guard;
        let outcome = chain.import(block.clone(), now)?;
        match outcome {
            ImportOutcome::AlreadyKnown => Ok(false),
            ImportOutcome::Stored | ImportOutcome::NewTip { .. } => {
                if let Err(e) = store.append(&block) {
                    self.info(format!("falha ao gravar bloco: {e}"));
                }
                if let ImportOutcome::NewTip { reorg } = outcome {
                    let state = chain.state();
                    mempool.prune(&state, &ExecParams::from_genesis(chain.genesis()));
                    if reorg {
                        self.info(format!(
                            "reorganização: nova ponta {} na altura {}",
                            block.id(),
                            block.header.height
                        ));
                    }
                }
                Ok(true)
            }
        }
    }

    fn produce_loop(self: Arc<Self>) {
        let Some(key) = &self.key else { return };
        let me = key.public_key();
        let mut last_slot: Option<u64> = None;
        while self.running() {
            thread::sleep(Duration::from_millis(50));
            let now = now_ms();
            let Some(slot) = self.genesis.slot_at(now) else {
                continue;
            };
            if last_slot.is_some_and(|s| s >= slot) {
                continue;
            }
            last_slot = Some(slot);
            if self.genesis.proposer_for_slot(slot) != Some(&me) {
                continue;
            }
            let built = {
                let c = lock(&self.core);
                if c.chain.tip_slot().is_some_and(|s| s >= slot) {
                    continue;
                }
                let state = c.chain.state();
                let params = ExecParams::from_genesis(&self.genesis);
                let txs = c
                    .mempool
                    .select(&state, &params, self.genesis.max_block_txs as usize);
                Block::build(
                    &self.genesis,
                    c.chain.tip(),
                    c.chain.height(),
                    &state,
                    slot,
                    txs,
                    key,
                )
            };
            match built {
                Ok((block, _)) => {
                    let (h, n, id) = (block.header.height, block.txs.len(), block.id());
                    match self.process_block(block.clone(), Some(now)) {
                        Ok(true) => {
                            self.info(format!(
                                "bloco produzido: altura {h}, slot {slot}, {n} transações, id {id}"
                            ));
                            self.broadcast(&Message::Block(Box::new(block)), None);
                        }
                        Ok(false) => {}
                        Err(e) => self.info(format!("bloco próprio rejeitado: {e}")),
                    }
                }
                Err(e) => self.info(format!("falha ao montar bloco: {e}")),
            }
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
            let candidates: Vec<SocketAddr> = {
                let mut p = lock(&self.peers);
                let now = Instant::now();
                p.banned.retain(|_, until| *until > now);
                let connected: HashSet<SocketAddr> = p
                    .conns
                    .values()
                    .flat_map(|c| [c.outbound_target, c.listen])
                    .flatten()
                    .collect();
                let outbound = p
                    .conns
                    .values()
                    .filter(|c| c.outbound_target.is_some())
                    .count()
                    + p.dialing.len();
                let slots = self.max_outbound.saturating_sub(outbound);
                let pool: Vec<SocketAddr> = self
                    .bootstrap
                    .iter()
                    .chain(p.known.iter())
                    .copied()
                    .filter(|a| {
                        !self.is_self(a)
                            && !connected.contains(a)
                            && !p.dialing.contains(a)
                            && !p.banned.contains_key(&a.ip())
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .take(slots)
                    .collect();
                for a in &pool {
                    p.dialing.insert(*a);
                }
                pool
            };
            for addr in candidates {
                let s = self.clone();
                thread::spawn(move || {
                    if let Ok(stream) = TcpStream::connect_timeout(&addr, DIAL_TIMEOUT) {
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

    fn is_self(&self, a: &SocketAddr) -> bool {
        a.port() == self.listen_addr.port()
            && (a.ip() == self.listen_addr.ip()
                || a.ip().is_loopback()
                || self.listen_addr.ip().is_unspecified())
    }

    fn run_connection(self: Arc<Self>, stream: TcpStream, outbound: Option<SocketAddr>) {
        let result = self.clone().connection(stream, outbound);
        if let Some(addr) = outbound {
            lock(&self.peers).dialing.remove(&addr);
        }
        if let Err(e) = result {
            self.debug(format!("conexão encerrada: {e}"));
        }
    }

    fn connection(
        self: Arc<Self>,
        mut stream: TcpStream,
        outbound: Option<SocketAddr>,
    ) -> io::Result<()> {
        let remote = stream.peer_addr()?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;

        // Handshake (spec/P2P.md §3).
        write_frame(&mut stream, &Message::Hello(self.hello())).map_err(frame_io)?;
        let hello = match read_frame(&mut stream) {
            Ok(Message::Hello(h)) => h,
            Ok(Message::Reject(r)) => {
                return Err(io::Error::other(format!("rejeitado pelo par: {r}")))
            }
            Ok(_) => {
                let _ = write_frame(&mut stream, &Message::Reject("esperado HELLO".into()));
                return Err(io::Error::other("mensagem antes do HELLO"));
            }
            Err(e) => return Err(frame_io(e)),
        };
        if let Err(e) = check_hello(&hello, &self.genesis.network_id, &self.genesis_hash) {
            let _ = write_frame(&mut stream, &Message::Reject(e.to_string()));
            return Err(io::Error::other(format!("handshake: {e}")));
        }
        stream.set_read_timeout(None)?;

        let listen = match outbound {
            Some(a) => Some(a),
            None if hello.listen_port != 0 => Some(SocketAddr::new(remote.ip(), hello.listen_port)),
            None => None,
        };
        let relay = listen.is_some();

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
                    outbound_target: outbound,
                    listen,
                    relay,
                },
            );
            if let Some(l) = listen {
                if !self.is_self(&l) {
                    p.known.insert(l);
                }
            }
            id
        };
        if outbound.is_some() {
            self.debug(format!("conectado a {remote}"));
        } else {
            self.debug(format!("conexão recebida de {remote}"));
        }

        let mut writer = stream.try_clone()?;
        let writer_thread = thread::spawn(move || {
            for msg in rx {
                if write_frame(&mut writer, &msg).is_err() {
                    break;
                }
            }
            let _ = writer.shutdown(Shutdown::Both);
        });

        let mut ctx = ConnCtx {
            id,
            relay,
            score: PeerScore::default(),
            bucket: TokenBucket::new(400, 200),
            peer_height: hello.height,
            last_request_from: None,
        };
        if relay {
            let _ = tx.try_send(Message::GetPeers);
        }
        self.maybe_sync(&mut ctx);

        let banned = self.read_loop(&mut stream, &mut ctx);

        {
            let mut p = lock(&self.peers);
            p.conns.remove(&id);
            if banned {
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
    fn read_loop(&self, stream: &mut TcpStream, ctx: &mut ConnCtx) -> bool {
        while self.running() {
            let flow = match read_frame(stream) {
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
                    .filter_map(|(_, p)| p.listen)
                    .take(MAX_PEERS_PER_MSG)
                    .map(PeerAddr)
                    .collect();
                self.send_to(ctx.id, Message::Peers(peers));
                Flow::Continue
            }
            Message::Peers(list) => {
                let mut p = lock(&self.peers);
                for PeerAddr(a) in list {
                    if a.port() != 0 && !self.is_self(&a) && p.known.len() < 1024 {
                        p.known.insert(a);
                    }
                }
                Flow::Continue
            }
            Message::Transaction(tx) => self.handle_tx(ctx, tx),
            Message::Block(b) => self.handle_block(ctx, *b),
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
            Message::StemTransaction(tx) => self.handle_tx(ctx, tx),
            Message::Reject(_) => Flow::Disconnect,
            // Respostas não solicitadas são ignoradas.
            Message::Account { .. }
            | Message::TxResult { .. }
            | Message::Status { .. }
            | Message::Outputs { .. }
            | Message::KeyImages { .. } => Flow::Continue,
        }
    }

    fn handle_tx(&self, ctx: &mut ConnCtx, tx: rz_core::Transaction) -> Flow {
        let id = tx.id();
        let result = {
            let mut guard = lock(&self.core);
            let Core { chain, mempool, .. } = &mut *guard;
            let state = chain.state();
            mempool.insert(tx.clone(), &state, &ExecParams::from_genesis(&self.genesis))
        };
        match result {
            Ok(_) => {
                self.debug(format!("transação aceita: {id}"));
                self.broadcast(&Message::Transaction(tx), Some(ctx.id));
                if !ctx.relay {
                    self.send_to(
                        ctx.id,
                        Message::TxResult {
                            id,
                            accepted: true,
                            reason: String::new(),
                        },
                    );
                }
                Flow::Continue
            }
            Err(e) => {
                if !ctx.relay {
                    self.send_to(
                        ctx.id,
                        Message::TxResult {
                            id,
                            accepted: false,
                            reason: e.to_string(),
                        },
                    );
                }
                match e {
                    // Erros que um par honesto nunca produz.
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
        }
    }

    fn handle_block(&self, ctx: &mut ConnCtx, block: Block) -> Flow {
        ctx.peer_height = ctx.peer_height.max(block.header.height);
        match self.process_block(block.clone(), Some(now_ms())) {
            Ok(true) => {
                self.broadcast(&Message::Block(Box::new(block)), Some(ctx.id));
                Flow::Continue
            }
            Ok(false) => Flow::Continue,
            Err(ChainError::UnknownParent(_)) => {
                let fin = lock(&self.core).chain.finalized_height();
                self.request_blocks(ctx, fin + 1);
                Flow::Continue
            }
            Err(e) if benign(&e) => Flow::Continue,
            Err(e) => {
                self.debug(format!("bloco inválido recebido: {e}"));
                Flow::Penalize(Offense::InvalidBlock)
            }
        }
    }

    fn handle_blocks(&self, ctx: &mut ConnCtx, blocks: Vec<Block>) -> Flow {
        let mut imported = false;
        let mut unknown_parent = false;
        for b in blocks {
            ctx.peer_height = ctx.peer_height.max(b.header.height);
            match self.process_block(b, Some(now_ms())) {
                Ok(new) => imported |= new,
                Err(ChainError::UnknownParent(_)) => unknown_parent = true,
                Err(e) if benign(&e) => {}
                Err(e) => {
                    self.debug(format!("bloco inválido na sincronização: {e}"));
                    return Flow::Penalize(Offense::InvalidBlock);
                }
            }
        }
        let (height, fin) = {
            let c = lock(&self.core);
            (c.chain.height(), c.chain.finalized_height())
        };
        if imported && ctx.peer_height > height {
            self.request_blocks(ctx, height + 1);
        } else if unknown_parent && ctx.last_request_from != Some(fin + 1) {
            self.request_blocks(ctx, fin + 1);
        }
        Flow::Continue
    }

    fn blocks_from(&self, from: u64, max: u32) -> Vec<Block> {
        let c = lock(&self.core);
        let max = (max as usize).min(MAX_BLOCKS_PER_MSG);
        let mut out = Vec::new();
        let mut bytes = 0usize;
        let mut h = from.max(1);
        while out.len() < max {
            let Some(b) = c.chain.block_at(h) else { break };
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

/// Rejeições que podem ocorrer com pares honestos (relógios diferentes,
/// ramificações antigas) e por isso não geram penalidade.
fn benign(e: &ChainError) -> bool {
    matches!(
        e,
        ChainError::ConflictsWithFinality
            | ChainError::Consensus(ConsensusError::FutureSlot { .. })
    )
}

fn frame_io(e: FrameError) -> io::Error {
    match e {
        FrameError::Io(e) => e,
        other => io::Error::other(other.to_string()),
    }
}
