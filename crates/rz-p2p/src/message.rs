//! Mensagens P2P (`SPEC §23–§25`).

use std::fmt;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_core::governance::{LockEntry, ProposalSummary, ProtocolParams};
use rz_core::limits::MAX_NETWORK_ID_LEN;
use rz_core::market::{AssetId, AssetView, BookLevel, Order, MAX_ASSETS};
use rz_core::{BlockId, CommittedBlock, Proposal, Transaction, TxId, Vote};
use rz_crypto::{Address, Hash32};
use rz_privacy::note::OutputData;

use crate::P2P_VERSION;

/// Máximo de blocos em uma resposta `Blocks`.
pub const MAX_BLOCKS_PER_MSG: usize = 64;
/// Máximo de endereços em uma resposta `Peers`.
pub const MAX_PEERS_PER_MSG: usize = 32;
/// Máximo de saídas privadas em uma resposta `Outputs`.
pub const MAX_OUTPUTS_PER_MSG: usize = 4096;
/// Máximo de imagens de chave em uma resposta `KeyImages`.
pub const MAX_KEY_IMAGES_PER_MSG: usize = 8192;
/// Máximo de propostas em uma resposta `Governance`.
pub const MAX_PROPOSALS_PER_MSG: usize = 256;
/// Máximo de bloqueios em uma resposta `Governance`.
pub const MAX_LOCKS_PER_MSG: usize = 1024;
const MAX_REASON_LEN: usize = 256;
/// Níveis de preço por lado numa resposta `MARKET`.
pub const MAX_BOOK_LEVELS: usize = 256;
/// Ordens próprias numa resposta `MARKET`.
pub const MAX_OWN_ORDERS: usize = 256;

/// Comprimento máximo de um nome de host (DNS).
pub const MAX_HOST_LEN: usize = 253;

/// Endereço de par: IP + porta, ou nome de host + porta.
///
/// Nomes de host permitem endereços de serviços onion (`….onion`), que só
/// são alcançáveis através de um proxy Tor e não revelam o IP do Node
/// (ADR-0011).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PeerAddr {
    Ip(SocketAddr),
    Host { host: String, port: u16 },
}

impl PeerAddr {
    pub fn port(&self) -> u16 {
        match self {
            PeerAddr::Ip(a) => a.port(),
            PeerAddr::Host { port, .. } => *port,
        }
    }

    pub fn is_loopback(&self) -> bool {
        match self {
            PeerAddr::Ip(a) => a.ip().is_loopback(),
            PeerAddr::Host { host, .. } => host == "localhost",
        }
    }

    pub fn is_onion(&self) -> bool {
        matches!(self, PeerAddr::Host { host, .. } if host.ends_with(".onion"))
    }

    fn valid_host(host: &str) -> bool {
        !host.is_empty()
            && host.len() <= MAX_HOST_LEN
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            && !host.starts_with('.')
    }
}

impl std::fmt::Display for PeerAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerAddr::Ip(a) => write!(f, "{a}"),
            PeerAddr::Host { host, port } => write!(f, "{host}:{port}"),
        }
    }
}

impl std::str::FromStr for PeerAddr {
    type Err = &'static str;

    /// Aceita `1.2.3.4:7100`, `[::1]:7100` ou `nome.onion:7100`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(a) = s.parse::<SocketAddr>() {
            return Ok(PeerAddr::Ip(a));
        }
        let (host, port) = s.rsplit_once(':').ok_or("endereço sem porta")?;
        let port: u16 = port.parse().map_err(|_| "porta inválida")?;
        let host = host.to_ascii_lowercase();
        if !Self::valid_host(&host) {
            return Err("nome de host inválido");
        }
        Ok(PeerAddr::Host { host, port })
    }
}

impl From<SocketAddr> for PeerAddr {
    fn from(a: SocketAddr) -> Self {
        PeerAddr::Ip(a)
    }
}

