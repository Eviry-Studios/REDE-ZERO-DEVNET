//! Transações (`SPEC §6–§11`, `spec/TRANSACTIONS.md`).

use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{context, hash, Address, CryptoError, Hash32, PublicKey, SecretKey, Signature};
use rz_privacy::excess::ExcessProof;

use crate::block::SignedHeader;
use crate::community::{Approval, DecisionRule, NameKind, MAX_CONTROLLERS, MAX_NAME_LEN};
use crate::defense::{
    Attestation, DefenseMode, IncidentStatus, Scope, MAX_ATTESTATIONS, MAX_SCOPES,
};
use crate::governance::{Category, Choice, ParamChange, MAX_PARAM_CHANGES};
use crate::market::{AssetId, Side};
use crate::private::{PrivateTx, ShieldedOutput, MAX_OUTPUTS};

/// Versão atual do formato de transação.
pub const TX_VERSION: u16 = 1;

/// Identificador de transação: `H(TX_ID, enc(TxBody))`.
///
/// A assinatura não participa do identificador, de modo que nenhum terceiro
/// consegue alterar o ID sem alterar o conteúdo assinado (THR-TX-004).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TxId(pub Hash32);

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxId({:?})", self.0)
    }
}

impl Encode for TxId {
    fn encode(&self, e: &mut Encoder) {
        self.0.encode(e);
    }
}

impl Decode for TxId {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self(d.get()?))
    }
}

