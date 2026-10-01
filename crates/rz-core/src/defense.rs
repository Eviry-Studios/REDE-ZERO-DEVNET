//! Defesa da Exonet (`SPEC §51–§57`, REQ-051..063, ADR-0016,
//! `spec/DEFENSE.md`).
//!
//! * **Modos** NORMAL → VIGILÂNCIA → INCIDENTE → GUERRA_CIBERNÉTICA, com
//!   transições por **atestação** de validadores (vários participantes,
//!   AC-DEF-006) e evidência registrada (`SPEC §52`). Subir exige mais poder
//!   que descer.
//! * **Sem poder permanente** (REQ-059, INV-010): todo modo vence e desce um
//!   nível sozinho se não for renovado.
//! * **Credenciais temporárias** com escopo apenas defensivo, validade e
//!   revogação (`SPEC §54`, INV-009). Não existe escopo ofensivo (`SPEC
//!   §55`).
//! * **Encerramento verificável** (`SPEC §56`) que preserva as evidências.
//! * **Registro de contribuição** distinto de participação (`SPEC §57`).
//!
//! Este módulo contém os tipos e a verificação de atestações. A aplicação ao
//! estado fica em `state.rs`.

use std::collections::{BTreeMap, BTreeSet};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Hash32, PublicKey, Signature};

use crate::consensus::ValidatorSet;

/// Contexto das assinaturas de atestação de defesa.
pub const DEFENSE_ATTESTATION: &str = "rede-zero/defense/v1";
/// Contexto da raiz da parte de defesa do estado.
pub const DEFENSE_ROOT: &str = "rede-zero/defense-root/v1";

/// Atestações por transação (limite de decodificação).
pub const MAX_ATTESTATIONS: usize = crate::consensus::MAX_VALIDATORS_DECODE;
/// Escopos por credencial.
pub const MAX_SCOPES: usize = 5;
/// Ações registradas por incidente.
pub const MAX_ACTIONS_PER_INCIDENT: usize = 1_024;
/// Evidências por incidente.
pub const MAX_EVIDENCE_PER_INCIDENT: usize = 256;

/// Modo de defesa da rede (`SPEC §53`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefenseMode {
    #[default]
    Normal = 0,
    Vigilance = 1,
    Incident = 2,
    CyberWar = 3,
}

impl DefenseMode {
    pub fn name(self) -> &'static str {
        match self {
            DefenseMode::Normal => "NORMAL",
            DefenseMode::Vigilance => "VIGILÂNCIA",
            DefenseMode::Incident => "INCIDENTE",
            DefenseMode::CyberWar => "GUERRA_CIBERNÉTICA",
        }
    }

    /// Um nível abaixo (retorno gradual, REQ-063).
    pub fn step_down(self) -> DefenseMode {
        match self {
            DefenseMode::Normal | DefenseMode::Vigilance => DefenseMode::Normal,
            DefenseMode::Incident => DefenseMode::Vigilance,
            DefenseMode::CyberWar => DefenseMode::Incident,
        }
    }
}

impl Encode for DefenseMode {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for DefenseMode {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(DefenseMode::Normal),
            1 => Ok(DefenseMode::Vigilance),
            2 => Ok(DefenseMode::Incident),
            3 => Ok(DefenseMode::CyberWar),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Escopo de uma credencial defensiva (`ARCHITECTURE §38`). A lista é
/// fechada e **só defensiva**: não existe escopo para invadir, instalar
/// código, coletar dados de terceiros ou destruir infraestrutura externa
/// (`SPEC §55`, AT-CYBER-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Diagnose = 0,
    Isolate = 1,
    Recover = 2,
    Coordinate = 3,
    PreserveEvidence = 4,
}

impl Scope {
    pub const ALL: [Scope; 5] = [
        Scope::Diagnose,
        Scope::Isolate,
        Scope::Recover,
        Scope::Coordinate,
        Scope::PreserveEvidence,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scope::Diagnose => "diagnostico",
            Scope::Isolate => "isolamento",
            Scope::Recover => "recuperacao",
            Scope::Coordinate => "coordenacao",
            Scope::PreserveEvidence => "evidencias",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|x| x.name() == s)
    }
}