impl Encode for PeerAddr {
    fn encode(&self, e: &mut Encoder) {
        match self {
            PeerAddr::Ip(a) => {
                let ip = match a.ip() {
                    IpAddr::V4(v4) => v4.to_ipv6_mapped(),
                    IpAddr::V6(v6) => v6,
                };
                e.u8(0).fixed(&ip.octets()).u16(a.port());
            }
            PeerAddr::Host { host, port } => {
                e.u8(1).str(host).u16(*port);
            }
        }
    }
}

impl Decode for PeerAddr {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => {
                let v6 = Ipv6Addr::from(d.fixed::<16>()?);
                let port = d.u16()?;
                let ip = match v6.to_ipv4_mapped() {
                    Some(v4) => IpAddr::V4(v4),
                    None => IpAddr::V6(v6),
                };
                Ok(PeerAddr::Ip(SocketAddr::new(ip, port)))
            }
            1 => {
                let host = d.str(MAX_HOST_LEN)?;
                if !PeerAddr::valid_host(&host) {
                    return Err(DecodeError::InvalidValue("nome de host"));
                }
                Ok(PeerAddr::Host {
                    host,
                    port: d.u16()?,
                })
            }
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Primeira mensagem de toda conexão (`SPEC §24`), já dentro do canal
/// cifrado (ADR-0011).
///
/// Contém apenas o necessário para verificar compatibilidade: sem versão de
/// software, sistema operacional, fuso horário ou identificadores
/// persistentes (THR-PRIV-003). Uma Wallet envia `listen_port = 0`,
/// `relay = false` e nenhum endereço.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub p2p_version: u16,
    pub network_id: String,
    pub genesis: Hash32,
    pub height: u64,
    /// Porta em que o remetente aceita conexões (0 = não aceita).
    pub listen_port: u16,
    /// O remetente quer receber propagação de blocos e transações (Nodes,
    /// inclusive Nodes privados que não aceitam conexões).
    pub relay: bool,
    /// Endereço público anunciado (ex.: serviço onion). Quando presente, é
    /// usado no lugar do IP observado da conexão.
    pub advertise: Option<PeerAddr>,
}

impl Encode for Hello {
    fn encode(&self, e: &mut Encoder) {
        e.u16(self.p2p_version)
            .str(&self.network_id)
            .put(&self.genesis)
            .u64(self.height)
            .u16(self.listen_port)
            .bool(self.relay)
            .option(&self.advertise);
    }
}

impl Decode for Hello {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            p2p_version: d.u16()?,
            network_id: d.str(MAX_NETWORK_ID_LEN)?,
            genesis: d.get()?,
            height: d.u64()?,
            listen_port: d.u16()?,
            relay: d.bool()?,
            advertise: d.option()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelloError {
    Version { ours: u16, theirs: u16 },
    Network,
    Genesis,
}

impl fmt::Display for HelloError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version { ours, theirs } => {
                write!(f, "versão P2P incompatível: local {ours}, remota {theirs}")
            }
            Self::Network => write!(f, "rede diferente"),
            Self::Genesis => write!(f, "genesis diferente"),
        }
    }
}

impl std::error::Error for HelloError {}

/// Verifica a compatibilidade do HELLO recebido (`AT-P2P-002`, `AT-GEN-002`).
pub fn check_hello(theirs: &Hello, network_id: &str, genesis: &Hash32) -> Result<(), HelloError> {
    if theirs.p2p_version != P2P_VERSION {
        return Err(HelloError::Version {
            ours: P2P_VERSION,
            theirs: theirs.p2p_version,
        });
    }
    if theirs.network_id != network_id {
        return Err(HelloError::Network);
    }
    if &theirs.genesis != genesis {
        return Err(HelloError::Genesis);
    }
    Ok(())
}

