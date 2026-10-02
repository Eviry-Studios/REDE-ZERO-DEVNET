//! HTTP/1.1 mínimo para a interface local (sem dependências externas).
//!
//! Só escuta em loopback, uma requisição por conexão, com limites estritos de
//! cabeçalho e corpo. Não é um servidor de uso geral.

use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_BODY: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// Caminho sem a consulta, já decodificado de `%XX`.
    pub path: String,
    pub query: BTreeMap<String, String>,
    /// Nomes em minúsculas.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn host(&self) -> &str {
        self.header("host").unwrap_or("")
    }

    /// Campos de um corpo `application/x-www-form-urlencoded`.
    pub fn form(&self) -> BTreeMap<String, String> {
        parse_query(std::str::from_utf8(&self.body).unwrap_or(""))
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: body.into(),
        }
    }

    pub fn html(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::new(status, "text/html; charset=utf-8", body)
    }

    pub fn json(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::new(status, "application/json", body)
    }

    pub fn text(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::new(status, "text/plain; charset=utf-8", body)
    }

    pub fn redirect(location: &str) -> Self {
        let mut r = Self::text(303, "");
        r.headers.push(("Location".into(), location.into()));
        r
    }

    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        421 => "Misdirected Request",
        502 => "Bad Gateway",
        _ => "Status",
    }
}

/// Lê uma requisição. `None` para entrada inválida ou acima dos limites.
pub fn read_request(stream: &TcpStream) -> io::Result<Option<Request>> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut reader = BufReader::new(stream.take((MAX_HEADER_BYTES + MAX_BODY) as u64));
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(None);
        }
        head.extend_from_slice(&line);
        if head.len() > MAX_HEADER_BYTES {
            return Ok(None);
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    let Ok(text) = std::str::from_utf8(&head) else {
        return Ok(None);
    };
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return Ok(None);
    };
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Ok(None);
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return Ok(None);
    }
    let mut headers = BTreeMap::new();
    for l in lines {
        if l.is_empty() {
            break;
        }
        let Some((k, v)) = l.split_once(':') else {
            return Ok(None);
        };
        let k = k.trim().to_ascii_lowercase();
        // Cabeçalhos repetidos sensíveis tornam a requisição ambígua.
        if headers.contains_key(&k) && matches!(k.as_str(), "host" | "origin" | "content-length") {
            return Ok(None);
        }
        headers.insert(k, v.trim().to_string());
    }
    if headers.contains_key("transfer-encoding") {
        return Ok(None);
    }
    let len: usize = match headers.get("content-length") {
        Some(v) => match v.parse() {
            Ok(n) if n <= MAX_BODY => n,
            _ => return Ok(None),
        },
        None => 0,
    };
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    let (raw_path, raw_query) = target.split_once('?').unwrap_or((target, ""));
    let Some(path) = percent_decode(raw_path) else {
        return Ok(None);
    };
    Ok(Some(Request {
        method: method.to_string(),
        path,
        query: parse_query(raw_query),
        headers,
        body,
    }))
}

pub fn write_response(mut stream: &TcpStream, r: &Response) -> io::Result<()> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", r.status, reason(r.status)).into_bytes();
    for (k, v) in &r.headers {
        // Valores com quebra de linha nunca são emitidos (injeção de cabeçalho).
        if k.contains(['\r', '\n']) || v.contains(['\r', '\n']) {
            continue;
        }
        out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            r.body.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&r.body);
    stream.write_all(&out)?;
    stream.flush()
}

pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

pub fn parse_query(q: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let dec = |s: &str| percent_decode(&s.replace('+', " "));
        if let (Some(k), Some(v)) = (dec(k), dec(v)) {
            m.insert(k, v);
        }
    }
    m
}

pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for c in s.bytes() {
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~') {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_percent() {
        let q = parse_query("a=1&b=zero%3A%2F%2Fx.app&c=um+dois&d");
        assert_eq!(q["b"], "zero://x.app");
        assert_eq!(q["c"], "um dois");
        assert_eq!(q["d"], "");
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_encode("zero://x"), "zero%3A%2F%2Fx");
    }
}
