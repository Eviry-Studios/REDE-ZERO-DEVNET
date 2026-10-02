//! Comunidades e nomes `zero://` (`SPEC §60–§62`, REQ-068..074, ADR-0015,
//! `spec/COMMUNITIES.md`, `spec/NAMING.md`, `docs/DEMOCRACIA_ORGANICA.md`
//! N-1..N-8).
//!
//! * Uma Comunidade é **declarada** com nome, hash do manifesto e a sua
//!   **regra de decisão**: um conjunto de chaves com limiar (N-6). Os membros
//!   não são listados (N-5): a Comunidade escolhe internamente como essas
//!   chaves decidem.
//! * O **reconhecimento** ocorre por proposta de governança da categoria
//!   Comunidade que aponta para a declaração (N-7).
//! * A **posição** de uma Comunidade sobre uma proposta só é aceita se
//!   assinada segundo a regra declarada, e é registrada **sem alterar** a
//!   apuração oficial (N-1).
//! * **Nomes** `zero://nome.tipo` são uma camada sobre identificadores
//!   criptográficos (N-3), com regras objetivas e rejeição de nomes
//!   confundíveis (THR-BRW-002).
//!
//! Nenhuma operação daqui altera consenso, saldos de terceiros, regras
//! monetárias ou outras Comunidades (`SPEC §61`, INV-007).

use std::collections::{BTreeMap, BTreeSet};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Address, Hash32, PublicKey, Signature};

use crate::governance::Choice;

/// Contexto das assinaturas dos controladores de uma Comunidade.
pub const COMMUNITY_APPROVAL: &str = "rede-zero/community/v1";
/// Contexto do hash do manifesto de uma Comunidade (arquivo fora da cadeia).
pub const COMMUNITY_MANIFEST: &str = "rede-zero/community-manifest/v1";
/// Contexto da raiz da parte de Comunidades e nomes do estado.
pub const COMMUNITY_ROOT: &str = "rede-zero/community-root/v1";

pub const MIN_NAME_LEN: usize = 3;
pub const MAX_NAME_LEN: usize = 32;
/// Chaves na regra de decisão.
pub const MAX_CONTROLLERS: usize = 32;
/// Versões guardadas no histórico de uma Comunidade.
pub const MAX_VERSION_HISTORY: usize = 64;

/// Nomes reservados: funções do próprio protocolo (contra phishing).
pub const RESERVED_NAMES: &[&str] = &[
    "zero",
    "rede-zero",
    "redezero",
    "exonet",
    "grande-mercado",
    "grandemercado",
    "mercado",
    "pool",
    "wallet",
    "carteira",
    "governanca",
    "node",
    "agente-zero",
    "navegador-zero",
];

// ------------------------------------------------------------------- nomes

/// Tipo de publicação, o sufixo de `zero://nome.tipo` (N-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameKind {
    /// Só para Comunidades reconhecidas (N-3, N-7); nunca registrável
    /// diretamente.
    Community = 0,
    Blog = 1,
    App = 2,
    /// Interfaces do Grande Mercado e comércio que liquida por ele; nunca
    /// uma segunda DEX (N-4).
    Market = 3,
    Forum = 4,
    Video = 5,
    Service = 6,
}

impl NameKind {
    pub const ALL: [NameKind; 7] = [
        NameKind::Community,
        NameKind::Blog,
        NameKind::App,
        NameKind::Market,
        NameKind::Forum,
        NameKind::Video,
        NameKind::Service,
    ];

    pub fn suffix(self) -> &'static str {
        match self {
            NameKind::Community => "comunidade",
            NameKind::Blog => "blog",
            NameKind::App => "app",
            NameKind::Market => "market",
            NameKind::Forum => "forum",
            NameKind::Video => "video",
            NameKind::Service => "service",
        }
    }

    pub fn from_suffix(s: &str) -> Option<NameKind> {
        NameKind::ALL.into_iter().find(|k| k.suffix() == s)
    }
}

impl Encode for NameKind {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for NameKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let t = d.u8()?;
        NameKind::ALL
            .into_iter()
            .find(|k| *k as u8 == t)
            .ok_or(DecodeError::InvalidTag(t))
    }
}

/// Regras de sintaxe de um nome: 3..=32 de `[a-z0-9-]`, sem hífen no início,
/// no fim ou repetido, e fora da lista reservada.
pub fn check_name(name: &str) -> Result<(), &'static str> {
    let len = name.len();
    if !(MIN_NAME_LEN..=MAX_NAME_LEN).contains(&len) {
        return Err("nome deve ter de 3 a 32 caracteres");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("nome aceita apenas a-z, 0-9 e hífen");
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return Err("hífen no início, no fim ou repetido");
    }
    let sk = skeleton(name);
    if RESERVED_NAMES.iter().any(|r| skeleton(r) == sk) {
        return Err("nome reservado");
    }
    Ok(())
}

