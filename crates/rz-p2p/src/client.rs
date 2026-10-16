//! Cliente síncrono simples, usado por Wallets e testes.
//!
//! Conecta a um Node (diretamente ou por proxy SOCKS5/Tor), estabelece o
//! canal cifrado (ADR-0011), realiza o handshake de protocolo (verificando
//! rede e Genesis) e permite requisições do tipo pergunta/resposta.
//!
//! O cliente não possui identidade persistente: usa chaves efêmeras por
//! conexão e anuncia `listen_port = 0`, `relay = false`, sem endereço.

use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use rz_crypto::{Hash32, NodeId, PublicKey};

use crate::secure::{initiate, SecureReader, SecureWriter};
use crate::{check_hello, FrameError, Hello, HelloError, Message, PeerAddr, P2P_VERSION};

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Frame(FrameError),
    Handshake(HelloError),
    Rejected(String),
    Protocol(&'static str),
    Timeout,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "conexão: {e}"),
            Self::Frame(e) => write!(f, "{e}"),
            Self::Handshake(e) => write!(f, "handshake: {e}"),
            Self::Rejected(r) => write!(f, "rejeitado pelo node: {r}"),
            Self::Protocol(p) => write!(f, "protocolo: {p}"),
            Self::Timeout => write!(f, "tempo esgotado"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<FrameError> for ClientError {
    fn from(e: FrameError) -> Self {
        match e {
            FrameError::Io(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                ClientError::Timeout
            }
            e => ClientError::Frame(e),
        }
    }
}

/// Opções de conexão.
#[derive(Clone, Debug)]
pub struct ConnectOptions {
    /// Proxy SOCKS5 (ex.: Tor em `127.0.0.1:9050`).
    pub proxy: Option<SocketAddr>,
    /// Identidade esperada do Node; impede interceptação ativa.
    pub expected_node: Option<NodeId>,
    pub timeout: Duration,
    /// Porta anunciada no HELLO (0 para clientes).
    pub listen_port: u16,
    /// Pede propagação de blocos e transações (Nodes).
    pub relay: bool,
}

impl ConnectOptions {
    pub fn new(timeout: Duration) -> Self {
        Self {
            proxy: None,
            expected_node: None,
            timeout,
            listen_port: 0,
            relay: false,
        }
    }
}

pub struct Client {
    reader: SecureReader<TcpStream>,
    writer: SecureWriter<TcpStream>,
    /// HELLO recebido do Node.
    pub hello: Hello,
    /// Identidade comprovada do Node no canal cifrado.
    pub node: PublicKey,
    timeout: Duration,
}

impl Client {
    /// Conexão direta, sem proxy.
    pub fn connect(
        addr: SocketAddr,
        network_id: &str,
        genesis: Hash32,
        timeout: Duration,
    ) -> Result<Self, ClientError> {
        Self::connect_opts(
            &PeerAddr::Ip(addr),
            network_id,
            genesis,
            &ConnectOptions::new(timeout),
        )
    }

    /// Como [`Client::connect`], anunciando `listen_port` e pedindo
    /// propagação (usado em testes que simulam um par).
    pub fn connect_with_port(
        addr: SocketAddr,
        network_id: &str,
        genesis: Hash32,
        timeout: Duration,
        listen_port: u16,
    ) -> Result<Self, ClientError> {
        let mut o = ConnectOptions::new(timeout);
        o.listen_port = listen_port;
        o.relay = true;
        Self::connect_opts(&PeerAddr::Ip(addr), network_id, genesis, &o)
    }

    /// Conecta e verifica que o Node pertence à rede esperada. A Wallet não
    /// confia no Node para dizer qual é a rede (THR-ID-003).
    pub fn connect_opts(
        addr: &PeerAddr,
        network_id: &str,
        genesis: Hash32,
        o: &ConnectOptions,
    ) -> Result<Self, ClientError> {
        let stream = crate::socks::connect(addr, o.proxy, o.timeout).map_err(ClientError::Io)?;
        stream
            .set_read_timeout(Some(o.timeout))
            .map_err(ClientError::Io)?;
        stream.set_nodelay(true).map_err(ClientError::Io)?;
        let (mut reader, mut writer, node) = initiate(
            stream.try_clone().map_err(ClientError::Io)?,
            stream,
            network_id,
            o.expected_node,
        )?;
        writer.write_message(&Message::Hello(Hello {
            p2p_version: P2P_VERSION,
            network_id: network_id.to_owned(),
            genesis,
            height: 0,
            listen_port: o.listen_port,
            relay: o.relay,
            advertise: None,
        }))?;
        let hello = match reader.read_message()? {
            Message::Hello(h) => h,
            Message::Reject(r) => return Err(ClientError::Rejected(r)),
            _ => return Err(ClientError::Protocol("esperado HELLO")),
        };
        check_hello(&hello, network_id, &genesis).map_err(ClientError::Handshake)?;
        Ok(Self {
            reader,
            writer,
            hello,
            node,
            timeout: o.timeout,
        })
    }

    pub fn send(&mut self, msg: &Message) -> Result<(), ClientError> {
        self.writer.write_message(msg).map_err(Into::into)
    }

    pub fn recv(&mut self) -> Result<Message, ClientError> {
        match self.reader.read_message()? {
            Message::Reject(r) => Err(ClientError::Rejected(r)),
            m => Ok(m),
        }
    }

    /// Envia `msg` e aguarda a primeira resposta aceita por `want`,
    /// ignorando mensagens não relacionadas.
    pub fn request<T>(
        &mut self,
        msg: &Message,
        mut want: impl FnMut(Message) -> Option<T>,
    ) -> Result<T, ClientError> {
        self.send(msg)?;
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if let Some(v) = want(self.recv()?) {
                return Ok(v);
            }
        }
        Err(ClientError::Timeout)
    }

    /// Acesso ao socket, para testes que precisam enviar bytes brutos.
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        self.writer.get_mut()
    }
}
