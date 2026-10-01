//! Navegador Zero (ADR-0017, `spec/BROWSER.md`).
//!
//! Interface local para a Exonet. Roda na máquina da pessoa e usa o
//! navegador web do sistema como tela:
//!
//! * resolve `zero://` pelo Node e baixa as publicações **verificando cada
//!   pedaço contra o identificador registrado** na cadeia;
//! * serve cada publicação numa origem própria em `localhost`, com uma
//!   política de conteúdo que impede acesso a qualquer servidor externo (o IP
//!   da pessoa não vaza para terceiros) e isola publicações entre si;
//! * mantém as chaves fora das publicações: operações são pedidos que a
//!   pessoa aprova na interface do Navegador, outra origem, sem scripts;
//! * oferece uma identidade diferente por site, que não revela a Wallet.
//!
//! O Navegador não tem nenhuma autoridade sobre o protocolo (REQ-065,
//! INV-008): é um cliente como qualquer outro, e o protocolo não sabe qual
//! software o usa (AC-BRW-004).

mod html;
pub mod http;
pub mod origin;
pub mod pedido;
mod ui;

use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rand_core::{OsRng, RngCore};
use rz_codec::Decode;
use rz_core::community::{CommunityStatus, NameKind};
use rz_core::content::{Bundle, Manifest};
use rz_core::Genesis;
use rz_crypto::{Hash32, SecretKey};
use rz_p2p::{Client, PeerAddr};
use rz_wallet::{Keys, NetOptions};

use crate::html::esc;
use crate::http::{Request, Response};
pub use crate::origin::Origin;
use crate::pedido::{Operation, Pedidos, State};

/// Política das publicações: só a própria origem. Nada de servidores
/// externos (vazaria o IP), quadros, plugins ou envio de formulários para
/// fora.
pub const SITE_CSP: &str =
    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; connect-src 'self'; \
worker-src 'self'; object-src 'none'; frame-src 'none'; frame-ancestors 'none'; \
base-uri 'none'; form-action 'self'";
/// Política da interface do Navegador: nenhum script.
pub const UI_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src 'self' data:; \
form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

const MAX_CONNECTIONS: usize = 64;
const CACHE_BYTES: usize = 64 << 20;
const FETCH_WAIT: Duration = Duration::from_secs(15);

pub struct Config {
    pub genesis: Genesis,
    pub node: PeerAddr,
    pub net: NetOptions,
    /// Chave da Wallet. Sem ela, o Navegador só lê.
    pub key: Option<SecretKey>,
    /// Onde guardar os identificadores fixados.
    pub data_dir: PathBuf,
    /// Endereço local de escuta; sempre loopback.
    pub listen: SocketAddr,
}

