//! Cliente síncrono simples, usado por Wallets e testes.
//!
//! Conecta a um Node, realiza o handshake (verificando rede e Genesis) e
//! permite requisições do tipo pergunta/resposta.

use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use rz_crypto::Hash32;

use crate::{
    check_hello, read_frame, write_frame, FrameError, Hello, HelloError, Message, P2P_VERSION,
};

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

pub struct Client {
    stream: TcpStream,
    /// HELLO recebido do Node.
    pub hello: Hello,
    timeout: Duration,
}

impl Client {
    /// Conecta e verifica que o Node pertence à rede esperada. A Wallet não
    /// confia no Node para dizer qual é a rede (THR-ID-003).
    pub fn connect(
        addr: SocketAddr,
        network_id: &str,
        genesis: Hash32,
        timeout: Duration,
    ) -> Result<Self, ClientError> {
        Self::connect_with_port(addr, network_id, genesis, timeout, 0)
    }

    /// Como [`Client::connect`], anunciando `listen_port` no HELLO. Com porta
    /// diferente de zero, o Node trata a conexão como par (recebe propagação).
    pub fn connect_with_port(
        addr: SocketAddr,
        network_id: &str,
        genesis: Hash32,
        timeout: Duration,
        listen_port: u16,
    ) -> Result<Self, ClientError> {
        let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(ClientError::Io)?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(ClientError::Io)?;
        stream.set_nodelay(true).map_err(ClientError::Io)?;
        write_frame(
            &mut stream,
            &Message::Hello(Hello {
                p2p_version: P2P_VERSION,
                network_id: network_id.to_owned(),
                genesis,
                height: 0,
                listen_port,
            }),
        )?;
        let hello = match read_frame(&mut stream)? {
            Message::Hello(h) => h,
            Message::Reject(r) => return Err(ClientError::Rejected(r)),
            _ => return Err(ClientError::Protocol("esperado HELLO")),
        };
        check_hello(&hello, network_id, &genesis).map_err(ClientError::Handshake)?;
        Ok(Self {
            stream,
            hello,
            timeout,
        })
    }

    pub fn send(&mut self, msg: &Message) -> Result<(), ClientError> {
        write_frame(&mut self.stream, msg).map_err(Into::into)
    }

    pub fn recv(&mut self) -> Result<Message, ClientError> {
        match read_frame(&mut self.stream)? {
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

    /// Acesso ao socket, para testes que precisam de controle fino.
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}
