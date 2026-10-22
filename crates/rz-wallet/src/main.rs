//! `zero-wallet` — Wallet de linha de comando da Rede Zero (DEVNET e TESTNET).
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
zero-wallet — Wallet da Rede Zero (DEVNET e TESTNET)

USO:
  zero-wallet new     --key FILE [--plaintext]       cifrada com senha, salvo --plaintext (só DEVNET)
  zero-wallet key-encrypt --key FILE                 cifra um arquivo de chave em aberto
Rotação e recuperação (ADR-0023) — o endereço da conta não muda:
  zero-wallet key-info --genesis FILE --node ADDR (--key FILE | --public-key HEX)
  zero-wallet key-rotate --genesis FILE --node ADDR --key ATUAL --new-key NOVA
                      [--new-passphrase-file ARQUIVO]
  zero-wallet recovery-set --genesis FILE --node ADDR --key FILE --delay-blocks N
                      (--guardians HEX,HEX,... --threshold K | --recovery-public-key HEX | --remove)
                      também cancela uma recuperação agendada
  zero-wallet recovery-approve --genesis FILE --node ADDR --key GUARDIÃO --account HEX
                      --new-public-key HEX                 aprovação de guardião (hex)
  zero-wallet recover --genesis FILE --node ADDR --key FILE --account HEX --new-key NOVA
                      [--approvals HEX,HEX,...]            sem --approvals: a chave é a guardiã (1 de 1)
                      a troca só vale depois do prazo; até lá a chave atual pode cancelar
  Pontos de contribuição e vínculos de validador ficam com a chave antiga (não migram).
  Notas privadas pertencem à chave: envie-as ao endereço zs… da nova chave.
  zero-wallet address --key FILE
Faucet da TESTNET (docs/TESTNET.md) — prova de trabalho, limite por endereço, sem registro de IP:
  zero-wallet faucet --genesis FILE --faucet ADDR (--address HEX | --key FILE) [--proxy ADDR | --direct]
  zero-wallet faucet-serve --genesis FILE --node ADDR --key FILE [--listen 127.0.0.1:7200]
                      [--amount ZERO (1000)] [--cooldown-hours H (24)] [--bits B (22)]
                      [--max-per-hour N (120)] [--fee ZERO]
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
                      [--secret [--salt-out ARQUIVO]]   voto secreto, se a governança o ligou
  zero-wallet vote-reveal --genesis FILE --node ADDR --key FILE --proposal HEX
                      --choice (sim|nao|abstencao) (--salt HEX | --salt-file ARQUIVO)

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
  zero-wallet name-renew --genesis FILE --node ADDR --key FILE --name zero://NOME.TIPO
                      mais um período (~1 ano); até dois períodos à frente
  zero-wallet resolve --genesis FILE --node ADDR --name zero://NOME.TIPO
Conteúdo da Exonet (ADR-0017) — publicações verificadas por hash:
  zero-wallet content-pack --dir DIRETORIO --out ARQUIVO     empacota um site e mostra o id
  zero-wallet content-publish --genesis FILE --node ADDR --file ARQUIVO
  zero-wallet content-get --genesis FILE --node ADDR --id HEX --out ARQUIVO
  zero-wallet manifest-new --name NOME --version N [--description TEXTO] [--frontend HEX]
                      [--module HEX] --out ARQUIVO          manifesto estruturado de Comunidade
  zero-wallet manifest-publish --genesis FILE --node ADDR --file ARQUIVO
  Publicar um site: content-pack → name-register --target ID → content-publish.
Exonet Runtime (ADR-0018) — lógica de Comunidades em WebAssembly isolado:
  zero-wallet module-check --file MODULO.wasm               valida localmente e mostra o id
  zero-wallet module-publish --genesis FILE --node ADDR --key FILE --file MODULO.wasm
  zero-wallet module-bind-approve --genesis FILE --node ADDR --key CHAVE_DA_COMUNIDADE
                      --community NOME|HEX (--module HEX | --module nenhum)
  zero-wallet module-bind --genesis FILE --node ADDR --key FILE --community NOME|HEX
                      (--module HEX | --module nenhum) --approvals HEX,HEX,...
  zero-wallet call  --genesis FILE --node ADDR --key FILE --community NOME|HEX --method M
                      [--args TEXTO | --args-hex HEX] [--fuel N]
  zero-wallet query --genesis FILE --node ADDR --community NOME|HEX --method M [--args ...]
  zero-wallet receipt --genesis FILE --node ADDR --tx HEX
  O módulo só alcança o armazenamento da própria Comunidade: não há acesso a saldos,
  consenso, governança ou outras Comunidades. Chamadas que falham pagam a taxa.
Reputação (ADR-0020) — pontos de contribuição verificáveis, com decaimento:
  zero-wallet reputation --genesis FILE --node ADDR (--public-key HEX | --key FILE)
Atualizações verificáveis (ADR-0021) — só a governança adota uma versão:
  zero-wallet release-new --version V --compat (compativel|parcial|incompativel) --out ARQUIVO
                      [--root DIR] [--spec spec,docs] [--source crates,Cargo.toml,Cargo.lock]
                      [--tests conformance] [--artifacts nome=arquivo,...] [--previous HEX]
                      [--notes ARQUIVO]
  zero-wallet release-publish --genesis FILE --node ADDR --key FILE --file ARQUIVO
                      publica o manifesto (queima o depósito de proposta vigente)
  zero-wallet releases --genesis FILE --node ADDR
  zero-wallet release-verify (--genesis FILE --node ADDR [--release HEX] | --manifest ARQUIVO)
                      [--artifact NOME --file ARQUIVO] [--root DIR]
  Adotar: propose --category constitucional --release ID. A versão precisa suceder a
  vigente (--previous); o Node avisa quando a cadeia adota versão diferente da sua.
Ponte para ativos externos (ADR-0019) — sem custodiante:
  zero-wallet bridge --genesis FILE --node ADDR --chain REDE
  zero-wallet bridge-burn-script --genesis FILE --to HEX [--relay-fee UNIDADES]
                      script OP_RETURN da queima (IRREVERSÍVEL na rede de origem)
  zero-wallet bridge-headers --genesis FILE --node ADDR --key FILE --chain REDE --file CABEÇALHOS
                      um cabeçalho de 80 bytes em hex por linha (ex.: getblockheader HASH false)
  zero-wallet bridge-deposit --genesis FILE --node ADDR --key FILE --chain REDE
                      --tx HEX (sem testemunhas) --merkleblock HEX (gettxoutproof)
  zero-wallet bridge-chain-new --name REDE --asset ATIVO --decimals N --pow-limit HEX
                      --retarget (sim|nao) --interval N --timespan SEGUNDOS --checkpoint HEX80
                      --checkpoint-height N --confirmations N --out ARQUIVO
  zero-wallet bridge-chain-publish --genesis FILE --node ADDR --key FILE --file ARQUIVO
                      publica a nova rede (queima o depósito de proposta); só a governança adota:
                      propose --category constitucional --params bridge_add_chain=ID
