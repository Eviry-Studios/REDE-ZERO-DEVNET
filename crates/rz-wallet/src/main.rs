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

use rz_codec::{Decode, Encode};
use rz_core::governance::{Category, Choice, ParamChange};
use rz_core::TxKind;
use rz_core::{format_zero, parse_zero, Genesis};
use rz_crypto::Hash32;
use rz_crypto::{keyfile, Address, SecretKey};
use rz_p2p::PeerAddr;
use rz_p2p::{Client, Message};
use rz_privacy::keys::ShieldedAddress;
use rz_wallet::{
    account, connect_to, format_units, governance, parse_units, price_from_zero_per_unit,
    private_view, send_account, send_private, send_transparent, shield, zero_per_unit, Destination,
    Keys, NetOptions, PROPOSAL_CONTENT,
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

Consenso Zero-BFT (ADR-0012) — vínculo de validador, público:
  zero-wallet bond    --genesis FILE --node ADDR --key FILE --amount ZERO
  zero-wallet unbond  --genesis FILE --node ADDR --key FILE --amount ZERO
  O vínculo entra no conjunto de validadores na próxima época; a desvinculação
  só libera o saldo após o período de desvinculação (e continua punível).

Grande Mercado e Pool permanente (ADR-0014) — operações transparentes:
  zero-wallet assets  --genesis FILE --node ADDR [--key FILE]
  zero-wallet market  --genesis FILE --node ADDR --asset ATIVO [--key FILE]
  zero-wallet order   --genesis FILE --node ADDR --key FILE --asset ATIVO --side (compra|venda)
                      --amount QTD --price ZERO_POR_UNIDADE [--blocks N]
  zero-wallet cancel  --genesis FILE --node ADDR --key FILE --order HEX
  zero-wallet send-asset  --genesis FILE --node ADDR --key FILE --asset ATIVO --to HEX --amount QTD
  zero-wallet pool-deposit --genesis FILE --node ADDR --key FILE --asset (ZERO|ATIVO) --amount QTD
                      --permanente
  ATIVO: identificador HEX ou rede:ativo (ex.: testnet-externa:ATV). Todo par é cotado em ZERO:
  as ordens de todos que cruzam num bloco executam ao mesmo preço (leilão por bloco).
  O depósito no Pool é IRREVERSÍVEL: não existe operação de retirada.

Comunidades e nomes zero:// (ADR-0015):
  zero-wallet community-declare --genesis FILE --node ADDR --key FILE --name NOME
                      (--manifest ARQUIVO | --manifest-hash HEX) --keys HEX,HEX,... --threshold N
  zero-wallet community --genesis FILE --node ADDR --name NOME
  zero-wallet community-approve --genesis FILE --key FILE --community HEX
                      (--proposal HEX --choice (sim|nao|abstencao)
                       | --manifest-hash HEX --version N [--new-keys HEX,... --new-threshold N])
  zero-wallet community-position --genesis FILE --node ADDR --key FILE --community HEX
                      --proposal HEX --choice (sim|nao|abstencao) --approvals HEX,HEX,...
  zero-wallet community-update --genesis FILE --node ADDR --key FILE --community HEX
                      --manifest-hash HEX --version N [--new-keys HEX,... --new-threshold N]
                      --approvals HEX,HEX,...
  zero-wallet name-register --genesis FILE --node ADDR --key FILE --name zero://NOME.TIPO --target HEX
  zero-wallet name-update   --genesis FILE --node ADDR --key FILE --name zero://NOME.TIPO --target HEX
                      [--new-owner HEX]
  zero-wallet resolve --genesis FILE --node ADDR --name zero://NOME.TIPO
Defesa da Exonet (ADR-0016) — decisões exigem atestações de validadores:
  zero-wallet defense --genesis FILE --node ADDR
  zero-wallet defense-attest --genesis FILE --node ADDR --key VALIDADOR.key DECISÃO
  zero-wallet defense-submit --genesis FILE --node ADDR --key FILE DECISÃO --attestations HEX,HEX,...
  zero-wallet defense-action --genesis FILE --node ADDR --key FILE --credential HEX
                      --scope (diagnostico|isolamento|recuperacao|coordenacao|evidencias) --subject HEX
  DECISÃO:
    --decision transicao --to (normal|vigilancia|incidente|guerra) --evidence HEX
    --decision atualizar --status (contido|recuperado) --evidence HEX
    --decision encerrar  --archive HEX
    --decision credencial --holder CHAVE --scopes a,b --expires-at ALTURA
    --decision revogar   --credential HEX
    --decision contribuicao --incident HEX --holder CHAVE --role ESCOPO --evidence HEX
  Subir para vigilância exige > 1/3 do poder; incidente, guerra, encerramento,
  credenciais e contribuições exigem > 2/3; descer e revogar, > 1/3. Todo modo
  vence sozinho sem renovação. Não existe operação ofensiva.

  O reconhecimento de uma Comunidade é uma proposta de categoria comunidade com
  --content-hash igual ao id da declaração. A posição de uma Comunidade é
  registrada, mas não altera o resultado oficial das votações.

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
const SWITCHES: &[&str] = &["direct", "permanente"];

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
        "bond" => cmd_bond(&args, true),
        "assets" => cmd_assets(&args),
        "market" => cmd_market(&args),
        "order" => cmd_order(&args),
        "cancel" => cmd_cancel(&args),
        "send-asset" => cmd_send_asset(&args),
        "pool-deposit" => cmd_pool_deposit(&args),
        "community-declare" => cmd_community_declare(&args),
        "community" => cmd_community(&args),
        "community-approve" => cmd_community_approve(&args),
        "community-position" => cmd_community_position(&args),
        "community-update" => cmd_community_update(&args),
        "name-register" => cmd_name(&args, true),
        "name-update" => cmd_name(&args, false),
        "resolve" => cmd_resolve(&args),
        "defense" => cmd_defense(&args),
        "defense-attest" => cmd_defense_decision(&args, false),
        "defense-submit" => cmd_defense_decision(&args, true),
        "defense-action" => cmd_defense_action(&args),
        "unbond" => cmd_bond(&args, false),
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
    let g = governance(&mut client, None)?.params.governance;
    if blocks < g.lock_min_blocks || blocks > g.lock_max_blocks {
        let block_s = genesis.consensus.block_interval_ms as f64 / 1000.0;
        return Err(format!(
            "--blocks deve estar entre {} e {} blocos (~{:.0} a {:.0} dias nesta rede)",
            g.lock_min_blocks,
            g.lock_max_blocks,
            g.lock_min_blocks as f64 * block_s / 86_400.0,
            g.lock_max_blocks as f64 * block_s / 86_400.0
        ));
    }
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

/// Resolve `--asset` (HEX ou `rede:ativo`) contra os ativos registrados.
fn find_asset(
    client: &mut Client,
    spec: &str,
    address: Option<Address>,
) -> Result<rz_core::market::AssetView, String> {
    let v = rz_wallet::assets(client, address)?;
    let id = match spec.split_once(':') {
        Some((net, r)) => rz_core::market::AssetId::external(net, r),
        None => rz_core::market::AssetId(
            Hash32::from_hex(spec).ok_or_else(|| format!("--asset: ativo inválido '{spec}'"))?,
        ),
    };
    v.assets
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("ativo não registrado: {spec}"))
}

