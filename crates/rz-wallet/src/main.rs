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

use rz_codec::Decode;
use rz_core::{format_zero, parse_zero, Genesis};
use rz_crypto::{keyfile, Address, SecretKey};
use rz_p2p::{Client, Message};
use rz_privacy::keys::ShieldedAddress;
use rz_wallet::{
    account, connect, private_view, send_private, send_transparent, shield, Destination, Keys,
};

const USAGE: &str = "\
zero-wallet — Wallet da DEVNET da Rede Zero

USO:
  zero-wallet new     --key FILE
  zero-wallet address --key FILE
  zero-wallet balance --genesis FILE --node ADDR (--key FILE | --address HEX)
  zero-wallet shield  --genesis FILE --node ADDR --key FILE --amount ZERO [--to zs…] [--fee ZERO]
  zero-wallet send    --genesis FILE --node ADDR --key FILE --to (zs…|HEX) --amount ZERO [--fee ZERO]
  zero-wallet unshield --genesis FILE --node ADDR --key FILE --to HEX --amount ZERO [--fee ZERO]
  zero-wallet status  --genesis FILE --node ADDR

Endereços:
  zs…  endereço PRIVADO (padrão recomendado): remetente, destinatário e valor ocultos
  HEX  endereço transparente: tudo público (necessário para taxas e validadores)

send para zs… gasta saldo privado. send para HEX gasta saldo transparente.
shield move saldo transparente para privado; unshield faz o inverso.

A privacidade é probabilística e não constitui anonimato absoluto.
AVISO: DEVNET — o ZERO desta rede não possui valor econômico.
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

    fn amount(&self, name: &str) -> Result<u64, String> {
        parse_zero(self.req(name)?).ok_or_else(|| format!("--{name}: valor inválido"))
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
        "shield" => cmd_shield(&args),
        "send" => send(&args),
        "unshield" => unshield(&args),
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

fn load_keys(args: &Args) -> Result<Keys, String> {
    let path = args.req("key")?;
    keyfile::load(Path::new(path))
        .map(Keys::from_secret)
        .map_err(|e| format!("{path}: {e}"))
}

fn open(args: &Args) -> Result<(Client, Genesis), String> {
    let path = args.req("genesis")?;
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let genesis =
        Genesis::from_canonical_bytes(&bytes).map_err(|e| format!("genesis inválido: {e}"))?;
    let node: SocketAddr = args
        .req("node")?
        .parse()
        .map_err(|_| "--node: endereço inválido")?;
    Ok((connect(node, &genesis)?, genesis))
}

fn fee(args: &Args, genesis: &Genesis) -> Result<u64, String> {
    match args.get("fee") {
        Some(f) => parse_zero(f).ok_or_else(|| "--fee: valor inválido".into()),
        None => Ok(genesis.min_fee),
    }
}

fn new(args: &Args) -> Result<(), String> {
    let path = args.req("key")?;
    let sk = SecretKey::generate();
    keyfile::save(Path::new(path), &sk).map_err(|e| format!("{path}: {e}"))?;
    let keys = Keys::from_secret(sk);
    println!("chave criada em {path} (não compartilhe nem versione este arquivo)");
    println!("endereço privado:      {}", keys.shielded_address());
    println!("endereço transparente: {}", keys.address());
    Ok(())
}

fn address(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    println!("privado:      {}", keys.shielded_address());
    println!("transparente: {}", keys.address());
    Ok(())
}

fn balance(args: &Args) -> Result<(), String> {
    if let Some(h) = args.get("address") {
        let addr = Address::from_hex(h).ok_or("--address: endereço transparente inválido")?;
        let (mut client, _) = open(args)?;
        let (bal, nonce, height) = account(&mut client, addr)?;
        println!("endereço:     {addr}");
        println!("transparente: {} ZERO (nonce {nonce})", format_zero(bal));
        println!("altura:       {height}");
        return Ok(());
    }
    let keys = load_keys(args)?;
    let (mut client, _) = open(args)?;
    let (bal, nonce, height) = account(&mut client, keys.address())?;
    let (_, notes) = private_view(&mut client, &keys)?;
    let private: u64 = notes.iter().map(|n| n.note.amount).sum();
    println!(
        "privado:      {} ZERO ({} notas)",
        format_zero(private),
        notes.len()
    );
    println!("transparente: {} ZERO (nonce {nonce})", format_zero(bal));
    println!("altura:       {height}");
    Ok(())
}

fn cmd_shield(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let amount = args.amount("amount")?;
    let to = match args.get("to") {
        Some(s) => ShieldedAddress::decode(s).ok_or("--to: endereço privado inválido")?,
        None => keys.shielded_address(),
    };
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = shield(&mut client, &genesis, &keys, &to, amount, fee)?;
    println!(
        "blindando {} ZERO (valor público nesta etapa; gastos futuros serão privados)",
        format_zero(amount)
    );
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn send(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let amount = args.amount("amount")?;
    let to = Destination::parse(args.req("to")?).ok_or("--to: endereço inválido")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = match to {
        Destination::Shielded(_) => {
            let id = send_private(&mut client, &genesis, &keys, to, amount, fee)?;
            println!("envio PRIVADO de {} ZERO", format_zero(amount));
            id
        }
        Destination::Transparent(a) => {
            eprintln!("aviso: envio transparente — remetente, destinatário e valor ficam públicos");
            send_transparent(&mut client, &genesis, &keys, a, amount, fee)?
        }
    };
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn unshield(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let amount = args.amount("amount")?;
    let to = Address::from_hex(args.req("to")?).ok_or("--to: endereço transparente inválido")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_private(
        &mut client,
        &genesis,
        &keys,
        Destination::Transparent(to),
        amount,
        fee,
    )?;
    eprintln!("aviso: o valor retirado e o destino ficam públicos; a origem permanece oculta");
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn status(args: &Args) -> Result<(), String> {
    let (mut client, genesis) = open(args)?;
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