Trocas atômicas (HTLC, SHA-256, prazo em altura):
  zero-wallet htlc-secret                                   gera segredo e hash
  zero-wallet htlc-lock  --genesis FILE --node ADDR --key FILE --asset (ZERO|ATIVO) --amount QTD
                      --to HEX --hash HEX --timeout-blocks N
  zero-wallet htlc-claim  --genesis FILE --node ADDR --key FILE --id HEX --preimage HEX
  zero-wallet htlc-refund --genesis FILE --node ADDR --key FILE --id HEX
  zero-wallet htlc --genesis FILE --node ADDR --id HEX
  O node só hospeda conteúdo referenciado pelo estado (nome ou manifesto).
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

Chaves (REQ-012): arquivos cifrados pedem a senha por --passphrase-file ARQUIVO,
pela variável RZ_KEY_PASSPHRASE ou pelo terminal (sem eco).
Rede (ADR-0011) — válido para todos os comandos que usam --node:
  --node ADDR       IP:porta ou nome.onion:porta
  --proxy ADDR      proxy SOCKS5 (Tor: 127.0.0.1:9050); o node não vê seu IP
  --node-id HEX     identidade esperada do node (impede interceptação)
  --direct          aceita conectar a node remoto SEM proxy (expõe seu IP ao node)
Por padrão, sem --proxy só são aceitos nodes locais (127.0.0.1).

A privacidade é probabilística e não constitui anonimato absoluto.
AVISO: DEVNET e TESTNET — o ZERO destas redes não possui valor econômico.
";

struct Args(HashMap<String, String>);

/// Opções sem valor.
const SWITCHES: &[&str] = &["direct", "permanente", "plaintext", "remove", "secret"];

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
        "key-encrypt" => cmd_key_encrypt(&args),
        "key-info" => cmd_key_info(&args),
        "key-rotate" => cmd_key_rotate(&args),
        "recovery-set" => cmd_recovery_set(&args),
        "recover" => cmd_recover(&args),
        "recovery-approve" => cmd_recovery_approve(&args),
        "address" => address(&args),
        "faucet" => cmd_faucet(&args),
        "faucet-serve" => cmd_faucet_serve(&args),
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
        "name-renew" => cmd_name_renew(&args),
        "resolve" => cmd_resolve(&args),
        "content-pack" => cmd_content_pack(&args),
        "content-publish" => cmd_content_publish(&args),
        "content-get" => cmd_content_get(&args),
        "manifest-new" => cmd_manifest_new(&args),
        "manifest-publish" => cmd_manifest_publish(&args),
        "module-check" => cmd_module_check(&args),
        "module-publish" => cmd_module_publish(&args),
        "module-bind-approve" => cmd_module_bind(&args, false),
        "module-bind" => cmd_module_bind(&args, true),
        "call" => cmd_call(&args),
        "query" => cmd_query(&args),
        "receipt" => cmd_receipt(&args),
        "reputation" => cmd_reputation(&args),
        "release-new" => cmd_release_new(&args),
        "release-publish" => cmd_release_publish(&args),
        "releases" => cmd_releases(&args),
        "release-verify" => cmd_release_verify(&args),
        "bridge" => cmd_bridge(&args),
        "bridge-burn-script" => cmd_bridge_burn_script(&args),
        "bridge-headers" => cmd_bridge_headers(&args),
        "bridge-chain-new" => cmd_bridge_chain_new(&args),
        "bridge-chain-publish" => cmd_bridge_chain_publish(&args),
        "bridge-deposit" => cmd_bridge_deposit(&args),
        "htlc-secret" => cmd_htlc_secret(),
        "htlc-lock" => cmd_htlc_lock(&args),
        "htlc-claim" => cmd_htlc_settle(&args, true),
        "htlc-refund" => cmd_htlc_settle(&args, false),
        "htlc" => cmd_htlc(&args),
        "defense" => cmd_defense(&args),
        "defense-attest" => cmd_defense_decision(&args, false),
        "defense-submit" => cmd_defense_decision(&args, true),
        "defense-action" => cmd_defense_action(&args),
        "unbond" => cmd_bond(&args, false),
        "propose" => cmd_propose(&args),
        "vote" => cmd_vote(&args),
        "vote-reveal" => cmd_vote_reveal(&args),
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

fn passphrase(args: &Args) -> std::io::Result<String> {
    keyfile::passphrase(args.get("passphrase-file").map(Path::new))
}

fn load_keys(args: &Args) -> Result<Keys, String> {
    let path = args.req("key")?;
    keyfile::load_with(Path::new(path), || passphrase(args))
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
    if args.get("plaintext").is_some() {
        keyfile::save(Path::new(path), &sk).map_err(|e| format!("{path}: {e}"))?;
        println!("AVISO: chave gravada SEM cifragem (--plaintext); use só na DEVNET");
    } else {
        let pass = passphrase(args).map_err(|e| e.to_string())?;
        keyfile::save_encrypted(Path::new(path), &sk, &pass).map_err(|e| format!("{path}: {e}"))?;
        println!("chave cifrada com a senha (Argon2id + XChaCha20-Poly1305)");
    }
    let keys = Keys::from_secret(sk);
    println!("chave criada em {path} (não compartilhe nem versione este arquivo)");
    println!("endereço privado:      {}", keys.shielded_address());
    println!("endereço transparente: {}", keys.address());
    println!("chave pública:         {}", keys.transparent.public_key());
    Ok(())
}

fn cmd_key_encrypt(args: &Args) -> Result<(), String> {
    let path = args.req("key")?;
    let pass = passphrase(args).map_err(|e| e.to_string())?;
    keyfile::encrypt_in_place(Path::new(path), &pass).map_err(|e| format!("{path}: {e}"))?;
    println!("{path} agora está cifrado; o conteúdo em aberto foi sobrescrito");
    println!("cópias antigas (backups) continuam em aberto: apague-as também");
    Ok(())
}