/// Mensagens do protocolo. A tag `u8` precede o conteúdo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// `0x00`
    Hello(Hello),
    /// `0x01`
    Ping(u64),
    /// `0x02`
    Pong(u64),
    /// `0x03` — solicita endereços conhecidos.
    GetPeers,
    /// `0x04`
    Peers(Vec<PeerAddr>),
    /// `0x05` — propagação de transação.
    Transaction(Transaction),
    /// `0x06` — propagação de bloco finalizado (com certificado).
    Block(Box<CommittedBlock>),
    /// `0x07` — solicita blocos da cadeia preferida a partir de uma altura.
    GetBlocks { from_height: u64, max: u32 },
    /// `0x08` — blocos finalizados consecutivos.
    Blocks(Vec<CommittedBlock>),
    /// `0x09` — consulta de conta (usada por Wallets).
    GetAccount(Address),
    /// `0x0a`
    Account {
        address: Address,
        balance: u64,
        nonce: u64,
        height: u64,
    },
    /// `0x0b` — resposta a uma `Transaction` enviada por cliente.
    TxResult {
        id: TxId,
        accepted: bool,
        reason: String,
    },
    /// `0x0c`
    GetStatus,
    /// `0x0d`
    Status {
        height: u64,
        tip: BlockId,
        finalized_height: u64,
        peers: u32,
        mempool: u32,
    },
    /// `0x0e` — rejeição seguida de encerramento da conexão.
    Reject(String),
    /// `0x0f` — solicita saídas privadas a partir de um índice global.
    GetOutputs { from: u64, max: u32 },
    /// `0x10`
    Outputs {
        start: u64,
        outputs: Vec<OutputData>,
    },
    /// `0x11` — solicita imagens de chave gastas a partir de uma posição.
    GetKeyImages { from: u64, max: u32 },
    /// `0x12`
    KeyImages { start: u64, images: Vec<[u8; 32]> },
    /// `0x13` — transação em fase de haste (Dandelion++, `spec/P2P.md §4`).
    StemTransaction(Transaction),
    /// `0x14` — consulta de governança; `address` filtra os bloqueios.
    GetGovernance { address: Option<Address> },
    /// `0x15`
    Governance {
        height: u64,
        params: ProtocolParams,
        proposals: Vec<ProposalSummary>,
        locks: Vec<LockEntry>,
    },
    /// `0x16` — proposta de bloco (consenso Zero-BFT).
    ConsensusProposal(Box<Proposal>),
    /// `0x17` — voto de consenso.
    ConsensusVote(Vote),
    /// `0x18` — consulta de ativos e do Pool; `address` inclui os saldos.
    GetAssets { address: Option<Address> },
    /// `0x19`
    Assets {
        height: u64,
        /// ZERO no Pool permanente.
        pool_zero: u64,
        assets: Vec<AssetView>,
    },
    /// `0x1a` — consulta do livro de um ativo; `owner` inclui suas ordens.
    GetMarket {
        asset: AssetId,
        owner: Option<Address>,
    },
    /// `0x1b`
    Market {
        height: u64,
        asset: AssetId,
        last_price: Option<u64>,
        bids: Vec<BookLevel>,
        asks: Vec<BookLevel>,
        own: Vec<Order>,
    },
}

