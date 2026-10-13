//! Formato dos arquivos de vetores (`spec/CONFORMANCE.md §2`).
//!
//! ```text
//! # comentário
//! [caso nome-do-caso]
//! campo = valor
//! ```
//!
//! Texto UTF-8, uma entrada por linha. Valores binários em hexadecimal
//! minúsculo; números em decimal. A ordem dos casos importa (casos de
//! cadeia são aplicados em sequência); a ordem dos campos, não.

use std::fmt::Write as _;

/// Um caso: nome e campos.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

impl Case {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: Vec::new(),
        }
    }

    pub fn set(mut self, key: &str, value: impl ToString) -> Self {
        self.fields.push((key.to_string(), value.to_string()));
        self
    }

    pub fn hex(self, key: &str, bytes: &[u8]) -> Self {
        self.set(key, rz_crypto::hex::encode(bytes))
    }

    pub fn get(&self, key: &str) -> Result<&str, String> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| format!("caso {}: campo '{key}' ausente", self.name))
    }

    /// Todos os valores de um campo repetido, na ordem do arquivo.
    pub fn all(&self, key: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Texto entre aspas (`"..."`), sem as aspas.
    pub fn text(&self, key: &str) -> Result<&str, String> {
        let v = self.get(key)?;
        v.strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .ok_or_else(|| format!("caso {}: '{key}' deve estar entre aspas", self.name))
    }

    pub fn opt(&self, key: &str) -> Option<&str> {
        self.get(key).ok()
    }

    pub fn bytes(&self, key: &str) -> Result<Vec<u8>, String> {
        let v = self.get(key)?;
        rz_crypto::hex::decode(v).ok_or_else(|| format!("caso {}: '{key}' não é hex", self.name))
    }

    pub fn u64(&self, key: &str) -> Result<u64, String> {
        self.get(key)?
            .parse()
            .map_err(|_| format!("caso {}: '{key}' não é número", self.name))
    }
}

/// Um arquivo de vetores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suite {
    pub title: String,
    pub cases: Vec<Case>,
}

impl Suite {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut suite = Suite::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(c) = line.strip_prefix('#') {
                if suite.title.is_empty() && suite.cases.is_empty() {
                    suite.title = c.trim().to_string();
                }
                continue;
            }
            if let Some(name) = line
                .strip_prefix("[caso ")
                .and_then(|r| r.strip_suffix(']'))
            {
                suite.cases.push(Case::new(name.trim()));
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("linha {}: esperado 'campo = valor'", n + 1))?;
            let case = suite
                .cases
                .last_mut()
                .ok_or_else(|| format!("linha {}: campo fora de um caso", n + 1))?;
            case.fields
                .push((k.trim().to_string(), v.trim().to_string()));
        }
        Ok(suite)
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# {}", self.title);
        let _ = writeln!(
            out,
            "# Gerado por `zero-conformidade gerar`; formato em spec/CONFORMANCE.md."
        );
        for c in &self.cases {
            let _ = writeln!(out, "\n[caso {}]", c.name);
            for (k, v) in &c.fields {
                let _ = writeln!(out, "{k} = {v}");
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Suite {
            title: "teste".into(),
            cases: vec![Case::new("a").set("x", 1).hex("y", &[0xab])],
        };
        let back = Suite::parse(&s.render()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.cases[0].bytes("y").unwrap(), vec![0xab]);
        assert!(Suite::parse("x = 1").is_err());
    }
}