fn load_new_key(args: &Args) -> Result<SecretKey, String> {
    let path = args.req("new-key")?;
    keyfile::load_with(Path::new(path), || {
        keyfile::passphrase(
            args.get("new-passphrase-file")
                .or(args.get("passphrase-file"))
                .map(Path::new),
        )
    })
    .map_err(|e| format!("{path}: {e}"))
}

fn rotation_proof(genesis: &Genesis, account: &Address, new: &SecretKey) -> rz_crypto::Signature {
    new.sign(
        rz_core::rotation::KEY_ROTATION,
        &genesis.network_id,
        &rz_core::rotation::rotation_payload(account, &new.public_key()),
    )
}

fn cmd_key_info(args: &Args) -> Result<(), String> {
    let key = match args.get("public-key") {
        Some(h) => rz_crypto::PublicKey::from_hex(h).map_err(|_| "--public-key: chave inválida")?,
        None => load_keys(args)?.transparent.public_key(),
    };
    let (mut client, _) = open(args)?;
    let v = rz_wallet::key_info(&mut client, key)?;
    println!("chave:        {key}");
    match v.account {
        Some(a) => println!("assina pela conta {a}"),
        None => println!("SUBSTITUÍDA: não assina mais por nenhuma conta"),
    }
    if let Some(c) = v.current {
        println!("chave atual da conta: {c}");
    }
    match v.recovery {
        Some(r) => println!(
            "recuperação: {} de {} guardiões (prazo de {} blocos): {}",
            r.threshold,
            r.guardians.len(),
            r.delay,
            r.guardians
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => println!("recuperação: nenhuma"),
    }
    if let Some(p) = v.pending {
        println!(
            "ATENÇÃO: recuperação agendada para {} na altura {} (altura atual {}); cancele com recovery-set se não foi você",
            p.new_key, p.at, v.height
        );
    }
    Ok(())
}

fn cmd_key_rotate(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let new = load_new_key(args)?;
    let (mut client, genesis) = open(args)?;
    let account = rz_wallet::account_address(&mut client, &keys)?;
    let fee = fee(args, &genesis)?;
    let proof = rotation_proof(&genesis, &account, &new);
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::RotateKey {
            new_key: new.public_key(),
            proof,
        },
        fee,
    )?;
    println!(
        "rotação {id} enviada: a conta {account} passa a ser da chave {}",
        new.public_key()
    );
    println!("a chave antiga deixa de assinar assim que o bloco for finalizado");
    Ok(())
}

fn cmd_recovery_set(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let recovery = if args.get("remove").is_some() {
        None
    } else {
        let delay = args
            .req("delay-blocks")?
            .parse()
            .map_err(|_| "--delay-blocks: número inválido")?;
        let r = match (args.get("guardians"), args.get("recovery-public-key")) {
            (Some(list), _) => rz_core::rotation::Recovery {
                guardians: split_list(list)
                    .iter()
                    .map(|h| rz_crypto::PublicKey::from_hex(h))
                    .collect::<Result<_, _>>()
                    .map_err(|_| "--guardians: chave inválida")?,
                threshold: args
                    .req("threshold")?
                    .parse()
                    .map_err(|_| "--threshold: número inválido")?,
                delay,
            },
            (None, Some(h)) => rz_core::rotation::Recovery::single(
                rz_crypto::PublicKey::from_hex(h)
                    .map_err(|_| "--recovery-public-key: chave inválida")?,
                delay,
            ),
            (None, None) => {
                return Err(
                    "informe --guardians HEX,... --threshold N ou --recovery-public-key HEX".into(),
                )
            }
        };
        r.validate()?;
        Some(r)
    };
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::SetRecovery {
            recovery: recovery.clone(),
        },
        fee,
    )?;
    match recovery {
        Some(r) => println!(
            "recuperação {id}: {} de {} guardiões poderão trocar a chave depois de {} blocos",
            r.threshold,
            r.guardians.len(),
            r.delay
        ),
        None => println!("recuperação {id}: removida"),
    }
    println!("qualquer recuperação agendada foi cancelada");
    Ok(())
}

/// Aprovação de guardião (assinada localmente) para recuperar `account`.
fn cmd_recovery_approve(args: &Args) -> Result<(), String> {
    let guardian = load_keys(args)?;
    let account = Address::from_hex(args.req("account")?).ok_or("--account: endereço inválido")?;
    let new_key = rz_crypto::PublicKey::from_hex(args.req("new-public-key")?)
        .map_err(|_| "--new-public-key: chave inválida")?;
    let (mut client, genesis) = open(args)?;
    let (recovery, nonce, _) = rz_wallet::recovery_info(&mut client, account)?;
    let r = recovery.ok_or("a conta não tem recuperação registrada")?;
    if !r.guardians.contains(&guardian.transparent.public_key()) {
        return Err("esta chave não é guardiã da conta".into());
    }
    let a = rz_core::rotation::approve_recovery(
        &guardian.transparent,
        &genesis.network_id,
        &account,
        &new_key,
        nonce,
    );
    println!("aprovação (entregue a quem vai submeter a recuperação):");
    println!("{}", rz_crypto::hex::encode(&a.to_canonical_bytes()));
    println!("confira antes de aprovar: a conta {account} passará para a chave {new_key}");
    Ok(())
}