/// Forma canônica para detectar nomes visualmente confundíveis
/// (THR-BRW-002): dígitos parecidos com letras, `rn`/`m`, `vv`/`w`, `l`/`i`
/// e hífens são normalizados. Dois nomes do mesmo tipo com o mesmo esqueleto
/// não podem coexistir.
pub fn skeleton(name: &str) -> String {
    let mapped: String = name
        .chars()
        .filter(|c| *c != '-')
        .map(|c| match c {
            '0' => 'o',
            '1' | 'i' => 'l',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            '8' => 'b',
            '9' => 'g',
            c => c,
        })
        .collect();
    mapped.replace("rn", "m").replace("vv", "w")
}

/// Interpreta `zero://nome.tipo`.
pub fn parse_address(s: &str) -> Option<(String, NameKind)> {
    let rest = s.strip_prefix("zero://")?;
    let (name, suffix) = rest.rsplit_once('.')?;
    let kind = NameKind::from_suffix(suffix)?;
    check_name(name).ok()?;
    Some((name.to_owned(), kind))
}

pub fn format_address(name: &str, kind: NameKind) -> String {
    format!("zero://{name}.{}", kind.suffix())
}

/// Nome registrado (tipos diferentes de Comunidade).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameRecord {
    pub owner: Address,
    /// Identificador criptográfico da publicação (hash de conteúdo, chave
    /// de aplicação etc.).
    pub target: Hash32,
    pub registered_at: u64,
}

impl Encode for NameRecord {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.owner).put(&self.target).u64(self.registered_at);
    }
}

impl Decode for NameRecord {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            owner: d.get()?,
            target: d.get()?,
            registered_at: d.u64()?,
        })
    }
}

// ------------------------------------------------------------- Comunidades

/// Regra de decisão declarada (N-6): `threshold` de `keys` assinam as
/// manifestações da Comunidade. Como essas chaves decidem (voto direto,
/// representantes, consenso interno) é escolha da Comunidade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionRule {
    pub keys: Vec<PublicKey>,
    pub threshold: u8,
}

impl DecisionRule {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.keys.is_empty() || self.keys.len() > MAX_CONTROLLERS {
            return Err("regra de decisão deve ter de 1 a 32 chaves");
        }
        let unique: BTreeSet<_> = self.keys.iter().collect();
        if unique.len() != self.keys.len() {
            return Err("chave repetida na regra de decisão");
        }
        if self.threshold == 0 || self.threshold as usize > self.keys.len() {
            return Err("limiar fora dos limites");
        }
        Ok(())
    }

    /// Verifica aprovações: assinaturas válidas de pelo menos `threshold`
    /// chaves distintas da regra sobre `payload`.
    pub fn verify(&self, network_id: &str, payload: &[u8], approvals: &[Approval]) -> bool {
        let mut ok = BTreeSet::new();
        for a in approvals {
            if !self.keys.contains(&a.key) || ok.contains(&a.key) {
                return false;
            }
            if a.key
                .verify(COMMUNITY_APPROVAL, network_id, payload, &a.signature)
                .is_err()
            {
                return false;
            }
            ok.insert(a.key);
        }
        ok.len() >= self.threshold as usize
    }
}

impl Encode for DecisionRule {
    fn encode(&self, e: &mut Encoder) {
        e.list(&self.keys).u8(self.threshold);
    }
}

impl Decode for DecisionRule {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            keys: d.list(MAX_CONTROLLERS)?,
            threshold: d.u8()?,
        })
    }
}

/// Assinatura de uma chave da regra de decisão.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub key: PublicKey,
    pub signature: Signature,
}

impl Encode for Approval {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.key).put(&self.signature);
    }
}

impl Decode for Approval {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            key: d.get()?,
            signature: d.get()?,
        })
    }
}

/// Conteúdo assinado pelas chaves de uma Comunidade para cada manifestação.
pub fn position_payload(community: &Hash32, proposal: &Hash32, choice: Choice) -> Vec<u8> {
    let mut e = Encoder::new();
    e.u8(1).put(community).put(proposal).put(&choice);
    e.into_bytes()
}

pub fn update_payload(
    community: &Hash32,
    manifest_hash: &Hash32,
    version: u32,
    rule: &Option<DecisionRule>,
) -> Vec<u8> {
    let mut e = Encoder::new();
    e.u8(2)
        .put(community)
        .put(manifest_hash)
        .u32(version)
        .option(rule);
    e.into_bytes()
}

