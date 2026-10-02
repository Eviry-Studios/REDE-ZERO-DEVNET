//! Navegador Zero (ADR-0017).
//!
//! zero-navegador --genesis FILE --node ADDR [--key FILE] [--data DIR] [--listen 127.0.0.1:PORTA]
//!                [--proxy ADDR] [--node-id HEX] [--direct]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rz_codec::Decode;
use rz_core::Genesis;
use rz_crypto::keyfile;
use rz_p2p::PeerAddr;
use rz_wallet::NetOptions;

const USAGE: &str = "\
Navegador Zero — interface local para a Exonet

  zero-navegador --genesis FILE --node ADDR [opções]
      --key FILE          chave da Wallet (sem ela, somente leitura)
      --data DIR          onde guardar identificadores fixados (padrão ./navegador-data)
      --listen ADDR       endereço local (padrão 127.0.0.1:7300; só loopback)
      --proxy ADDR        Tor/SOCKS5 até o Node (o Node não vê seu IP)
      --node-id HEX       identidade esperada do Node
      --direct            aceita expor seu IP a um Node remoto sem proxy

Abra a interface no navegador do sistema em http://navegador.localhost:PORTA.
Publicações abrem em http://NOME.TIPO.localhost:PORTA, cada uma isolada das
demais e sem acesso a servidores externos. As chaves nunca são entregues às
publicações: operações aparecem em Pedidos para você aprovar.
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = BTreeMap::new();
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        if a == "help" || a == "--help" || a == "-h" {
            print!("{USAGE}");
            return Ok(());
        }
        let name = a
            .strip_prefix("--")
            .ok_or(format!("argumento inesperado: {a}\n\n{USAGE}"))?
            .to_string();
        let value = match it.peek() {
            Some(v) if !v.starts_with("--") => it.next().unwrap_or_default(),
            _ => String::new(),
        };
        args.insert(name, value);
    }
    let req = |k: &str| {
        args.get(k)
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or(format!("--{k} é obrigatório\n\n{USAGE}"))
    };
    let gpath = req("genesis")?;
    let bytes = std::fs::read(&gpath).map_err(|e| format!("{gpath}: {e}"))?;
    let genesis =
        Genesis::from_canonical_bytes(&bytes).map_err(|e| format!("genesis inválido: {e}"))?;
    let node: PeerAddr = req("node")?.parse().map_err(|e| format!("--node: {e}"))?;
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
        allow_direct: args.contains_key("direct"),
    };
    if node.is_onion() && net.proxy.is_none() {
        return Err("endereços .onion exigem --proxy (Tor)".into());
    }
    if net.proxy.is_none() && !node.is_loopback() && !net.allow_direct {
        return Err(rz_wallet::DIRECT_REFUSED.into());
    }
    let key = match args.get("key") {
        Some(k) => Some(keyfile::load(Path::new(k)).map_err(|e| format!("{k}: {e}"))?),
        None => None,
    };
    let listen: SocketAddr = args
        .get("listen")
        .map(String::as_str)
        .unwrap_or("127.0.0.1:7300")
        .parse()
        .map_err(|_| "--listen: endereço inválido")?;
    let data_dir = PathBuf::from(
        args.get("data")
            .map(String::as_str)
            .unwrap_or("navegador-data"),
    );
    let server = rz_browser::Server::start(rz_browser::Config {
        genesis,
        node,
        net,
        key,
        data_dir,
        listen,
    })
    .map_err(|e| e.to_string())?;
    println!("Navegador Zero em {}", server.ui_url());
    println!("(Ctrl+C para encerrar)");
    loop {
        std::thread::park();
    }
}
