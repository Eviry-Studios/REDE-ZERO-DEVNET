//! Transações privadas (`spec/PRIVACY.md`, ADR-0009).
//!
//! Uma transação privada gasta notas existentes (escondidas em anéis) e cria
//! novas notas (valores e destinatários ocultos). Opcionalmente retira um
//! valor público para uma conta transparente.

use std::collections::BTreeSet;
use std::fmt;

use rand_core::{OsRng, RngCore};
use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Address, Hash32};
use rz_privacy::clsag::{self, Clsag, RingMember};
use rz_privacy::keys::ShieldedAddress;
use rz_privacy::note::{create_output, key_image, OutputData, OwnedNote};
use rz_privacy::{commitment, encode_point};

use rz_crypto::SecretKey;
use rz_privacy::excess;

use crate::tx::{Transaction, TxBody, TxError, TxId, TxKind, TX_VERSION};

/// Tamanho do anel: a nota real se esconde entre `RING_SIZE − 1` disfarces.
pub const RING_SIZE: usize = 11;
pub const MAX_PRIVATE_INPUTS: usize = 16;
pub const MAX_OUTPUTS: usize = 16;
pub const PRIVATE_TX_VERSION: u16 = 1;

pub mod context {
    pub const PRIVATE_TX_ID: &str = "rede-zero/private-tx-id/v1";
    pub const PRIVATE_TX_MSG: &str = "rede-zero/private-tx-message/v1";
    pub const SHIELD_MSG: &str = "rede-zero/shield-message/v1";
    pub const OUTPUT_ACC: &str = "rede-zero/output-accumulator/v1";
    pub const KEY_IMAGE_ACC: &str = "rede-zero/key-image-accumulator/v1";
}

/// Saída privada com prova de faixa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedOutput {
    pub data: OutputData,
    pub range_proof: Vec<u8>,
}

impl Encode for ShieldedOutput {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.data).bytes(&self.range_proof);
    }
}

impl Decode for ShieldedOutput {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            data: d.get()?,
            range_proof: d.bytes(commitment::MAX_RANGE_PROOF_LEN)?,
        })
    }
}

/// Entrada: anel de índices globais de saídas, imagem de chave e
/// pseudo-saída (compromisso do mesmo valor com outra ocultação).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingInput {
    /// Índices estritamente crescentes — a posição da nota real não é revelada.
    pub ring: Vec<u64>,
    pub key_image: [u8; 32],
    pub pseudo_out: [u8; 32],
}

impl Encode for RingInput {
    fn encode(&self, e: &mut Encoder) {
        e.list(&self.ring)
            .fixed(&self.key_image)
            .fixed(&self.pseudo_out);
    }
}

impl Decode for RingInput {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            ring: d.list(RING_SIZE)?,
            key_image: d.fixed()?,
            pseudo_out: d.fixed()?,
        })
    }
}

/// Retirada pública para conta transparente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unshield {
    pub to: Address,
    pub amount: u64,
}

impl Encode for Unshield {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.to).u64(self.amount);
    }
}

impl Decode for Unshield {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            to: d.get()?,
            amount: d.u64()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateTx {
    pub version: u16,
    pub inputs: Vec<RingInput>,
    pub outputs: Vec<ShieldedOutput>,
    pub unshield: Option<Unshield>,
    pub fee: u64,
    /// Uma assinatura CLSAG por entrada, na mesma ordem.
    pub signatures: Vec<Clsag>,
}

impl PrivateTx {
    /// Prefixo: tudo exceto as assinaturas.
    pub fn prefix_bytes(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        e.u16(self.version)
            .list(&self.inputs)
            .list(&self.outputs)
            .option(&self.unshield)
            .u64(self.fee);
        e.into_bytes()
    }

    pub fn id(&self) -> TxId {
        TxId(hash(context::PRIVATE_TX_ID, &self.prefix_bytes()))
    }

    /// Mensagem assinada por todas as entradas, vinculada à rede (THR-ID-003).
    pub fn message(&self, network_id: &str) -> [u8; 32] {
        let mut e = Encoder::new();
        e.str(network_id).fixed(&self.prefix_bytes());
        hash(context::PRIVATE_TX_MSG, &e.into_bytes()).0
    }

    pub fn public_amount(&self) -> Option<u64> {
        self.fee
            .checked_add(self.unshield.as_ref().map_or(0, |u| u.amount))
    }
}

impl Encode for PrivateTx {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.prefix_bytes()).list(&self.signatures);
    }
}