/// Assina uma manifestação (ferramenta para Wallets e testes).
pub fn approve(key: &rz_crypto::SecretKey, network_id: &str, payload: &[u8]) -> Approval {
    Approval {
        key: key.public_key(),
        signature: key.sign(COMMUNITY_APPROVAL, network_id, payload),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommunityStatus {
    /// Declarada, aguardando reconhecimento até `expires_at`.
    Declared = 0,
    /// Reconhecida por proposta aprovada da categoria Comunidade (N-7).
    Recognized = 1,
}

impl Encode for CommunityStatus {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for CommunityStatus {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(CommunityStatus::Declared),
            1 => Ok(CommunityStatus::Recognized),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Registro de uma Comunidade no estado (`SPEC §60`: identidade, versão,
/// componentes e regras verificáveis pelo manifesto; origem pelo
/// declarante).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Community {
    /// Identificador: o `TxId` da declaração.
    pub id: Hash32,
    pub name: String,
    pub declarant: Address,
    pub manifest_hash: Hash32,
    pub version: u32,
    pub rule: DecisionRule,
    pub status: CommunityStatus,
    pub declared_at: u64,
    /// Validade da declaração enquanto não reconhecida.
    pub expires_at: u64,
    /// Histórico `(versão, manifesto, altura)` para fixação de versão
    /// (THR-COM-002).
    pub history: Vec<(u32, Hash32, u64)>,
}

impl Encode for Community {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id)
            .str(&self.name)
            .put(&self.declarant)
            .put(&self.manifest_hash)
            .u32(self.version)
            .put(&self.rule)
            .put(&self.status)
            .u64(self.declared_at)
            .u64(self.expires_at)
            .u32(self.history.len() as u32);
        for (v, h, at) in &self.history {
            e.u32(*v).put(h).u64(*at);
        }
    }
}

impl Decode for Community {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let id = d.get()?;
        let name = d.str(MAX_NAME_LEN)?;
        let declarant = d.get()?;
        let manifest_hash = d.get()?;
        let version = d.u32()?;
        let rule = d.get()?;
        let status = d.get()?;
        let declared_at = d.u64()?;
        let expires_at = d.u64()?;
        let n = d.u32()? as usize;
        if n > MAX_VERSION_HISTORY {
            return Err(DecodeError::LengthExceeded {
                declared: n as u64,
                max: MAX_VERSION_HISTORY as u64,
            });
        }
        let mut history = Vec::with_capacity(n);
        for _ in 0..n {
            history.push((d.u32()?, d.get()?, d.u64()?));
        }
        Ok(Self {
            id,
            name,
            declarant,
            manifest_hash,
            version,
            rule,
            status,
            declared_at,
            expires_at,
            history,
        })
    }
}

/// Parâmetros de Comunidades e nomes (no estado, alteráveis por governança).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommunityParams {
    /// Taxa de registro de nome, depositada no Pool permanente.
    pub name_fee: u64,
    /// Validade de uma declaração não reconhecida, em blocos.
    pub declaration_ttl_blocks: u64,
    /// Máximo de declarações pendentes.
    pub max_pending: u32,
}

impl Default for CommunityParams {
    fn default() -> Self {
        Self {
            name_fee: 10 * crate::UNITS_PER_ZERO,
            // ~30 dias com blocos de 2 s.
            declaration_ttl_blocks: 1_296_000,
            max_pending: 1_000,
        }
    }
}

impl CommunityParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.declaration_ttl_blocks == 0 || self.max_pending == 0 {
            return Err("parâmetros de Comunidades devem ser positivos");
        }
        Ok(())
    }
}

impl Encode for CommunityParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.name_fee)
            .u64(self.declaration_ttl_blocks)
            .u32(self.max_pending);
    }
}

impl Decode for CommunityParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            name_fee: d.u64()?,
            declaration_ttl_blocks: d.u64()?,
            max_pending: d.u32()?,
        })
    }
}

/// Parte do estado dedicada a Comunidades e nomes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommunityState {
    pub communities: BTreeMap<Hash32, Community>,
    /// Nomes registrados dos tipos diferentes de Comunidade.
    pub names: BTreeMap<(NameKind, String), NameRecord>,
    /// Posições de Comunidades reconhecidas sobre propostas (N-1:
    /// registradas, não vinculantes).
    pub positions: BTreeMap<(Hash32, Hash32), Choice>,
}

impl CommunityState {
    /// Comunidade (reconhecida ou pendente) que usa `name`.
    pub fn by_name(&self, name: &str) -> Option<&Community> {
        self.communities.values().find(|c| c.name == name)
    }

    pub fn pending(&self) -> usize {
        self.communities
            .values()
            .filter(|c| c.status == CommunityStatus::Declared)
            .count()
    }

