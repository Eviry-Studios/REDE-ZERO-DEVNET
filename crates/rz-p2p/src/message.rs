//! Mensagens P2P (`SPEC §23–§25`).

use std::fmt;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_core::limits::MAX_NETWORK_ID_LEN;
use rz_core::{Block, BlockId, Transaction, TxId};
use rz_crypto::{Address, Hash32};

use crate::P2P_VERSION;

/// Máximo de blocos em uma resposta `Blocks`.
pub const MAX_BLOCKS_PER_MSG: usize = 64;
/// Máximo de endereços em uma resposta `Peers`.
pub const MAX_PEERS_PER_MSG: usize = 32;
const MAX_REASON_LEN: usize = 256;

/// Endereço de par: IPv6 (IPv4 mapeado) de 16 bytes + porta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PeerAddr(pub SocketAddr);

impl Encode for PeerAddr {
    fn encode(&self, e: &mut Encoder) {
        let ip = match self.0.ip() {
            IpAddr::V4(v4) => v4.to_ipv6_mapped(),
            IpAddr::V6(v6) => v6,
        };
        e.fixed(&ip.octets()).u16(self.0.port());
    }
}

impl Decode for PeerAddr {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let v6 = Ipv6Addr::from(d.fixed::<16>()?);
        let port = d.u16()?;
        let ip = match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        };
        Ok(Self(SocketAddr::new(ip, port)))
    }
}

/// Primeira mensagem de toda conexão (`SPEC §24`).
///
/// Contém apenas o necessário para verificar compatibilidade: sem versão de
/// software, sistema operacional, fuso horário ou identificadores
/// persistentes (THR-PRIV-003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub p2p_version: u16,
    pub network_id: String,
    pub genesis: Hash32,
    pub height: u64,
    /// Porta em que o remetente aceita conexões (0 = não aceita, ex.: Wallet).
    pub listen_port: u16,
}

impl Encode for Hello {
    fn encode(&self, e: &mut Encoder) {
        e.u16(self.p2p_version)
            .str(&self.network_id)
            .put(&self.genesis)
            .u64(self.height)
            .u16(self.listen_port);
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
    /// `0x06` — propagação de bloco.
    Block(Box<Block>),
    /// `0x07` — solicita blocos da cadeia preferida a partir de uma altura.
    GetBlocks { from_height: u64, max: u32 },
    /// `0x08`
    Blocks(Vec<Block>),
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
                PeerAddr("127.0.0.1:7000".parse().unwrap()),
                PeerAddr("[::1]:7001".parse().unwrap()),
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
