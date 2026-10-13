//! Atualizações de protocolo verificáveis (`spec/RELEASES.md`, ADR-0021,
//! `SPEC §63–§65`).
//!
//! Uma versão é descrita por um **manifesto** que identifica versão,
//! especificação, implementação, testes e artefatos (`SPEC §63`). O
//! identificador da versão (`release_id`) é o hash do manifesto.
//!
//! Não há mantenedor que assine sozinho: o manifesto é publicado na cadeia, e
//! só a governança o **adota**, por proposta constitucional aprovada nas duas
//! câmaras. Qualquer pessoa verifica um binário baixado comparando seu hash com
//! o manifesto adotado (`SPEC §64`).

use std::collections::BTreeMap;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Hash32};

/// Contexto do identificador de uma versão.
pub const RELEASE_ID: &str = "rede-zero/release/v1";
/// Contexto do hash de um arquivo (artefato ou arquivo de uma árvore).
pub const RELEASE_FILE: &str = "rede-zero/release-file/v1";
/// Contexto do hash de uma árvore de arquivos (especificação, código).
pub const RELEASE_TREE: &str = "rede-zero/release-tree/v1";
/// Contexto da raiz da parte de versões do estado.
pub const RELEASES_ROOT: &str = "rede-zero/releases-root/v1";

pub const MAX_VERSION_LEN: usize = 32;
pub const MAX_ARTIFACTS: usize = 32;
pub const MAX_ARTIFACT_NAME: usize = 64;
/// Versões guardadas no estado.
pub const MAX_RELEASES: usize = 256;

/// Compatibilidade declarada com a versão anterior (`SPEC §65`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Compatibility {
    /// Sem mudança de regras nem de mensagens: atualizar quando quiser.
    Compatible = 0,
    /// Acrescenta mensagens ou comportamento opcional; versões anteriores
    /// continuam validando a cadeia.
    PartiallyCompatible = 1,
    /// Muda regras de validade: a partir da ativação, versões anteriores
    /// deixam de acompanhar a cadeia.
    Incompatible = 2,
}

impl Compatibility {
    pub fn name(self) -> &'static str {
        match self {
            Compatibility::Compatible => "COMPATIBLE",
            Compatibility::PartiallyCompatible => "PARTIALLY_COMPATIBLE",
            Compatibility::Incompatible => "INCOMPATIBLE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "compativel" | "compatível" | "COMPATIBLE" => Compatibility::Compatible,
            "parcial" | "PARTIALLY_COMPATIBLE" => Compatibility::PartiallyCompatible,
            "incompativel" | "incompatível" | "INCOMPATIBLE" => Compatibility::Incompatible,
            _ => return None,
        })
    }
}

impl Encode for Compatibility {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for Compatibility {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(Compatibility::Compatible),
            1 => Ok(Compatibility::PartiallyCompatible),
            2 => Ok(Compatibility::Incompatible),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Artefato distribuído (binário, pacote) e seu hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub name: String,
    pub hash: Hash32,
}

impl Encode for Artifact {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.name).put(&self.hash);
    }
}

impl Decode for Artifact {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            name: d.str(MAX_ARTIFACT_NAME)?,
            hash: d.get()?,
        })
    }
}

/// Manifesto de uma versão (`SPEC §63`: versão, especificação,
/// implementação, testes e identificador).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Versão do software (ex.: `0.2.0`).
    pub version: String,
    pub protocol_version: u16,
    pub p2p_version: u16,
    pub compatibility: Compatibility,
    /// Versão anterior adotada, se houver.
    pub previous: Option<Hash32>,
    /// Árvore da especificação (`spec/` e `docs/`).
    pub specification: Hash32,
    /// Árvore do código-fonte (`crates/`, `Cargo.toml`, `Cargo.lock`).
    pub implementation: Hash32,
    /// Vetores de conformidade (`spec/CONFORMANCE.md`).
    pub tests: Hash32,
    /// Artefatos, em ordem estritamente crescente de nome.
    pub artifacts: Vec<Artifact>,
    /// Notas da versão (texto fora da cadeia).
    pub notes: Hash32,
}

impl Release {
    pub fn id(&self) -> Hash32 {
        hash(RELEASE_ID, &self.to_canonical_bytes())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let v = &self.version;
        if v.is_empty()
            || v.len() > MAX_VERSION_LEN
            || !v
                .bytes()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == b'.' || c == b'-')
        {
            return Err("versão inválida");
        }
        if self.artifacts.len() > MAX_ARTIFACTS {
            return Err("artefatos demais");
        }
        for (i, a) in self.artifacts.iter().enumerate() {
            if a.name.is_empty()
                || !a
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
            {
                return Err("nome de artefato inválido");
            }
            if i > 0 && self.artifacts[i - 1].name >= a.name {
                return Err("artefatos repetidos ou fora de ordem");
            }
        }
        Ok(())
    }

    pub fn artifact(&self, name: &str) -> Option<&Artifact> {
        self.artifacts.iter().find(|a| a.name == name)
    }
}

impl Encode for Release {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.version)
            .u16(self.protocol_version)
            .u16(self.p2p_version)
            .put(&self.compatibility)
            .option(&self.previous)
            .put(&self.specification)
            .put(&self.implementation)
            .put(&self.tests)
            .list(&self.artifacts)
            .put(&self.notes);
    }
}

impl Decode for Release {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let r = Self {
            version: d.str(MAX_VERSION_LEN)?,
            protocol_version: d.u16()?,
            p2p_version: d.u16()?,
            compatibility: d.get()?,
            previous: d.option()?,
            specification: d.get()?,
            implementation: d.get()?,
            tests: d.get()?,
            artifacts: d.list(MAX_ARTIFACTS)?,
            notes: d.get()?,
        };
        r.validate().map_err(DecodeError::InvalidValue)?;
        Ok(r)
    }
}

