//! Codificação canônica (`spec/ENCODING.md`) dos tipos de privacidade.
//!
//! A decodificação valida apenas a forma (tamanhos e limites). A validade
//! criptográfica (pontos, escalares, provas) é verificada pelo estado.

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};

use crate::clsag::Clsag;
use crate::excess::ExcessProof;
use crate::note::OutputData;

/// Máximo de membros em um anel aceito na decodificação.
pub const MAX_RING_DECODE: usize = 64;

impl Encode for OutputData {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.one_time_key)
            .fixed(&self.tx_pub)
            .fixed(&self.commitment)
            .fixed(&self.enc_amount);
    }
}

impl Decode for OutputData {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            one_time_key: d.fixed()?,
            tx_pub: d.fixed()?,
            commitment: d.fixed()?,
            enc_amount: d.fixed()?,
        })
    }
}

impl Encode for Clsag {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.c0).list(&self.s).fixed(&self.d);
    }
}

impl Decode for Clsag {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            c0: d.fixed()?,
            s: d.list(MAX_RING_DECODE)?,
            d: d.fixed()?,
        })
    }
}

impl Encode for ExcessProof {
    fn encode(&self, e: &mut Encoder) {
        e.fixed(&self.r).fixed(&self.s);
    }
}

impl Decode for ExcessProof {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            r: d.fixed()?,
            s: d.fixed()?,
        })
    }
}