fn cmd_recover(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let new = load_new_key(args)?;
    let account = Address::from_hex(args.req("account")?).ok_or("--account: endereço inválido")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let proof = rotation_proof(&genesis, &account, &new);
    let approvals = match args.get("approvals") {
        Some(list) => split_list(list)
            .iter()
            .map(|h| {
                rz_crypto::hex::decode(h)
                    .and_then(|b| rz_core::community::Approval::from_canonical_bytes(&b).ok())
                    .ok_or_else(|| format!("--approvals: aprovação inválida '{h}'"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        // 1-de-1: a própria chave é a guardiã.
        None => {
            let (_, nonce, _) = rz_wallet::recovery_info(&mut client, account)?;
            vec![rz_core::rotation::approve_recovery(
                &keys.transparent,
                &genesis.network_id,
                &account,
                &new.public_key(),
                nonce,
            )]
        }
    };
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::RecoverAccount {
            account,
            new_key: new.public_key(),
            proof,
            approvals,
        },
        fee,
    )?;
    println!("recuperação {id} agendada para a conta {account}");
    println!("a troca vale depois do prazo; a chave atual ainda pode cancelar até lá");
    Ok(())
}

fn address(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    println!("privado:      {}", keys.shielded_address());
    println!("transparente: {}", keys.address());
    println!("chave pública: {}", keys.transparent.public_key());
    Ok(())
}

fn cmd_faucet(args: &Args) -> Result<(), String> {
    let genesis = load_genesis(args)?;
    let to = match args.get("address") {
        Some(h) => Address::from_hex(h).ok_or("--address: endereço transparente inválido")?,
        None => load_keys(args)?.address(),
    };
    let target: PeerAddr = args
        .req("faucet")?
        .parse()
        .map_err(|e| format!("--faucet: {e}"))?;
    let proxy = match args.get("proxy") {
        Some(p) => Some(
            p.parse::<SocketAddr>()
                .map_err(|_| "--proxy: endereço inválido")?,
        ),
        None => None,
    };
    if proxy.is_none() && !target.is_loopback() && args.get("direct").is_none() {
        return Err(rz_wallet::DIRECT_REFUSED.into());
    }
    let stream = rz_p2p::socks::connect(&target, proxy, std::time::Duration::from_secs(30))
        .map_err(|e| format!("--faucet: {e}"))?;
    println!("resolvendo a prova de trabalho…");
    let id = rz_wallet::faucet::request(stream, &genesis.network_id, &to)?;
    println!("transação {id} enviada para {to}");
    Ok(())
}

fn cmd_faucet_serve(args: &Args) -> Result<(), String> {
    let (client, genesis) = open(args)?;
    let keys = load_keys(args)?;
    let num = |name: &str, default: u64| -> Result<u64, String> {
        args.get(name).map_or(Ok(default), |v| {
            v.parse().map_err(|_| format!("--{name}: valor inválido"))
        })
    };
    let cfg = rz_wallet::faucet::FaucetConfig {
        amount: match args.get("amount") {
            Some(_) => args.amount("amount")?,
            None => 1_000 * rz_core::UNITS_PER_ZERO,
        },
        cooldown: std::time::Duration::from_secs(num("cooldown-hours", 24)?.saturating_mul(3_600)),
        bits: u32::try_from(num("bits", 22)?)
            .ok()
            .filter(|b| *b <= 32)
            .ok_or("--bits: de 0 a 32")?,
        max_per_hour: usize::try_from(num("max-per-hour", 120)?).unwrap_or(usize::MAX),
        fee: fee(args, &genesis)?,
    };
    let listen = args.get("listen").unwrap_or("127.0.0.1:7200");
    let listener = std::net::TcpListener::bind(listen).map_err(|e| format!("--listen: {e}"))?;
    println!(
        "faucet de {} em {listen}: {} ZERO por endereço a cada {} h; prova de {} bits",
        keys.address(),
        format_zero(cfg.amount),
        cfg.cooldown.as_secs() / 3_600,
        cfg.bits
    );
    println!("nenhum endereço IP é registrado; para Tor, aponte um serviço onion para {listen}");
    rz_wallet::faucet::serve(listener, genesis, keys, client, cfg).map_err(|e| e.to_string())
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
    let addr = rz_wallet::account_address(&mut client, &keys)?;
    if addr != keys.address() {
        println!("conta:        {addr} (chave girada)");
    }
    let (bal, nonce, height) = account(&mut client, addr)?;
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
    let mine = match &keys {
        Some(k) => Some(rz_wallet::account_address(&mut client, k)?),
        None => None,
    };
    let v = governance(&mut client, mine)?;
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
            println!("      {} = {}", c.name(), c.value_text());
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
    let mine = match &keys {
        Some(k) => Some(rz_wallet::account_address(&mut client, k)?),
        None => None,
    };
    let v = rz_wallet::assets(&mut client, mine)?;
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
    let mine = match &keys {
        Some(k) => Some(rz_wallet::account_address(&mut client, k)?),
        None => None,
    };
    let v = rz_wallet::market(&mut client, a.id, mine)?;
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
            rz_core::community::CommunityStatus::Withdrawn => {
                "reconhecimento RETIRADO pela governança".into()
            }
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

fn cmd_name_renew(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let (name, kind) = address_arg(args)?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::RenewName { name, kind },
        fee,
    )?;
    eprintln!("aviso: a taxa de renovação vai para o Pool permanente");
    println!("transação: {id} — aceita, aguardando inclusão em bloco");
    Ok(())
}

fn cmd_resolve(args: &Args) -> Result<(), String> {
    let (name, kind) = address_arg(args)?;
    let (mut client, _) = open(args)?;
    let (target, owner, expires_at) = rz_wallet::resolve(&mut client, &name, kind)?;
    let address = rz_core::community::format_address(&name, kind);
    match (target, owner) {
        (Some(t), _) => {
            println!("{address} → {t}");
            if let Some(o) = owner {
                println!("  dono: {o}");
            }
            if let Some(e) = expires_at {
                println!("  vence na altura {e} (renove com name-renew)");
            }
        }
        (None, Some(o)) => {
            println!("{address} VENCIDO (dono {o}); só o dono pode renovar até a liberação")
        }
        (None, None) => println!("{address} não está registrado"),
    }
    Ok(())
}

// ------------------------------------------------------- Conteúdo

fn cmd_content_pack(args: &Args) -> Result<(), String> {
    let dir = args.req("dir")?;
    let out = args.req("out")?;
    let bundle = rz_wallet::pack_dir(Path::new(dir))?;
    let bytes = bundle.to_canonical_bytes();
    let info = rz_core::content::describe(&bytes)?;
    fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    println!(
        "{} arquivos, {} bytes em {out}",
        bundle.files.len(),
        bytes.len()
    );
    if bundle.get("index.html").is_none() {
        println!("aviso: sem index.html, o Navegador Zero abrirá a lista de arquivos");
    }
    println!("id do conteúdo: {}", info.id());
    Ok(())
}

fn cmd_content_publish(args: &Args) -> Result<(), String> {
    let file = args.req("file")?;
    let bytes = fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let (mut client, _) = open(args)?;
    let id = rz_wallet::publish_content(&mut client, &bytes)?;
    println!("conteúdo {id} hospedado pelo node; os pares o replicam");
    Ok(())
}

fn cmd_content_get(args: &Args) -> Result<(), String> {
    let id = Hash32::from_hex(args.req("id")?).ok_or("--id: hash inválido")?;
    let out = args.req("out")?;
    let (mut client, _) = open(args)?;
    let bytes = rz_wallet::fetch_content(&mut client, &id, std::time::Duration::from_secs(15))?;
    fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    println!("{} bytes verificados gravados em {out}", bytes.len());
    Ok(())
}

fn cmd_manifest_new(args: &Args) -> Result<(), String> {
    let opt_hash = |name: &str| -> Result<Option<Hash32>, String> {
        args.get(name)
            .map(|h| Hash32::from_hex(h).ok_or(format!("--{name}: hash inválido")))
            .transpose()
    };
    let m = rz_core::content::Manifest {
        name: args.req("name")?.into(),
        version: args
            .req("version")?
            .parse()
            .map_err(|_| "--version: valor inválido")?,
        description: args.get("description").unwrap_or("").into(),
        frontend: opt_hash("frontend")?,
        module: opt_hash("module")?,
    };
    let bytes = m.to_bytes();
    let out = args.req("out")?;
    fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    println!("manifesto gravado em {out}");
    println!(
        "hash do manifesto: {}",
        rz_core::content::Manifest::hash_of(&bytes)
    );
    Ok(())
}

fn cmd_manifest_publish(args: &Args) -> Result<(), String> {
    let file = args.req("file")?;
    let bytes = fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let (mut client, _) = open(args)?;
    let h = rz_wallet::publish_manifest(&mut client, &bytes)?;
    println!("manifesto {h} hospedado pelo node");
    Ok(())
}

// ------------------------------------------------------- Exonet Runtime

fn module_file(args: &Args) -> Result<Vec<u8>, String> {
    let file = args.req("file")?;
    let code = fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    rz_core::runtime::validate_module(&code)?;
    Ok(code)
}

fn cmd_module_check(args: &Args) -> Result<(), String> {
    let code = module_file(args)?;
    println!("módulo válido: {} bytes", code.len());
    println!("id: {}", rz_core::runtime::module_id(&code));
    let fee = code.len() as u64 * rz_core::runtime::RuntimeParams::default().module_byte_fee;
    println!(
        "taxa de publicação (parâmetros padrão): {} ZERO, para o Pool",
        format_zero(fee)
    );
    Ok(())
}

fn cmd_module_publish(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let code = module_file(args)?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::PublishModule { code: code.clone() },
        fee,
    )?;
    println!("publicação {id} enviada");
    println!("id do módulo: {}", rz_core::runtime::module_id(&code));
    Ok(())
}

fn module_arg(args: &Args) -> Result<Option<Hash32>, String> {
    match args.req("module")? {
        "nenhum" => Ok(None),
        h => Hash32::from_hex(h)
            .map(Some)
            .ok_or_else(|| "--module: use HEX ou nenhum".into()),
    }
}

fn cmd_module_bind(args: &Args, submit: bool) -> Result<(), String> {
    let keys = load_keys(args)?;
    let module = module_arg(args)?;
    let (mut client, genesis) = open(args)?;
    let community = rz_wallet::community_id(&mut client, args.req("community")?)?;
    if !submit {
        let (_, seq, _) = rz_wallet::module_info(&mut client, community)?;
        let payload = rz_core::runtime::bind_payload(&community, &module, seq);
        let approval =
            rz_core::community::approve(&keys.transparent, &genesis.network_id, &payload);
        println!("{}", rz_crypto::hex::encode(&approval.to_canonical_bytes()));
        return Ok(());
    }
    let approvals = approvals_arg(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::BindModule {
            community,
            module,
            approvals,
        },
        fee,
    )?;
    println!("vinculação {id} enviada");
    Ok(())
}

fn call_args(args: &Args) -> Result<Vec<u8>, String> {
    match (args.get("args"), args.get("args-hex")) {
        (Some(t), None) => Ok(t.as_bytes().to_vec()),
        (None, Some(h)) => rz_crypto::hex::decode(h).ok_or_else(|| "--args-hex inválido".into()),
        (None, None) => Ok(Vec::new()),
        (Some(_), Some(_)) => Err("use --args ou --args-hex, não ambos".into()),
    }
}

fn show_output(out: &[u8]) {
    match std::str::from_utf8(out) {
        Ok(t) if !t.is_empty() && t.chars().all(|c| !c.is_control() || c == '\n') => {
            println!("saída: {t}")
        }
        _ => println!("saída (hex): {}", rz_crypto::hex::encode(out)),
    }
}

fn cmd_call(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let input = call_args(args)?;
    let fuel: u64 = match args.get("fuel") {
        Some(f) => f.parse().map_err(|_| "--fuel: número inválido")?,
        None => 1_000_000,
    };
    let (mut client, genesis) = open(args)?;
    let community = rz_wallet::community_id(&mut client, args.req("community")?)?;
    let (id, r) = rz_wallet::call_module(
        &mut client,
        &genesis,
        &keys,
        community,
        args.req("method")?,
        &input,
        fuel,
    )?;
    println!("chamada {id} na altura {}", r.height);
    println!("combustível usado: {} de {fuel}", r.fuel_used);
    if r.ok {
        show_output(&r.output);
    } else {
        println!("falhou (taxa paga, nada gravado): {}", r.error);
    }
    Ok(())
}

fn cmd_query(args: &Args) -> Result<(), String> {
    let input = call_args(args)?;
    let (mut client, _) = open(args)?;
    let community = rz_wallet::community_id(&mut client, args.req("community")?)?;
    let v = rz_wallet::query(&mut client, community, args.req("method")?, &input, None)?;
    println!("altura {} | combustível {}", v.height, v.fuel_used);
    if v.ok {
        show_output(&v.output);
    } else {
        println!("falhou: {}", v.error);
    }
    Ok(())
}

fn cmd_receipt(args: &Args) -> Result<(), String> {
    let tx = hash_arg(args, "tx")?;
    let (mut client, _) = open(args)?;
    match rz_wallet::receipt(&mut client, tx)? {
        Some(r) => {
            println!(
                "{} na altura {} | método {} | combustível {}",
                if r.ok { "sucesso" } else { "falha" },
                r.height,
                r.method,
                r.fuel_used
            );
            if r.ok {
                show_output(&r.output);
            } else {
                println!("erro: {}", r.error);
            }
        }
        None => println!("sem recibo (pendente, antigo demais ou não é chamada ao Runtime)"),
    }
    Ok(())
}

// ---------------------------------------------------------- Reputação

fn cmd_reputation(args: &Args) -> Result<(), String> {
    let key = match args.get("public-key") {
        Some(h) => rz_crypto::PublicKey::from_hex(h).map_err(|_| "--public-key: chave inválida")?,
        None => load_keys(args)?.transparent.public_key(),
    };
    let (mut client, _) = open(args)?;
    let (points, events) = rz_wallet::reputation(&mut client, key)?;
    let scale = rz_core::governance::SCALE;
    println!("identidade: {key}");
    println!(
        "pontos de contribuição: {}.{:06} (decaem com o tempo)",
        points / scale,
        points % scale
    );
    for e in &events {
        let what = match &e.kind {
            rz_core::reputation::RepEventKind::DefenseContribution { incident, role } => {
                format!(
                    "contribuição defensiva ({}) no incidente {incident}",
                    role.name()
                )
            }
            rz_core::reputation::RepEventKind::Slashed { evidence } => {
                format!("punição por evidência {evidence}")
            }
        };
        println!("  altura {}: {what}", e.height);
    }
    if events.is_empty() {
        println!("  sem eventos registrados além da produção de blocos");
    }
    Ok(())
}

// -------------------------------------------------------------- Versões

fn split_list(v: &str) -> Vec<&str> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

const DEFAULT_SPEC: &str = "spec,docs";
const DEFAULT_SOURCE: &str = "crates,Cargo.toml,Cargo.lock";
const DEFAULT_TESTS: &str = "conformance";

fn cmd_release_new(args: &Args) -> Result<(), String> {
    use rz_core::release::{file_hash, Artifact, Compatibility, Release};
    let root = Path::new(args.get("root").unwrap_or("."));
    let tree = |name: &str, default: &str| {
        rz_wallet::tree_hash_of(root, &split_list(args.get(name).unwrap_or(default)))
    };
    let mut artifacts = Vec::new();
    if let Some(list) = args.get("artifacts") {
        for item in split_list(list) {
            let (name, path) = item
                .split_once('=')
                .ok_or_else(|| format!("--artifacts: esperado nome=arquivo em '{item}'"))?;
            let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            artifacts.push(Artifact {
                name: name.to_string(),
                hash: file_hash(&bytes),
            });
        }
    }
    artifacts.sort_by(|a, b| a.name.cmp(&b.name));
    let notes = match args.get("notes") {
        Some(path) => file_hash(&fs::read(path).map_err(|e| format!("{path}: {e}"))?),
        None => Hash32::ZERO,
    };
    let release = Release {
        version: args.req("version")?.to_string(),
        protocol_version: rz_core::PROTOCOL_VERSION,
        p2p_version: rz_p2p::P2P_VERSION,
        compatibility: Compatibility::parse(args.req("compat")?)
            .ok_or("--compat: use compativel, parcial ou incompativel")?,
        previous: match args.get("previous") {
            Some(h) => Some(Hash32::from_hex(h).ok_or("--previous: hash inválido")?),
            None => None,
        },
        specification: tree("spec", DEFAULT_SPEC)?,
        implementation: tree("source", DEFAULT_SOURCE)?,
        tests: tree("tests", DEFAULT_TESTS)?,
        artifacts,
        notes,
    };
    release.validate()?;
    let out = args.req("out")?;
    fs::write(out, release.to_canonical_bytes()).map_err(|e| format!("{out}: {e}"))?;
    print_release(&release);
    println!("manifesto gravado em {out}");
    Ok(())
}

fn print_release(r: &rz_core::release::Release) {
    println!("versão {} — id {}", r.version, r.id());
    println!(
        "  protocolo {} | P2P {} | {}",
        r.protocol_version,
        r.p2p_version,
        r.compatibility.name()
    );
    match &r.previous {
        Some(p) => println!("  anterior:       {p}"),
        None => println!("  anterior:       nenhuma"),
    }
    println!("  especificação:  {}", r.specification);
    println!("  implementação:  {}", r.implementation);
    println!("  testes:         {}", r.tests);
    for a in &r.artifacts {
        println!("  artefato {}: {}", a.name, a.hash);
    }
}

fn load_release(path: &str) -> Result<rz_core::release::Release, String> {
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    rz_core::release::Release::from_canonical_bytes(&bytes)
        .map_err(|e| format!("{path}: manifesto inválido: {e}"))
}

fn cmd_release_publish(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let release = load_release(args.req("file")?)?;
    let (mut client, genesis) = open(args)?;
    let cost = governance(&mut client, None)?.params.governance.deposit;
    let fee = fee(args, &genesis)?;
    let rid = release.id();
    let tx = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::PublishRelease { release },
        fee,
    )?;
    println!(
        "publicação {tx} enviada — {} ZERO queimados",
        format_zero(cost)
    );
    println!("id da versão: {rid}");
    println!("para adotá-la: propose --category constitucional --release {rid}");
    Ok(())
}

fn cmd_releases(args: &Args) -> Result<(), String> {
    let (mut client, _) = open(args)?;
    let (height, current, list) = rz_wallet::releases(&mut client)?;
    println!("altura {height}");
    match current {
        Some(c) => println!("versão vigente: {c}"),
        None => println!("nenhuma versão adotada pela governança"),
    }
    for r in &list {
        print_release(&r.release);
        match r.adopted_at {
            Some(h) => println!("  publicada em {}, ADOTADA em {h}", r.published_at),
            None => println!("  publicada em {}, não adotada", r.published_at),
        }
    }
    Ok(())
}

fn cmd_release_verify(args: &Args) -> Result<(), String> {
    let release = match args.get("manifest") {
        Some(path) => load_release(path)?,
        None => {
            let (mut client, _) = open(args)?;
            let (_, current, list) = rz_wallet::releases(&mut client)?;
            let want = match args.get("release") {
                Some(h) => Hash32::from_hex(h).ok_or("--release: hash inválido")?,
                None => current.ok_or("nenhuma versão adotada; informe --release")?,
            };
            let rec = list
                .into_iter()
                .find(|r| r.release.id() == want)
                .ok_or("versão não registrada na cadeia")?;
            if rec.adopted_at.is_none() {
                println!("ATENÇÃO: versão publicada, mas NÃO adotada pela governança");
            }
            rec.release
        }
    };
    print_release(&release);
    let mut ok = true;
    if let Some(name) = args.get("artifact") {
        let path = args.req("file")?;
        let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let got = rz_core::release::file_hash(&bytes);
        match release.artifact(name) {
            Some(a) if a.hash == got => println!("artefato {name}: CONFERE"),
            Some(_) => {
                println!("artefato {name}: NÃO CONFERE ({got})");
                ok = false;
            }
            None => return Err(format!("o manifesto não lista o artefato {name}")),
        }
    }
    if let Some(root) = args.get("root") {
        let root = Path::new(root);
        for (label, flag, default, want) in [
            ("especificação", "spec", DEFAULT_SPEC, release.specification),
            (
                "implementação",
                "source",
                DEFAULT_SOURCE,
                release.implementation,
            ),
            ("testes", "tests", DEFAULT_TESTS, release.tests),
        ] {
            let got =
                rz_wallet::tree_hash_of(root, &split_list(args.get(flag).unwrap_or(default)))?;
            if got == want {
                println!("{label}: CONFERE");
            } else {
                println!("{label}: NÃO CONFERE ({got})");
                ok = false;
            }
        }
    }
    if ok {
        Ok(())
    } else {
        Err("verificação falhou: não use estes arquivos".into())
    }
}

// ------------------------------------------------ Ponte e trocas atômicas

fn load_genesis(args: &Args) -> Result<Genesis, String> {
    let path = args.req("genesis")?;
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Genesis::from_canonical_bytes(&bytes).map_err(|e| format!("genesis inválido: {e}"))
}

fn cmd_bridge(args: &Args) -> Result<(), String> {
    let chain = args.req("chain")?;
    let (mut client, _) = open(args)?;
    let v = rz_wallet::bridge_status(&mut client, chain)?;
    match v.best {
        None => println!("rede de origem '{chain}' não configurada no Genesis"),
        Some((h, n)) => {
            println!("rede de origem: {chain}");
            println!(
                "ponta conhecida: altura {n}, bloco {}",
                rz_core::bridge::display_hex(&h)
            );
            println!("confirmações exigidas: {}", v.confirmations);
            println!("ativo representado: {}", v.asset);
            println!("queimas já usadas: {}", v.claimed);
        }
    }
    Ok(())
}

fn cmd_bridge_burn_script(args: &Args) -> Result<(), String> {
    let genesis = load_genesis(args)?;
    let to = Address::from_hex(args.req("to")?).ok_or("--to: endereço inválido")?;
    // Taxa em unidades mínimas da origem (ex.: satoshis).
    let fee: u64 = match args.get("relay-fee") {
        Some(v) => v.parse().map_err(|_| "--relay-fee: número inválido")?,
        None => 0,
    };
    let script = rz_core::bridge::burn_script_with_fee(&genesis.network_id, &to, fee);
    println!("{}", rz_crypto::hex::encode(&script));
    if fee > 0 {
        eprintln!(
            "taxa de retransmissão: {fee} unidades da origem vão para quem trouxer a prova; o restante, para {to}"
        );
    }
    eprintln!(
        "aviso: o valor enviado a esta saída é DESTRUÍDO na rede de origem, sem resgate. \
         A Rede Zero cunha a representação para {to} na rede {}.",
        genesis.network_id
    );
    Ok(())
}

fn cmd_bridge_chain_new(args: &Args) -> Result<(), String> {
    use rz_core::bridge::{chain_config_id, ChainConfig, PowRules};
    let num = |name: &str| -> Result<u64, String> {
        args.req(name)?
            .parse()
            .map_err(|_| format!("--{name}: número inválido"))
    };
    let checkpoint = rz_crypto::hex::decode_array::<80>(args.req("checkpoint")?)
        .ok_or("--checkpoint: cabeçalho de 80 bytes em hex")?;
    let config = ChainConfig {
        name: args.req("name")?.to_string(),
        asset_ref: args.req("asset")?.to_string(),
        decimals: u8::try_from(num("decimals")?).map_err(|_| "--decimals")?,
        rules: PowRules {
            pow_limit: Hash32::from_hex(args.req("pow-limit")?).ok_or("--pow-limit: hex")?,
            retarget: match args.req("retarget")? {
                "sim" => true,
                "nao" | "não" => false,
                _ => return Err("--retarget: sim ou nao".into()),
            },
            interval: u32::try_from(num("interval")?).map_err(|_| "--interval")?,
            target_timespan: u32::try_from(num("timespan")?).map_err(|_| "--timespan")?,
        },
        checkpoint,
        checkpoint_height: num("checkpoint-height")?,
        confirmations: u32::try_from(num("confirmations")?).map_err(|_| "--confirmations")?,
    };
    config.validate()?;
    let out = args.req("out")?;
    fs::write(out, config.to_canonical_bytes()).map_err(|e| format!("{out}: {e}"))?;
    println!(
        "rede {} ({}) — id {}",
        config.name,
        config.asset_ref,
        chain_config_id(&config)
    );
    println!("configuração gravada em {out}");
    Ok(())
}

fn cmd_bridge_chain_publish(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let path = args.req("file")?;
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let config = rz_core::bridge::ChainConfig::from_canonical_bytes(&bytes)
        .map_err(|e| format!("{path}: configuração inválida: {e}"))?;
    let id = rz_core::bridge::chain_config_id(&config);
    let (mut client, genesis) = open(args)?;
    let cost = governance(&mut client, None)?.params.governance.deposit;
    let fee = fee(args, &genesis)?;
    let tx = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::PublishBridgeChain { config },
        fee,
    )?;
    println!(
        "publicação {tx} enviada — {} ZERO queimados",
        format_zero(cost)
    );
    println!("id da configuração: {id}");
    println!("para adotá-la: propose --category constitucional --params bridge_add_chain={id}");
    Ok(())
}