/// Hash de um arquivo distribuído.
pub fn file_hash(bytes: &[u8]) -> Hash32 {
    hash(RELEASE_FILE, bytes)
}

/// Hash de uma árvore: `(caminho relativo, hash do arquivo)` em ordem de
/// caminho. Independe do sistema de arquivos e da ordem de leitura.
pub fn tree_hash(files: &BTreeMap<String, Vec<u8>>) -> Hash32 {
    let mut e = Encoder::new();
    e.u64(files.len() as u64);
    for (path, bytes) in files {
        e.str(path).put(&file_hash(bytes));
    }
    hash(RELEASE_TREE, &e.into_bytes())
}

/// Versão registrada no estado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseRecord {
    pub release: Release,
    pub published_at: u64,
    /// Altura em que a governança a adotou.
    pub adopted_at: Option<u64>,
}

impl Encode for ReleaseRecord {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.release)
            .u64(self.published_at)
            .option(&self.adopted_at);
    }
}

impl Decode for ReleaseRecord {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            release: d.get()?,
            published_at: d.u64()?,
            adopted_at: d.option()?,
        })
    }
}

/// Parte do estado dedicada a versões.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReleaseState {
    pub releases: BTreeMap<Hash32, ReleaseRecord>,
    /// Última versão adotada.
    pub current: Option<Hash32>,
}

impl ReleaseState {
    pub fn current(&self) -> Option<(&Hash32, &ReleaseRecord)> {
        let id = self.current.as_ref()?;
        self.releases.get(id).map(|r| (id, r))
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.releases.len() as u64);
        for (id, r) in &self.releases {
            e.put(id).put(r);
        }
        e.option(&self.current);
        hash(RELEASES_ROOT, &e.into_bytes())
    }
}

/// Como um Node deve tratar a versão adotada pela cadeia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeNotice {
    /// O Node executa a versão adotada (ou uma compatível com ela).
    UpToDate,
    /// Há versão adotada mais nova; esta continua acompanhando a cadeia.
    Recommended { version: String },
    /// A versão adotada muda regras: este Node deixa de acompanhar a cadeia
    /// a partir da ativação.
    Required {
        version: String,
        protocol_version: u16,
    },
}

/// Compara a versão em execução com a adotada.
pub fn upgrade_notice(
    running_version: &str,
    running_protocol: u16,
    adopted: &Release,
) -> UpgradeNotice {
    if adopted.version == running_version {
        return UpgradeNotice::UpToDate;
    }
    if adopted.protocol_version > running_protocol
        || adopted.compatibility == Compatibility::Incompatible
    {
        return UpgradeNotice::Required {
            version: adopted.version.clone(),
            protocol_version: adopted.protocol_version,
        };
    }
    UpgradeNotice::Recommended {
        version: adopted.version.clone(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample() -> Release {
        Release {
            version: "0.2.0".into(),
            protocol_version: 1,
            p2p_version: 11,
            compatibility: Compatibility::Incompatible,
            previous: None,
            specification: Hash32([1; 32]),
            implementation: Hash32([2; 32]),
            tests: Hash32([3; 32]),
            artifacts: vec![
                Artifact {
                    name: "rede-zero-node-linux-x86_64".into(),
                    hash: file_hash(b"binario do node"),
                },
                Artifact {
                    name: "zero-wallet-linux-x86_64".into(),
                    hash: file_hash(b"binario da wallet"),
                },
            ],
            notes: Hash32([4; 32]),
        }
    }

    #[test]
    fn manifest_identity_and_rules() {
        let r = sample();
        r.validate().unwrap();
        let back = Release::from_canonical_bytes(&r.to_canonical_bytes()).unwrap();
        assert_eq!(back.id(), r.id());
        let mut other = r.clone();
        other.artifacts[0].hash = file_hash(b"binario adulterado");
        assert_ne!(
            other.id(),
            r.id(),
            "trocar um artefato muda o identificador"
        );
        let mut bad = r.clone();
        bad.artifacts.swap(0, 1);
        assert!(bad.validate().is_err());
        bad = r.clone();
        bad.version = "V 1".into();
        assert!(bad.validate().is_err());
        assert_eq!(
            r.artifact("zero-wallet-linux-x86_64").unwrap().hash,
            file_hash(b"binario da wallet")
        );
    }

    #[test]
    fn tree_hash_is_order_independent_and_content_sensitive() {
        let mut a = BTreeMap::new();
        a.insert("spec/A.md".to_string(), b"a".to_vec());
        a.insert("spec/B.md".to_string(), b"b".to_vec());
        let mut b = BTreeMap::new();
        b.insert("spec/B.md".to_string(), b"b".to_vec());
        b.insert("spec/A.md".to_string(), b"a".to_vec());
        assert_eq!(tree_hash(&a), tree_hash(&b));
        b.insert("spec/A.md".to_string(), b"A".to_vec());
        assert_ne!(tree_hash(&a), tree_hash(&b));
    }

    #[test]
    fn upgrade_notices() {
        let r = sample();
        assert_eq!(upgrade_notice("0.2.0", 1, &r), UpgradeNotice::UpToDate);
        assert!(matches!(
            upgrade_notice("0.1.0", 1, &r),
            UpgradeNotice::Required { .. }
        ));
        let mut c = r.clone();
        c.compatibility = Compatibility::PartiallyCompatible;
        assert!(matches!(
            upgrade_notice("0.1.0", 1, &c),
            UpgradeNotice::Recommended { .. }
        ));
        c.protocol_version = 2;
        assert!(matches!(
            upgrade_notice("0.1.0", 1, &c),
            UpgradeNotice::Required { .. }
        ));
    }
}