impl Encode for Scope {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for Scope {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let t = d.u8()?;
        Scope::ALL
            .into_iter()
            .find(|s| *s as u8 == t)
            .ok_or(DecodeError::InvalidTag(t))
    }
}

/// Estado de um incidente (`SPEC §51`, §56).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncidentStatus {
    Open = 0,
    Contained = 1,
    Recovered = 2,
    Closed = 3,
    /// O modo INCIDENTE venceu sem renovação nem encerramento: o incidente
    /// sai de vigor, com as evidências preservadas.
    Lapsed = 4,
}

impl IncidentStatus {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            IncidentStatus::Open | IncidentStatus::Contained | IncidentStatus::Recovered
        )
    }
}

impl Encode for IncidentStatus {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for IncidentStatus {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(IncidentStatus::Open),
            1 => Ok(IncidentStatus::Contained),
            2 => Ok(IncidentStatus::Recovered),
            3 => Ok(IncidentStatus::Closed),
            4 => Ok(IncidentStatus::Lapsed),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Ação registrada por um portador de credencial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenseActionRecord {
    pub credential: Hash32,
    pub holder: PublicKey,
    pub scope: Scope,
    /// Alvo da ação (ex.: Node ID isolado, hash do pacote de evidências).
    pub subject: Hash32,
    pub height: u64,
}

impl Encode for DefenseActionRecord {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.credential)
            .put(&self.holder)
            .put(&self.scope)
            .put(&self.subject)
            .u64(self.height);
    }
}

impl Decode for DefenseActionRecord {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            credential: d.get()?,
            holder: d.get()?,
            scope: d.get()?,
            subject: d.get()?,
            height: d.u64()?,
        })
    }
}

/// Incidente (`SPEC §51`): identificador, evidências, estado e ações.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incident {
    /// `TxId` da transição que o abriu.
    pub id: Hash32,
    pub opened_at: u64,
    pub status: IncidentStatus,
    /// Hashes dos pacotes de evidência, na ordem em que foram atestados.
    /// Nunca são apagados (AC-DEF-010).
    pub evidence: Vec<Hash32>,
    pub actions: Vec<DefenseActionRecord>,
    pub closed_at: Option<u64>,
}

impl Encode for Incident {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id)
            .u64(self.opened_at)
            .put(&self.status)
            .list(&self.evidence)
            .list(&self.actions)
            .option(&self.closed_at);
    }
}

impl Decode for Incident {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: d.get()?,
            opened_at: d.u64()?,
            status: d.get()?,
            evidence: d.list(MAX_EVIDENCE_PER_INCIDENT)?,
            actions: d.list(MAX_ACTIONS_PER_INCIDENT)?,
            closed_at: d.option()?,
        })
    }
}

/// Credencial defensiva temporária (`SPEC §54`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    /// `TxId` da concessão.
    pub id: Hash32,
    pub incident: Hash32,
    pub holder: PublicKey,
    pub scopes: Vec<Scope>,
    pub granted_at: u64,
    pub expires_at: u64,
    pub revoked: bool,
}

impl Credential {
    /// Autoriza `scope` na altura `h`?
    pub fn allows(&self, scope: Scope, h: u64) -> bool {
        !self.revoked && h < self.expires_at && self.scopes.contains(&scope)
    }
}

impl Encode for Credential {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id)
            .put(&self.incident)
            .put(&self.holder)
            .list(&self.scopes)
            .u64(self.granted_at)
            .u64(self.expires_at)
            .bool(self.revoked);
    }
}

impl Decode for Credential {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: d.get()?,
            incident: d.get()?,
            holder: d.get()?,
            scopes: d.list(MAX_SCOPES)?,
            granted_at: d.u64()?,
            expires_at: d.u64()?,
            revoked: d.bool()?,
        })
    }
}

/// Registro de contribuição defensiva **verificada** (`SPEC §57`). Ter
/// credencial ou registrar ações é participação; só a atestação após o
/// encerramento gera este registro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenseRecord {
    pub incident: Hash32,
    pub role: Scope,
    pub evidence: Hash32,
    pub height: u64,
}

impl Encode for DefenseRecord {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.incident)
            .put(&self.role)
            .put(&self.evidence)
            .u64(self.height);
    }
}

