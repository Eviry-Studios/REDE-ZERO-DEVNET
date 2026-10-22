//! `rede-zero-node` — executável do Node da Rede Zero (DEVNET e TESTNET).
//!
//! ```text
//! rede-zero-node init-devnet --out DIR [--validators N] [--network-id ID]
//!                            [--block-ms MS] [--faucet ZERO] [--base-port P]
//! rede-zero-node run --genesis FILE --data DIR [--listen ADDR | --no-listen] [--peer ADDR]...
//!                    [--proxy ADDR] [--advertise ADDR] [--validator-key FILE] [--verbose | --quiet]
//! rede-zero-node genesis-info --genesis FILE
//! ```

use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rz_codec::{Decode, Encode};
use rz_core::{
    format_zero, parse_zero, Allocation, ConsensusParams, Genesis, GenesisValidator, NetworkKind,
    PROTOCOL_VERSION,
};
use rz_crypto::{keyfile, SecretKey};
use rz_node::{LogLevel, Node, NodeConfig};

const USAGE: &str = "\
rede-zero-node — Node da Rede Zero (DEVNET e TESTNET)

USO:
  rede-zero-node init-devnet --out DIR [opções]
      --validators N      número de validadores (padrão 4; tolera ⌊(N−1)/3⌋ falhas)
      --network-id ID     identificador da rede (padrão rede-zero-devnet-1)
      --block-ms MS       intervalo alvo entre blocos em ms (padrão 2000)
      --stake ZERO        ZERO vinculado por validador (padrão 10000)
      --test-asset R:A    cria o ativo externo DE TESTE A da rede R (ex.: testnet-externa:ATV),
                          com 1000000 unidades para o faucet (só DEVNET; ADR-0014)
      --faucet ZERO       saldo inicial da chave faucet (padrão 1000000)
      --min-fee ZERO      taxa mínima (padrão 0.00001)
      --block-reward ZERO emissão por bloco, ao proponente (padrão 0.1; ADR-0025)
      --fee-burn-bps N    parte das taxas queimada, em pontos-base (padrão 5000)
      --base-port P       porta do primeiro node (padrão 7100)

  rede-zero-node genesis-testnet --out FILE --validators FILE --faucet HEX [opções]
      --validators FILE   uma chave pública de validador (hex) por linha; # comenta
      --faucet HEX        chave pública do faucet; recebe a oferta menos os vínculos
      --network-id ID     identificador da rede (padrão rede-zero-testnet-1)
      --supply ZERO       oferta do Genesis (padrão 100000000; ADR-0025)
      --stake ZERO        ZERO vinculado por validador (padrão 10000)
      --block-ms MS       intervalo alvo entre blocos em ms (padrão 2000)
      --min-fee ZERO      taxa mínima (padrão 0.00001)
      --block-reward ZERO emissão por bloco (padrão 0.1)
      --fee-burn-bps N    parte das taxas queimada (padrão 5000)
    Nenhuma chave privada é lida nem gerada (docs/TESTNET.md, Fase 1).

  rede-zero-node run --genesis FILE --data DIR [opções]
      --listen ADDR       endereço de escuta (padrão 127.0.0.1:7100)
      --no-listen         node privado: só disca, não aceita conexões nem anuncia endereço
      --peer ADDR         par inicial: IP:porta ou nome.onion:porta (pode repetir)
      --proxy ADDR        proxy SOCKS5 para conexões de saída (ex.: Tor 127.0.0.1:9050)
      --advertise ADDR    endereço público anunciado (ex.: seu serviço nome.onion:porta)
      --max-inbound-per-ip N  conexões de entrada por IP (padrão 8)
      --validator-key F   chave do validador (omitir para node não validador)
      --passphrase-file F senha da chave do validador, se cifrada (ou RZ_KEY_PASSPHRASE)
      --content-quota-mb N  cota de conteúdo da Exonet hospedado (padrão 1024 MiB)
      --checkpoint H:ID   ponto de verificação: o bloco da altura H deve ter o id
                          ID (obtenha de fontes em que confia; pode repetir)
      --verbose           registros detalhados (endereços de pares; nunca de clientes)
      --quiet             sem registros

  Privacidade de IP (ADR-0011): para operar só por Tor, use --proxy e
  --advertise com um serviço onion apontando para --listen 127.0.0.1:PORTA.

  rede-zero-node genesis-info --genesis FILE

AVISO: DEVNET e TESTNET. O ZERO destas redes não possui valor econômico.
";

struct Args {
    flags: HashMap<String, Vec<String>>,
    switches: Vec<String>,
}