/// Operação transportada pela transação.
///
/// Novas operações (governança, Grande Mercado, Pool) recebem novas tags;
/// tags desconhecidas são rejeitadas na decodificação.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxKind {
    /// Tag `0x00` — transferência transparente de ZERO.
    Transfer { to: Address, amount: u64 },
    /// Tag `0x01` — blindagem: move `amount` da conta para notas privadas.
    /// A prova de excesso demonstra que as saídas somam exatamente `amount`.
    Shield {
        amount: u64,
        outputs: Vec<ShieldedOutput>,
        excess: ExcessProof,
    },
    /// Tag `0x10` — bloqueia ZERO para a câmara econômica da governança.
    LockStake { amount: u64, unlock_height: u64 },
    /// Tag `0x11` — devolve um bloqueio vencido.
    Unlock { lock_id: u64 },
    /// Tag `0x12` — submete proposta; debita `deposit` (≥ depósito vigente).
    Propose {
        category: Category,
        /// Hash do texto completo, armazenado fora da cadeia.
        content_hash: Hash32,
        params: Vec<ParamChange>,
        /// Identificador de versão/especificação/implementação/testes (`SPEC §63`).
        release_id: Option<Hash32>,
        deposit: u64,
    },
    /// Tag `0x13` — registra ou substitui o voto do remetente.
    Vote { proposal: Hash32, choice: Choice },
    /// Tag `0x14` — denuncia equivocação: dois cabeçalhos distintos assinados
    /// pelo mesmo proponente para a mesma altura e rodada (`SPEC §49`, THR-CON-004).
    ReportEquivocation {
        first: Box<SignedHeader>,
        second: Box<SignedHeader>,
    },
    /// Tag `0x20` — bloqueia ZERO como validador (ADR-0012). A chave do
    /// remetente passa a ser candidata; efeito a partir da próxima época.
    Bond { amount: u64 },
    /// Tag `0x21` — desbloqueia; o valor só volta à conta após o período de
    /// desvinculação, durante o qual ainda pode ser punido.
    Unbond { amount: u64 },
    /// Tag `0x22` — denuncia dupla assinatura de votos de consenso.
    ReportDoubleVote {
        first: Box<crate::consensus::Vote>,
        second: Box<crate::consensus::Vote>,
    },
    /// Tag `0x23` — denuncia duas propostas diferentes assinadas pelo mesmo
    /// proponente para a mesma altura e rodada.
    ReportDoubleProposal {
        first: Box<crate::consensus::SignedProposal>,
        second: Box<crate::consensus::SignedProposal>,
    },
    /// Tag `0x30` — transfere um ativo externo entre contas.
    TransferAsset {
        asset: AssetId,
        to: Address,
        amount: u64,
    },
    /// Tag `0x31` — deposita no Pool permanente. **Irreversível**: não existe
    /// operação de retirada (`SPEC §39–§40`).
    PoolDeposit { asset: AssetId, amount: u64 },
    /// Tag `0x32` — coloca uma ordem-limite no Grande Mercado. O par é
    /// sempre `asset`/ZERO; a quantidade é do ativo e o preço segue
    /// `market::PRICE_SCALE`. O valor é reservado até a execução, o
    /// cancelamento ou o vencimento em `expires_at`.
    PlaceOrder {
        asset: AssetId,
        side: Side,
        amount: u64,
        price: u64,
        expires_at: u64,
    },
    /// Tag `0x33` — cancela uma ordem própria e devolve o valor reservado.
    CancelOrder { order: Hash32 },
    /// Tag `0x40` — declara uma Comunidade (nome, manifesto e regra de
    /// decisão). O reconhecimento vem por proposta de categoria Comunidade
    /// que aponta para esta declaração (ADR-0015, N-7).
    DeclareCommunity {
        name: String,
        manifest_hash: Hash32,
        rule: DecisionRule,
    },
    /// Tag `0x41` — posição de uma Comunidade reconhecida sobre uma proposta,
    /// aprovada pela regra de decisão declarada. Registrada, não vinculante
    /// (N-1, N-6).
    CommunityPosition {
        community: Hash32,
        proposal: Hash32,
        choice: Choice,
        approvals: Vec<Approval>,
    },
    /// Tag `0x42` — nova versão de uma Comunidade reconhecida (e,
    /// opcionalmente, nova regra de decisão), aprovada pela regra vigente.
    UpdateCommunity {
        community: Hash32,
        manifest_hash: Hash32,
        version: u32,
        rule: Option<DecisionRule>,
        approvals: Vec<Approval>,
    },
    /// Tag `0x43` — registra `zero://name.kind` apontando para `target`. A
    /// taxa de nome vai para o Pool permanente.
    RegisterName {
        name: String,
        kind: NameKind,
        target: Hash32,
    },
    /// Tag `0x44` — o dono altera o alvo e/ou transfere o nome.
    UpdateName {
        name: String,
        kind: NameKind,
        target: Hash32,
        new_owner: Option<Address>,
    },
    /// Tag `0x50` — muda (ou renova) o modo de defesa, com evidência e
    /// atestações de validadores (ADR-0016, `SPEC §53`).
    DefenseTransition {
        to: DefenseMode,
        evidence: Hash32,
        seq: u64,
        attestations: Vec<Attestation>,
    },
    /// Tag `0x51` — avança o incidente: contido, depois recuperado.
    IncidentUpdate {
        incident: Hash32,
        status: IncidentStatus,
        evidence: Hash32,
        seq: u64,
        attestations: Vec<Attestation>,
    },
    /// Tag `0x52` — encerra o incidente recuperado; o pacote de evidências
    /// é registrado e preservado (`SPEC §56`).
    CloseIncident {
        incident: Hash32,
        archive: Hash32,
        seq: u64,
        attestations: Vec<Attestation>,
    },
    /// Tag `0x53` — concede credencial defensiva temporária (`SPEC §54`).
    GrantCredential {
        incident: Hash32,
        holder: PublicKey,
        scopes: Vec<Scope>,
        expires_at: u64,
        seq: u64,
        attestations: Vec<Attestation>,
    },
    /// Tag `0x54` — revoga uma credencial.
    RevokeCredential {
        credential: Hash32,
        seq: u64,
        attestations: Vec<Attestation>,
    },
    /// Tag `0x55` — o portador registra uma ação dentro do escopo da sua
    /// credencial.
    DefenseAction {
        credential: Hash32,
        scope: Scope,
        subject: Hash32,
    },
    /// Tag `0x56` — registra contribuição defensiva verificada, após o
    /// encerramento (`SPEC §57`).
    AttestContribution {
        incident: Hash32,
        node: PublicKey,
        role: Scope,
        evidence: Hash32,
        seq: u64,
        attestations: Vec<Attestation>,
    },
    /// Tag `0x60` — publica um módulo do Exonet Runtime (ADR-0018). A taxa
    /// por byte vai para o Pool permanente.
    PublishModule { code: Vec<u8> },
    /// Tag `0x61` — vincula (ou desvincula) o módulo de uma Comunidade
    /// reconhecida, com aprovações da regra de decisão dela.
    BindModule {
        community: Hash32,
        module: Option<Hash32>,
        approvals: Vec<Approval>,
    },
    /// Tag `0x62` — chama um método do módulo de uma Comunidade, reservando
    /// `fuel` de combustível (pago na taxa, mesmo se a chamada falhar).
    CallModule {
        community: Hash32,
        method: String,
        args: Vec<u8>,
        fuel: u64,
    },
    /// Tag `0x70` — cabeçalhos de uma rede de origem para o cliente leve
    /// (ADR-0019). Qualquer conta pode retransmitir.
    BridgeHeaders {
        chain: String,
        headers: Vec<[u8; 80]>,
    },
    /// Tag `0x71` — prova de queima na origem: cunha a representação para o
    /// destinatário gravado na saída `OP_RETURN`.
    BridgeDeposit {
        chain: String,
        block: Hash32,
        tx: Vec<u8>,
        path: Vec<Hash32>,
        index: u32,
    },
    /// Tag `0x72` — bloqueia ZERO ou um ativo num contrato de troca atômica.
    HtlcLock {
        asset: AssetId,
        amount: u64,
        recipient: Address,
        hashlock: Hash32,
        timeout: u64,
    },
    /// Tag `0x73` — resgata revelando a pré-imagem (antes do prazo).
    HtlcClaim { id: Hash32, preimage: Vec<u8> },
    /// Tag `0x74` — devolve ao remetente depois do prazo.
    HtlcRefund { id: Hash32 },
}