impl Decode for DefenseRecord {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            incident: d.get()?,
            role: d.get()?,
            evidence: d.get()?,
            height: d.u64()?,
        })
    }
}

/// Assinatura de um validador sobre uma decisão de defesa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attestation {
    pub validator: PublicKey,
    pub signature: Signature,
}

impl Encode for Attestation {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.validator).put(&self.signature);
    }
}

impl Decode for Attestation {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            validator: d.get()?,
            signature: d.get()?,
        })
    }
}

/// Limiar de poder exigido por uma decisão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quorum {
    /// Mais de 1/3: entrar em vigilância, descer de modo, revogar.
    OneThird,
    /// Mais de 2/3: incidente, guerra, encerramento, credenciais, registros.
    TwoThirds,
}

/// Poder de voto dos validadores que atestaram `payload` validamente.
/// Assinaturas inválidas, repetidas ou de quem não é validador invalidam o
/// conjunto inteiro (evidência falsificada é rejeitada, AT-DEF-003).
pub fn attested_power(
    network_id: &str,
    payload: &[u8],
    attestations: &[Attestation],
    validators: &ValidatorSet,
) -> Option<u128> {
    let mut seen = BTreeSet::new();
    let mut power = 0u128;
    for a in attestations {
        if !validators.contains(&a.validator) || !seen.insert(a.validator) {
            return None;
        }
        a.validator
            .verify(DEFENSE_ATTESTATION, network_id, payload, &a.signature)
            .ok()?;
        power += validators.power_of(&a.validator) as u128;
    }
    Some(power)
}

pub fn meets(quorum: Quorum, power: u128, validators: &ValidatorSet) -> bool {
    match quorum {
        Quorum::OneThird => validators.exceeds_one_third(power),
        Quorum::TwoThirds => validators.is_supermajority(power),
    }
}

/// Assina uma decisão de defesa (ferramenta para validadores e testes).
pub fn attest(key: &rz_crypto::SecretKey, network_id: &str, payload: &[u8]) -> Attestation {
    Attestation {
        validator: key.public_key(),
        signature: key.sign(DEFENSE_ATTESTATION, network_id, payload),
    }
}

/// Conteúdos atestados. Todos incluem `seq`, o contador de decisões de
/// defesa do estado, que impede reutilizar atestações antigas.
pub mod payload {
    use super::*;

    pub fn transition(seq: u64, to: DefenseMode, evidence: &Hash32) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u8(1).u64(seq).put(&to).put(evidence);
        e.into_bytes()
    }

    pub fn incident_update(
        seq: u64,
        incident: &Hash32,
        status: IncidentStatus,
        evidence: &Hash32,
    ) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u8(2).u64(seq).put(incident).put(&status).put(evidence);
        e.into_bytes()
    }

    pub fn close(seq: u64, incident: &Hash32, archive: &Hash32) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u8(3).u64(seq).put(incident).put(archive);
        e.into_bytes()
    }

    pub fn grant(
        seq: u64,
        incident: &Hash32,
        holder: &PublicKey,
        scopes: &[Scope],
        expires_at: u64,
    ) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u8(4)
            .u64(seq)
            .put(incident)
            .put(holder)
            .list(scopes)
            .u64(expires_at);
        e.into_bytes()
    }

    pub fn revoke(seq: u64, credential: &Hash32) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u8(5).u64(seq).put(credential);
        e.into_bytes()
    }

    pub fn contribution(
        seq: u64,
        incident: &Hash32,
        node: &PublicKey,
        role: Scope,
        evidence: &Hash32,
    ) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u8(6)
            .u64(seq)
            .put(incident)
            .put(node)
            .put(&role)
            .put(evidence);
        e.into_bytes()
    }
}

/// Parâmetros de defesa (no estado, durações alteráveis por governança).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefenseParams {
    /// Duração máxima de cada modo sem renovação.
    pub vigilance_max_blocks: u64,
    pub incident_max_blocks: u64,
    pub war_max_blocks: u64,
    /// Tempo mínimo de incidente aberto para escalar à guerra ("ameaça
    /// persistente").
    pub war_persistence_blocks: u64,
    /// Validade máxima de uma credencial.
    pub credential_max_blocks: u64,
    /// Pontos de contribuição mínimos para receber credencial sem ser
    /// validador ("histórico verificável", `ARCHITECTURE §38`).
    pub credential_min_contribution: u64,
}