impl Args {
    fn parse(raw: &[String]) -> Result<Self, String> {
        let mut flags: HashMap<String, Vec<String>> = HashMap::new();
        let mut switches = Vec::new();
        let mut it = raw.iter().peekable();
        while let Some(a) = it.next() {
            let Some(name) = a.strip_prefix("--") else {
                return Err(format!("argumento inesperado: {a}"));
            };
            match it.peek() {
                Some(v) if !v.starts_with("--") => {
                    flags.entry(name.into()).or_default().push((*v).clone());
                    it.next();
                }
                _ => switches.push(name.into()),
            }
        }
        Ok(Self { flags, switches })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.flags
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    fn all(&self, name: &str) -> &[String] {
        self.flags.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    fn req(&self, name: &str) -> Result<&str, String> {
        self.get(name)
            .ok_or_else(|| format!("--{name} é obrigatório"))
    }

    fn has(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    fn num<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.get(name) {
            None => Ok(default),
            Some(v) => v
                .parse()
                .map_err(|_| format!("--{name}: valor inválido '{v}'")),
        }
    }
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = raw.split_first() else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let result = Args::parse(rest).and_then(|args| match cmd.as_str() {
        "init-devnet" => init_devnet(&args),
        "genesis-testnet" => genesis_testnet(&args),
        "run" => run(&args),
        "genesis-info" => genesis_info(&args),
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

fn load_genesis(path: &Path) -> Result<Genesis, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let g = Genesis::from_canonical_bytes(&bytes).map_err(|e| format!("genesis inválido: {e}"))?;
    g.validate().map_err(|e| format!("genesis inválido: {e}"))?;
    Ok(g)
}

fn init_devnet(args: &Args) -> Result<(), String> {
    let out = PathBuf::from(args.req("out")?);
    let n: u8 = args.num("validators", 4)?;
    if n == 0 {
        return Err("--validators deve ser ao menos 1".into());
    }
    let network_id = args
        .get("network-id")
        .unwrap_or("rede-zero-devnet-1")
        .to_string();
    let block_ms: u64 = args.num("block-ms", 2_000)?;
    let stake =
        parse_zero(args.get("stake").unwrap_or("10000")).ok_or("--stake: valor inválido")?;
    let faucet_amount =
        parse_zero(args.get("faucet").unwrap_or("1000000")).ok_or("--faucet: valor inválido")?;
    let min_fee =
        parse_zero(args.get("min-fee").unwrap_or("0.00001")).ok_or("--min-fee: valor inválido")?;
    let monetary = monetary_args(args)?;
    let base_port: u16 = args.num("base-port", 7100)?;

    if out.join("genesis.bin").exists() {
        return Err(format!(
            "{} já contém uma DEVNET; use outro diretório",
            out.display()
        ));
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;

    let mut validators = Vec::new();
    for i in 0..n {
        let k = SecretKey::generate();
        keyfile::save(&out.join(format!("validator-{i}.key")), &k).map_err(|e| e.to_string())?;
        validators.push(GenesisValidator::new(k.public_key(), stake));
    }
    let faucet = SecretKey::generate();
    keyfile::save(&out.join("faucet.key"), &faucet).map_err(|e| e.to_string())?;

    // Cada validador recebe uma pequena alocação para pagar taxas de
    // participação na governança (voto na câmara de contribuição).
    let mut allocations: Vec<Allocation> = validators
        .iter()
        .map(|v| Allocation {
            address: v.key.address(),
            amount: 1_000 * rz_core::UNITS_PER_ZERO,
        })
        .collect();
    allocations.push(Allocation {
        address: faucet.public_key().address(),
        amount: faucet_amount,
    });
    allocations.sort_by_key(|a| a.address);
    let mut assets = Vec::new();
    if let Some(spec) = args.get("test-asset") {
        let (network, asset_ref) = spec.split_once(':').ok_or("--test-asset: use REDE:ATIVO")?;
        assets.push(rz_core::genesis::GenesisAsset {
            network: network.into(),
            asset_ref: asset_ref.into(),
            decimals: 8,
            allocations: vec![Allocation {
                address: faucet.public_key().address(),
                amount: 1_000_000 * 100_000_000,
            }],
        });
    }
    let genesis = Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id,
        consensus: ConsensusParams::devnet(block_ms),
        min_fee,
        max_block_txs: 1_000,
        validators,
        allocations,
        governance: rz_core::GovernanceParams::for_block_ms(block_ms),
        assets,
        bridges: vec![],
        monetary,
    };
    genesis.validate().map_err(|e| e.to_string())?;
    let genesis_path = out.join("genesis.bin");
    fs::write(&genesis_path, genesis.to_canonical_bytes()).map_err(|e| e.to_string())?;

    println!("DEVNET criada em {}", out.display());
    print_genesis(&genesis);
    println!("\nChaves geradas localmente (NÃO versione estes arquivos):");
    for i in 0..n {
        println!("  {}/validator-{i}.key", out.display());
    }
    println!("  {}/faucet.key", out.display());
    println!("\nPara iniciar os nodes (um terminal por node):");
    for i in 0..n {
        let port = base_port + u16::from(i);
        let peer = if i == 0 {
            String::new()
        } else {
            format!(" --peer 127.0.0.1:{base_port}")
        };
        println!(
            "  rede-zero-node run --genesis {g} --data {d}/node-{i} --listen 127.0.0.1:{port} --validator-key {d}/validator-{i}.key{peer}",
            g = genesis_path.display(),
            d = out.display(),
        );
    }
    Ok(())
}

/// Política monetária pela linha de comando; padrão ADR-0025.
fn monetary_args(args: &Args) -> Result<rz_core::monetary::MonetaryParams, String> {
    Ok(rz_core::monetary::MonetaryParams {
        block_reward: parse_zero(args.get("block-reward").unwrap_or("0.1"))
            .ok_or("--block-reward: valor inválido")?,
        fee_burn_bps: args.num("fee-burn-bps", 5_000)?,
    })
}

/// Lê uma chave pública hexadecimal por linha; linhas vazias e `#` são
/// ignoradas.
fn read_public_keys(path: &Path) -> Result<Vec<rz_crypto::PublicKey>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(i, l)| (i, l.split('#').next().unwrap_or("").trim()))
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, l)| {
            rz_crypto::PublicKey::from_hex(l)
                .map_err(|_| format!("{}:{}: chave pública inválida", path.display(), i + 1))
        })
        .collect()
}