pub(crate) struct Navegador {
    pub genesis: Genesis,
    pub node: PeerAddr,
    pub net: NetOptions,
    pub keys: Option<Keys>,
    pub data_dir: PathBuf,
    pub port: u16,
    /// Token anti-falsificação dos formulários da interface.
    pub token: String,
    client: Mutex<Option<Client>>,
    cache: Mutex<BTreeMap<Hash32, Arc<Bundle>>>,
    pub pins: Mutex<BTreeMap<String, Hash32>>,
    pub pedidos: Mutex<Pedidos>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Navegador em execução.
pub struct Server {
    nav: Arc<Navegador>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(cfg: Config) -> io::Result<Server> {
        if !cfg.listen.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "o Navegador só escuta em loopback (127.0.0.1)",
            ));
        }
        std::fs::create_dir_all(&cfg.data_dir)?;
        let listener = TcpListener::bind(cfg.listen)?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let nav = Arc::new(Navegador {
            pins: Mutex::new(load_pins(&cfg.data_dir)),
            genesis: cfg.genesis,
            node: cfg.node,
            net: cfg.net,
            keys: cfg.key.map(Keys::from_secret),
            data_dir: cfg.data_dir,
            port,
            token: random_hex(32),
            client: Mutex::new(None),
            cache: Mutex::new(BTreeMap::new()),
            pedidos: Mutex::new(Pedidos::default()),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (n, s) = (nav.clone(), stop.clone());
        let thread = thread::spawn(move || accept_loop(n, s, listener));
        Ok(Server {
            nav,
            stop,
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.nav.port
    }

    /// Endereço da interface, para abrir no navegador do sistema.
    pub fn ui_url(&self) -> String {
        Origin::Ui.url(self.nav.port)
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(nav: Arc<Navegador>, stop: Arc<AtomicBool>, listener: TcpListener) {
    let active = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer.ip().is_loopback() || active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    continue;
                }
                let _ = stream.set_nonblocking(false);
                active.fetch_add(1, Ordering::SeqCst);
                let (n, a) = (nav.clone(), active.clone());
                thread::spawn(move || {
                    serve(&n, &stream);
                    a.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(15));
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn serve(nav: &Navegador, stream: &TcpStream) {
    let resp = match http::read_request(stream) {
        Ok(Some(req)) => nav.handle(&req),
        Ok(None) => Response::text(400, "requisição inválida"),
        Err(_) => return,
    };
    let _ = http::write_response(stream, &resp);
}

fn security_headers(r: Response, csp: &str) -> Response {
    r.with("Content-Security-Policy", csp)
        .with("X-Content-Type-Options", "nosniff")
        .with("Referrer-Policy", "no-referrer")
        .with("Cross-Origin-Opener-Policy", "same-origin")
        .with("Cross-Origin-Resource-Policy", "same-origin")
        .with("X-DNS-Prefetch-Control", "off")
        .with(
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=(), usb=(), serial=(), bluetooth=(), payment=()",
        )
        .with("Cache-Control", "no-store")
}

/// Resultado da resolução de uma publicação.
pub(crate) enum Site {
    Content {
        id: Hash32,
        bundle: Arc<Bundle>,
    },
    /// O identificador mudou desde que foi fixado.
    Changed {
        pinned: Hash32,
        current: Hash32,
    },
}

impl Navegador {
    fn handle(&self, req: &Request) -> Response {
        let Some(origin) = Origin::parse(req.host(), self.port) else {
            let local = [
                format!("127.0.0.1:{}", self.port),
                format!("localhost:{}", self.port),
            ];
            if local.iter().any(|h| h == req.host()) {
                return Response::redirect(&format!("{}/", Origin::Ui.url(self.port)));
            }
            return Response::text(421, "host não reconhecido pelo Navegador Zero");
        };
        match origin {
            Origin::Ui => security_headers(ui::handle(self, req), UI_CSP),
            o => self.site(o, req),
        }
    }

    // ----------------------------------------------------------------- Node

    /// Executa `f` com a conexão ao Node (reconectando se preciso).
    pub fn with_client<T>(
        &self,
        f: impl FnOnce(&mut Client) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = lock(&self.client);
        if guard.is_none() {
            *guard = Some(rz_wallet::connect_to(&self.node, &self.genesis, &self.net)?);
        }
        let Some(client) = guard.as_mut() else {
            return Err("sem conexão".into());
        };
        let r = f(client);
        if r.is_err() {
            // A conexão pode ter caído: a próxima chamada reconecta.
            *guard = None;
        }
        r
    }

    pub fn bundle(&self, id: &Hash32) -> Result<Arc<Bundle>, String> {
        if let Some(b) = lock(&self.cache).get(id) {
            return Ok(b.clone());
        }
        let bytes = self.with_client(|c| rz_wallet::fetch_content(c, id, FETCH_WAIT))?;
        let bundle = Arc::new(
            Bundle::from_canonical_bytes(&bytes).map_err(|e| format!("pacote inválido: {e}"))?,
        );
        let mut cache = lock(&self.cache);
        let size = |b: &Bundle| b.files.iter().map(|f| f.data.len()).sum::<usize>();
        while !cache.is_empty() && cache.values().map(|b| size(b)).sum::<usize>() > CACHE_BYTES {
            if let Some(k) = cache.keys().next().copied() {
                cache.remove(&k);
            }
        }
        cache.insert(*id, bundle.clone());
        Ok(bundle)
    }

    /// `(valor fixável, objeto da interface)` de uma origem. Para Comunidades,
    /// o valor fixável é o manifesto (fixa a versão, THR-COM-002).
    pub fn resolve(&self, o: &Origin) -> Result<(Hash32, Hash32), String> {
        match o {
            Origin::Ui => Err("origem da interface".into()),
            Origin::Id(h) => Ok((*h, *h)),
            Origin::Named {
                name,
                kind: NameKind::Community,
            } => {
                let (_, c) = self.with_client(|cl| rz_wallet::community(cl, name))?;
                let c = c.ok_or("Comunidade não encontrada")?;
                if c.status != CommunityStatus::Recognized {
                    return Err(
                        "Comunidade declarada, ainda não reconhecida pela governança".into(),
                    );
                }
                let frontend = self.frontend_of(&c.manifest_hash)?;
                Ok((c.manifest_hash, frontend))
            }
            Origin::Named { name, kind } => {
                let (target, _) = self.with_client(|cl| rz_wallet::resolve(cl, name, *kind))?;
                let t = target.ok_or("nome não registrado")?;
                Ok((t, t))
            }
        }
    }

    pub fn manifest(&self, h: &Hash32) -> Result<Option<Manifest>, String> {
        let bytes = self.with_client(|c| rz_wallet::fetch_manifest(c, h, FETCH_WAIT))?;
        Ok(bytes.and_then(|b| Manifest::from_bytes(&b)))
    }

    fn frontend_of(&self, manifest: &Hash32) -> Result<Hash32, String> {
        self.manifest(manifest)?
            .ok_or("manifesto indisponível ou sem formato estruturado")?
            .frontend
            .ok_or_else(|| "a Comunidade não publicou interface".into())
    }

    pub fn site_content(&self, o: &Origin) -> Result<Site, String> {
        let (value, content) = self.resolve(o)?;
        if !matches!(o, Origin::Id(_)) {
            let key = o.address();
            let pinned = lock(&self.pins).get(&key).copied();
            match pinned {
                Some(p) if p != value => {
                    return Ok(Site::Changed {
                        pinned: p,
                        current: value,
                    })
                }
                Some(_) => {}
                // Primeiro acesso: fixa (confiança no primeiro uso).
                None => self.pin(&key, value),
            }
        }
        Ok(Site::Content {
            id: content,
            bundle: self.bundle(&content)?,
        })
    }

    pub fn pin(&self, address: &str, value: Hash32) {
        let mut pins = lock(&self.pins);
        pins.insert(address.to_string(), value);
        save_pins(&self.data_dir, &pins);
    }

    // ----------------------------------------------------------- publicações

    fn site(&self, o: Origin, req: &Request) -> Response {
        if let Some(api) = req.path.strip_prefix("/.zero/") {
            return security_headers(self.site_api(&o, api, req), SITE_CSP);
        }
        if req.method != "GET" && req.method != "HEAD" {
            return Response::text(405, "método não permitido");
        }
        let resp = match self.site_content(&o) {
            Ok(Site::Content { id, bundle }) => serve_file(&o, &id, &bundle, &req.path),
            Ok(Site::Changed { pinned, current }) => {
                let body = format!(
                    "<!doctype html><meta charset=\"utf-8\"><title>Identificador alterado</title>\
<body style=\"font-family:sans-serif;max-width:40rem;margin:2rem auto\">\
<h1>O identificador de {} mudou</h1><p>Fixado: <code>{}</code><br>Atual: <code>{}</code></p>\
<p>Isso pode ser uma atualização legítima ou uma troca maliciosa. O conteúdo não foi aberto.</p>\
<p><a href=\"{}/ir?endereco={}\">Revisar no Navegador Zero</a></p></body>",
                    esc(&o.address()),
                    pinned,
                    current,
                    Origin::Ui.url(self.port),
                    http::percent_encode(&o.address())
                );
                return security_headers(Response::html(409, body), UI_CSP);
            }
            Err(e) => Response::html(
                502,
                format!(
                    "<!doctype html><meta charset=\"utf-8\"><title>Indisponível</title>\
<body style=\"font-family:sans-serif;max-width:40rem;margin:2rem auto\"><h1>{}</h1><p>{}</p>\
<p><a href=\"{}/\">Navegador Zero</a></p></body>",
                    esc(&o.address()),
                    esc(&e),
                    Origin::Ui.url(self.port)
                ),
            ),
        };
        let mut resp = security_headers(resp, SITE_CSP);
        if req.method == "HEAD" {
            resp.body.clear();
        }
        resp
    }

    /// API das publicações: criar e consultar pedidos de assinatura.
    fn site_api(&self, o: &Origin, api: &str, req: &Request) -> Response {
        match (req.method.as_str(), api) {
            ("POST", "pedido") => {
                // Só a própria publicação cria pedidos em seu nome: um
                // formulário de outra origem chega com outro `Origin`.
                if req.header("origin") != Some(o.url(self.port).as_str()) {
                    return Response::json(403, "{\"erro\":\"origem inválida\"}");
                }
                match self.create_pedido(o.clone(), &req.form()) {
                    Ok(id) => {
                        let json = lock(&self.pedidos)
                            .items
                            .get(&id)
                            .map(|p| p.json())
                            .unwrap_or_default();
                        Response::json(200, json)
                    }
                    Err(e) => Response::json(400, format!("{{\"erro\":{}}}", html::json_str(&e))),
                }
            }
            ("GET", p) if p.starts_with("pedido/") => {
                let id = &p["pedido/".len()..];
                match lock(&self.pedidos).items.get(id) {
                    Some(ped) if ped.origin == *o => Response::json(200, ped.json()),
                    _ => Response::json(404, "{\"erro\":\"pedido não encontrado\"}"),
                }
            }
            _ => Response::json(404, "{\"erro\":\"rota desconhecida\"}"),
        }
    }

    pub fn create_pedido(
        &self,
        origin: Origin,
        form: &BTreeMap<String, String>,
    ) -> Result<String, String> {
        let op = pedido::parse(form, || {
            self.with_client(|c| rz_wallet::assets(c, None))
                .map(|v| v.assets)
        })?;
        if matches!(op, Operation::Sign { .. }) && origin == Origin::Ui {
            return Err("autenticação só faz sentido para uma publicação".into());
        }
        let id = random_hex(16);
        lock(&self.pedidos).add(id.clone(), origin, op)?;
        Ok(id)
    }

    /// Executa um pedido aprovado pela pessoa na interface.
    pub fn execute(&self, id: &str) {
        let Some(p) = lock(&self.pedidos).items.get(id).cloned() else {
            return;
        };
        if p.state != State::Pending {
            return;
        }
        let result = self.run(&p.origin, &p.op);
        let state = match result {
            Ok(s) => s,
            Err(e) => State::Failed(e),
        };
        lock(&self.pedidos).resolve(id, state);
    }

    fn run(&self, origin: &Origin, op: &Operation) -> Result<State, String> {
        let keys = self
            .keys
            .as_ref()
            .ok_or("Navegador sem chave: inicie com --key")?;
        if let Operation::Sign { message } = op {
            let address = origin.address();
            let sk = rz_core::content::site_secret(&keys.transparent.seed(), &address);
            let sig = sk.sign(
                rz_core::content::EXONET_LOGIN,
                &self.genesis.network_id,
                &rz_core::content::login_payload(&address, message),
            );
            return Ok(State::Approved {
                txid: None,
                key: Some(sk.public_key().to_hex()),
                signature: Some(sig.to_hex()),
            });
        }
        let fee = self.genesis.min_fee;
        let txid = self.with_client(|c| {
            let (_, _, height) = rz_wallet::account(c, keys.address())?;
            let kind = op.tx_kind(height).ok_or("operação sem transação")?;
            rz_wallet::send_account(c, &self.genesis, keys, kind, fee)
        })?;
        Ok(State::Approved {
            txid: Some(txid.0),
            key: None,
            signature: None,
        })
    }
}

fn serve_file(o: &Origin, id: &Hash32, bundle: &Bundle, path: &str) -> Response {
    let rel = path.trim_start_matches('/');
    let mut candidates = Vec::new();
    if rel.is_empty() || rel.ends_with('/') {
        candidates.push(format!("{rel}index.html"));
    } else {
        candidates.push(rel.to_string());
        candidates.push(format!("{rel}/index.html"));
    }
    for c in &candidates {
        if let Some(f) = bundle.get(c) {
            return Response::new(200, &f.mime, f.data.clone());
        }
    }
    if rel.is_empty() {
        // Sem index.html: lista os arquivos verificados.
        let mut body = format!(
            "<!doctype html><meta charset=\"utf-8\"><title>{0}</title><h1>{0}</h1>\
<p>Conteúdo <code>{1}</code> verificado.</p><ul>",
            esc(&o.address()),
            id
        );
        for f in &bundle.files {
            body.push_str(&format!(
                "<li><a href=\"/{0}\">{0}</a> ({1} bytes)</li>",
                esc(&f.path),
                f.data.len()
            ));
        }
        body.push_str("</ul>");
        return Response::html(200, body);
    }
    Response::html(
        404,
        "<!doctype html><meta charset=\"utf-8\"><h1>Não encontrado</h1>",
    )
}

fn pins_path(dir: &std::path::Path) -> PathBuf {
    dir.join("fixados.txt")
}

fn load_pins(dir: &std::path::Path) -> BTreeMap<String, Hash32> {
    std::fs::read_to_string(pins_path(dir))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (a, h) = l.split_once(' ')?;
            Some((a.to_string(), Hash32::from_hex(h)?))
        })
        .collect()
}

fn save_pins(dir: &std::path::Path, pins: &BTreeMap<String, Hash32>) {
    let text: String = pins.iter().map(|(a, h)| format!("{a} {h}\n")).collect();
    let tmp = pins_path(dir).with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(tmp, pins_path(dir));
    }
}