impl Encode for Message {
    fn encode(&self, e: &mut Encoder) {
        match self {
            Message::Hello(h) => {
                e.u8(0x00).put(h);
            }
            Message::Ping(n) => {
                e.u8(0x01).u64(*n);
            }
            Message::Pong(n) => {
                e.u8(0x02).u64(*n);
            }
            Message::GetPeers => {
                e.u8(0x03);
            }
            Message::Peers(p) => {
                e.u8(0x04).list(p);
            }
            Message::Transaction(tx) => {
                e.u8(0x05).put(tx);
            }
            Message::Block(b) => {
                e.u8(0x06).put(b.as_ref());
            }
            Message::GetBlocks { from_height, max } => {
                e.u8(0x07).u64(*from_height).u32(*max);
            }
            Message::Blocks(bs) => {
                e.u8(0x08).list(bs);
            }
            Message::GetAccount(a) => {
                e.u8(0x09).put(a);
            }
            Message::Account {
                address,
                balance,
                nonce,
                height,
            } => {
                e.u8(0x0a)
                    .put(address)
                    .u64(*balance)
                    .u64(*nonce)
                    .u64(*height);
            }
            Message::TxResult {
                id,
                accepted,
                reason,
            } => {
                e.u8(0x0b).put(id).bool(*accepted).str(reason);
            }
            Message::GetStatus => {
                e.u8(0x0c);
            }
            Message::Status {
                height,
                tip,
                finalized_height,
                peers,
                mempool,
            } => {
                e.u8(0x0d)
                    .u64(*height)
                    .put(tip)
                    .u64(*finalized_height)
                    .u32(*peers)
                    .u32(*mempool);
            }
            Message::Reject(r) => {
                e.u8(0x0e).str(r);
            }
            Message::GetOutputs { from, max } => {
                e.u8(0x0f).u64(*from).u32(*max);
            }
            Message::Outputs { start, outputs } => {
                e.u8(0x10).u64(*start).list(outputs);
            }
            Message::GetKeyImages { from, max } => {
                e.u8(0x11).u64(*from).u32(*max);
            }
            Message::KeyImages { start, images } => {
                e.u8(0x12).u64(*start).list(images);
            }
            Message::StemTransaction(tx) => {
                e.u8(0x13).put(tx);
            }
            Message::GetGovernance { address } => {
                e.u8(0x14).option(address);
            }
            Message::Governance {
                height,
                params,
                proposals,
                locks,
            } => {
                e.u8(0x15)
                    .u64(*height)
                    .put(params)
                    .list(proposals)
                    .list(locks);
            }
            Message::ConsensusProposal(p) => {
                e.u8(0x16).put(p.as_ref());
            }
            Message::ConsensusVote(v) => {
                e.u8(0x17).put(v);
            }
            Message::GetAssets { address } => {
                e.u8(0x18).option(address);
            }
            Message::Assets {
                height,
                pool_zero,
                assets,
            } => {
                e.u8(0x19).u64(*height).u64(*pool_zero).list(assets);
            }
            Message::GetMarket { asset, owner } => {
                e.u8(0x1a).put(asset).option(owner);
            }
            Message::Market {
                height,
                asset,
                last_price,
                bids,
                asks,
                own,
            } => {
                e.u8(0x1b)
                    .u64(*height)
                    .put(asset)
                    .option(last_price)
                    .list(bids)
                    .list(asks)
                    .list(own);
            }
        }
    }
}