impl Default for DefenseParams {
    fn default() -> Self {
        // Com blocos de 2 s: vigilância 1 dia, incidente 3 dias, guerra 1 dia
        // (renovável), persistência 6 horas, credencial 1 dia.
        Self {
            vigilance_max_blocks: 43_200,
            incident_max_blocks: 129_600,
            war_max_blocks: 43_200,
            war_persistence_blocks: 10_800,
            credential_max_blocks: 43_200,
            credential_min_contribution: 100 * crate::governance::SCALE,
        }
    }
}

impl DefenseParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        let all = [
            self.vigilance_max_blocks,
            self.incident_max_blocks,
            self.war_max_blocks,
            self.credential_max_blocks,
        ];
        if all.contains(&0) {
            return Err("durações de defesa devem ser positivas");
        }
        // Guerra nunca dura mais que um incidente (limite temporal, REQ-058).
        if self.war_max_blocks > self.incident_max_blocks {
            return Err("guerra não pode durar mais que um incidente");
        }
        Ok(())
    }
}

impl Encode for DefenseParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.vigilance_max_blocks)
            .u64(self.incident_max_blocks)
            .u64(self.war_max_blocks)
            .u64(self.war_persistence_blocks)
            .u64(self.credential_max_blocks)
            .u64(self.credential_min_contribution);
    }
}

impl Decode for DefenseParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            vigilance_max_blocks: d.u64()?,
            incident_max_blocks: d.u64()?,
            war_max_blocks: d.u64()?,
            war_persistence_blocks: d.u64()?,
            credential_max_blocks: d.u64()?,
            credential_min_contribution: d.u64()?,
        })
    }
}

/// Parte do estado dedicada à defesa.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefenseState {
    pub mode: DefenseMode,
    pub mode_since: u64,
    /// Vencimento do modo atual (0 em NORMAL).
    pub mode_expires_at: u64,
    /// Contador de decisões atestadas (proteção contra repetição).
    pub seq: u64,
    /// Incidente em vigor, se houver.
    pub active_incident: Option<Hash32>,
    /// Todos os incidentes, inclusive encerrados (evidências preservadas).
    pub incidents: BTreeMap<Hash32, Incident>,
    pub credentials: BTreeMap<Hash32, Credential>,
    /// Contribuições verificadas por identidade criptográfica.
    pub records: BTreeMap<PublicKey, Vec<DefenseRecord>>,
    /// Evidências das transições de modo (vigilância inclusive).
    pub mode_evidence: Vec<(u64, DefenseMode, Hash32)>,
}