fn cmd_bridge_headers(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let chain = args.req("chain")?.to_string();
    let file = args.req("file")?;
    let text = fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let headers: Vec<[u8; 80]> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| rz_crypto::hex::decode_array::<80>(l).ok_or(format!("cabeçalho inválido: {l}")))
        .collect::<Result<_, _>>()?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    for batch in headers.chunks(rz_core::bridge::MAX_HEADERS_PER_TX) {
        let id = send_account(
            &mut client,
            &genesis,
            &keys,
            TxKind::BridgeHeaders {
                chain: chain.clone(),
                headers: batch.to_vec(),
            },
            fee,
        )?;
        println!("{} cabeçalhos enviados: {id}", batch.len());
        // O próximo lote depende do anterior estar no estado.
        if batch.len() == rz_core::bridge::MAX_HEADERS_PER_TX {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    }
    Ok(())
}

fn cmd_bridge_deposit(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let chain = args.req("chain")?.to_string();
    let tx = rz_crypto::hex::decode(args.req("tx")?).ok_or("--tx: hex inválido")?;
    let mb =
        rz_crypto::hex::decode(args.req("merkleblock")?).ok_or("--merkleblock: hex inválido")?;
    let txid = Hash32(rz_core::bridge::sha256d(&tx));
    let (header, path, index) = rz_core::bridge::path_from_merkleblock(&mb, &txid)?;
    let block = rz_core::bridge::Header::parse(&header).hash();
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::BridgeDeposit {
            chain,
            block,
            tx,
            path,
            index,
        },
        fee,
    )?;
    println!(
        "prova de queima {} enviada: {id}",
        rz_core::bridge::display_hex(&txid)
    );
    Ok(())
}

