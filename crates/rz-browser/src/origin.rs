//! Origens: cada publicação da Exonet é servida num host próprio sob
//! `localhost`, para que a política de mesma origem do navegador do sistema
//! isole publicações entre si e da interface do Navegador (THR-COM-001,
//! THR-BRW-001).
//!
//! ```text
//! navegador.localhost:P            interface do Navegador (sem scripts)
//! NOME.TIPO.localhost:P            zero://NOME.TIPO
//! bBASE32.id.localhost:P           publicação sem nome, pelo identificador
//! ```

use rz_core::community::{check_name, NameKind};
use rz_crypto::Hash32;

pub const UI_HOST: &str = "navegador.localhost";

/// O que um host identifica.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Ui,
    Named { name: String, kind: NameKind },
    Id(Hash32),
}

impl Origin {
    /// Interpreta o cabeçalho `Host`. A porta precisa ser exatamente a do
    /// Navegador; qualquer outro host é recusado (defesa contra DNS
    /// rebinding).
    pub fn parse(host: &str, port: u16) -> Option<Origin> {
        let (name, p) = host.rsplit_once(':')?;
        if p.parse::<u16>().ok()? != port {
            return None;
        }
        let name = name.to_ascii_lowercase();
        if name == UI_HOST {
            return Some(Origin::Ui);
        }
        let rest = name.strip_suffix(".localhost")?;
        let (label, kind) = rest.split_once('.')?;
        if kind == "id" {
            let b = label.strip_prefix('b')?;
            return base32_decode(b).map(Hash32).map(Origin::Id);
        }
        let kind = NameKind::from_suffix(kind)?;
        check_name(label).ok()?;
        Some(Origin::Named {
            name: label.to_string(),
            kind,
        })
    }

    pub fn host(&self, port: u16) -> String {
        match self {
            Origin::Ui => format!("{UI_HOST}:{port}"),
            Origin::Named { name, kind } => format!("{name}.{}.localhost:{port}", kind.suffix()),
            Origin::Id(h) => format!("b{}.id.localhost:{port}", base32_encode(&h.0)),
        }
    }

    pub fn url(&self, port: u16) -> String {
        format!("http://{}", self.host(port))
    }

    /// Forma canônica do endereço da Exonet (usada para identidade por site).
    pub fn address(&self) -> String {
        match self {
            Origin::Ui => "zero://navegador".into(),
            Origin::Named { name, kind } => rz_core::community::format_address(name, *kind),
            Origin::Id(h) => format!("zero://{}", h.to_hex()),
        }
    }

    /// Interpreta um endereço digitado: `zero://nome.tipo`, `nome.tipo` ou
    /// `zero://HEX` (64 dígitos).
    pub fn from_address(input: &str) -> Option<Origin> {
        let s = input.trim();
        let s = s.strip_prefix("zero://").unwrap_or(s).trim_end_matches('/');
        if s.len() == 64 {
            if let Some(h) = Hash32::from_hex(s) {
                return Some(Origin::Id(h));
            }
        }
        let (name, kind) = rz_core::community::parse_address(&format!("zero://{s}"))?;
        Some(Origin::Named { name, kind })
    }
}

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Base32 minúsculo sem preenchimento (52 caracteres para 32 bytes, cabe num
/// rótulo DNS).
pub fn base32_encode(data: &[u8; 32]) -> String {
    let mut out = String::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<[u8; 32]> {
    if s.len() != 52 {
        return None;
    }
    let mut out = Vec::with_capacity(33);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    // Bits finais não nulos: codificação não canônica.
    if buf & ((1 << bits) - 1) != 0 {
        return None;
    }
    out.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_roundtrip_and_reject_foreign() {
        let p = 7300;
        let id = Hash32([0xab; 32]);
        for o in [
            Origin::Ui,
            Origin::Named {
                name: "laboratorio".into(),
                kind: NameKind::App,
            },
            Origin::Id(id),
        ] {
            assert_eq!(Origin::parse(&o.host(p), p), Some(o));
        }
        assert_eq!(Origin::parse("navegador.localhost:7301", p), None);
        assert_eq!(Origin::parse("navegador.localhost", p), None);
        assert_eq!(Origin::parse("evil.example:7300", p), None);
        assert_eq!(Origin::parse("x.app.localhost.evil.com:7300", p), None);
        assert_eq!(Origin::parse("ab.app.localhost:7300", p), None);
        assert_eq!(Origin::parse("bzz.id.localhost:7300", p), None);
        assert_eq!(base32_decode(&base32_encode(&id.0)), Some(id.0));
        assert_eq!(
            Origin::from_address("zero://laboratorio.app"),
            Some(Origin::Named {
                name: "laboratorio".into(),
                kind: NameKind::App
            })
        );
        assert_eq!(
            Origin::from_address(&format!("zero://{}", id.to_hex())),
            Some(Origin::Id(id))
        );
    }
}