fn cmd_assets(args: &Args) -> Result<(), String> {
    let keys = match args.get("key") {
        Some(_) => Some(load_keys(args)?),
        None => None,
    };
    let (mut client, _) = open(args)?;
    let v = rz_wallet::assets(&mut client, keys.as_ref().map(Keys::address))?;
    println!("altura: {}", v.height);
    println!("Pool permanente: {} ZERO", format_zero(v.pool_zero));
    if v.assets.is_empty() {
        println!("nenhum ativo externo registrado");
    }
    for a in &v.assets {
        let d = a.info.decimals;
        println!("{}:{}  {}", a.info.network, a.info.asset_ref, a.id);
        println!(
            "  verificação: {:?}  oferta: {}  no Pool: {}",
            a.info.verification,
            format_units(a.info.supply, d),
            format_units(a.pool, d)
        );
        if let Some(p) = a.last_price {
            println!(
                "  último preço: {} ZERO por unidade",
                format_zero_u128(zero_per_unit(p, d))
            );
        }
        if let Some(b) = a.balance {
            println!("  seu saldo: {}", format_units(b, d));
        }
    }
    Ok(())
}

fn format_zero_u128(v: u128) -> String {
    u64::try_from(v).map_or_else(|_| "∞".into(), format_zero)
}