/// Genesis da TESTNET a partir das chaves públicas dos operadores
/// (`docs/TESTNET.md`). Nenhuma chave privada passa por aqui.
fn genesis_testnet(args: &Args) -> Result<(), String> {
    let out = PathBuf::from(args.req("out")?);
    if out.exists() {
        return Err(format!("{} já existe", out.display()));
    }
    let keys = read_public_keys(Path::new(args.req("validators")?))?;
    let faucet = rz_crypto::PublicKey::from_hex(args.req("faucet")?)
        .map_err(|_| "--faucet: chave pública inválida")?;
    if keys.contains(&faucet) {
        return Err("o faucet não pode ser validador (docs/TESTNET.md)".into());
    }
    let network_id = args
        .get("network-id")
        .unwrap_or("rede-zero-testnet-1")
        .to_string();
    let block_ms: u64 = args.num("block-ms", 2_000)?;
    let supply =
        parse_zero(args.get("supply").unwrap_or("100000000")).ok_or("--supply: valor inválido")?;
    let stake =
        parse_zero(args.get("stake").unwrap_or("10000")).ok_or("--stake: valor inválido")?;
    let min_fee =
        parse_zero(args.get("min-fee").unwrap_or("0.00001")).ok_or("--min-fee: valor inválido")?;
    let bonded = stake
        .checked_mul(keys.len() as u64)
        .ok_or("oferta excede u64")?;
    let faucet_amount = supply
        .checked_sub(bonded)
        .filter(|a| *a > 0)
        .ok_or("--supply menor que a soma dos vínculos")?;
    let genesis = Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Testnet,
        network_id,
        consensus: ConsensusParams::testnet(block_ms),
        min_fee,
        max_block_txs: 1_000,
        validators: keys
            .iter()
            .map(|k| GenesisValidator::new(*k, stake))
            .collect(),
        allocations: vec![Allocation {
            address: faucet.address(),
            amount: faucet_amount,
        }],
        governance: rz_core::GovernanceParams::for_block_ms(block_ms),
        assets: vec![],
        bridges: vec![],
        monetary: monetary_args(args)?,
    };
    genesis.validate().map_err(|e| e.to_string())?;
    fs::write(&out, genesis.to_canonical_bytes()).map_err(|e| e.to_string())?;
    println!("TESTNET gravada em {}", out.display());
    print_genesis(&genesis);
    println!("\nPublique o arquivo e o hash; cada operador confere com genesis-info.");
    Ok(())
}

fn print_genesis(g: &Genesis) {
    println!("  rede:            {} ({:?})", g.network_id, g.kind);
    println!("  genesis:         {}", g.hash());
    let c = &g.consensus;
    println!("  consenso:        Zero-BFT (finalidade imediata)");
    println!("  bloco:           {} ms", c.block_interval_ms);
    println!("  época:           {} blocos", c.epoch_blocks);
    println!("  vínculo mínimo:  {} ZERO", format_zero(c.min_bond));
    println!("  desvinculação:   {} blocos", c.unbonding_blocks);
    println!("  punição:         {} bps", c.slash_bps);
    println!("  taxa mínima:     {} ZERO", format_zero(g.min_fee));
    println!(
        "  emissão:         {} ZERO por bloco; {} bps das taxas queimados",
        format_zero(g.monetary.block_reward),
        g.monetary.fee_burn_bps
    );
    println!("  validadores:     {}", g.validators.len());
    for v in &g.validators {
        println!("    {} ({} ZERO)", v.key, format_zero(v.stake));
    }
    for a in &g.assets {
        println!(
            "  ativo de teste:  {}:{} ({}) — NÃO representa nada na rede de origem",
            a.network,
            a.asset_ref,
            a.id()
        );
    }
    for a in &g.allocations {
        println!(
            "  alocação:        {} → {} ZERO",
            a.address,
            format_zero(a.amount)
        );
    }
}

