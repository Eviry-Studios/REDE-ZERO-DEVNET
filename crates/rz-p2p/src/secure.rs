//! Canal cifrado e autenticado (ADR-0011, `spec/P2P.md §1`).
//!
//! ```text
//! Iniciador → Respondedor:  MAGIC(4) ‖ e_i(32)
//! Respondedor → Iniciador:  e_r(32) ‖ N(32) ‖ σ(64)
//!     σ = Ed25519(chave_do_node, P2P_HANDSHAKE, network_id, e_i ‖ e_r)
//! k   = X25519(e, E)
//! K→  = H(P2P_KEY_I2R, k ‖ e_i ‖ e_r ‖ N)      K← = H(P2P_KEY_R2I, …)
//! ```
//!
//! * Chaves efêmeras novas a cada conexão: conexões diferentes do mesmo
//!   cliente não são ligáveis pelo canal.
//! * O respondedor (Node) prova posse da sua identidade `N`; o iniciador
//!   (Wallet ou Node) permanece anônimo. Um cliente pode fixar a identidade
//!   esperada para impedir interceptação ativa.
//! * Mensagens: `u32 len(c) ‖ c`, `c = ChaCha20-Poly1305(K, nonce=contador,
//!   u32 len(m) ‖ m ‖ zeros)`, com preenchimento até múltiplo de
//!   [`PAD_TO`] bytes para reduzir análise de tamanho (REQ-025).

use std::io::{Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand_core::OsRng;
use rz_codec::{Decode, Encode};
use rz_crypto::{hash, NodeId, PublicKey, SecretKey, Signature};
use x25519_dalek::{EphemeralSecret, PublicKey as XPublic};

use crate::{FrameError, Message, MAX_FRAME_SIZE};

/// Identifica o protocolo e a versão do handshake.
pub const MAGIC: [u8; 4] = *b"RZ\x00\x02";
/// Granularidade do preenchimento.
pub const PAD_TO: usize = 256;
const TAG_LEN: usize = 16;
const MAX_CIPHERTEXT: usize = MAX_FRAME_SIZE as usize + 4 + PAD_TO + TAG_LEN;

pub mod context {
    pub const P2P_HANDSHAKE: &str = "rede-zero/p2p-handshake/v1";
    pub const P2P_KEY_I2R: &str = "rede-zero/p2p-key-i2r/v1";
    pub const P2P_KEY_R2I: &str = "rede-zero/p2p-key-r2i/v1";
}

struct CipherState {
    cipher: ChaCha20Poly1305,
    counter: u64,
}

impl CipherState {
    fn new(key: [u8; 32]) -> Self {
        Self {
            cipher: ChaCha20Poly1305::new(&key.into()),
            counter: 0,
        }
    }

    fn next_nonce(&mut self) -> Result<Nonce, FrameError> {
        let c = self.counter;
        self.counter = c.checked_add(1).ok_or(FrameError::Crypto)?;
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&c.to_be_bytes());
        Ok(n.into())
    }
}

/// Metade de escrita do canal.
pub struct SecureWriter<W> {
    inner: W,
    state: CipherState,
}