impl Encode for TxKind {
    fn encode(&self, e: &mut Encoder) {
        match self {
            TxKind::Transfer { to, amount } => {
                e.u8(0x00).put(to).u64(*amount);
            }
            TxKind::Shield {
                amount,
                outputs,
                excess,
            } => {
                e.u8(0x01).u64(*amount).list(outputs).put(excess);
            }
            TxKind::LockStake {
                amount,
                unlock_height,
            } => {
                e.u8(0x10).u64(*amount).u64(*unlock_height);
            }
            TxKind::Unlock { lock_id } => {
                e.u8(0x11).u64(*lock_id);
            }
            TxKind::Propose {
                category,
                content_hash,
                params,
                release_id,
                deposit,
            } => {
                e.u8(0x12)
                    .put(category)
                    .put(content_hash)
                    .list(params)
                    .option(release_id)
                    .u64(*deposit);
            }
            TxKind::Vote { proposal, choice } => {
                e.u8(0x13).put(proposal).put(choice);
            }
            TxKind::ReportEquivocation { first, second } => {
                e.u8(0x14).put(first.as_ref()).put(second.as_ref());
            }
            TxKind::Bond { amount } => {
                e.u8(0x20).u64(*amount);
            }
            TxKind::Unbond { amount } => {
                e.u8(0x21).u64(*amount);
            }
            TxKind::ReportDoubleVote { first, second } => {
                e.u8(0x22).put(first.as_ref()).put(second.as_ref());
            }
            TxKind::ReportDoubleProposal { first, second } => {
                e.u8(0x23).put(first.as_ref()).put(second.as_ref());
            }
            TxKind::TransferAsset { asset, to, amount } => {
                e.u8(0x30).put(asset).put(to).u64(*amount);
            }
            TxKind::PoolDeposit { asset, amount } => {
                e.u8(0x31).put(asset).u64(*amount);
            }
            TxKind::PlaceOrder {
                asset,
                side,
                amount,
                price,
                expires_at,
            } => {
                e.u8(0x32)
                    .put(asset)
                    .put(side)
                    .u64(*amount)
                    .u64(*price)
                    .u64(*expires_at);
            }
            TxKind::CancelOrder { order } => {
                e.u8(0x33).put(order);
            }
            TxKind::DeclareCommunity {
                name,
                manifest_hash,
                rule,
            } => {
                e.u8(0x40).str(name).put(manifest_hash).put(rule);
            }
            TxKind::CommunityPosition {
                community,
                proposal,
                choice,
                approvals,
            } => {
                e.u8(0x41)
                    .put(community)
                    .put(proposal)
                    .put(choice)
                    .list(approvals);
            }
            TxKind::UpdateCommunity {
                community,
                manifest_hash,
                version,
                rule,
                approvals,
            } => {
                e.u8(0x42)
                    .put(community)
                    .put(manifest_hash)
                    .u32(*version)
                    .option(rule)
                    .list(approvals);
            }
            TxKind::RegisterName { name, kind, target } => {
                e.u8(0x43).str(name).put(kind).put(target);
            }
            TxKind::UpdateName {
                name,
                kind,
                target,
                new_owner,
            } => {
                e.u8(0x44).str(name).put(kind).put(target).option(new_owner);
            }
            TxKind::DefenseTransition {
                to,
                evidence,
                seq,
                attestations,
            } => {
                e.u8(0x50)
                    .put(to)
                    .put(evidence)
                    .u64(*seq)
                    .list(attestations);
            }
            TxKind::IncidentUpdate {
                incident,
                status,
                evidence,
                seq,
                attestations,
            } => {
                e.u8(0x51)
                    .put(incident)
                    .put(status)
                    .put(evidence)
                    .u64(*seq)
                    .list(attestations);
            }
            TxKind::CloseIncident {
                incident,
                archive,
                seq,
                attestations,
            } => {
                e.u8(0x52)
                    .put(incident)
                    .put(archive)
                    .u64(*seq)
                    .list(attestations);
            }
            TxKind::GrantCredential {
                incident,
                holder,
                scopes,
                expires_at,
                seq,
                attestations,
            } => {
                e.u8(0x53)
                    .put(incident)
                    .put(holder)
                    .list(scopes)
                    .u64(*expires_at)
                    .u64(*seq)
                    .list(attestations);
            }
            TxKind::RevokeCredential {
                credential,
                seq,
                attestations,
            } => {
                e.u8(0x54).put(credential).u64(*seq).list(attestations);
            }
            TxKind::DefenseAction {
                credential,
                scope,
                subject,
            } => {
                e.u8(0x55).put(credential).put(scope).put(subject);
            }
            TxKind::AttestContribution {
                incident,
                node,
                role,
                evidence,
                seq,
                attestations,
            } => {
                e.u8(0x56)
                    .put(incident)
                    .put(node)
                    .put(role)
                    .put(evidence)
                    .u64(*seq)
                    .list(attestations);
            }
            TxKind::PublishModule { code } => {
                e.u8(0x60).bytes(code);
            }
            TxKind::BindModule {
                community,
                module,
                approvals,
            } => {
                e.u8(0x61).put(community).option(module).list(approvals);
            }
            TxKind::CallModule {
                community,
                method,
                args,
                fuel,
            } => {
                e.u8(0x62).put(community).str(method).bytes(args).u64(*fuel);
            }
            TxKind::BridgeHeaders { chain, headers } => {
                e.u8(0x70).str(chain).u32(headers.len() as u32);
                for h in headers {
                    e.fixed(h);
                }
            }
            TxKind::BridgeDeposit {
                chain,
                block,
                tx,
                path,
                index,
            } => {
                e.u8(0x71)
                    .str(chain)
                    .put(block)
                    .bytes(tx)
                    .list(path)
                    .u32(*index);
            }
            TxKind::HtlcLock {
                asset,
                amount,
                recipient,
                hashlock,
                timeout,
            } => {
                e.u8(0x72)
                    .put(asset)
                    .u64(*amount)
                    .put(recipient)
                    .put(hashlock)
                    .u64(*timeout);
            }
            TxKind::HtlcClaim { id, preimage } => {
                e.u8(0x73).put(id).bytes(preimage);
            }
            TxKind::HtlcRefund { id } => {
                e.u8(0x74).put(id);
            }
        }
    }
}