fn cmd_market(args: &Args) -> Result<(), String> {
    let keys = match args.get("key") {
        Some(_) => Some(load_keys(args)?),
        None => None,
    };
    let (mut client, _) = open(args)?;
    let a = find_asset(&mut client, args.req("asset")?, None)?;
    let d = a.info.decimals;
    let v = rz_wallet::market(&mut client, a.id, keys.as_ref().map(Keys::address))?;
    println!(
        "{}:{} / ZERO — altura {}",
        a.info.network, a.info.asset_ref, v.height
    );
    if let Some(p) = v.last_price {
        println!(
            "último preço de equilíbrio: {} ZERO",
            format_zero_u128(zero_per_unit(p, d))
        );
    }
    println!("vendas (menor preço primeiro):");
    for l in &v.asks {
        println!(
            "  {:>20} ZERO  {}",
            format_zero_u128(zero_per_unit(l.price, d)),
            format_units(l.amount, d)
        );
    }
    println!("compras (maior preço primeiro):");
    for l in &v.bids {
        println!(
            "  {:>20} ZERO  {}",
            format_zero_u128(zero_per_unit(l.price, d)),
            format_units(l.amount, d)
        );
    }
    if keys.is_some() {
        println!("suas ordens:");
        for o in &v.own {
            println!(
                "  {} {} {} a {} ZERO, vence na altura {}",
                o.id,
                match o.side {
                    rz_core::market::Side::Buy => "compra",
                    rz_core::market::Side::Sell => "venda",
                },
                format_units(o.remaining, d),
                format_zero_u128(zero_per_unit(o.price, d)),
                o.expires_at
            );
        }
    }
    Ok(())
}

fn cmd_order(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let side = match args.req("side")? {
        "compra" | "buy" => rz_core::market::Side::Buy,
        "venda" | "sell" => rz_core::market::Side::Sell,
        other => return Err(format!("--side: use compra ou venda, recebido '{other}'")),
    };
    let (mut client, genesis) = open(args)?;
    let a = find_asset(&mut client, args.req("asset")?, None)?;
    let d = a.info.decimals;
    let amount = parse_units(args.req("amount")?, d).ok_or("--amount: quantidade inválida")?;
    let price = price_from_zero_per_unit(args.amount("price")?, d)
        .ok_or("--price: preço inválido ou pequeno demais")?;
    let (_, _, height) = account(&mut client, keys.address())?;
    let blocks: u64 = match args.get("blocks") {
        Some(b) => b.parse().map_err(|_| "--blocks: número inválido")?,
        None => 1_000,
    };
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::PlaceOrder {
            asset: a.id,
            side,
            amount,
            price,
            expires_at: height + 1 + blocks,
        },
        fee,
    )?;
    eprintln!("aviso: ordens do Grande Mercado são públicas (conta, quantidade e preço)");
    println!("ordem {id} enviada; executa no leilão do bloco em que for incluída, se cruzar");
    Ok(())
}

fn cmd_cancel(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let order = Hash32::from_hex(args.req("order")?).ok_or("--order: identificador inválido")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::CancelOrder { order },
        fee,
    )?;
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn cmd_send_asset(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let to = Address::from_hex(args.req("to")?).ok_or("--to: endereço transparente inválido")?;
    let (mut client, genesis) = open(args)?;
    let a = find_asset(&mut client, args.req("asset")?, None)?;
    let amount =
        parse_units(args.req("amount")?, a.info.decimals).ok_or("--amount: quantidade inválida")?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::TransferAsset {
            asset: a.id,
            to,
            amount,
        },
        fee,
    )?;
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn cmd_pool_deposit(args: &Args) -> Result<(), String> {
    if args.get("permanente").is_none() {
        return Err(
            "o depósito no Pool é IRREVERSÍVEL: não existe operação de retirada. \
             Repita com --permanente para confirmar"
                .into(),
        );
    }
    let keys = load_keys(args)?;
    let (mut client, genesis) = open(args)?;
    let spec = args.req("asset")?;
    let (asset, amount) = if spec.eq_ignore_ascii_case("zero") {
        (rz_core::market::AssetId::ZERO, args.amount("amount")?)
    } else {
        let a = find_asset(&mut client, spec, None)?;
        let amount = parse_units(args.req("amount")?, a.info.decimals)
            .ok_or("--amount: quantidade inválida")?;
        (a.id, amount)
    };
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::PoolDeposit { asset, amount },
        fee,
    )?;
    println!("depósito permanente no Pool enviado: {id}");
    Ok(())
}

