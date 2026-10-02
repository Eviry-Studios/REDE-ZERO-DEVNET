use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};

use crate::{context, hash, hex, Hash32};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Bytes não representam uma chave pública Ed25519 válida.
    InvalidPublicKey,
    /// A assinatura não confere com a mensagem e a chave.
    InvalidSignature,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPublicKey => write!(f, "chave pública inválida"),
            Self::InvalidSignature => write!(f, "assinatura inválida"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// Constrói a mensagem efetivamente assinada (`spec/CRYPTOGRAPHY.md §5`):
///
/// ```text
/// bytes(contexto) || bytes(network_id) || payload
/// ```
///
/// onde `bytes(x)` é a codificação canônica com prefixo `u32`.
pub fn signing_message(context: &str, network_id: &str, payload: &[u8]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.str(context).str(network_id).fixed(payload);
    e.into_bytes()
}

/// Chave pública Ed25519 (32 bytes), sempre validada na construção.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, CryptoError> {
        let vk = VerifyingKey::from_bytes(&bytes).map_err(|_| CryptoError::InvalidPublicKey)?;
        // Chaves de ordem pequena permitiriam assinaturas triviais.
        if vk.is_weak() {
            return Err(CryptoError::InvalidPublicKey);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, CryptoError> {
        let bytes = hex::decode_array(s).ok_or(CryptoError::InvalidPublicKey)?;
        Self::from_bytes(bytes)
    }

    /// Verifica uma assinatura com verificação estrita (rejeita assinaturas
    /// maleáveis e componentes de ordem pequena).
    pub fn verify(
        &self,
        context: &str,
        network_id: &str,
        payload: &[u8],
        signature: &Signature,
    ) -> Result<(), CryptoError> {
        let vk = VerifyingKey::from_bytes(&self.0).map_err(|_| CryptoError::InvalidPublicKey)?;
        let sig = ed25519_dalek::Signature::from_bytes(&signature.0);
        let msg = signing_message(context, network_id, payload);
        vk.verify_strict(&msg, &sig)
            .map_err(|_| CryptoError::InvalidSignature)
    }

    pub fn node_id(&self) -> NodeId {
        NodeId(hash(context::NODE_ID, &self.0))
    }

    pub fn address(&self) -> Address {
        Address(hash(context::ADDRESS, &self.0))
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({}…)", &self.to_hex()[..12])
    }
}

impl Encode for PublicKey {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.0);
    }
}

impl Decode for PublicKey {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Self::from_bytes(d.fixed()?).map_err(|_| DecodeError::InvalidValue("chave pública"))
    }
}

/// Assinatura Ed25519 (64 bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({}…)", &self.to_hex()[..12])
    }
}

impl Encode for Signature {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.0);
    }
}

impl Decode for Signature {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self(d.fixed()?))
    }
}

/// Chave privada. Nunca é serializada pelo protocolo (`SPEC-ID-001`); o
/// material é apagado da memória ao ser descartado.
pub struct SecretKey(SigningKey);

impl SecretKey {
    /// Gera uma chave nova a partir da entropia do sistema operacional.
    pub fn generate() -> Self {
        Self(SigningKey::generate(&mut OsRng))
    }

    /// Reconstrói a chave a partir da semente de 32 bytes (RFC 8032).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes(&seed))
    }

    /// Semente de 32 bytes, apenas para armazenamento local pela Wallet/Node.
    pub fn seed(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    pub fn sign(&self, context: &str, network_id: &str, payload: &[u8]) -> Signature {
        let msg = signing_message(context, network_id, payload);
        Signature(self.0.sign(&msg).to_bytes())
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Nunca imprime o material privado.
        write!(f, "SecretKey(<oculta>, pub={:?})", self.public_key())
    }
}

macro_rules! id_type {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub Hash32);

        impl $name {
            pub fn to_hex(&self) -> String { self.0.to_hex() }
            pub fn from_hex(s: &str) -> Option<Self> { Hash32::from_hex(s).map(Self) }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { fmt::Display::fmt(&self.0, f) }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), self.0)
            }
        }

        impl Encode for $name {
            fn encode(&self, e: &mut Encoder) { self.0.encode(e); }
        }

        impl Decode for $name {
            fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> { Ok(Self(d.get()?)) }
        }
    };
}

id_type!(
    /// Identidade técnica de um Node: `H(NODE_ID, chave_pública)`.
    NodeId
);
id_type!(
    /// Endereço de Wallet: `H(ADDRESS, chave_pública)`.
    Address
);

#[cfg(test)]
mod tests {
    use super::*;

    const NET: &str = "rede-zero-devnet-test";

    fn key(n: u8) -> SecretKey {
        SecretKey::from_seed([n; 32])
    }

    // AT-ID-001 — Geração de identidade
    #[test]
    fn at_id_001_generation() {
        let sk = SecretKey::generate();
        let pk = sk.public_key();
        assert_eq!(PublicKey::from_bytes(*pk.as_bytes()), Ok(pk));
        assert_ne!(pk.node_id(), NodeId::default());
    }

    // AT-ID-002 — Determinismo do Node ID
    #[test]
    fn at_id_002_node_id_deterministic() {
        let pk = key(1).public_key();
        assert_eq!(pk.node_id(), pk.node_id());
        assert_eq!(key(1).public_key().node_id(), pk.node_id());
    }