impl Decode for TxKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0x00 => Ok(TxKind::Transfer {
                to: d.get()?,
                amount: d.u64()?,
            }),
            0x01 => Ok(TxKind::Shield {
                amount: d.u64()?,
                outputs: d.list(MAX_OUTPUTS)?,
                excess: d.get()?,
            }),
            0x10 => Ok(TxKind::LockStake {
                amount: d.u64()?,
                unlock_height: d.u64()?,
            }),
            0x11 => Ok(TxKind::Unlock { lock_id: d.u64()? }),
            0x12 => Ok(TxKind::Propose {
                category: d.get()?,
                content_hash: d.get()?,
                params: d.list(MAX_PARAM_CHANGES)?,
                release_id: d.option()?,
                deposit: d.u64()?,
            }),
            0x13 => Ok(TxKind::Vote {
                proposal: d.get()?,
                choice: d.get()?,
            }),
            0x14 => Ok(TxKind::ReportEquivocation {
                first: Box::new(d.get()?),
                second: Box::new(d.get()?),
            }),
            0x20 => Ok(TxKind::Bond { amount: d.u64()? }),
            0x21 => Ok(TxKind::Unbond { amount: d.u64()? }),
            0x22 => Ok(TxKind::ReportDoubleVote {
                first: Box::new(d.get()?),
                second: Box::new(d.get()?),
            }),
            0x23 => Ok(TxKind::ReportDoubleProposal {
                first: Box::new(d.get()?),
                second: Box::new(d.get()?),
            }),
            0x30 => Ok(TxKind::TransferAsset {
                asset: d.get()?,
                to: d.get()?,
                amount: d.u64()?,
            }),
            0x31 => Ok(TxKind::PoolDeposit {
                asset: d.get()?,
                amount: d.u64()?,
            }),
            0x32 => Ok(TxKind::PlaceOrder {
                asset: d.get()?,
                side: d.get()?,
                amount: d.u64()?,
                price: d.u64()?,
                expires_at: d.u64()?,
            }),
            0x33 => Ok(TxKind::CancelOrder { order: d.get()? }),
            0x40 => Ok(TxKind::DeclareCommunity {
                name: d.str(MAX_NAME_LEN)?,
                manifest_hash: d.get()?,
                rule: d.get()?,
            }),
            0x41 => Ok(TxKind::CommunityPosition {
                community: d.get()?,
                proposal: d.get()?,
                choice: d.get()?,
                approvals: d.list(MAX_CONTROLLERS)?,
            }),
            0x42 => Ok(TxKind::UpdateCommunity {
                community: d.get()?,
                manifest_hash: d.get()?,
                version: d.u32()?,
                rule: d.option()?,
                approvals: d.list(MAX_CONTROLLERS)?,
            }),
            0x43 => Ok(TxKind::RegisterName {
                name: d.str(MAX_NAME_LEN)?,
                kind: d.get()?,
                target: d.get()?,
            }),
            0x44 => Ok(TxKind::UpdateName {
                name: d.str(MAX_NAME_LEN)?,
                kind: d.get()?,
                target: d.get()?,
                new_owner: d.option()?,
            }),
            0x50 => Ok(TxKind::DefenseTransition {
                to: d.get()?,
                evidence: d.get()?,
                seq: d.u64()?,
                attestations: d.list(MAX_ATTESTATIONS)?,
            }),
            0x51 => Ok(TxKind::IncidentUpdate {
                incident: d.get()?,
                status: d.get()?,
                evidence: d.get()?,
                seq: d.u64()?,
                attestations: d.list(MAX_ATTESTATIONS)?,
            }),
            0x52 => Ok(TxKind::CloseIncident {
                incident: d.get()?,
                archive: d.get()?,
                seq: d.u64()?,
                attestations: d.list(MAX_ATTESTATIONS)?,
            }),
            0x53 => Ok(TxKind::GrantCredential {
                incident: d.get()?,
                holder: d.get()?,
                scopes: d.list(MAX_SCOPES)?,
                expires_at: d.u64()?,
                seq: d.u64()?,
                attestations: d.list(MAX_ATTESTATIONS)?,
            }),
            0x54 => Ok(TxKind::RevokeCredential {
                credential: d.get()?,
                seq: d.u64()?,
                attestations: d.list(MAX_ATTESTATIONS)?,
            }),
            0x55 => Ok(TxKind::DefenseAction {
                credential: d.get()?,
                scope: d.get()?,
                subject: d.get()?,
            }),
            0x56 => Ok(TxKind::AttestContribution {
                incident: d.get()?,
                node: d.get()?,
                role: d.get()?,
                evidence: d.get()?,
                seq: d.u64()?,
                attestations: d.list(MAX_ATTESTATIONS)?,
            }),
            0x60 => Ok(TxKind::PublishModule {
                code: d.bytes(crate::runtime::MAX_CODE)?,
            }),
            0x61 => Ok(TxKind::BindModule {
                community: d.get()?,
                module: d.option()?,
                approvals: d.list(MAX_CONTROLLERS)?,
            }),
            0x62 => Ok(TxKind::CallModule {
                community: d.get()?,
                method: d.str(crate::runtime::MAX_METHOD)?,
                args: d.bytes(crate::runtime::MAX_ARGS)?,
                fuel: d.u64()?,
            }),
            0x70 => {
                let chain = d.str(crate::bridge::MAX_CHAIN_NAME)?;
                let n = d.u32()? as usize;
                if n > crate::bridge::MAX_HEADERS_PER_TX {
                    return Err(DecodeError::LengthExceeded {
                        declared: n as u64,
                        max: crate::bridge::MAX_HEADERS_PER_TX as u64,
                    });
                }
                let mut headers = Vec::with_capacity(n);
                for _ in 0..n {
                    headers.push(d.fixed::<80>()?);
                }
                Ok(TxKind::BridgeHeaders { chain, headers })
            }
            0x71 => Ok(TxKind::BridgeDeposit {
                chain: d.str(crate::bridge::MAX_CHAIN_NAME)?,
                block: d.get()?,
                tx: d.bytes(crate::bridge::MAX_ORIGIN_TX)?,
                path: d.list(crate::bridge::MAX_MERKLE_DEPTH)?,
                index: d.u32()?,
            }),
            0x72 => Ok(TxKind::HtlcLock {
                asset: d.get()?,
                amount: d.u64()?,
                recipient: d.get()?,
                hashlock: d.get()?,
                timeout: d.u64()?,
            }),
            0x73 => Ok(TxKind::HtlcClaim {
                id: d.get()?,
                preimage: d.bytes(crate::bridge::MAX_PREIMAGE)?,
            }),
            0x74 => Ok(TxKind::HtlcRefund { id: d.get()? }),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Conteúdo assinado da transação.