fn cmd_htlc_secret() -> Result<(), String> {
    let sk = SecretKey::generate();
    let secret = sk.seed();
    println!(
        "segredo (guarde, não compartilhe antes da hora): {}",
        rz_crypto::hex::encode(&secret)
    );
    println!(
        "hash SHA-256 (use nos dois contratos): {}",
        rz_crypto::hex::encode(&rz_core::bridge::sha256(&secret))
    );
    Ok(())
}

fn cmd_htlc_lock(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let to = Address::from_hex(args.req("to")?).ok_or("--to: endereço inválido")?;
    let hashlock = hash_arg(args, "hash")?;
    let blocks: u64 = args
        .req("timeout-blocks")?
        .parse()
        .map_err(|_| "--timeout-blocks: número inválido")?;
    let (mut client, genesis) = open(args)?;
    let spec = args.req("asset")?;
    let (asset, amount) = if spec.eq_ignore_ascii_case("zero") {
        (rz_core::market::AssetId::ZERO, args.amount("amount")?)
    } else {
        let a = find_asset(&mut client, spec, None)?;
        let q = parse_units(args.req("amount")?, a.info.decimals)
            .ok_or("--amount: quantidade inválida")?;
        (a.id, q)
    };
    let (_, _, height) = account(&mut client, keys.address())?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::HtlcLock {
            asset,
            amount,
            recipient: to,
            hashlock,
            timeout: height + 1 + blocks,
        },
        fee,
    )?;
    println!(
        "contrato {id} enviado; vence na altura {}",
        height + 1 + blocks
    );
    println!("o identificador do contrato é o da transação");
    Ok(())
}

