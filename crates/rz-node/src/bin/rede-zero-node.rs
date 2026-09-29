//! `rede-zero-node` — executável do Node da DEVNET.
//!
//! ```text
//! rede-zero-node init-devnet --out DIR [--validators N] [--network-id ID]
//!                            [--slot-ms MS] [--finality K] [--faucet ZERO] [--base-port P]
//! rede-zero-node run --genesis FILE --data DIR [--listen ADDR] [--peer ADDR]...
//!                    [--validator-key FILE] [--verbose | --quiet]
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
use rz_core::{format_zero, parse_zero, Allocation, Genesis, NetworkKind, PROTOCOL_VERSION};
use rz_crypto::{keyfile, SecretKey};
use rz_node::{now_ms, LogLevel, Node, NodeConfig};

const USAGE: &str = "\
rede-zero-node — Node da DEVNET da Rede Zero

USO:
  rede-zero-node init-devnet --out DIR [opções]
      --validators N      número de validadores (padrão 3)
      --network-id ID     identificador da rede (padrão rede-zero-devnet-1)
      --slot-ms MS        duração do slot em ms (padrão 2000)
      --finality K        profundidade de finalidade (padrão 5)
      --faucet ZERO       saldo inicial da chave faucet (padrão 1000000)
      --min-fee ZERO      taxa mínima (padrão 0.00001)
      --base-port P       porta do primeiro node (padrão 7100)

  rede-zero-node run --genesis FILE --data DIR [opções]
      --listen ADDR       endereço de escuta (padrão 127.0.0.1:7100)
      --peer ADDR         par inicial (pode repetir)
      --validator-key F   chave do validador (omitir para node não validador)
      --verbose           registros detalhados (inclui endereços de pares)
      --quiet             sem registros

  rede-zero-node genesis-info --genesis FILE

AVISO: DEVNET. O ZERO desta rede não possui valor econômico.
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
    let n: u8 = args.num("validators", 3)?;
    if n == 0 {
        return Err("--validators deve ser ao menos 1".into());
    }
    let network_id = args
        .get("network-id")
        .unwrap_or("rede-zero-devnet-1")
        .to_string();
    let slot_ms: u64 = args.num("slot-ms", 2_000)?;
    let finality: u32 = args.num("finality", 5)?;
    let faucet_amount =
        parse_zero(args.get("faucet").unwrap_or("1000000")).ok_or("--faucet: valor inválido")?;
    let min_fee =
        parse_zero(args.get("min-fee").unwrap_or("0.00001")).ok_or("--min-fee: valor inválido")?;
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
        validators.push(k.public_key());
    }
    let faucet = SecretKey::generate();
    keyfile::save(&out.join("faucet.key"), &faucet).map_err(|e| e.to_string())?;

    let genesis = Genesis {
        protocol_version: PROTOCOL_VERSION,
        kind: NetworkKind::Devnet,
        network_id,
        genesis_time_ms: now_ms(),
        slot_duration_ms: slot_ms,
        finality_depth: finality,
        min_fee,
        max_block_txs: 1_000,
        validators,
        allocations: vec![Allocation {
            address: faucet.public_key().address(),
            amount: faucet_amount,
        }],
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

fn print_genesis(g: &Genesis) {
    println!("  rede:            {} ({:?})", g.network_id, g.kind);
    println!("  genesis:         {}", g.hash());
    println!("  slot:            {} ms", g.slot_duration_ms);
    println!("  finalidade:      {} blocos", g.finality_depth);
    println!("  taxa mínima:     {} ZERO", format_zero(g.min_fee));
    println!("  validadores:     {}", g.validators.len());
    for v in &g.validators {
        println!("    {v}");
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
    let data = PathBuf::from(args.req("data")?);
    let listen: SocketAddr = args
        .get("listen")
        .unwrap_or("127.0.0.1:7100")
        .parse()
        .map_err(|_| "--listen: endereço inválido")?;
    let mut cfg = NodeConfig::new(genesis, data, listen);
    for p in args.all("peer") {
        cfg.bootstrap.push(
            p.parse()
                .map_err(|_| format!("--peer: endereço inválido '{p}'"))?,
        );
    }
    if let Some(k) = args.get("validator-key") {
        let key = keyfile::load(Path::new(k)).map_err(|e| format!("{k}: {e}"))?;
        if !cfg.genesis.validators.contains(&key.public_key()) {
            return Err("a chave informada não é validadora neste Genesis".into());
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

    let node = Node::start(cfg).map_err(|e| format!("falha ao iniciar: {e}"))?;
    eprintln!("AVISO: DEVNET — o ZERO desta rede não possui valor econômico.");

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