///
/// Não há timestamp nem campos de metadados opcionais: somente o necessário
/// para validar a operação (`SPEC §32`, THR-PRIV-003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxBody {
    pub version: u16,
    pub sender: PublicKey,
    /// Deve ser igual ao nonce atual da conta remetente (THR-ID-004).
    pub nonce: u64,
    /// Taxa explícita em unidades mínimas (`SPEC §36`).
    pub fee: u64,
    pub kind: TxKind,
}

impl Encode for TxBody {
    fn encode(&self, e: &mut Encoder) {
        e.u16(self.version)
            .put(&self.sender)
            .u64(self.nonce)
            .u64(self.fee)
            .put(&self.kind);
    }
}

impl Decode for TxBody {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            version: d.u16()?,
            sender: d.get()?,
            nonce: d.u64()?,
            fee: d.u64()?,
            kind: d.get()?,
        })
    }
}

impl TxBody {
    pub fn id(&self) -> TxId {
        TxId(hash(context::TX_ID, &self.to_canonical_bytes()))
    }

    pub fn sender_address(&self) -> Address {
        self.sender.address()
    }

    /// Assina localmente (`SPEC-WAL-002`). A chave nunca sai da Wallet.
    ///
    /// Retorna erro se a chave não corresponder a `sender`.
    pub fn sign(self, key: &SecretKey, network_id: &str) -> Result<Transaction, TxError> {
        if key.public_key() != self.sender {
            return Err(TxError::Signature);
        }
        let signature = key.sign(
            context::TX_SIGNATURE,
            network_id,
            &self.to_canonical_bytes(),
        );
        Ok(Transaction::Account(AccountTx {
            body: self,
            signature,
        }))
    }
}

/// Transação de conta (transparente), assinada pelo remetente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountTx {
    pub body: TxBody,
    pub signature: Signature,
}

/// Transação: de conta (tag `0x00`) ou privada (tag `0x01`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transaction {
    Account(AccountTx),
    Private(PrivateTx),
}

impl Encode for Transaction {
    fn encode(&self, e: &mut Encoder) {
        match self {
            Transaction::Account(a) => {
                e.u8(0x00).put(a);
            }
            Transaction::Private(p) => {
                e.u8(0x01).put(p);
            }
        }
    }
}