impl Decode for PrivateTx {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            version: d.u16()?,
            inputs: d.list(MAX_PRIVATE_INPUTS)?,
            outputs: d.list(MAX_OUTPUTS)?,
            unshield: d.option()?,
            fee: d.u64()?,
            signatures: d.list(MAX_PRIVATE_INPUTS)?,
        })
    }
}

/// Mensagem vinculada pela prova de excesso de uma operação de blindagem.
pub fn shield_message(sender: &[u8; 32], amount: u64, outputs: &[ShieldedOutput]) -> [u8; 32] {
    let mut e = Encoder::new();
    e.fixed(sender).u64(amount).list(outputs);
    hash(context::SHIELD_MSG, &e.into_bytes()).0
}

/// Acumulador encadeado: `acc' = H(ctx, acc ‖ item)`.
pub fn accumulate(ctx: &str, acc: &Hash32, item: &[u8]) -> Hash32 {
    let mut e = Encoder::new();
    e.put(acc).fixed(item);
    hash(ctx, &e.into_bytes())
}

/// Tamanho de anel exigido quando existem `total_outputs` saídas.
pub fn min_ring_size(total_outputs: usize) -> usize {
    RING_SIZE.min(total_outputs)
}

// ------------------------------------------------------------------ Wallet

/// Nota própria pronta para ser gasta.
#[derive(Clone, Debug)]
pub struct SpendableNote {
    pub global_index: u64,
    pub data: OutputData,
    pub note: OwnedNote,
}

impl SpendableNote {
    pub fn key_image(&self) -> [u8; 32] {
        key_image(&self.note.one_time_secret, &self.data.one_time_key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    NoInputs,
    TooManyInputs,
    TooManyOutputs,
    InsufficientFunds { available: u64, required: u64 },
    Overflow,
    NotEnoughOutputsForRing,
    Signing,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputs => write!(f, "nenhuma nota para gastar"),
            Self::TooManyInputs => write!(f, "notas demais em uma transação"),
            Self::TooManyOutputs => write!(f, "saídas demais"),
            Self::InsufficientFunds {
                available,
                required,
            } => write!(f, "saldo privado insuficiente: {available} < {required}"),
            Self::Overflow => write!(f, "overflow aritmético"),
            Self::NotEnoughOutputsForRing => write!(f, "saídas insuficientes para formar o anel"),
            Self::Signing => write!(f, "falha ao assinar"),
        }
    }
}

impl std::error::Error for BuildError {}

fn random_below(n: u64) -> u64 {
    // Amostragem por rejeição: distribuição uniforme sem viés de módulo.
    let zone = u64::MAX - (u64::MAX % n);
    loop {
        let v = OsRng.next_u64();
        if v < zone {
            return v % n;
        }
    }
}

/// Escolhe disfarces: metade entre as 100 saídas mais recentes (imitando o
/// padrão real de gasto, que favorece saídas novas) e metade uniforme.
fn select_ring(real: u64, total: u64, size: usize) -> Vec<u64> {
    let mut set = BTreeSet::from([real]);
    let recent_start = total.saturating_sub(100);
    let mut toggle = false;
    while set.len() < size {
        let idx = if toggle {
            recent_start + random_below(total - recent_start)
        } else {
            random_below(total)
        };
        toggle = !toggle;
        set.insert(idx);
    }
    set.into_iter().collect()
}

fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = random_below(i as u64 + 1) as usize;
        v.swap(i, j);
    }
}