fn genesis_info(args: &Args) -> Result<(), String> {
    let g = load_genesis(Path::new(args.req("genesis")?))?;
    print_genesis(&g);
    Ok(())
}

fn run(args: &Args) -> Result<(), String> {
    let genesis = load_genesis(Path::new(args.req("genesis")?))?;
    let kind = genesis.kind;
    let data = PathBuf::from(args.req("data")?);
    let listen: SocketAddr = args
        .get("listen")
        .unwrap_or("127.0.0.1:7100")
        .parse()
        .map_err(|_| "--listen: endereço inválido")?;
    let mut cfg = NodeConfig::new(genesis, data, listen);
    if args.has("no-listen") {
        cfg.listen = None;
    }
    if let Some(p) = args.get("proxy") {
        cfg.proxy = Some(p.parse().map_err(|_| "--proxy: endereço inválido")?);
    }
    if let Some(a) = args.get("advertise") {
        cfg.advertise = Some(a.parse().map_err(|e| format!("--advertise: {e}"))?);
    }
    cfg.max_inbound_per_ip = args.num("max-inbound-per-ip", cfg.max_inbound_per_ip)?;
    let quota_mb: u64 = args.num("content-quota-mb", cfg.content_quota >> 20)?;
    cfg.content_quota = quota_mb.saturating_mul(1 << 20);
    for p in args.all("peer") {
        cfg.bootstrap.push(
            p.parse()
                .map_err(|_| format!("--peer: endereço inválido '{p}'"))?,
        );
    }
    for c in args.all("checkpoint") {
        let (h, id) = c
            .split_once(':')
            .ok_or_else(|| format!("--checkpoint: use ALTURA:ID, recebido '{c}'"))?;
        let h: u64 = h
            .parse()
            .map_err(|_| format!("--checkpoint: altura inválida '{h}'"))?;
        let id = rz_crypto::Hash32::from_hex(id)
            .ok_or_else(|| format!("--checkpoint: id inválido '{id}'"))?;
        cfg.checkpoints.push((h, rz_core::BlockId(id)));
    }
    if let Some(k) = args.get("validator-key") {
        let key = keyfile::load_with(Path::new(k), || {
            keyfile::passphrase(args.get("passphrase-file").map(Path::new))
        })
        .map_err(|e| format!("{k}: {e}"))?;
        if !cfg
            .genesis
            .validators
            .iter()
            .any(|v| v.key == key.public_key())
        {
            eprintln!(
                "aviso: a chave não é validadora no Genesis; participará do consenso \
                 apenas após vincular ZERO (Bond) e a próxima época"
            );
        }
        cfg.validator_key = Some(key);
    }
    cfg.log = if args.has("quiet") {
        LogLevel::Quiet
    } else if args.has("verbose") {
        LogLevel::Debug
    } else {
        LogLevel::Info
    };

    if cfg.proxy.is_none() && cfg.bootstrap.iter().any(|p| p.is_onion()) {
        return Err("pares .onion exigem --proxy (Tor)".into());
    }
    let node = Node::start(cfg).map_err(|e| format!("falha ao iniciar: {e}"))?;
    if kind != NetworkKind::Mainnet {
        eprintln!(
            "AVISO: {} — o ZERO desta rede não possui valor econômico.",
            format!("{kind:?}").to_uppercase()
        );
    }

    let stop = Arc::new(AtomicBool::new(false));
    // Sem dependências para sinais: em terminal interativo, encerra com
    // Ctrl+D; em qualquer caso, SIGINT/SIGTERM encerram o processo.
    if std::io::stdin().is_terminal() {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut buf = String::new();
            while std::io::stdin()
                .read_line(&mut buf)
                .map(|n| n > 0)
                .unwrap_or(false)
            {
                buf.clear();
            }
            stop.store(true, Ordering::SeqCst);
        });
    }
    let mut last = None;
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_secs(10));
        let s = node.status();
        if last.as_ref() != Some(&s) {
            eprintln!(
                "status: altura {} | finalizado {} | pares {} | mempool {}",
                s.height, s.finalized_height, s.peers, s.mempool
            );
            last = Some(s);
        }
    }
    Ok(())
}
