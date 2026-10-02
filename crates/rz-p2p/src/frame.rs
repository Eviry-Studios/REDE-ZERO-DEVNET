//! Enquadramento: `u32` big-endian de comprimento ‖ corpo canônico.

use std::fmt;
use std::io::{self, Read, Write};

use rz_codec::{Decode, DecodeError, Encode};

use crate::Message;

/// Tamanho máximo de uma mensagem (4 MiB). Verificado **antes** de ler o
/// corpo (THR-P2P-002, `AT-P2P-004`).
pub const MAX_FRAME_SIZE: u32 = 4 * 1024 * 1024;

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    TooLarge(u32),
    Decode(DecodeError),
    /// Falha de autenticação do canal cifrado (adulteração, repetição).
    Crypto,
    /// Falha no handshake do canal cifrado.
    Handshake(&'static str),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "E/S: {e}"),
            Self::TooLarge(n) => write!(f, "mensagem de {n} bytes excede o limite"),
            Self::Decode(e) => write!(f, "mensagem malformada: {e}"),
            Self::Crypto => write!(f, "falha de autenticação do canal"),
            Self::Handshake(w) => write!(f, "handshake: {w}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub fn write_frame<W: Write>(w: &mut W, msg: &Message) -> Result<(), FrameError> {
    let body = msg.to_canonical_bytes();
    let len = u32::try_from(body.len())
        .ok()
        .filter(|n| *n <= MAX_FRAME_SIZE)
        .ok_or(FrameError::TooLarge(u32::MAX))?;
    let mut buf = Vec::with_capacity(4 + body.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&body);
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

pub fn read_frame<R: Read>(r: &mut R) -> Result<Message, FrameError> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len);
    if len > MAX_FRAME_SIZE {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    Message::from_canonical_bytes(&body).map_err(FrameError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &Message::Ping(7)).unwrap();
        let msg = read_frame(&mut buf.as_slice()).unwrap();
        assert_eq!(msg, Message::Ping(7));
    }

    // AT-P2P-004 — mensagem excessivamente grande
    #[test]
    fn at_p2p_004_oversized_rejected_before_reading() {
        let data = (MAX_FRAME_SIZE + 1).to_be_bytes();
        assert!(matches!(
            read_frame(&mut data.as_slice()),
            Err(FrameError::TooLarge(_))
        ));
    }

    // AT-P2P-003 — mensagem inválida
    #[test]
    fn at_p2p_003_malformed_rejected() {
        let mut data = 2u32.to_be_bytes().to_vec();
        data.extend_from_slice(&[0xee, 0x00]);
        assert!(matches!(
            read_frame(&mut data.as_slice()),
            Err(FrameError::Decode(_))
        ));
    }
}
