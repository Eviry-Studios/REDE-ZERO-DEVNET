//! Transações (`SPEC §6–§11`, `spec/TRANSACTIONS.md`).

use std::fmt;

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{context, hash, Address, CryptoError, Hash32, PublicKey, SecretKey, Signature};

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
    /// Tag `0x00` — transferência de ZERO.
    Transfer { to: Address, amount: u64 },
}

impl Encode for TxKind {
    fn encode(&self, e: &mut Encoder) {
        match self {
            TxKind::Transfer { to, amount } => {
                e.u8(0x00).put(to).u64(*amount);
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
        Ok(Transaction {
            body: self,
            signature,
        })
    }
}

/// Transação assinada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub body: TxBody,
    pub signature: Signature,
}

impl Encode for Transaction {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.body).put(&self.signature);
    }
}

impl Decode for Transaction {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            body: d.get()?,
            signature: d.get()?,
        })
    }
}

impl Transaction {
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
    FeeTooLow { fee: u64, min: u64 },
    Signature,
    BadNonce { expected: u64, got: u64 },
    InsufficientBalance { balance: u64, required: u64 },
    Overflow,
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
        }
    }
}

impl std::error::Error for TxError {}

#[cfg(test)]
mod tests {
    use super::*;

    const NET: &str = "rede-zero-devnet-test";

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
        let tx = body(&k).sign(&k, NET).unwrap();
        let bytes = tx.to_canonical_bytes();
        assert_eq!(Transaction::from_canonical_bytes(&bytes).unwrap(), tx);
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
        let tx = body(&k).sign(&k, NET).unwrap();
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
        let tx = body(&k).sign(&k, NET).unwrap();
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
        let tx = b.sign(&k, NET).unwrap();
        assert_eq!(tx.check_stateless(0), Err(TxError::ZeroAmount));
    }

    #[test]
    fn unsupported_version_rejected() {
        let k = SecretKey::from_seed([1; 32]);
        let mut b = body(&k);
        b.version = 2;
        let tx = b.sign(&k, NET).unwrap();
        assert_eq!(tx.check_stateless(0), Err(TxError::UnsupportedVersion(2)));
    }
}