fn cmd_htlc_settle(args: &Args, claim: bool) -> Result<(), String> {
    let keys = load_keys(args)?;
    let id = hash_arg(args, "id")?;
    let kind = if claim {
        let preimage =
            rz_crypto::hex::decode(args.req("preimage")?).ok_or("--preimage: hex inválido")?;
        TxKind::HtlcClaim { id, preimage }
    } else {
        TxKind::HtlcRefund { id }
    };
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let tx = send_account(&mut client, &genesis, &keys, kind, fee)?;
    println!(
        "{} enviado: {tx}",
        if claim { "resgate" } else { "devolução" }
    );
    Ok(())
}

fn cmd_htlc(args: &Args) -> Result<(), String> {
    let id = hash_arg(args, "id")?;
    let (mut client, _) = open(args)?;
    match rz_wallet::htlc(&mut client, id)? {
        (Some(h), _) => {
            println!(
                "em aberto: {} de {} para {}",
                h.amount, h.asset, h.recipient
            );
            println!("remetente: {} | vence na altura {}", h.sender, h.timeout);
            println!("hash: {}", h.hashlock);
        }
        (None, Some(s)) => match s.preimage {
            Some(p) => println!(
                "resgatado na altura {}; pré-imagem revelada: {}",
                s.height,
                rz_crypto::hex::encode(&p)
            ),
            None => println!("devolvido ao remetente na altura {}", s.height),
        },
        (None, None) => println!("contrato desconhecido"),
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
    if args.get("secret").is_some() {
        // Sal aleatório de 32 bytes (a semente de uma chave descartável).
        let salt = SecretKey::generate().seed();
        let voter = rz_wallet::account_address(&mut client, &keys)?;
        let commitment = rz_core::governance::vote_commitment(&proposal, &voter, choice, &salt);
        let id = send_account(
            &mut client,
            &genesis,
            &keys,
            TxKind::CommitVote {
                proposal,
                commitment,
            },
            fee,
        )?;
        let salt_hex = rz_crypto::hex::encode(&salt);
        if let Some(out) = args.get("salt-out") {
            fs::write(out, format!("{salt_hex}\n")).map_err(|e| format!("{out}: {e}"))?;
            println!("sal gravado em {out}");
        }
        println!("voto secreto registrado: {id}");
        println!("GUARDE para revelar depois da votação (sem revelar, o voto não conta):");
        println!(
            "  zero-wallet vote-reveal --proposal {proposal} --choice {} --salt {salt_hex}",
            args.req("choice")?
        );
        return Ok(());
    }
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::Vote { proposal, choice },
        fee,
    )?;
    eprintln!(
        "aviso: este voto é público; use --secret quando a governança tiver ligado o voto secreto"
    );
    println!("voto registrado: {id} — aguardando inclusão em bloco");
    Ok(())
}

fn cmd_vote_reveal(args: &Args) -> Result<(), String> {
    let keys = load_keys(args)?;
    let proposal = Hash32::from_hex(args.req("proposal")?).ok_or("--proposal: id inválido")?;
    let choice = Choice::parse(args.req("choice")?).ok_or("--choice: use sim, nao ou abstencao")?;
    let salt_text = match (args.get("salt"), args.get("salt-file")) {
        (Some(s), _) => s.to_string(),
        (None, Some(f)) => fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?,
        (None, None) => return Err("informe --salt HEX ou --salt-file ARQUIVO".into()),
    };
    let salt = rz_crypto::hex::decode_array::<32>(salt_text.trim()).ok_or("sal inválido")?;
    let (mut client, genesis) = open(args)?;
    let fee = fee(args, &genesis)?;
    let id = send_account(
        &mut client,
        &genesis,
        &keys,
        TxKind::RevealVote {
            proposal,
            choice,
            salt,
        },
        fee,
    )?;
    println!("voto revelado: {id} — agora público e contado na apuração");
    Ok(())
}