// ------------------------------------------------------ Comunidades e nomes

fn parse_keys(list: &str) -> Result<Vec<rz_crypto::PublicKey>, String> {
    list.split(',')
        .map(|h| {
            rz_crypto::PublicKey::from_hex(h.trim()).map_err(|_| format!("chave inválida '{h}'"))
        })
        .collect()
}

fn rule_arg(
    args: &Args,
    keys: &str,
    threshold: &str,
) -> Result<Option<rz_core::community::DecisionRule>, String> {
    match args.get(keys) {
        None => Ok(None),
        Some(list) => {
            let threshold: u8 = args
                .req(threshold)?
                .parse()
                .map_err(|_| format!("--{threshold}: número inválido"))?;
            let rule = rz_core::community::DecisionRule {
                keys: parse_keys(list)?,
                threshold,
            };
            rule.validate()?;
            Ok(Some(rule))
        }
    }
}

fn hash_arg(args: &Args, name: &str) -> Result<Hash32, String> {
    Hash32::from_hex(args.req(name)?).ok_or_else(|| format!("--{name}: hash inválido"))
}

fn approvals_arg(args: &Args) -> Result<Vec<rz_core::community::Approval>, String> {
    args.req("approvals")?
        .split(',')
        .map(|h| {
            let bytes = rz_crypto::hex::decode(h.trim()).ok_or("aprovação inválida")?;
            rz_core::community::Approval::from_canonical_bytes(&bytes)
                .map_err(|e| format!("aprovação inválida: {e}"))
        })
        .collect()
}

fn address_arg(args: &Args) -> Result<(String, rz_core::community::NameKind), String> {
    let spec = args.req("name")?;
    rz_core::community::parse_address(spec)
        .ok_or_else(|| format!("--name: use zero://nome.tipo, recebido '{spec}'"))
}

fn cmd_community_declare(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let name = args.req("name")?.to_string();
    rz_core::community::check_name(&name)?;
    let manifest_hash = match (args.get("manifest"), args.get("manifest-hash")) {
        (Some(path), _) => {
            let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            rz_crypto::hash(rz_core::community::COMMUNITY_MANIFEST, &bytes)
        }
        (None, Some(_)) => hash_arg(args, "manifest-hash")?,
        (None, None) => return Err("informe --manifest ou --manifest-hash".into()),
    };
    let rule = rule_arg(args, "keys", "threshold")?.ok_or("--keys é obrigatório")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::DeclareCommunity {
            name: name.clone(),
            manifest_hash,
            rule,
        },
        fee,
    )?;
    println!("declaração de zero://{name}.comunidade enviada");
    println!("id da Comunidade: {id}");
    println!("para o reconhecimento, proponha na categoria comunidade com --content-hash {id}");
    Ok(())
}

fn cmd_community(args: &Args) -> Result<(), String> {
    let (mut client, _) = open(args)?;
    let name = args.req("name")?;
    let (height, c) = rz_wallet::community(&mut client, name)?;
    println!("altura: {height}");
    let Some(c) = c else {
        println!("nenhuma Comunidade chamada {name}");
        return Ok(());
    };
    println!("zero://{}.comunidade", c.name);
    println!("  id:          {}", c.id);
    println!(
        "  estado:      {}",
        match c.status {
            rz_core::community::CommunityStatus::Declared =>
                format!("declarada (aguardando reconhecimento até {})", c.expires_at),
            rz_core::community::CommunityStatus::Recognized => "reconhecida".into(),
        }
    );
    println!("  manifesto:   {} (versão {})", c.manifest_hash, c.version);
    println!(
        "  decisão:     {} de {} chaves",
        c.rule.threshold,
        c.rule.keys.len()
    );
    for k in &c.rule.keys {
        println!("    {k}");
    }
    println!("  histórico:");
    for (v, h, at) in &c.history {
        println!("    versão {v}: {h} (altura {at})");
    }
    Ok(())
}

