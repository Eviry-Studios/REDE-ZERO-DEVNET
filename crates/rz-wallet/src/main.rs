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
use rz_core::governance::{Category, Choice, ParamChange};
use rz_core::TxKind;
use rz_core::{format_zero, parse_zero, Genesis};
use rz_crypto::Hash32;
use rz_crypto::{keyfile, Address, SecretKey};
use rz_p2p::PeerAddr;
use rz_p2p::{Client, Message};
use rz_privacy::keys::ShieldedAddress;
use rz_wallet::{
    account, connect_to, governance, private_view, send_account, send_private, send_transparent,
    shield, Destination, Keys, NetOptions, PROPOSAL_CONTENT,
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

Governança (ADR-0008) — operações transparentes, por natureza públicas:
  zero-wallet governance --genesis FILE --node ADDR [--key FILE]
  zero-wallet lock    --genesis FILE --node ADDR --key FILE --amount ZERO --blocks N
  zero-wallet unlock  --genesis FILE --node ADDR --key FILE --id N
  zero-wallet propose --genesis FILE --node ADDR --key FILE --category (ordinaria|comunidade|constitucional)
                      (--content ARQUIVO | --content-hash HEX) [--params nome=valor,…] [--release HEX]
  zero-wallet vote    --genesis FILE --node ADDR --key FILE --proposal HEX --choice (sim|nao|abstencao)

Validadores votam na câmara de contribuição usando a própria chave de validador.

Endereços:
  zs…  endereço PRIVADO (padrão recomendado): remetente, destinatário e valor ocultos
  HEX  endereço transparente: tudo público (necessário para taxas e validadores)

send para zs… gasta saldo privado. send para HEX gasta saldo transparente.
shield move saldo transparente para privado; unshield faz o inverso.

Rede (ADR-0011) — válido para todos os comandos que usam --node:
  --node ADDR       IP:porta ou nome.onion:porta
  --proxy ADDR      proxy SOCKS5 (Tor: 127.0.0.1:9050); o node não vê seu IP
  --node-id HEX     identidade esperada do node (impede interceptação)
  --direct          aceita conectar a node remoto SEM proxy (expõe seu IP ao node)
Por padrão, sem --proxy só são aceitos nodes locais (127.0.0.1).

A privacidade é probabilística e não constitui anonimato absoluto.
AVISO: DEVNET — o ZERO desta rede não possui valor econômico.
";

struct Args(HashMap<String, String>);

/// Opções sem valor.
const SWITCHES: &[&str] = &["direct"];

impl Args {
    fn parse(raw: &[String]) -> Result<Self, String> {
        let mut map = HashMap::new();
        let mut it = raw.iter();
        while let Some(a) = it.next() {
            if let Some(sw) = a.strip_prefix("--").filter(|n| SWITCHES.contains(n)) {
                map.insert(sw.to_string(), String::new());
                continue;
            }
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
        "governance" => cmd_governance(&args),
        "lock" => cmd_lock(&args),
        "unlock" => cmd_unlock(&args),
        "propose" => cmd_propose(&args),
        "vote" => cmd_vote(&args),
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
    let node: PeerAddr = args
        .req("node")?
        .parse()
        .map_err(|e| format!("--node: {e}"))?;
    let net = NetOptions {
        proxy: match args.get("proxy") {
            Some(p) => Some(
                p.parse::<SocketAddr>()
                    .map_err(|_| "--proxy: endereço inválido")?,
            ),
            None => None,
        },
        node_id: match args.get("node-id") {
            Some(h) => {
                Some(rz_crypto::NodeId::from_hex(h).ok_or("--node-id: identidade inválida")?)
            }
            None => None,
        },
        allow_direct: args.get("direct").is_some(),
    };
    if node.is_onion() && net.proxy.is_none() {
        return Err("endereços .onion exigem --proxy (Tor)".into());
    }
    Ok((connect_to(&node, &genesis, &net)?, genesis))
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

fn cmd_governance(args: &Args) -> Result<(), String> {
    let keys = match args.get("key") {
        Some(_) => Some(load_keys(args)?),
        None => None,
    };
    let (mut client, _) = open(args)?;
    let v = governance(&mut client, keys.as_ref().map(Keys::address))?;
    let g = &v.params.governance;
    println!("altura: {}", v.height);
    println!("parâmetros vigentes:");
    println!(
        "  min_fee                       {} ZERO",
        format_zero(v.params.min_fee)
    );
    println!("  max_block_txs                 {}", v.params.max_block_txs);
    println!(
        "  gov_deposit                   {} ZERO",
        format_zero(g.deposit)
    );
    println!("  analysis_blocks               {}", g.analysis_blocks);
    println!("  voting_blocks                 {}", g.voting_blocks);
    println!(
        "  ordinary_delay_blocks         {}",
        g.ordinary_delay_blocks
    );
    println!(
        "  constitutional_delay_blocks   {}",
        g.constitutional_delay_blocks
    );
    println!("  lock_min_blocks               {}", g.lock_min_blocks);
    println!("  lock_max_blocks               {}", g.lock_max_blocks);
    println!(
        "  contribution_half_life_blocks {}",
        g.contribution_half_life_blocks
    );
    println!("propostas ({}):", v.proposals.len());
    for p in &v.proposals {
        let phase = if p.status != rz_core::ProposalStatus::Pending {
            p.status.name().to_string()
        } else if v.height < p.voting_start {
            format!("em análise até {}", p.voting_start)
        } else {
            format!("em votação até {}", p.voting_end)
        };
        println!("  {} [{}] {}", p.id, p.category.name(), phase);
        for c in &p.params {
            println!("      {} = {}", c.name(), c.value());
        }
        println!(
            "      conteúdo {} | ativação na altura {}",
            p.content_hash, p.activation_height
        );
        if let Some(t) = &p.tally {
            println!(
                "      econômica  sim {} não {} abst {} de {}",
                t.economic.yes, t.economic.no, t.economic.abstain, t.economic.eligible
            );
            println!(
                "      contribuição sim {} não {} abst {} de {}",
                t.contribution.yes,
                t.contribution.no,
                t.contribution.abstain,
                t.contribution.eligible
            );
        }
    }
    if keys.is_some() {
        println!("seus bloqueios ({}):", v.locks.len());
        for l in &v.locks {
            println!(
                "  #{} {} ZERO, desbloqueio na altura {}",
                l.id,
                format_zero(l.lock.amount),
                l.lock.unlock_height
            );
        }
    }
    Ok(())
}

fn cmd_lock(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let amount = args.amount("amount")?;
    let blocks: u64 = args
        .req("blocks")?
        .parse()
        .map_err(|_| "--blocks: número inválido")?;
    let (mut client, genesis) = open(args)?;
    let (_, _, height) = account(&mut client, keys.address())?;
    let fee = fee(args, &genesis)?;
    // A transação entra, no mínimo, na altura seguinte.
    let unlock_height = height + 1 + blocks;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::LockStake {
            amount,
            unlock_height,
        },
        fee,
    )?;
    eprintln!("aviso: bloqueios de governança são públicos (valor e conta)");
    println!(
        "bloqueando {} ZERO até a altura {unlock_height}",
        format_zero(amount)
    );
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn cmd_unlock(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let lock_id: u64 = args
        .req("id")?
        .parse()
        .map_err(|_| "--id: número inválido")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::Unlock { lock_id },
        fee,
    )?;
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn cmd_propose(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let category = match args.req("category")? {
        "ordinaria" | "ordinária" => Category::Ordinary,
        "comunidade" => Category::Community,
        "constitucional" => Category::Constitutional,
        other => return Err(format!("--category: categoria desconhecida '{other}'")),
    };
    let content_hash = match (args.get("content"), args.get("content-hash")) {
        (Some(path), _) => {
            let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            rz_crypto::hash(PROPOSAL_CONTENT, &bytes)
        }
        (None, Some(h)) => Hash32::from_hex(h).ok_or("--content-hash: hash inválido")?,
        (None, None) => return Err("informe --content ou --content-hash".into()),
    };
    let mut params = Vec::new();
    if let Some(list) = args.get("params") {
        for item in list.split(',').filter(|s| !s.is_empty()) {
            let (name, value) = item
                .split_once('=')
                .ok_or_else(|| format!("--params: esperado nome=valor em '{item}'"))?;
            params.push(
                ParamChange::parse(name.trim(), value.trim())
                    .ok_or_else(|| format!("--params: parâmetro inválido '{item}'"))?,
            );
        }
    }
    let release_id = match args.get("release") {
        Some(h) => Some(Hash32::from_hex(h).ok_or("--release: hash inválido")?),
        None => None,
    };
    let (mut client, genesis) = open(args)?;
    let deposit = governance(&mut client, None)?.params.governance.deposit;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::Propose {
            category,
            content_hash,
            params,
            release_id,
            deposit,
        },
        fee,
    )?;
    println!(
        "proposta {} [{}] — depósito de {} ZERO",
        id.0,
        category.name(),
        format_zero(deposit)
    );
    println!("conteúdo: {content_hash}");
    println!("o depósito é devolvido se a proposta atingir quórum nas duas câmaras");
    Ok(())
}

fn cmd_vote(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let proposal = Hash32::from_hex(args.req("proposal")?).ok_or("--proposal: id inválido")?;
    let choice = Choice::parse(args.req("choice")?).ok_or("--choice: use sim, nao ou abstencao")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::Vote { proposal, choice },
        fee,
    )?;
    eprintln!("aviso: votos são públicos nesta versão (ADR-0008)");
    println!("voto registrado: {id} — aguardando inclusão em bloco");
    Ok(())
}