impl DefenseState {
    /// Node IDs isolados por ações vigentes (incidente ativo, credencial
    /// válida), para os Nodes aplicarem localmente (AT-DEF-002).
    pub fn isolated(&self, height: u64) -> BTreeSet<Hash32> {
        let Some(id) = self.active_incident else {
            return BTreeSet::new();
        };
        let Some(inc) = self.incidents.get(&id) else {
            return BTreeSet::new();
        };
        inc.actions
            .iter()
            .filter(|a| a.scope == Scope::Isolate)
            .filter(|a| {
                self.credentials
                    .get(&a.credential)
                    .is_some_and(|c| c.allows(Scope::Isolate, height))
            })
            .map(|a| a.subject)
            .collect()
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.put(&self.mode)
            .u64(self.mode_since)
            .u64(self.mode_expires_at)
            .u64(self.seq)
            .option(&self.active_incident);
        e.u64(self.incidents.len() as u64);
        for i in self.incidents.values() {
            e.put(i);
        }
        e.u64(self.credentials.len() as u64);
        for c in self.credentials.values() {
            e.put(c);
        }
        e.u64(self.records.len() as u64);
        for (k, rs) in &self.records {
            e.put(k).list(rs);
        }
        e.u64(self.mode_evidence.len() as u64);
        for (h, m, ev) in &self.mode_evidence {
            e.u64(*h).put(m).put(ev);
        }
        hash(DEFENSE_ROOT, &e.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::Validator;
    use rz_crypto::SecretKey;

    fn set(keys: &[SecretKey]) -> ValidatorSet {
        ValidatorSet::new(
            keys.iter()
                .map(|k| Validator {
                    key: k.public_key(),
                    power: 10,
                })
                .collect(),
        )
    }

    #[test]
    fn attestation_power_and_thresholds() {
        let ks: Vec<SecretKey> = (1..=4).map(|i| SecretKey::from_seed([i; 32])).collect();
        let vs = set(&ks);
        let p = payload::transition(0, DefenseMode::Vigilance, &Hash32([1; 32]));
        let two: Vec<_> = ks[..2].iter().map(|k| attest(k, "net", &p)).collect();
        let power = attested_power("net", &p, &two, &vs).unwrap();
        assert!(meets(Quorum::OneThird, power, &vs));
        assert!(!meets(Quorum::TwoThirds, power, &vs));
        let three: Vec<_> = ks[..3].iter().map(|k| attest(k, "net", &p)).collect();
        assert!(meets(
            Quorum::TwoThirds,
            attested_power("net", &p, &three, &vs).unwrap(),
            &vs
        ));
    }

    // AT-DEF-003 — evidência falsificada é rejeitada.
    #[test]
    fn forged_attestations_rejected() {
        let ks: Vec<SecretKey> = (1..=4).map(|i| SecretKey::from_seed([i; 32])).collect();
        let vs = set(&ks);
        let p = payload::transition(0, DefenseMode::Incident, &Hash32([1; 32]));
        let mut a: Vec<_> = ks[..3].iter().map(|k| attest(k, "net", &p)).collect();
        a[1].signature.0[0] ^= 1;
        assert_eq!(attested_power("net", &p, &a, &vs), None);
        // Quem não é validador (ex.: o Agente Zero) não atesta.
        let outsider = SecretKey::from_seed([99; 32]);
        let b = vec![attest(&outsider, "net", &p)];
        assert_eq!(attested_power("net", &p, &b, &vs), None);
        // A mesma assinatura repetida não soma poder.
        let dup = vec![attest(&ks[0], "net", &p), attest(&ks[0], "net", &p)];
        assert_eq!(attested_power("net", &p, &dup, &vs), None);
        // Outra decisão, outra rede ou outro `seq`: não vale.
        let other = payload::transition(1, DefenseMode::Incident, &Hash32([1; 32]));
        let good: Vec<_> = ks[..3].iter().map(|k| attest(k, "net", &p)).collect();
        assert_eq!(attested_power("net", &other, &good, &vs), None);
        assert_eq!(attested_power("outra", &p, &good, &vs), None);
    }

    // AT-CYBER-006 — o conjunto de escopos é fechado e só defensivo.
    #[test]
    fn scopes_are_closed_and_defensive() {
        let names: Vec<&str> = Scope::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            [
                "diagnostico",
                "isolamento",
                "recuperacao",
                "coordenacao",
                "evidencias"
            ]
        );
        for t in 0..=255u8 {
            let known = Scope::decode(&mut Decoder::new(&[t])).is_ok();
            assert_eq!(known, t < 5, "escopo {t} fora do conjunto");
        }
    }

    #[test]
    fn modes_step_down_gradually() {
        assert_eq!(DefenseMode::CyberWar.step_down(), DefenseMode::Incident);
        assert_eq!(DefenseMode::Incident.step_down(), DefenseMode::Vigilance);
        assert_eq!(DefenseMode::Vigilance.step_down(), DefenseMode::Normal);
        assert_eq!(DefenseMode::Normal.step_down(), DefenseMode::Normal);
    }

    #[test]
    fn credential_scope_expiry_revocation() {
        let mut c = Credential {
            id: Hash32([1; 32]),
            incident: Hash32([2; 32]),
            holder: SecretKey::from_seed([3; 32]).public_key(),
            scopes: vec![Scope::Diagnose, Scope::Isolate],
            granted_at: 10,
            expires_at: 20,
            revoked: false,
        };
        assert!(c.allows(Scope::Isolate, 15));
        assert!(!c.allows(Scope::Recover, 15)); // fora do escopo
        assert!(!c.allows(Scope::Isolate, 20)); // vencida
        c.revoked = true;
        assert!(!c.allows(Scope::Diagnose, 15)); // revogada
    }
}