fn cmd_community_approve(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let genesis_path = args.req("genesis")?;
    let bytes = fs::read(genesis_path).map_err(|e| format!("{genesis_path}: {e}"))?;
    let genesis =
        Genesis::from_canonical_bytes(&bytes).map_err(|e| format!("genesis inválido: {e}"))?;
    let community = hash_arg(args, "community")?;
    let payload = match args.get("proposal") {
        Some(_) => {
            let proposal = hash_arg(args, "proposal")?;
            let choice =
                Choice::parse(args.req("choice")?).ok_or("--choice: use sim, nao ou abstencao")?;
            rz_core::community::position_payload(&community, &proposal, choice)
        }
        None => {
            let manifest = hash_arg(args, "manifest-hash")?;
            let version: u32 = args
                .req("version")?
                .parse()
                .map_err(|_| "--version: número inválido")?;
            let rule = rule_arg(args, "new-keys", "new-threshold")?;
            rz_core::community::update_payload(&community, &manifest, version, &rule)
        }
    };
    let approval = rz_core::community::approve(&keys.transparent, &genesis.network_id, &payload);
    println!("{}", rz_crypto::hex::encode(&approval.to_canonical_bytes()));
    Ok(())
}

fn cmd_community_position(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let community = hash_arg(args, "community")?;
    let proposal = hash_arg(args, "proposal")?;
    let choice = Choice::parse(args.req("choice")?).ok_or("--choice: use sim, nao ou abstencao")?;
    let approvals = approvals_arg(args)?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::CommunityPosition {
            community,
            proposal,
            choice,
            approvals,
        },
        fee,
    )?;
    println!("posição enviada: {id} (registrada; não altera o resultado oficial)");
    Ok(())
}

fn cmd_community_update(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let community = hash_arg(args, "community")?;
    let manifest_hash = hash_arg(args, "manifest-hash")?;
    let version: u32 = args
        .req("version")?
        .parse()
        .map_err(|_| "--version: número inválido")?;
    let rule = rule_arg(args, "new-keys", "new-threshold")?;
    let approvals = approvals_arg(args)?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::UpdateCommunity {
            community,
            manifest_hash,
            version,
            rule,
            approvals,
        },
        fee,
    )?;
    println!("atualização enviada: {id}");
    Ok(())
}

