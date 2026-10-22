//! Faucet público da TESTNET (`docs/TESTNET.md`).
//!
//! Protocolo de texto sobre TCP, uma troca por conexão:
//!
//! ```text
//! faucet  → rz-faucet/1 <desafio hex32> <bits>
//! cliente → <network_id> <endereço hex> <nonce>
//! faucet  → ok <txid> | erro <motivo>
//! ```
//!
//! O cliente prova trabalho sobre um desafio novo a cada conexão:
//! `H("rede-zero/faucet-pow/v1", desafio ‖ endereço ‖ u64(nonce))` precisa
//! começar com `bits` bits zero. O faucet limita pedidos por **endereço** e
//! por hora. Nenhum endereço IP é registrado (Manifesto §8): o limite usa só
//! o endereço transparente, que é público.

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rz_codec::Encoder;
use rz_core::Genesis;
use rz_crypto::{hash, Address, Hash32};
use rz_p2p::Client;

use crate::Keys;

/// Contexto da prova de trabalho.
pub const FAUCET_POW: &str = "rede-zero/faucet-pow/v1";
/// Versão do protocolo, na primeira linha do faucet.
pub const PROTOCOL: &str = "rz-faucet/1";
/// Tamanho máximo de uma linha do cliente.
const MAX_LINE: u64 = 256;
/// Conexões simultâneas.
const MAX_CONNECTIONS: usize = 64;
/// Tempo para o cliente responder ao desafio.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(120);

/// Hash da prova de trabalho.
pub fn pow_hash(challenge: &[u8; 32], address: &Address, nonce: u64) -> Hash32 {
    let mut e = Encoder::new();
    e.fixed(challenge).put(address).u64(nonce);
    hash(FAUCET_POW, &e.into_bytes())
}

/// Bits zero no início do hash.
pub fn leading_zero_bits(h: &Hash32) -> u32 {
    let mut n = 0;
    for b in h.0 {
        if b == 0 {
            n += 8;
        } else {
            return n + b.leading_zeros();
        }
    }
    n
}

pub fn check_pow(challenge: &[u8; 32], address: &Address, nonce: u64, bits: u32) -> bool {
    leading_zero_bits(&pow_hash(challenge, address, nonce)) >= bits
}

/// Procura um nonce válido. Com `bits = 22`, são cerca de 4 milhões de
/// hashes: segundos para quem pede uma vez, caro para quem pede milhares.
pub fn solve(challenge: &[u8; 32], address: &Address, bits: u32) -> u64 {
    (0u64..)
        .find(|n| check_pow(challenge, address, *n, bits))
        .unwrap_or(u64::MAX)
}

#[derive(Clone, Debug)]
pub struct FaucetConfig {
    /// Unidades por pedido.
    pub amount: u64,
    /// Intervalo mínimo entre pagamentos ao mesmo endereço.
    pub cooldown: Duration,
    /// Dificuldade da prova de trabalho.
    pub bits: u32,
    /// Pagamentos por hora, no total.
    pub max_per_hour: usize,
    /// Taxa de cada transferência.
    pub fee: u64,
}

/// Limites em memória, só por endereço e por hora.
#[derive(Default)]
pub struct Limits {
    last: HashMap<Address, Instant>,
    recent: VecDeque<Instant>,
}

impl Limits {
    pub fn allow(
        &mut self,
        address: &Address,
        now: Instant,
        cfg: &FaucetConfig,
    ) -> Result<(), String> {
        let hour = Duration::from_secs(3_600);
        while self
            .recent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= hour)
        {
            self.recent.pop_front();
        }
        self.last
            .retain(|_, t| now.duration_since(*t) < cfg.cooldown);
        if self.last.contains_key(address) {
            return Err("endereço já atendido; tente mais tarde".into());
        }
        if self.recent.len() >= cfg.max_per_hour {
            return Err("limite por hora atingido; tente mais tarde".into());
        }
        Ok(())
    }

    pub fn record(&mut self, address: Address, now: Instant) {
        self.last.insert(address, now);
        self.recent.push_back(now);
    }
}