impl Decode for Message {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(match d.u8()? {
            0x00 => Message::Hello(d.get()?),
            0x01 => Message::Ping(d.u64()?),
            0x02 => Message::Pong(d.u64()?),
            0x03 => Message::GetPeers,
            0x04 => Message::Peers(d.list(MAX_PEERS_PER_MSG)?),
            0x05 => Message::Transaction(d.get()?),
            0x06 => Message::Block(Box::new(d.get()?)),
            0x07 => Message::GetBlocks {
                from_height: d.u64()?,
                max: d.u32()?,
            },
            0x08 => Message::Blocks(d.list(MAX_BLOCKS_PER_MSG)?),
            0x09 => Message::GetAccount(d.get()?),
            0x0a => Message::Account {
                address: d.get()?,
                balance: d.u64()?,
                nonce: d.u64()?,
                height: d.u64()?,
            },
            0x0b => Message::TxResult {
                id: d.get()?,
                accepted: d.bool()?,
                reason: d.str(MAX_REASON_LEN)?,
            },
            0x0c => Message::GetStatus,
            0x0d => Message::Status {
                height: d.u64()?,
                tip: d.get()?,
                finalized_height: d.u64()?,
                peers: d.u32()?,
                mempool: d.u32()?,
            },
            0x0e => Message::Reject(d.str(MAX_REASON_LEN)?),
            0x0f => Message::GetOutputs {
                from: d.u64()?,
                max: d.u32()?,
            },
            0x10 => Message::Outputs {
                start: d.u64()?,
                outputs: d.list(MAX_OUTPUTS_PER_MSG)?,
            },
            0x11 => Message::GetKeyImages {
                from: d.u64()?,
                max: d.u32()?,
            },
            0x12 => Message::KeyImages {
                start: d.u64()?,
                images: d.list(MAX_KEY_IMAGES_PER_MSG)?,
            },
            0x13 => Message::StemTransaction(d.get()?),
            0x14 => Message::GetGovernance {
                address: d.option()?,
            },
            0x15 => Message::Governance {
                height: d.u64()?,
                params: d.get()?,
                proposals: d.list(MAX_PROPOSALS_PER_MSG)?,
                locks: d.list(MAX_LOCKS_PER_MSG)?,
            },
            0x16 => Message::ConsensusProposal(Box::new(d.get()?)),
            0x17 => Message::ConsensusVote(d.get()?),
            0x18 => Message::GetAssets {
                address: d.option()?,
            },
            0x19 => Message::Assets {
                height: d.u64()?,
                pool_zero: d.u64()?,
                assets: d.list(MAX_ASSETS)?,
            },
            0x1a => Message::GetMarket {
                asset: d.get()?,
                owner: d.option()?,
            },
            0x1b => Message::Market {
                height: d.u64()?,
                asset: d.get()?,
                last_price: d.option()?,
                bids: d.list(MAX_BOOK_LEVELS)?,
                asks: d.list(MAX_BOOK_LEVELS)?,
                own: d.list(MAX_OWN_ORDERS)?,
            },
            t => return Err(DecodeError::InvalidTag(t)),
        })
    }
}