fn cmd_name(args: &Args, register: bool) -> Result<(), String> {
    let keys = load_keys(args)?;
    let (name, kind) = address_arg(args)?;
    let target = hash_arg(args, "target")?;
    let new_owner = match args.get("new-owner") {
        Some(h) => Some(Address::from_hex(h).ok_or("--new-owner: endereço inválido")?),
        None => None,
    };
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let kind_tx = if register {
        TxKind::RegisterName { name, kind, target }
    } else {
        TxKind::UpdateName {
            name,
            kind,
            target,
            new_owner,
        }
    };
    let id = send_account(&mut client, &genesis, &keys, kind_tx, fee)?;
    if register {
        eprintln!("aviso: a taxa de registro de nome vai para o Pool permanente");
    }
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn cmd_resolve(args: &Args) -> Result<(), String> {
    let (name, kind) = address_arg(args)?;
    let (mut client, _) = open(args)?;
    let (target, owner) = rz_wallet::resolve(&mut client, &name, kind)?;
    let address = rz_core::community::format_address(&name, kind);
    match target {
        Some(t) => {
            println!("{address} → {t}");
            if let Some(o) = owner {
                println!("  dono: {o}");
            }
        }
        None => println!("{address} não está registrado"),
    }
    Ok(())
}

// ------------------------------------------------------------------ Defesa

fn cmd_defense(args: &Args) -> Result<(), String> {
    let (mut client, _) = open(args)?;
    let v = rz_wallet::defense(&mut client)?;
    println!("altura: {}", v.height);
    println!("modo: {} desde a altura {}", v.mode.name(), v.mode_since);
    if v.mode != rz_core::defense::DefenseMode::Normal {
        println!("  vence na altura {} sem renovação", v.mode_expires_at);
    }
    println!("sequência de decisões: {}", v.seq);
    if let Some(i) = &v.incident {
        println!(
            "incidente {} ({:?}), aberto na altura {}",
            i.id, i.status, i.opened_at
        );
        println!("  evidências: {}", i.evidence.len());
        for e in &i.evidence {
            println!("    {e}");
        }
        println!("  ações registradas: {}", i.actions.len());
        for c in &v.credentials {
            let scopes: Vec<&str> = c.scopes.iter().map(|s| s.name()).collect();
            println!(
                "  credencial {} → {} [{}] até {}{}",
                c.id,
                c.holder,
                scopes.join(","),
                c.expires_at,
                if c.revoked { " (revogada)" } else { "" }
            );
        }
    }
    Ok(())
}

fn scope_arg(args: &Args, name: &str) -> Result<rz_core::defense::Scope, String> {
    let v = args.req(name)?;
    rz_core::defense::Scope::parse(v).ok_or_else(|| format!("--{name}: escopo desconhecido '{v}'"))
}

/// Monta a decisão pedida: o conteúdo a atestar e a transação (sem as
/// atestações).
fn defense_decision(args: &Args, v: &rz_wallet::DefenseView) -> Result<(Vec<u8>, TxKind), String> {
    use rz_core::defense::{payload, DefenseMode, IncidentStatus};
    let seq = v.seq;
    let active = || {
        v.incident
            .as_ref()
            .map(|i| i.id)
            .ok_or_else(|| "nenhum incidente em vigor".to_string())
    };
    Ok(match args.req("decision")? {
        "transicao" => {
            let to = match args.req("to")? {
                "normal" => DefenseMode::Normal,
                "vigilancia" => DefenseMode::Vigilance,
                "incidente" => DefenseMode::Incident,
                "guerra" => DefenseMode::CyberWar,
                other => return Err(format!("--to: modo desconhecido '{other}'")),
            };
            let evidence = hash_arg(args, "evidence")?;
            (
                payload::transition(seq, to, &evidence),
                TxKind::DefenseTransition {
                    to,
                    evidence,
                    seq,
                    attestations: vec![],
                },
            )
        }
        "atualizar" => {
            let status = match args.req("status")? {
                "contido" => IncidentStatus::Contained,
                "recuperado" => IncidentStatus::Recovered,
                other => {
                    return Err(format!(
                        "--status: use contido ou recuperado, recebido '{other}'"
                    ))
                }
            };
            let incident = active()?;
            let evidence = hash_arg(args, "evidence")?;
            (
                payload::incident_update(seq, &incident, status, &evidence),
                TxKind::IncidentUpdate {
                    incident,
                    status,
                    evidence,
                    seq,
                    attestations: vec![],
                },
            )
        }
        "encerrar" => {
            let incident = active()?;
            let archive = hash_arg(args, "archive")?;
            (
                payload::close(seq, &incident, &archive),
                TxKind::CloseIncident {
                    incident,
                    archive,
                    seq,
                    attestations: vec![],
                },
            )
        }
        "credencial" => {
            let incident = active()?;
            let holder = rz_crypto::PublicKey::from_hex(args.req("holder")?)
                .map_err(|_| "--holder: chave inválida")?;
            let scopes = args
                .req("scopes")?
                .split(',')
                .map(|s| {
                    rz_core::defense::Scope::parse(s.trim())
                        .ok_or_else(|| format!("escopo desconhecido '{s}'"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let expires_at: u64 = args
                .req("expires-at")?
                .parse()
                .map_err(|_| "--expires-at: altura inválida")?;
            (
                payload::grant(seq, &incident, &holder, &scopes, expires_at),
                TxKind::GrantCredential {
                    incident,
                    holder,
                    scopes,
                    expires_at,
                    seq,
                    attestations: vec![],
                },
            )
        }
        "revogar" => {
            let credential = hash_arg(args, "credential")?;
            (
                payload::revoke(seq, &credential),
                TxKind::RevokeCredential {
                    credential,
                    seq,
                    attestations: vec![],
                },
            )
        }
        "contribuicao" => {
            let incident = hash_arg(args, "incident")?;
            let node = rz_crypto::PublicKey::from_hex(args.req("holder")?)
                .map_err(|_| "--holder: chave inválida")?;
            let role = scope_arg(args, "role")?;
            let evidence = hash_arg(args, "evidence")?;
            (
                payload::contribution(seq, &incident, &node, role, &evidence),
                TxKind::AttestContribution {
                    incident,
                    node,
                    role,
                    evidence,
                    seq,
                    attestations: vec![],
                },
            )
        }
        other => return Err(format!("--decision: decisão desconhecida '{other}'")),
    })
}

fn with_attestations(kind: TxKind, a: Vec<rz_core::defense::Attestation>) -> TxKind {
    match kind {
        TxKind::DefenseTransition {
            to, evidence, seq, ..
        } => TxKind::DefenseTransition {
            to,
            evidence,
            seq,
            attestations: a,
        },
        TxKind::IncidentUpdate {
            incident,
            status,
            evidence,
            seq,
            ..
        } => TxKind::IncidentUpdate {
            incident,
            status,
            evidence,
            seq,
            attestations: a,
        },
        TxKind::CloseIncident {
            incident,
            archive,
            seq,
            ..
        } => TxKind::CloseIncident {
            incident,
            archive,
            seq,
            attestations: a,
        },
        TxKind::GrantCredential {
            incident,
            holder,
            scopes,
            expires_at,
            seq,
            ..
        } => TxKind::GrantCredential {
            incident,
            holder,
            scopes,
            expires_at,
            seq,
            attestations: a,
        },
        TxKind::RevokeCredential {
            credential, seq, ..
        } => TxKind::RevokeCredential {
            credential,
            seq,
            attestations: a,
        },
        TxKind::AttestContribution {
            incident,
            node,
            role,
            evidence,
            seq,
            ..
        } => TxKind::AttestContribution {
            incident,
            node,
            role,
            evidence,
            seq,
            attestations: a,
        },
        other => other,
    }
}

fn cmd_defense_decision(args: &Args, submit: bool) -> Result<(), String> {
    let keys = load_keys(args)?;
    let (mut client, genesis) = open(args)?;
    let view = rz_wallet::defense(&mut client)?;
    let (payload, kind) = defense_decision(args, &view)?;
    if !submit {
        let a = rz_core::defense::attest(&keys.transparent, &genesis.network_id, &payload);
        println!("{}", rz_crypto::hex::encode(&a.to_canonical_bytes()));
        eprintln!(
            "atestação para a sequência {} (válida só até a próxima decisão)",
            view.seq
        );
        return Ok(());
    }
    let attestations = args
        .req("attestations")?
        .split(',')
        .map(|h| {
            let bytes = rz_crypto::hex::decode(h.trim()).ok_or("atestação inválida")?;
            rz_core::defense::Attestation::from_canonical_bytes(&bytes)
                .map_err(|e| format!("atestação inválida: {e}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        with_attestations(kind, attestations),
        fee,
    )?;
    println!("decisão de defesa enviada: {id}");
    Ok(())
}

fn cmd_defense_action(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let credential = hash_arg(args, "credential")?;
    let scope = scope_arg(args, "scope")?;
    let subject = hash_arg(args, "subject")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::DefenseAction {
            credential,
            scope,
            subject,
        },
        fee,
    )?;
    println!("ação registrada: {id}");
    Ok(())
}

fn cmd_bond(args: &Args, bond: bool) -> Result<(), String> {
    let keys = load_keys(args)?;
    let amount = args.amount("amount")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let kind = if bond {
        TxKind::Bond { amount }
    } else {
        TxKind::Unbond { amount }
    };
    let id = send_account(&mut client, &genesis, &keys, kind, fee)?;
    eprintln!("aviso: vínculos de validador são públicos (valor e conta)");
    println!(
        "{} {} ZERO",
        if bond { "vinculando" } else { "desvinculando" },
        format_zero(amount)
    );
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