/// Monta uma transação privada.
///
/// * `notes` — notas gastas;
/// * `all_outputs` — todas as saídas conhecidas da cadeia (para disfarces);
/// * `recipients` — destinatários privados;
/// * `change` — endereço para o troco (sempre criado, mesmo com valor zero,
///   para que transações não revelem se houve troco);
/// * `unshield` — retirada opcional para conta transparente.
pub fn build_private_tx(
    network_id: &str,
    notes: &[SpendableNote],
    all_outputs: &[OutputData],
    recipients: &[(ShieldedAddress, u64)],
    change: &ShieldedAddress,
    unshield: Option<Unshield>,
    fee: u64,
) -> Result<PrivateTx, BuildError> {
    if notes.is_empty() {
        return Err(BuildError::NoInputs);
    }
    if notes.len() > MAX_PRIVATE_INPUTS {
        return Err(BuildError::TooManyInputs);
    }
    if recipients.len() + 1 > MAX_OUTPUTS {
        return Err(BuildError::TooManyOutputs);
    }
    let available = notes
        .iter()
        .try_fold(0u64, |a, n| a.checked_add(n.note.amount))
        .ok_or(BuildError::Overflow)?;
    let mut required = fee
        .checked_add(unshield.as_ref().map_or(0, |u| u.amount))
        .ok_or(BuildError::Overflow)?;
    for (_, v) in recipients {
        required = required.checked_add(*v).ok_or(BuildError::Overflow)?;
    }
    if available < required {
        return Err(BuildError::InsufficientFunds {
            available,
            required,
        });
    }

    // Saídas: destinatários + troco, em ordem aleatória.
    let mut plan: Vec<(ShieldedAddress, u64)> = recipients.to_vec();
    plan.push((*change, available - required));
    shuffle(&mut plan);
    let created: Vec<_> = plan
        .iter()
        .map(|(to, v)| create_output(to, *v, &mut OsRng))
        .collect();
    let sum_out_blinding = created
        .iter()
        .fold(rz_privacy::Scalar::ZERO, |a, o| a + o.blinding);

    // Pseudo-saídas: ocultações aleatórias, exceto a última, que fecha o balanço.
    let mut pseudo_blindings: Vec<rz_privacy::Scalar> = (0..notes.len() - 1)
        .map(|_| rz_privacy::Scalar::random(&mut OsRng))
        .collect();
    let partial = pseudo_blindings
        .iter()
        .fold(rz_privacy::Scalar::ZERO, |a, y| a + y);
    pseudo_blindings.push(sum_out_blinding - partial);

    let total = all_outputs.len() as u64;
    let ring_size = min_ring_size(all_outputs.len());
    if ring_size == 0 {
        return Err(BuildError::NotEnoughOutputsForRing);
    }

    let mut inputs = Vec::with_capacity(notes.len());
    let mut secrets = Vec::with_capacity(notes.len());
    for (n, y) in notes.iter().zip(&pseudo_blindings) {
        if n.global_index >= total {
            return Err(BuildError::NotEnoughOutputsForRing);
        }
        let ring = select_ring(n.global_index, total, ring_size);
        let position = ring
            .iter()
            .position(|i| *i == n.global_index)
            .ok_or(BuildError::Signing)?;
        let members: Vec<RingMember> = ring
            .iter()
            .map(|i| {
                let o = &all_outputs[*i as usize];
                RingMember {
                    one_time_key: o.one_time_key,
                    commitment: o.commitment,
                }
            })
            .collect();
        inputs.push(RingInput {
            ring,
            key_image: n.key_image(),
            pseudo_out: encode_point(&commitment::commit(n.note.amount, y)),
        });
        secrets.push((
            members,
            position,
            n.note.one_time_secret,
            n.note.blinding - y,
        ));
    }

    let mut tx = PrivateTx {
        version: PRIVATE_TX_VERSION,
        inputs,
        outputs: created
            .into_iter()
            .map(|o| ShieldedOutput {
                data: o.data,
                range_proof: o.range_proof,
            })
            .collect(),
        unshield,
        fee,
        signatures: Vec::new(),
    };
    let msg = tx.message(network_id);
    for (input, (members, position, p, z)) in tx.inputs.iter().zip(secrets) {
        let (sig, ki) = clsag::sign(
            &members,
            position,
            &p,
            &z,
            &input.pseudo_out,
            &msg,
            &mut OsRng,
        )
        .ok_or(BuildError::Signing)?;
        if ki != input.key_image {
            return Err(BuildError::Signing);
        }
        tx.signatures.push(sig);
    }
    Ok(tx)
}

