//! `zero-wallet` — Wallet de linha de comando da DEVNET.
//!
//! A chave privada nunca sai deste processo: transações são assinadas
//! localmente e apenas a transação assinada é enviada ao Node
//! (`SPEC-WAL-002`, `SPEC-WAL-003`).

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use rz_codec::Decode;
use rz_core::{format_zero, parse_zero, Genesis, TxBody, TxKind};
use rz_crypto::{keyfile, Address, SecretKey};
use rz_p2p::{Client, Message};

const USAGE: &str = "\
zero-wallet — Wallet da DEVNET da Rede Zero

USO:
  zero-wallet new --key FILE
  zero-wallet address --key FILE
  zero-wallet balance --genesis FILE --node ADDR (--key FILE | --address HEX)
  zero-wallet send --genesis FILE --node ADDR --key FILE --to HEX --amount ZERO [--fee ZERO]
  zero-wallet status --genesis FILE --node ADDR

A Wallet verifica que o Node pertence à rede do Genesis informado antes de
assinar qualquer coisa. AVISO: DEVNET — o ZERO desta rede não possui valor.
";

struct Args(HashMap<String, String>);

impl Args {
    fn parse(raw: &[String]) -> Result<Self, String> {
        let mut map = HashMap::new();
        let mut it = raw.iter();
        while let Some(a) = it.next() {
            let name = a
                .strip_prefix("--")
                .ok_or_else(|| format!("argumento inesperado: {a}"))?;
            let value = it
                .next()
                .ok_or_else(|| format!("--{name} requer um valor"))?;
            map.insert(name.to_string(), value.clone());
        }
        Ok(Self(map))
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    fn req(&self, name: &str) -> Result<&str, String> {
        self.get(name)
            .ok_or_else(|| format!("--{name} é obrigatório"))
    }
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = raw.split_first() else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let result = Args::parse(rest).and_then(|args| match cmd.as_str() {
        "new" => new(&args),
        "address" => address(&args),
        "balance" => balance(&args),
        "send" => send(&args),
        "status" => status(&args),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("comando desconhecido: {other}\n\n{USAGE}")),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load_key(args: &Args) -> Result<SecretKey, String> {
    let path = args.req("key")?;
    keyfile::load(Path::new(path)).map_err(|e| format!("{path}: {e}"))
}

fn connect(args: &Args) -> Result<(Client, Genesis), String> {
    let path = args.req("genesis")?;
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let genesis =
        Genesis::from_canonical_bytes(&bytes).map_err(|e| format!("genesis inválido: {e}"))?;
    let node: SocketAddr = args
        .req("node")?
        .parse()
        .map_err(|_| "--node: endereço inválido")?;
    let client = Client::connect(
        node,
        &genesis.network_id,
        genesis.hash(),
        Duration::from_secs(10),
    )
    .map_err(|e| e.to_string())?;
    Ok((client, genesis))
}

fn account(client: &mut Client, address: Address) -> Result<(u64, u64, u64), String> {
    client
        .request(&Message::GetAccount(address), |m| match m {
            Message::Account {
                address: a,
                balance,
                nonce,
                height,
            } if a == address => Some((balance, nonce, height)),
            _ => None,
        })
        .map_err(|e| e.to_string())
}

fn new(args: &Args) -> Result<(), String> {
    let path = args.req("key")?;
    let key = SecretKey::generate();
    keyfile::save(Path::new(path), &key).map_err(|e| format!("{path}: {e}"))?;
    println!("chave criada em {path} (não compartilhe nem versione este arquivo)");
    println!("endereço: {}", key.public_key().address());
    Ok(())
}

fn address(args: &Args) -> Result<(), String> {
    println!("{}", load_key(args)?.public_key().address());
    Ok(())
}

fn balance(args: &Args) -> Result<(), String> {
    let addr = match args.get("address") {
        Some(h) => Address::from_hex(h).ok_or("--address: endereço inválido")?,
        None => load_key(args)?.public_key().address(),
    };
    let (mut client, _) = connect(args)?;
    let (balance, nonce, height) = account(&mut client, addr)?;
    println!("endereço: {addr}");
    println!("saldo:    {} ZERO", format_zero(balance));
    println!("nonce:    {nonce}");
    println!("altura:   {height}");
    Ok(())
}

fn send(args: &Args) -> Result<(), String> {
    let key = load_key(args)?;
    let to = Address::from_hex(args.req("to")?).ok_or("--to: endereço inválido")?;
    let amount = parse_zero(args.req("amount")?).ok_or("--amount: valor inválido")?;
    let (mut client, genesis) = connect(args)?;
    let fee = match args.get("fee") {
        Some(f) => parse_zero(f).ok_or("--fee: valor inválido")?,
        None => genesis.min_fee,
    };
    let (_, nonce, _) = account(&mut client, key.public_key().address())?;

    let tx = TxBody {
        version: rz_core::tx::TX_VERSION,
        sender: key.public_key(),
        nonce,
        fee,
        kind: TxKind::Transfer { to, amount },
    }
    // Assinatura local, vinculada à rede do Genesis informado pelo usuário.
    .sign(&key, &genesis.network_id)
    .map_err(|e| e.to_string())?;
    let id = tx.id();

    println!("enviando {} ZERO para {to}", format_zero(amount));
    println!("taxa:      {} ZERO", format_zero(fee));
    println!("transação: {id}");
    let (accepted, reason) = client
        .request(&Message::Transaction(tx), |m| match m {
            Message::TxResult {
                id: rid,
                accepted,
                reason,
            } if rid == id => Some((accepted, reason)),
            _ => None,
        })
        .map_err(|e| e.to_string())?;
    if accepted {
        println!("aceita pelo node — aguardando inclusão em bloco");
        Ok(())
    } else {
        Err(format!("rejeitada: {reason}"))
    }
}

fn status(args: &Args) -> Result<(), String> {
    let (mut client, genesis) = connect(args)?;
    let s = client
        .request(&Message::GetStatus, |m| match m {
            Message::Status {
                height,
                tip,
                finalized_height,
                peers,
                mempool,
            } => Some((height, tip, finalized_height, peers, mempool)),
            _ => None,
        })
        .map_err(|e| e.to_string())?;
    println!("rede:        {}", genesis.network_id);
    println!("altura:      {}", s.0);
    println!("ponta:       {}", s.1);
    println!("finalizado:  {}", s.2);
    println!("pares:       {}", s.3);
    println!("mempool:     {}", s.4);
    Ok(())
}