    /// Resolve `zero://nome.tipo` para o identificador criptográfico.
    pub fn resolve(&self, name: &str, kind: NameKind) -> Option<Hash32> {
        match kind {
            NameKind::Community => self
                .by_name(name)
                .filter(|c| c.status == CommunityStatus::Recognized)
                .map(|c| c.id),
            _ => self.names.get(&(kind, name.to_owned())).map(|r| r.target),
        }
    }

    /// Nome livre para o tipo, sem colisão de esqueleto com outro existente.
    pub fn name_available(&self, name: &str, kind: NameKind) -> Result<(), &'static str> {
        check_name(name)?;
        let sk = skeleton(name);
        let taken = match kind {
            NameKind::Community => self
                .communities
                .values()
                .any(|c| c.name == name || skeleton(&c.name) == sk),
            _ => self
                .names
                .keys()
                .any(|(k, n)| *k == kind && (n == name || skeleton(n) == sk)),
        };
        if taken {
            return Err("nome já usado ou confundível com outro do mesmo tipo");
        }
        Ok(())
    }

    /// Posições registradas sobre uma proposta.
    pub fn positions_on(&self, proposal: &Hash32) -> Vec<(Hash32, Choice)> {
        self.positions
            .iter()
            .filter(|((p, _), _)| p == proposal)
            .map(|((_, c), ch)| (*c, *ch))
            .collect()
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.communities.len() as u64);
        for c in self.communities.values() {
            e.put(c);
        }
        e.u64(self.names.len() as u64);
        for ((k, n), r) in &self.names {
            e.put(k).str(n).put(r);
        }
        e.u64(self.positions.len() as u64);
        for ((p, c), ch) in &self.positions {
            e.put(p).put(c).put(ch);
        }
        hash(COMMUNITY_ROOT, &e.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rz_crypto::SecretKey;

    #[test]
    fn name_rules() {
        assert!(check_name("desenvolvedores").is_ok());
        assert!(check_name("jornal-zero").is_ok());
        assert!(check_name("ab").is_err());
        assert!(check_name("Maiuscula").is_err());
        assert!(check_name("-inicio").is_err());
        assert!(check_name("duplo--hifen").is_err());
        assert!(check_name("rede-zero").is_err());
        // Esqueleto de nome reservado também é reservado.
        assert!(check_name("p00l").is_err());
        assert!(check_name("agentezero").is_err());
        assert!(check_name("n0de").is_err());
    }

    #[test]
    fn confusable_skeletons() {
        assert_eq!(skeleton("banco"), skeleton("banc0"));
        assert_eq!(skeleton("modern"), skeleton("rnodern"));
        assert_eq!(skeleton("ciencia"), skeleton("c1enc1a"));
        assert_ne!(skeleton("banco"), skeleton("bancos"));
    }

    #[test]
    fn address_parsing() {
        assert_eq!(
            parse_address("zero://desenvolvedores.comunidade"),
            Some(("desenvolvedores".into(), NameKind::Community))
        );
        assert_eq!(
            parse_address("zero://editorzero.app"),
            Some(("editorzero".into(), NameKind::App))
        );
        assert_eq!(parse_address("zero://x.app"), None);
        assert_eq!(parse_address("https://editorzero.app"), None);
        assert_eq!(parse_address("zero://editorzero.exe"), None);
        assert_eq!(
            format_address("jornalzero", NameKind::Blog),
            "zero://jornalzero.blog"
        );
    }

    #[test]
    fn decision_rule_threshold() {
        let ks: Vec<SecretKey> = (1..=3).map(|i| SecretKey::from_seed([i; 32])).collect();
        let rule = DecisionRule {
            keys: ks.iter().map(SecretKey::public_key).collect(),
            threshold: 2,
        };
        rule.validate().unwrap();
        let payload = b"posicao";
        let one = vec![approve(&ks[0], "net", payload)];
        let two = vec![
            approve(&ks[0], "net", payload),
            approve(&ks[2], "net", payload),
        ];
        assert!(!rule.verify("net", payload, &one));
        assert!(rule.verify("net", payload, &two));
        // Mesma chave duas vezes não conta como duas.
        let dup = vec![
            approve(&ks[0], "net", payload),
            approve(&ks[0], "net", payload),
        ];
        assert!(!rule.verify("net", payload, &dup));
        // Outra rede, outro conteúdo ou chave de fora: inválido.
        assert!(!rule.verify("outra", payload, &two));
        assert!(!rule.verify("net", b"outro", &two));
        let outsider = SecretKey::from_seed([9; 32]);
        let bad = vec![
            approve(&ks[0], "net", payload),
            approve(&outsider, "net", payload),
        ];
        assert!(!rule.verify("net", payload, &bad));

        assert!(DecisionRule {
            keys: vec![],
            threshold: 1
        }
        .validate()
        .is_err());
        assert!(DecisionRule {
            keys: rule.keys.clone(),
            threshold: 4
        }
        .validate()
        .is_err());
    }
}