/// Monta uma blindagem: move `amount` da conta transparente de `key` para
/// uma nota privada de `to`.
///
/// O valor blindado é público (sai de uma conta transparente); a partir daí,
/// gastos da nota são privados.
pub fn build_shield(
    key: &SecretKey,
    network_id: &str,
    nonce: u64,
    fee: u64,
    to: &ShieldedAddress,
    amount: u64,
) -> Result<Transaction, TxError> {
    let out = create_output(to, amount, &mut OsRng);
    let outputs = vec![ShieldedOutput {
        data: out.data,
        range_proof: out.range_proof,
    }];
    let sender = key.public_key();
    let msg = shield_message(sender.as_bytes(), amount, &outputs);
    let proof = excess::prove(&out.blinding, &msg, &mut OsRng);
    TxBody {
        version: TX_VERSION,
        sender,
        nonce,
        fee,
        kind: TxKind::Shield {
            amount,
            outputs,
            excess: proof,
        },
    }
    .sign(key, network_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::tests::sample;
    use crate::state::{ExecParams, State};
    use rz_privacy::keys::ShieldedSecret;
    use rz_privacy::note::scan_output;

    const NET: &str = "rede-zero-devnet-test";

    fn rich() -> SecretKey {
        SecretKey::from_seed([2; 32])
    }

    fn alice() -> ShieldedSecret {
        ShieldedSecret::from_seed(&[10; 32])
    }

    fn bob() -> ShieldedSecret {
        ShieldedSecret::from_seed(&[11; 32])
    }

    fn notes_of(state: &State, who: &ShieldedSecret) -> Vec<SpendableNote> {
        state
            .outputs()
            .iter()
            .enumerate()
            .filter_map(|(i, o)| {
                scan_output(who, o).map(|note| SpendableNote {
                    global_index: i as u64,
                    data: o.clone(),
                    note,
                })
            })
            .filter(|n| !state.is_spent(&n.key_image()))
            .collect()
    }

    fn private_balance(state: &State, who: &ShieldedSecret) -> u64 {
        notes_of(state, who).iter().map(|n| n.note.amount).sum()
    }

    /// Estado com várias blindagens (para formar anéis) e 1 000 para Alice.
    fn setup() -> (crate::Genesis, State) {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let decoy = ShieldedSecret::from_seed(&[99; 32]).address();
        for nonce in 0..12 {
            let (to, amount) = if nonce == 5 {
                (alice().address(), 1_000)
            } else {
                (decoy, 10 + nonce)
            };
            let tx = build_shield(&rich(), NET, nonce, 1, &to, amount).unwrap();
            s.apply_transaction(&tx, &p).unwrap();
        }
        // Fora de um bloco as 12 taxas ainda não foram creditadas ao produtor.
        assert!(matches!(
            s.check_supply(),
            Err(crate::StateError::SupplyMismatch { expected, actual }) if expected as u128 - actual == 12
        ));
        (g, s)
    }

    #[test]
    fn shield_creates_private_note() {
        let (_, s) = setup();
        assert_eq!(private_balance(&s, &alice()), 1_000);
        assert_eq!(s.outputs().len(), 12);
        assert_eq!(s.shielded_supply(), 1_000 + (10..22).sum::<u64>() - 15);
    }

    #[test]
    fn shield_with_inflated_amount_rejected() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let mut tx = build_shield(&rich(), NET, 0, 1, &alice().address(), 100).unwrap();
        // Troca a saída por uma de valor maior, mantendo prova de excesso antiga.
        if let Transaction::Account(a) = &mut tx {
            if let TxKind::Shield { outputs, .. } = &mut a.body.kind {
                let big = create_output(&alice().address(), 1_000_000, &mut OsRng);
                outputs[0] = ShieldedOutput {
                    data: big.data,
                    range_proof: big.range_proof,
                };
            }
            *a = match a.body.clone().sign(&rich(), NET).unwrap() {
                Transaction::Account(x) => x,
                Transaction::Private(_) => unreachable!(),
            };
        }
        assert_eq!(s.check_transaction(&tx, &p), Err(TxError::Excess));
    }

    // REQ-024 — transferência privada: remetente, destinatário e valor ocultos.
    #[test]
    fn private_transfer_end_to_end() {
        let (g, mut s) = setup();
        let p = ExecParams::from_genesis(&g);
        let notes = notes_of(&s, &alice());
        let tx = build_private_tx(
            NET,
            &notes,
            s.outputs(),
            &[(bob().address(), 400)],
            &alice().address(),
            None,
            5,
        )
        .unwrap();
        let tx = Transaction::Private(tx);
        let supply = s.shielded_supply();
        let fee = s.apply_transaction(&tx, &p).unwrap();
        assert_eq!(fee, 5);
        assert_eq!(private_balance(&s, &bob()), 400);
        assert_eq!(private_balance(&s, &alice()), 595);
        assert_eq!(s.shielded_supply(), supply - 5);

        // O anel esconde a nota real entre RING_SIZE membros.
        let ptx = tx.as_private().unwrap();
        assert_eq!(ptx.inputs[0].ring.len(), RING_SIZE);
        // Sempre há troco, mesmo que Alice gastasse tudo: 2 saídas.
        assert_eq!(ptx.outputs.len(), 2);

        // AT-DS-001 (privado) — a mesma nota não pode ser gasta de novo.
        let again = build_private_tx(
            NET,
            &notes,
            s.outputs(),
            &[(bob().address(), 1)],
            &alice().address(),
            None,
            5,
        )
        .unwrap();
        assert_eq!(
            s.check_transaction(&Transaction::Private(again), &p),
            Err(TxError::KeyImageSpent)
        );
    }

    #[test]
    fn unshield_to_transparent() {
        let (g, mut s) = setup();
        let p = ExecParams::from_genesis(&g);
        let dest = SecretKey::from_seed([77; 32]).public_key().address();
        let tx = build_private_tx(
            NET,
            &notes_of(&s, &alice()),
            s.outputs(),
            &[],
            &alice().address(),
            Some(Unshield {
                to: dest,
                amount: 300,
            }),
            5,
        )
        .unwrap();
        s.apply_transaction(&Transaction::Private(tx), &p).unwrap();
        assert_eq!(s.account(&dest).balance, 300);
        assert_eq!(private_balance(&s, &alice()), 695);
    }

    // Tentativa de criar ZERO: saídas somam mais que as entradas.
    #[test]
    fn inflation_rejected() {
        let (g, s) = setup();
        let p = ExecParams::from_genesis(&g);
        let mut tx = build_private_tx(
            NET,
            &notes_of(&s, &alice()),
            s.outputs(),
            &[(bob().address(), 100)],
            &alice().address(),
            None,
            5,
        )
        .unwrap();
        let extra = create_output(&bob().address(), 1_000_000, &mut OsRng);
        tx.outputs.push(ShieldedOutput {
            data: extra.data,
            range_proof: extra.range_proof,
        });
        assert_eq!(
            s.check_transaction(&Transaction::Private(tx), &p),
            Err(TxError::Balance)
        );
    }

    #[test]
    fn tampering_rejected() {
        let (g, s) = setup();
        let p = ExecParams::from_genesis(&g);
        let build = || {
            build_private_tx(
                NET,
                &notes_of(&s, &alice()),
                s.outputs(),
                &[(bob().address(), 100)],
                &alice().address(),
                None,
                5,
            )
            .unwrap()
        };

        // Taxa alterada depois da assinatura.
        let mut t = build();
        t.fee = 6;
        assert!(s.check_transaction(&Transaction::Private(t), &p).is_err());

        // Prova de faixa trocada.
        let mut t = build();
        t.outputs[0].range_proof = t.outputs[1].range_proof.clone();
        assert!(s.check_transaction(&Transaction::Private(t), &p).is_err());

        // Assinatura adulterada.
        let mut t = build();
        t.signatures[0].s[3][0] ^= 1;
        assert_eq!(
            s.check_transaction(&Transaction::Private(t), &p),
            Err(TxError::RingSignature)
        );

        // Anel com membro inexistente.
        let mut t = build();
        *t.inputs[0].ring.last_mut().unwrap() = 10_000;
        assert_eq!(
            s.check_transaction(&Transaction::Private(t), &p),
            Err(TxError::InvalidPrivate("membro inexistente"))
        );

        // THR-ID-003 — assinada para outra rede.
        let t = build_private_tx(
            "rede-zero-devnet-outra",
            &notes_of(&s, &alice()),
            s.outputs(),
            &[(bob().address(), 100)],
            &alice().address(),
            None,
            5,
        )
        .unwrap();
        assert_eq!(
            s.check_transaction(&Transaction::Private(t), &p),
            Err(TxError::RingSignature)
        );
    }

    #[test]
    fn codec_roundtrip() {
        let (_, s) = setup();
        let t = Transaction::Private(
            build_private_tx(
                NET,
                &notes_of(&s, &alice()),
                s.outputs(),
                &[(bob().address(), 1)],
                &alice().address(),
                None,
                5,
            )
            .unwrap(),
        );
        let bytes = t.to_canonical_bytes();
        assert_eq!(Transaction::from_canonical_bytes(&bytes).unwrap(), t);
    }

    #[test]
    fn insufficient_private_funds() {
        let (_, s) = setup();
        let err = build_private_tx(
            NET,
            &notes_of(&s, &alice()),
            s.outputs(),
            &[(bob().address(), 5_000)],
            &alice().address(),
            None,
            5,
        )
        .unwrap_err();
        assert!(matches!(err, BuildError::InsufficientFunds { .. }));
    }
}