    // AT-ID-003 — Chaves diferentes produzem identidades diferentes
    #[test]
    fn at_id_003_different_keys() {
        assert_ne!(key(1).public_key().node_id(), key(2).public_key().node_id());
    }

    // AT-ID-004 — Assinatura válida
    #[test]
    fn at_id_004_valid_signature() {
        let sk = key(1);
        let sig = sk.sign(context::TX_SIGNATURE, NET, b"payload");
        assert!(sk
            .public_key()
            .verify(context::TX_SIGNATURE, NET, b"payload", &sig)
            .is_ok());
    }

    // AT-ID-005 — Assinatura adulterada
    #[test]
    fn at_id_005_tampered_signature() {
        let sk = key(1);
        let mut sig = sk.sign(context::TX_SIGNATURE, NET, b"payload");
        sig.0[10] ^= 1;
        assert_eq!(
            sk.public_key()
                .verify(context::TX_SIGNATURE, NET, b"payload", &sig),
            Err(CryptoError::InvalidSignature)
        );
        let sig = sk.sign(context::TX_SIGNATURE, NET, b"payload");
        assert!(sk
            .public_key()
            .verify(context::TX_SIGNATURE, NET, b"payloaD", &sig)
            .is_err());
    }

    // AT-ID-006 — Chave incorreta
    #[test]
    fn at_id_006_wrong_key() {
        let sig = key(1).sign(context::TX_SIGNATURE, NET, b"payload");
        assert!(key(2)
            .public_key()
            .verify(context::TX_SIGNATURE, NET, b"payload", &sig)
            .is_err());
    }

    // THR-ID-002 — assinatura de um contexto não vale em outro
    #[test]
    fn signature_bound_to_context() {
        let sk = key(1);
        let sig = sk.sign(context::TX_SIGNATURE, NET, b"payload");
        assert!(sk
            .public_key()
            .verify(context::BLOCK_SIGNATURE, NET, b"payload", &sig)
            .is_err());
    }

    // THR-ID-003 — assinatura de uma rede não vale em outra
    #[test]
    fn signature_bound_to_network() {
        let sk = key(1);
        let sig = sk.sign(context::TX_SIGNATURE, NET, b"payload");
        assert!(sk
            .public_key()
            .verify(context::TX_SIGNATURE, "rede-zero-mainnet", b"payload", &sig)
            .is_err());
    }

    // O prefixo de comprimento impede ambiguidade entre contexto/rede/payload.
    #[test]
    fn signing_message_is_unambiguous() {
        assert_ne!(
            signing_message("ab", "c", b""),
            signing_message("a", "bc", b"")
        );
    }

    #[test]
    fn node_id_and_address_are_distinct_domains() {
        let pk = key(1).public_key();
        assert_ne!(pk.node_id().0, pk.address().0);
    }

    #[test]
    fn weak_public_key_rejected() {
        // Ponto identidade (ordem pequena).
        let mut identity = [0u8; 32];
        identity[0] = 1;
        assert_eq!(
            PublicKey::from_bytes(identity),
            Err(CryptoError::InvalidPublicKey)
        );
    }

    #[test]
    fn seed_roundtrip() {
        let sk = SecretKey::generate();
        assert_eq!(
            SecretKey::from_seed(sk.seed()).public_key(),
            sk.public_key()
        );
    }

    #[test]
    fn debug_never_prints_secret() {
        let sk = key(7);
        let dbg = format!("{sk:?}");
        assert!(!dbg.contains(&hex::encode(&sk.seed())));
    }

    #[test]
    fn public_key_codec_rejects_invalid_point() {
        let mut identity = [0u8; 32];
        identity[0] = 1;
        assert!(PublicKey::from_canonical_bytes(&identity).is_err());
    }
}

#[cfg(test)]
mod vectors {
    //! Vetores publicados em `spec/CRYPTOGRAPHY.md §8`. Qualquer implementação
    //! conforme deve reproduzi-los byte a byte.
    use super::*;

    #[test]
    fn published_vectors() {
        let sk = SecretKey::from_seed([1u8; 32]);
        let pk = sk.public_key();
        assert_eq!(
            pk.to_hex(),
            "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c"
        );
        assert_eq!(
            pk.node_id().to_hex(),
            "20a09f3a82f2ea8144e5b5fda01764841f287e572a210948d36cae69f034dd14"
        );
        assert_eq!(
            pk.address().to_hex(),
            "3c5385626729bf35550029e68fc6ae6f88e24c542f5d63b3e24b2cd687dd1fb6"
        );
        assert_eq!(
            hash(context::TX_ID, b"").to_hex(),
            "669381f31efb09384e1c5ebbd279b748028bdbb3f99be8f9ee136030bbe0126e"
        );
        assert_eq!(
            sk.sign(context::TX_SIGNATURE, "rede-zero-devnet-1", b"zero").to_hex(),
            "b159c866eb087d49f04636c2b758fa08196053e1e6bd83b2f369bd9e99ebf72843864f6a2dcb73d18d2ce285bf01549b19f14562ab64e4caaf9af22448d2710a"
        );
    }
}