impl<W: Write> SecureWriter<W> {
    pub fn write_message(&mut self, msg: &Message) -> Result<(), FrameError> {
        let body = msg.to_canonical_bytes();
        if body.len() > MAX_FRAME_SIZE as usize {
            return Err(FrameError::TooLarge(u32::MAX));
        }
        let mut plain = Vec::with_capacity(body.len() + 4 + PAD_TO);
        plain.extend_from_slice(&(body.len() as u32).to_be_bytes());
        plain.extend_from_slice(&body);
        let padded = plain.len().div_ceil(PAD_TO) * PAD_TO;
        plain.resize(padded, 0);
        let nonce = self.state.next_nonce()?;
        let ct = self
            .state
            .cipher
            .encrypt(&nonce, plain.as_slice())
            .map_err(|_| FrameError::Crypto)?;
        let mut buf = Vec::with_capacity(4 + ct.len());
        buf.extend_from_slice(&(ct.len() as u32).to_be_bytes());
        buf.extend_from_slice(&ct);
        self.inner.write_all(&buf)?;
        self.inner.flush()?;
        Ok(())
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

/// Metade de leitura do canal.
pub struct SecureReader<R> {
    inner: R,
    state: CipherState,
}

impl<R: Read> SecureReader<R> {
    pub fn read_message(&mut self) -> Result<Message, FrameError> {
        let mut len = [0u8; 4];
        self.inner.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len);
        if len as usize > MAX_CIPHERTEXT {
            return Err(FrameError::TooLarge(len));
        }
        if (len as usize) < TAG_LEN + 4 {
            return Err(FrameError::Crypto);
        }
        let mut ct = vec![0u8; len as usize];
        self.inner.read_exact(&mut ct)?;
        let nonce = self.state.next_nonce()?;
        let plain = self
            .state
            .cipher
            .decrypt(&nonce, ct.as_slice())
            .map_err(|_| FrameError::Crypto)?;
        let mut n = [0u8; 4];
        n.copy_from_slice(&plain[..4]);
        let n = u32::from_be_bytes(n) as usize;
        if n > plain.len() - 4 {
            return Err(FrameError::Crypto);
        }
        Message::from_canonical_bytes(&plain[4..4 + n]).map_err(FrameError::Decode)
    }
}

fn derive(
    shared: &[u8; 32],
    e_i: &[u8; 32],
    e_r: &[u8; 32],
    node: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let mut material = Vec::with_capacity(128);
    material.extend_from_slice(shared);
    material.extend_from_slice(e_i);
    material.extend_from_slice(e_r);
    material.extend_from_slice(node);
    (
        hash(context::P2P_KEY_I2R, &material).0,
        hash(context::P2P_KEY_R2I, &material).0,
    )
}

fn transcript(e_i: &[u8; 32], e_r: &[u8; 32]) -> [u8; 64] {
    let mut t = [0u8; 64];
    t[..32].copy_from_slice(e_i);
    t[32..].copy_from_slice(e_r);
    t
}

/// Lado que abre a conexão (Wallet ou Node discando). Retorna o canal e a
/// identidade comprovada do Node remoto.
pub fn initiate<R: Read, W: Write>(
    mut r: R,
    mut w: W,
    network_id: &str,
    expected: Option<NodeId>,
) -> Result<(SecureReader<R>, SecureWriter<W>, PublicKey), FrameError> {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let e_i = XPublic::from(&secret).to_bytes();
    let mut hello = [0u8; 36];
    hello[..4].copy_from_slice(&MAGIC);
    hello[4..].copy_from_slice(&e_i);
    w.write_all(&hello)?;
    w.flush()?;

    let mut resp = [0u8; 128];
    r.read_exact(&mut resp)?;
    let mut e_r = [0u8; 32];
    let mut node = [0u8; 32];
    let mut sig = [0u8; 64];
    e_r.copy_from_slice(&resp[..32]);
    node.copy_from_slice(&resp[32..64]);
    sig.copy_from_slice(&resp[64..]);

    let node_pk =
        PublicKey::from_bytes(node).map_err(|_| FrameError::Handshake("identidade inválida"))?;
    node_pk
        .verify(
            context::P2P_HANDSHAKE,
            network_id,
            &transcript(&e_i, &e_r),
            &Signature(sig),
        )
        .map_err(|_| FrameError::Handshake("assinatura do node inválida"))?;
    if let Some(exp) = expected {
        if node_pk.node_id() != exp {
            return Err(FrameError::Handshake(
                "identidade do node diferente da esperada",
            ));
        }
    }
    let shared = secret.diffie_hellman(&XPublic::from(e_r));
    if !shared.was_contributory() {
        return Err(FrameError::Handshake("chave efêmera fraca"));
    }
    let (k_i2r, k_r2i) = derive(shared.as_bytes(), &e_i, &e_r, &node);
    Ok((
        SecureReader {
            inner: r,
            state: CipherState::new(k_r2i),
        },
        SecureWriter {
            inner: w,
            state: CipherState::new(k_i2r),
        },
        node_pk,
    ))
}

/// Lado que aceita a conexão (Node), provando posse de `node_key`.
pub fn respond<R: Read, W: Write>(
    mut r: R,
    mut w: W,
    network_id: &str,
    node_key: &SecretKey,
) -> Result<(SecureReader<R>, SecureWriter<W>), FrameError> {
    let mut hello = [0u8; 36];
    r.read_exact(&mut hello)?;
    if hello[..4] != MAGIC {
        return Err(FrameError::Handshake("protocolo ou versão desconhecidos"));
    }
    let mut e_i = [0u8; 32];
    e_i.copy_from_slice(&hello[4..]);

    let secret = EphemeralSecret::random_from_rng(OsRng);
    let e_r = XPublic::from(&secret).to_bytes();
    let node = *node_key.public_key().as_bytes();
    let sig = node_key.sign(context::P2P_HANDSHAKE, network_id, &transcript(&e_i, &e_r));
    let mut resp = [0u8; 128];
    resp[..32].copy_from_slice(&e_r);
    resp[32..64].copy_from_slice(&node);
    resp[64..].copy_from_slice(&sig.0);
    w.write_all(&resp)?;
    w.flush()?;

    let shared = secret.diffie_hellman(&XPublic::from(e_i));
    if !shared.was_contributory() {
        return Err(FrameError::Handshake("chave efêmera fraca"));
    }
    let (k_i2r, k_r2i) = derive(shared.as_bytes(), &e_i, &e_r, &node);
    Ok((
        SecureReader {
            inner: r,
            state: CipherState::new(k_i2r),
        },
        SecureWriter {
            inner: w,
            state: CipherState::new(k_r2i),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    const NET: &str = "rede-zero-devnet-test";

    type Channel = (SecureReader<TcpStream>, SecureWriter<TcpStream>);
    type ClientSide =
        Result<(SecureReader<TcpStream>, SecureWriter<TcpStream>, PublicKey), FrameError>;

    fn pair(
        expected: Option<NodeId>,
    ) -> (ClientSide, thread::JoinHandle<Option<Channel>>, SecretKey) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let node_key = SecretKey::from_seed([5; 32]);
        let nk = SecretKey::from_seed(node_key.seed());
        let server = thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            respond(s.try_clone().unwrap(), s, NET, &nk).ok()
        });
        let c = TcpStream::connect(addr).unwrap();
        let client = initiate(c.try_clone().unwrap(), c, NET, expected);
        (client, server, node_key)
    }

    #[test]
    fn roundtrip_both_directions() {
        let (client, server, node_key) = pair(None);
        let (mut cr, mut cw, node) = client.unwrap();
        assert_eq!(node, node_key.public_key());
        let (mut sr, mut sw) = server.join().unwrap().unwrap();
        cw.write_message(&Message::Ping(1)).unwrap();
        cw.write_message(&Message::Ping(2)).unwrap();
        assert_eq!(sr.read_message().unwrap(), Message::Ping(1));
        assert_eq!(sr.read_message().unwrap(), Message::Ping(2));
        sw.write_message(&Message::Pong(3)).unwrap();
        assert_eq!(cr.read_message().unwrap(), Message::Pong(3));
    }

    #[test]
    fn pinned_identity_enforced() {
        let wrong = SecretKey::from_seed([6; 32]).public_key().node_id();
        let (client, _server, _) = pair(Some(wrong));
        assert!(matches!(client, Err(FrameError::Handshake(_))));
        let right = SecretKey::from_seed([5; 32]).public_key().node_id();
        let (client, _server, _) = pair(Some(right));
        assert!(client.is_ok());
    }

    /// Ciphertext com tamanho fixo por faixa: um PING e uma mensagem de
    /// ~200 bytes produzem quadros do mesmo tamanho.
    #[test]
    fn padding_hides_small_sizes() {
        let key = [9u8; 32];
        let mut out_a = Vec::new();
        let mut out_b = Vec::new();
        let mut wa = SecureWriter {
            inner: &mut out_a,
            state: CipherState::new(key),
        };
        wa.write_message(&Message::Ping(0)).unwrap();
        let mut wb = SecureWriter {
            inner: &mut out_b,
            state: CipherState::new(key),
        };
        wb.write_message(&Message::Reject("x".repeat(200))).unwrap();
        assert_eq!(out_a.len(), out_b.len());
        assert_eq!(out_a.len(), 4 + PAD_TO + TAG_LEN);
    }

    #[test]
    fn tampering_and_replay_detected() {
        let key = [9u8; 32];
        let mut buf = Vec::new();
        {
            let mut w = SecureWriter {
                inner: &mut buf,
                state: CipherState::new(key),
            };
            w.write_message(&Message::Ping(7)).unwrap();
        }
        // Adulteração de um byte do ciphertext.
        let mut bad = buf.clone();
        bad[10] ^= 1;
        let mut r = SecureReader {
            inner: bad.as_slice(),
            state: CipherState::new(key),
        };
        assert!(matches!(r.read_message(), Err(FrameError::Crypto)));
        // Repetição do mesmo quadro: o contador de nonce já avançou.
        let twice: Vec<u8> = buf.iter().chain(buf.iter()).copied().collect();
        let mut r = SecureReader {
            inner: twice.as_slice(),
            state: CipherState::new(key),
        };
        assert_eq!(r.read_message().unwrap(), Message::Ping(7));
        assert!(matches!(r.read_message(), Err(FrameError::Crypto)));
    }

    #[test]
    fn wrong_magic_rejected() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            respond(
                s.try_clone().unwrap(),
                s,
                NET,
                &SecretKey::from_seed([5; 32]),
            )
            .is_err()
        });
        let mut c = TcpStream::connect(addr).unwrap();
        c.write_all(&[0u8; 36]).unwrap();
        assert!(server.join().unwrap());
    }
}