impl Decode for Transaction {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0x00 => Ok(Transaction::Account(d.get()?)),
            0x01 => Ok(Transaction::Private(d.get()?)),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

impl Transaction {
    /// Combustível reservado (só chamadas ao Exonet Runtime).
    pub fn fuel(&self) -> u64 {
        match self {
            Transaction::Account(a) => match &a.body.kind {
                TxKind::CallModule { fuel, .. } => *fuel,
                _ => 0,
            },
            Transaction::Private(_) => 0,
        }
    }

    pub fn id(&self) -> TxId {
        match self {
            Transaction::Account(a) => a.id(),
            Transaction::Private(p) => p.id(),
        }
    }

    pub fn fee(&self) -> u64 {
        match self {
            Transaction::Account(a) => a.body.fee,
            Transaction::Private(p) => p.fee,
        }
    }

    pub fn as_account(&self) -> Option<&AccountTx> {
        match self {
            Transaction::Account(a) => Some(a),
            Transaction::Private(_) => None,
        }
    }

    pub fn as_private(&self) -> Option<&PrivateTx> {
        match self {
            Transaction::Private(p) => Some(p),
            Transaction::Account(_) => None,
        }
    }
}

impl Encode for AccountTx {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.body).put(&self.signature);
    }
}

impl Decode for AccountTx {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            body: d.get()?,
            signature: d.get()?,
        })
    }
}

impl AccountTx {
    pub fn id(&self) -> TxId {
        self.body.id()
    }

    /// Validações que não dependem do estado: formato e taxa mínima.
    pub fn check_stateless(&self, min_fee: u64) -> Result<(), TxError> {
        if self.body.version != TX_VERSION {
            return Err(TxError::UnsupportedVersion(self.body.version));
        }
        match &self.body.kind {
            TxKind::Transfer { amount, .. } if *amount == 0 => {
                return Err(TxError::ZeroAmount);
            }
            TxKind::Transfer { .. } => {}
            TxKind::Shield {
                amount, outputs, ..
            } => {
                if *amount == 0 {
                    return Err(TxError::ZeroAmount);
                }
                if outputs.is_empty() {
                    return Err(TxError::InvalidPrivate("blindagem sem saídas"));
                }
            }
            TxKind::LockStake { amount, .. }
            | TxKind::Bond { amount }
            | TxKind::Unbond { amount }
            | TxKind::TransferAsset { amount, .. }
            | TxKind::PoolDeposit { amount, .. }
            | TxKind::PlaceOrder { amount, .. }
                if *amount == 0 =>
            {
                return Err(TxError::ZeroAmount);
            }
            TxKind::LockStake { .. }
            | TxKind::Unlock { .. }
            | TxKind::Propose { .. }
            | TxKind::Vote { .. }
            | TxKind::ReportEquivocation { .. }
            | TxKind::Bond { .. }
            | TxKind::Unbond { .. }
            | TxKind::ReportDoubleVote { .. }
            | TxKind::ReportDoubleProposal { .. }
            | TxKind::TransferAsset { .. }
            | TxKind::PoolDeposit { .. }
            | TxKind::CancelOrder { .. }
            | TxKind::CommunityPosition { .. }
            | TxKind::UpdateName { .. }
            | TxKind::RevokeCredential { .. }
            | TxKind::DefenseAction { .. } => {}
            // Evidência é obrigatória nas decisões de defesa (`SPEC §52`).
            TxKind::DefenseTransition { evidence, .. }
            | TxKind::IncidentUpdate { evidence, .. }
            | TxKind::AttestContribution { evidence, .. }
            | TxKind::CloseIncident {
                archive: evidence, ..
            } if *evidence == Hash32::ZERO => {
                return Err(TxError::Defense("evidência ausente"));
            }
            TxKind::DefenseTransition { .. }
            | TxKind::IncidentUpdate { .. }
            | TxKind::AttestContribution { .. }
            | TxKind::CloseIncident { .. } => {}
            TxKind::GrantCredential { scopes, .. } => {
                let unique: std::collections::BTreeSet<_> = scopes.iter().collect();
                if scopes.is_empty() || unique.len() != scopes.len() {
                    return Err(TxError::Defense("escopos vazios ou repetidos"));
                }
            }
            TxKind::DeclareCommunity { name, rule, .. } => {
                crate::community::check_name(name).map_err(TxError::Community)?;
                rule.validate().map_err(TxError::Community)?;
            }
            TxKind::UpdateCommunity { rule, .. } => {
                if let Some(r) = rule {
                    r.validate().map_err(TxError::Community)?;
                }
            }
            TxKind::RegisterName { name, kind, .. } => {
                crate::community::check_name(name).map_err(TxError::Community)?;
                if *kind == NameKind::Community {
                    return Err(TxError::Community(
                        "nomes .comunidade vêm do reconhecimento de Comunidades",
                    ));
                }
            }
            TxKind::PlaceOrder { price, .. } if *price == 0 => {
                return Err(TxError::Market("preço nulo"));
            }
            TxKind::PlaceOrder { .. } => {}
            TxKind::PublishModule { code } => {
                if code.is_empty() {
                    return Err(TxError::Runtime("módulo vazio"));
                }
            }
            TxKind::BindModule { .. } => {}
            TxKind::BridgeHeaders { headers, .. } => {
                if headers.is_empty() {
                    return Err(TxError::Bridge("nenhum cabeçalho"));
                }
            }
            TxKind::BridgeDeposit { .. } | TxKind::HtlcClaim { .. } | TxKind::HtlcRefund { .. } => {
            }
            TxKind::HtlcLock { amount, .. } if *amount == 0 => return Err(TxError::ZeroAmount),
            TxKind::HtlcLock { .. } => {}
            TxKind::CallModule { method, fuel, .. } => {
                crate::runtime::check_method(method).map_err(TxError::Runtime)?;
                if *fuel == 0 {
                    return Err(TxError::Runtime("combustível nulo"));
                }
            }
        }
        if self.body.fee < min_fee {
            return Err(TxError::FeeTooLow {
                fee: self.body.fee,
                min: min_fee,
            });
        }
        Ok(())
    }

