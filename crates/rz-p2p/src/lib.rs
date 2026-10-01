//! Protocolo P2P da DEVNET (`spec/P2P.md`, ADR-0007).
//!
//! Este crate define mensagens, enquadramento, handshake e mecanismos
//! locais de proteção (limite de taxa e pontuação). A orquestração de
//! conexões fica no Node.

mod client;
mod frame;
mod message;
mod ratelimit;
mod score;
pub mod secure;
pub mod socks;

pub use client::{Client, ClientError, ConnectOptions};
pub use frame::{read_frame, write_frame, FrameError, MAX_FRAME_SIZE};
pub use message::{
    check_hello, Hello, HelloError, Message, PeerAddr, MAX_BLOCKS_PER_MSG, MAX_BOOK_LEVELS,
    MAX_CREDENTIALS_PER_MSG, MAX_KEY_IMAGES_PER_MSG, MAX_LOCKS_PER_MSG, MAX_OUTPUTS_PER_MSG,
    MAX_OWN_ORDERS, MAX_PEERS_PER_MSG, MAX_PROPOSALS_PER_MSG,
};
pub use ratelimit::TokenBucket;
pub use score::{Offense, PeerScore, BAN_THRESHOLD};

/// Versão do protocolo P2P.
pub const P2P_VERSION: u16 = 6;
