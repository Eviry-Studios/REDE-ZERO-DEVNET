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

pub use client::{Client, ClientError};
pub use frame::{read_frame, write_frame, FrameError, MAX_FRAME_SIZE};
pub use message::{
    check_hello, Hello, HelloError, Message, PeerAddr, MAX_BLOCKS_PER_MSG, MAX_PEERS_PER_MSG,
};
pub use ratelimit::TokenBucket;
pub use score::{Offense, PeerScore, BAN_THRESHOLD};

/// Versão do protocolo P2P.
pub const P2P_VERSION: u16 = 1;