    /// Verifica a assinatura contra a chave do remetente (`SPEC-WAL-004`).
    pub fn verify_signature(&self, network_id: &str) -> Result<(), TxError> {
        self.body
            .sender
            .verify(
                context::TX_SIGNATURE,
                network_id,
                &self.body.to_canonical_bytes(),
                &self.signature,
            )
            .map_err(|_: CryptoError| TxError::Signature)
    }
}

/// Motivo de rejeição de uma transação.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxError {
    UnsupportedVersion(u16),
    ZeroAmount,
    FeeTooLow {
        fee: u64,
        min: u64,
    },
    Signature,
    BadNonce {
        expected: u64,
        got: u64,
    },
    InsufficientBalance {
        balance: u64,
        required: u64,
    },
    Overflow,
    /// Estrutura de transação privada inválida.
    InvalidPrivate(&'static str),
    /// Prova de faixa inválida.
    RangeProof,
    /// Prova de excesso (blindagem) inválida.
    Excess,
    /// Assinatura em anel inválida.
    RingSignature,
    /// Entradas e saídas não se equilibram.
    Balance,
    /// Nota já gasta (imagem de chave repetida).
    KeyImageSpent,
    /// A oferta privada ficaria negativa — defesa contra inflação oculta.
    ShieldedSupplyUnderflow,
    /// Regra de governança violada.
    Governance(&'static str),
    /// Depósito de proposta abaixo do vigente.
    DepositTooLow {
        deposit: u64,
        min: u64,
    },
    /// Evidência de equivocação inválida.
    InvalidEvidence(&'static str),
    /// Regra do Grande Mercado ou do Pool violada.
    Market(&'static str),
    /// Regra de Comunidades ou de nomes violada.
    Community(&'static str),
    /// Regra de defesa violada.
    Defense(&'static str),
    /// Regra de staking violada.
    Staking(&'static str),
    /// Regra do Exonet Runtime violada.
    Runtime(&'static str),
    /// Regra da ponte ou de troca atômica violada.
    Bridge(&'static str),
}

impl fmt::Display for TxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "versão de transação não suportada: {v}"),
            Self::ZeroAmount => write!(f, "valor da transferência é zero"),
            Self::FeeTooLow { fee, min } => write!(f, "taxa {fee} abaixo do mínimo {min}"),
            Self::Signature => write!(f, "assinatura inválida"),
            Self::BadNonce { expected, got } => {
                write!(f, "nonce inválido: esperado {expected}, recebido {got}")
            }
            Self::InsufficientBalance { balance, required } => {
                write!(f, "saldo insuficiente: {balance} < {required}")
            }
            Self::Overflow => write!(f, "overflow aritmético"),
            Self::InvalidPrivate(w) => write!(f, "transação privada inválida: {w}"),
            Self::RangeProof => write!(f, "prova de faixa inválida"),
            Self::Excess => write!(f, "prova de excesso inválida"),
            Self::RingSignature => write!(f, "assinatura em anel inválida"),
            Self::Balance => write!(f, "entradas e saídas não se equilibram"),
            Self::KeyImageSpent => write!(f, "nota já gasta"),
            Self::ShieldedSupplyUnderflow => write!(f, "oferta privada ficaria negativa"),
            Self::Governance(w) => write!(f, "governança: {w}"),
            Self::DepositTooLow { deposit, min } => {
                write!(f, "depósito {deposit} abaixo do mínimo {min}")
            }
            Self::InvalidEvidence(w) => write!(f, "evidência inválida: {w}"),
            Self::Staking(w) => write!(f, "staking: {w}"),
            Self::Market(w) => write!(f, "mercado: {w}"),
            Self::Community(w) => write!(f, "comunidade: {w}"),
            Self::Defense(w) => write!(f, "defesa: {w}"),
            Self::Runtime(w) => write!(f, "runtime: {w}"),
            Self::Bridge(w) => write!(f, "ponte: {w}"),
        }
    }
}

impl std::error::Error for TxError {}

#[cfg(test)]
mod tests {
    use super::*;

    const NET: &str = "rede-zero-devnet-test";

    fn acc(tx: Transaction) -> AccountTx {
        match tx {
            Transaction::Account(a) => a,
            Transaction::Private(_) => unreachable!(),
        }
    }

    fn body(key: &SecretKey) -> TxBody {
        TxBody {
            version: TX_VERSION,
            sender: key.public_key(),
            nonce: 0,
            fee: 10,
            kind: TxKind::Transfer {
                to: SecretKey::from_seed([9; 32]).public_key().address(),
                amount: 1_000,
            },
        }
    }

    #[test]
    fn roundtrip() {
        let k = SecretKey::from_seed([1; 32]);
        let tx = acc(body(&k).sign(&k, NET).unwrap());
        let bytes = tx.to_canonical_bytes();
        assert_eq!(AccountTx::from_canonical_bytes(&bytes).unwrap(), tx);
    }

    // AT-CAN-001 — mesmo objeto, mesma codificação
    #[test]
    fn at_can_001_same_encoding() {
        let k = SecretKey::from_seed([1; 32]);
        assert_eq!(body(&k).to_canonical_bytes(), body(&k).to_canonical_bytes());
    }

    // AT-CAN-002 — campo alterado, codificação diferente
    #[test]
    fn at_can_002_changed_field() {
        let k = SecretKey::from_seed([1; 32]);
        let mut b = body(&k);
        let before = b.to_canonical_bytes();
        b.fee += 1;
        assert_ne!(before, b.to_canonical_bytes());
    }

    // AT-CAN-003 — ordem não ambígua: bytes extras ou tag desconhecida rejeitados
    #[test]
    fn at_can_003_unambiguous() {
        let k = SecretKey::from_seed([1; 32]);
        let mut bytes = body(&k).sign(&k, NET).unwrap().to_canonical_bytes();
        bytes.push(0);
        assert!(Transaction::from_canonical_bytes(&bytes).is_err());

        let mut bytes = body(&k).to_canonical_bytes();
        let tag_pos = 2 + 32 + 8 + 8;
        bytes[tag_pos] = 0x7f;
        assert_eq!(
            TxBody::from_canonical_bytes(&bytes),
            Err(DecodeError::InvalidTag(0x7f))
        );
    }

    // AT-TID-001 — determinismo do ID
    #[test]
    fn at_tid_001_deterministic() {
        let k = SecretKey::from_seed([1; 32]);
        assert_eq!(body(&k).id(), body(&k).id());
    }

    // AT-TID-002 — alteração relevante muda o ID
    #[test]
    fn at_tid_002_changed() {
        let k = SecretKey::from_seed([1; 32]);
        let mut b = body(&k);
        let id = b.id();
        b.nonce = 1;
        assert_ne!(id, b.id());
    }

    // AT-TX-002 — transação malformada
    #[test]
    fn at_tx_002_malformed() {
        assert!(Transaction::from_canonical_bytes(&[1, 2, 3]).is_err());
    }

    // AT-TX-003 / AT-TX-004 — assinatura inválida / conteúdo alterado
    #[test]
    fn at_tx_003_004_signature() {
        let k = SecretKey::from_seed([1; 32]);
        let tx = acc(body(&k).sign(&k, NET).unwrap());
        assert!(tx.verify_signature(NET).is_ok());

        let mut tampered = tx.clone();
        tampered.body.fee = 0;
        assert_eq!(tampered.verify_signature(NET), Err(TxError::Signature));

        let mut bad_sig = tx.clone();
        bad_sig.signature.0[0] ^= 1;
        assert_eq!(bad_sig.verify_signature(NET), Err(TxError::Signature));

        // THR-ID-003 — outra rede
        assert_eq!(
            tx.verify_signature("rede-zero-mainnet"),
            Err(TxError::Signature)
        );
    }

    // AT-TX-005 — taxa inválida
    #[test]
    fn at_tx_005_fee() {
        let k = SecretKey::from_seed([1; 32]);
        let tx = acc(body(&k).sign(&k, NET).unwrap());
        assert!(tx.check_stateless(10).is_ok());
        assert_eq!(
            tx.check_stateless(11),
            Err(TxError::FeeTooLow { fee: 10, min: 11 })
        );
    }

    #[test]
    fn sign_with_wrong_key_fails() {
        let k = SecretKey::from_seed([1; 32]);
        let other = SecretKey::from_seed([2; 32]);
        assert_eq!(body(&k).sign(&other, NET), Err(TxError::Signature));
    }

    #[test]
    fn zero_amount_rejected() {
        let k = SecretKey::from_seed([1; 32]);
        let mut b = body(&k);
        b.kind = TxKind::Transfer {
            to: b.sender_address(),
            amount: 0,
        };
        let tx = acc(b.sign(&k, NET).unwrap());
        assert_eq!(tx.check_stateless(0), Err(TxError::ZeroAmount));
    }

    #[test]
    fn unsupported_version_rejected() {
        let k = SecretKey::from_seed([1; 32]);
        let mut b = body(&k);
        b.version = 2;
        let tx = acc(b.sign(&k, NET).unwrap());
        assert_eq!(tx.check_stateless(0), Err(TxError::UnsupportedVersion(2)));
    }
}
