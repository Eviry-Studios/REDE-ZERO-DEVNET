//! Cliente SOCKS5 mínimo (RFC 1928), sem autenticação, para conexões via
//! Tor ou I2P (ADR-0011).
//!
//! Nomes de host são enviados ao proxy **sem resolução local**: nenhuma
//! consulta DNS sai do dispositivo, e endereços `.onion` funcionam.

use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::PeerAddr;

fn err(msg: &'static str) -> io::Error {
    io::Error::other(msg)
}

/// Conecta a `target` através do proxy SOCKS5 em `proxy`.
pub fn connect_via_socks5(
    proxy: SocketAddr,
    target: &PeerAddr,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let mut s = TcpStream::connect_timeout(&proxy, timeout)?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;

    // Saudação: versão 5, um método, "sem autenticação".
    s.write_all(&[0x05, 0x01, 0x00])?;
    let mut resp = [0u8; 2];
    s.read_exact(&mut resp)?;
    if resp != [0x05, 0x00] {
        return Err(err("proxy SOCKS5 recusou o método sem autenticação"));
    }

    let mut req = vec![0x05, 0x01, 0x00];
    match target {
        PeerAddr::Ip(a) => match a.ip() {
            IpAddr::V4(v4) => {
                req.push(0x01);
                req.extend_from_slice(&v4.octets());
            }
            IpAddr::V6(v6) => {
                req.push(0x04);
                req.extend_from_slice(&v6.octets());
            }
        },
        PeerAddr::Host { host, .. } => {
            let len = u8::try_from(host.len()).map_err(|_| err("nome de host longo demais"))?;
            req.push(0x03);
            req.push(len);
            req.extend_from_slice(host.as_bytes());
        }
    }
    req.extend_from_slice(&target.port().to_be_bytes());
    s.write_all(&req)?;

    let mut head = [0u8; 4];
    s.read_exact(&mut head)?;
    if head[0] != 0x05 {
        return Err(err("resposta SOCKS5 inválida"));
    }
    if head[1] != 0x00 {
        return Err(err("proxy SOCKS5 não conseguiu conectar ao destino"));
    }
    let skip = match head[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l)?;
            l[0] as usize
        }
        _ => return Err(err("resposta SOCKS5 inválida")),
    };
    let mut rest = vec![0u8; skip + 2];
    s.read_exact(&mut rest)?;
    s.set_read_timeout(None)?;
    s.set_write_timeout(None)?;
    Ok(s)
}

/// Conecta a um par diretamente ou por proxy.
///
/// Sem proxy, nomes de host são resolvidos localmente (revela ao servidor
/// DNS o destino) e endereços `.onion` são recusados.
pub fn connect(
    target: &PeerAddr,
    proxy: Option<SocketAddr>,
    timeout: Duration,
) -> io::Result<TcpStream> {
    match (proxy, target) {
        (Some(p), t) => connect_via_socks5(p, t, timeout),
        (None, PeerAddr::Ip(a)) => TcpStream::connect_timeout(a, timeout),
        (None, t) if t.is_onion() => Err(err("endereço .onion exige proxy Tor (--proxy)")),
        (None, PeerAddr::Host { host, port }) => {
            let addr = (host.as_str(), *port)
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| err("nome de host sem endereço"))?;
            TcpStream::connect_timeout(&addr, timeout)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    /// Proxy SOCKS5 de teste: registra os destinos pedidos e encaminha
    /// conexões para `127.0.0.1:porta` (nomes são "resolvidos" para
    /// localhost, simulando um serviço onion).
    pub fn test_proxy() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { continue };
                let log = log.clone();
                thread::spawn(move || {
                    let mut g = [0u8; 3];
                    c.read_exact(&mut g).unwrap();
                    c.write_all(&[5, 0]).unwrap();
                    let mut h = [0u8; 4];
                    c.read_exact(&mut h).unwrap();
                    let dest = match h[3] {
                        1 => {
                            let mut a = [0u8; 4];
                            c.read_exact(&mut a).unwrap();
                            std::net::Ipv4Addr::from(a).to_string()
                        }
                        3 => {
                            let mut l = [0u8; 1];
                            c.read_exact(&mut l).unwrap();
                            let mut n = vec![0u8; l[0] as usize];
                            c.read_exact(&mut n).unwrap();
                            String::from_utf8(n).unwrap()
                        }
                        _ => panic!("tipo não suportado no proxy de teste"),
                    };
                    let mut p = [0u8; 2];
                    c.read_exact(&mut p).unwrap();
                    let port = u16::from_be_bytes(p);
                    log.lock().unwrap().push(format!("{dest}:{port}"));
                    let up = TcpStream::connect(("127.0.0.1", port)).unwrap();
                    c.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
                    let (mut a, mut b) = (c.try_clone().unwrap(), up.try_clone().unwrap());
                    let (mut c2, mut up2) = (c, up);
                    thread::spawn(move || {
                        let _ = io::copy(&mut a, &mut up2);
                    });
                    let _ = io::copy(&mut b, &mut c2);
                });
            }
        });
        (addr, seen)
    }

    #[test]
    fn connects_through_proxy_by_name() {
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = target.local_addr().unwrap().port();
        thread::spawn(move || {
            let (mut s, _) = target.accept().unwrap();
            s.write_all(b"ola").unwrap();
        });
        let (proxy, seen) = test_proxy();
        let dest: PeerAddr = format!("servico.onion:{port}").parse().unwrap();
        let mut s = connect(&dest, Some(proxy), Duration::from_secs(5)).unwrap();
        let mut buf = [0u8; 3];
        s.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ola");
        // O proxy recebeu o NOME, não um IP resolvido localmente.
        assert_eq!(seen.lock().unwrap()[0], format!("servico.onion:{port}"));
    }

    #[test]
    fn onion_without_proxy_refused() {
        let dest: PeerAddr = "servico.onion:7100".parse().unwrap();
        assert!(connect(&dest, None, Duration::from_secs(1)).is_err());
    }
}