struct Request {
    address: Address,
    reply: mpsc::Sender<Result<String, String>>,
}

/// Envia os pagamentos em sequência. O Node valida a transação contra o
/// estado, então cada pagamento espera o anterior entrar num bloco.
struct Payer {
    client: Client,
    genesis: Genesis,
    keys: Keys,
    next_nonce: Option<u64>,
}

/// Espera máxima pela inclusão do pagamento anterior.
const INCLUSION_WAIT: Duration = Duration::from_secs(60);

impl Payer {
    fn pay(&mut self, to: Address, cfg: &FaucetConfig) -> Result<String, String> {
        let from = crate::account_address(&mut self.client, &self.keys)?;
        let deadline = Instant::now() + INCLUSION_WAIT;
        let (balance, nonce) = loop {
            let (balance, nonce, _) = crate::account(&mut self.client, from)?;
            if self.next_nonce.is_none_or(|n| nonce >= n) {
                break (balance, nonce);
            }
            if Instant::now() >= deadline {
                // O pagamento anterior não entrou: recomeça do estado.
                self.next_nonce = None;
                break (balance, nonce);
            }
            thread::sleep(Duration::from_millis(250));
        };
        if balance < cfg.amount.saturating_add(cfg.fee) {
            return Err("faucet sem saldo".into());
        }
        let tx = rz_core::TxBody {
            version: rz_core::tx::TX_VERSION,
            sender: self.keys.transparent.public_key(),
            nonce,
            fee: cfg.fee,
            kind: rz_core::TxKind::Transfer {
                to,
                amount: cfg.amount,
            },
        }
        .sign(&self.keys.transparent, &self.genesis.network_id)
        .map_err(|e| e.to_string())?;
        let id = crate::submit(&mut self.client, tx)?;
        self.next_nonce = Some(nonce + 1);
        Ok(id.to_string())
    }
}