impl Message {
    /// Nome curto para registros locais.
    pub fn kind(&self) -> &'static str {
        match self {
            Message::Hello(_) => "HELLO",
            Message::Ping(_) => "PING",
            Message::Pong(_) => "PONG",
            Message::GetPeers => "GET_PEERS",
            Message::Peers(_) => "PEERS",
            Message::Transaction(_) => "TRANSACTION",
            Message::Block(_) => "BLOCK",
            Message::GetBlocks { .. } => "GET_BLOCKS",
            Message::Blocks(_) => "BLOCKS",
            Message::GetAccount(_) => "GET_ACCOUNT",
            Message::Account { .. } => "ACCOUNT",
            Message::TxResult { .. } => "TX_RESULT",
            Message::GetStatus => "GET_STATUS",
            Message::Status { .. } => "STATUS",
            Message::Reject(_) => "REJECT",
            Message::GetOutputs { .. } => "GET_OUTPUTS",
            Message::Outputs { .. } => "OUTPUTS",
            Message::GetKeyImages { .. } => "GET_KEY_IMAGES",
            Message::KeyImages { .. } => "KEY_IMAGES",
            Message::StemTransaction(_) => "STEM_TRANSACTION",
            Message::GetGovernance { .. } => "GET_GOVERNANCE",
            Message::Governance { .. } => "GOVERNANCE",
            Message::ConsensusProposal(_) => "CONSENSUS_PROPOSAL",
            Message::ConsensusVote(_) => "CONSENSUS_VOTE",
            Message::GetAssets { .. } => "GET_ASSETS",
            Message::Assets { .. } => "ASSETS",
            Message::GetMarket { .. } => "GET_MARKET",
            Message::Market { .. } => "MARKET",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> Hello {
        Hello {
            p2p_version: P2P_VERSION,
            network_id: "rede-zero-devnet-test".into(),
            genesis: Hash32([1; 32]),
            height: 5,
            listen_port: 7000,
            relay: true,
            advertise: Some("abcdef.onion:7100".parse().unwrap()),
        }
    }

    #[test]
    fn roundtrip_all_simple() {
        let msgs = vec![
            Message::Hello(hello()),
            Message::Ping(1),
            Message::Pong(2),
            Message::GetPeers,
            Message::Peers(vec![
                "127.0.0.1:7000".parse().unwrap(),
                "[::1]:7001".parse().unwrap(),
                "exemplo2abc.onion:7100".parse().unwrap(),
            ]),
            Message::GetBlocks {
                from_height: 3,
                max: 10,
            },
            Message::Blocks(vec![]),
            Message::GetAccount(Address(Hash32([3; 32]))),
            Message::Account {
                address: Address(Hash32([3; 32])),
                balance: 10,
                nonce: 2,
                height: 9,
            },
            Message::TxResult {
                id: TxId(Hash32([4; 32])),
                accepted: false,
                reason: "saldo insuficiente".into(),
            },
            Message::GetStatus,
            Message::Status {
                height: 1,
                tip: BlockId(Hash32([5; 32])),
                finalized_height: 0,
                peers: 2,
                mempool: 0,
            },
            Message::Reject("rede diferente".into()),
            Message::GetOutputs { from: 0, max: 10 },
            Message::Outputs {
                start: 3,
                outputs: vec![OutputData {
                    one_time_key: [1; 32],
                    tx_pub: [2; 32],
                    commitment: [3; 32],
                    enc_amount: [4; 8],
                }],
            },
            Message::GetKeyImages { from: 1, max: 5 },
            Message::KeyImages {
                start: 1,
                images: vec![[9; 32]],
            },
        ];
        for m in msgs {
            let bytes = m.to_canonical_bytes();
            assert_eq!(Message::from_canonical_bytes(&bytes).unwrap(), m);
        }
    }

    // AT-P2P-001 — conexão válida
    #[test]
    fn at_p2p_001_compatible_hello() {
        assert!(check_hello(&hello(), "rede-zero-devnet-test", &Hash32([1; 32])).is_ok());
    }

    // AT-P2P-002 — versão incompatível
    #[test]
    fn at_p2p_002_incompatible() {
        let mut h = hello();
        h.p2p_version = 99;
        assert!(matches!(
            check_hello(&h, "rede-zero-devnet-test", &Hash32([1; 32])),
            Err(HelloError::Version { .. })
        ));
        assert_eq!(
            check_hello(&hello(), "rede-zero-mainnet", &Hash32([1; 32])),
            Err(HelloError::Network)
        );
        assert_eq!(
            check_hello(&hello(), "rede-zero-devnet-test", &Hash32([2; 32])),
            Err(HelloError::Genesis)
        );
    }

    #[test]
    fn peer_addr_parsing() {
        assert!(matches!(
            "1.2.3.4:5".parse::<PeerAddr>(),
            Ok(PeerAddr::Ip(_))
        ));
        let onion: PeerAddr = "ABC.onion:7100".parse().unwrap();
        assert!(onion.is_onion());
        assert_eq!(onion.to_string(), "abc.onion:7100");
        assert!("sem-porta".parse::<PeerAddr>().is_err());
        assert!("inv@lido:1".parse::<PeerAddr>().is_err());
        assert!(".comeca-com-ponto:1".parse::<PeerAddr>().is_err());
    }

    #[test]
    fn unknown_tag_rejected() {
        assert_eq!(
            Message::from_canonical_bytes(&[0xff]),
            Err(DecodeError::InvalidTag(0xff))
        );
    }

    #[test]
    fn too_many_peers_rejected() {
        let mut e = Encoder::new();
        e.u8(0x04).u32(MAX_PEERS_PER_MSG as u32 + 1);
        assert!(matches!(
            Message::from_canonical_bytes(&e.into_bytes()),
            Err(DecodeError::LengthExceeded { .. })
        ));
    }
}