/// Atende pedidos até o `listener` falhar.
pub fn serve(
    listener: TcpListener,
    genesis: Genesis,
    keys: Keys,
    client: Client,
    cfg: FaucetConfig,
) -> io::Result<()> {
    let (tx, rx) = mpsc::channel::<Request>();
    let payer_cfg = cfg.clone();
    let mut payer = Payer {
        client,
        genesis: genesis.clone(),
        keys,
        next_nonce: None,
    };
    thread::spawn(move || {
        let mut limits = Limits::default();
        for req in rx {
            let now = Instant::now();
            let result = limits
                .allow(&req.address, now, &payer_cfg)
                .and_then(|()| payer.pay(req.address, &payer_cfg));
            if result.is_ok() {
                limits.record(req.address, now);
            }
            let _ = req.reply.send(result);
        }
    });
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            continue;
        }
        active.fetch_add(1, Ordering::SeqCst);
        let (tx, active, network_id, bits) = (
            tx.clone(),
            Arc::clone(&active),
            genesis.network_id.clone(),
            cfg.bits,
        );
        thread::spawn(move || {
            let _ = handle(stream, &tx, &network_id, bits);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}

fn handle(
    mut stream: TcpStream,
    tx: &mpsc::Sender<Request>,
    network_id: &str,
    bits: u32,
) -> io::Result<()> {
    stream.set_read_timeout(Some(CLIENT_TIMEOUT))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let challenge = rz_crypto::random_bytes();
    writeln!(
        stream,
        "{PROTOCOL} {} {bits}",
        rz_crypto::hex::encode(&challenge)
    )?;
    let mut line = String::new();
    BufReader::new((&stream).take(MAX_LINE)).read_line(&mut line)?;
    let answer = match parse_request(&line, network_id) {
        Err(e) => Err(e),
        Ok((address, nonce)) if !check_pow(&challenge, &address, nonce, bits) => {
            Err("prova de trabalho inválida".to_string())
        }
        Ok((address, _)) => {
            let (reply, wait) = mpsc::channel();
            tx.send(Request { address, reply })
                .map_err(|_| io::Error::other("faucet parado"))?;
            wait.recv().unwrap_or_else(|_| Err("faucet parado".into()))
        }
    };
    match answer {
        Ok(id) => writeln!(stream, "ok {id}"),
        Err(e) => writeln!(stream, "erro {e}"),
    }
}

fn parse_request(line: &str, network_id: &str) -> Result<(Address, u64), String> {
    let mut it = line.split_whitespace();
    let (Some(net), Some(addr), Some(nonce), None) = (it.next(), it.next(), it.next(), it.next())
    else {
        return Err("pedido malformado".into());
    };
    if net != network_id {
        return Err("rede diferente".into());
    }
    let address = Address::from_hex(addr).ok_or("endereço inválido")?;
    let nonce = nonce.parse().map_err(|_| "nonce inválido")?;
    Ok((address, nonce))
}

/// Lado do cliente: lê o desafio, prova trabalho e pede ZERO para
/// `address`. Retorna o identificador da transação.
pub fn request(stream: TcpStream, network_id: &str, address: &Address) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut it = line.split_whitespace();
    let (Some(PROTOCOL), Some(ch), Some(bits)) = (it.next(), it.next(), it.next()) else {
        return Err("resposta inesperada do faucet".into());
    };
    let challenge: [u8; 32] = rz_crypto::hex::decode_array(ch).ok_or("desafio inválido")?;
    let bits: u32 = bits.parse().map_err(|_| "dificuldade inválida")?;
    if bits > 32 {
        return Err("dificuldade alta demais".into());
    }
    let nonce = solve(&challenge, address, bits);
    let mut w = stream;
    writeln!(w, "{network_id} {address} {nonce}").map_err(|e| e.to_string())?;
    line.clear();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    match line.trim().split_once(' ') {
        Some(("ok", id)) => Ok(id.to_string()),
        Some(("erro", e)) => Err(e.to_string()),
        _ => Err("resposta inesperada do faucet".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FaucetConfig {
        FaucetConfig {
            amount: 1,
            cooldown: Duration::from_secs(60),
            bits: 8,
            max_per_hour: 2,
            fee: 1,
        }
    }

    #[test]
    fn pow_round_trip() {
        let ch = [7u8; 32];
        let a = Address::from_hex(&"11".repeat(32)).unwrap();
        let n = solve(&ch, &a, 12);
        assert!(check_pow(&ch, &a, n, 12));
        // A prova vale só para o endereço e o desafio de origem.
        let b = Address::from_hex(&"22".repeat(32)).unwrap();
        assert!(!(0..4).all(|k| check_pow(&ch, &b, n + k, 12)));
        assert!(!check_pow(&[8u8; 32], &a, n, 40));
        assert_eq!(leading_zero_bits(&Hash32([0; 32])), 256);
    }

    #[test]
    fn limits_by_address_and_hour() {
        let c = cfg();
        let mut l = Limits::default();
        let t = Instant::now();
        let a = Address::from_hex(&"11".repeat(32)).unwrap();
        let b = Address::from_hex(&"22".repeat(32)).unwrap();
        let d = Address::from_hex(&"33".repeat(32)).unwrap();
        assert!(l.allow(&a, t, &c).is_ok());
        l.record(a, t);
        assert!(l.allow(&a, t, &c).is_err());
        assert!(l.allow(&b, t, &c).is_ok());
        l.record(b, t);
        // Limite por hora: 2.
        assert!(l.allow(&d, t, &c).is_err());
        // Depois do intervalo, o endereço volta a poder pedir.
        let later = t + Duration::from_secs(3_601);
        assert!(l.allow(&a, later, &c).is_ok());
    }

    #[test]
    fn request_parsing() {
        let a = "11".repeat(32);
        assert!(parse_request(&format!("rede-x {a} 5\n"), "rede-x").is_ok());
        assert!(parse_request(&format!("rede-y {a} 5\n"), "rede-x").is_err());
        assert!(parse_request(&format!("rede-x {a}\n"), "rede-x").is_err());
        assert!(parse_request("rede-x zz 5\n", "rede-x").is_err());
    }
}
