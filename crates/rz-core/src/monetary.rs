//! Política monetária do ZERO (ADR-0025, `spec/STATE.md §5`).
//!
//! Cada bloco finalizado cria `block_reward` unidades para o proponente, e
//! `fee_burn_bps` das taxas do bloco são queimadas. A emissão é constante
//! por altura e não tem teto: a inflação percentual cai com o tempo, e a
//! queima liga a oferta ao uso da rede.

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};

/// Recompensa máxima por bloco: 100 ZERO.
pub const MAX_BLOCK_REWARD: u64 = 100 * crate::amount::UNITS_PER_ZERO;
/// Denominador de `fee_burn_bps`.
pub const BPS: u64 = 10_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MonetaryParams {
    /// Unidades criadas a cada bloco, creditadas ao proponente.
    pub block_reward: u64,
    /// Parte das taxas do bloco que é queimada, em pontos-base.
    pub fee_burn_bps: u64,
}

impl MonetaryParams {
    /// Valores adotados para a TESTNET (ADR-0025): 0,1 ZERO por bloco e
    /// metade das taxas queimada.
    pub const TESTNET: Self = Self {
        block_reward: crate::amount::UNITS_PER_ZERO / 10,
        fee_burn_bps: 5_000,
    };

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.block_reward > MAX_BLOCK_REWARD {
            return Err("block_reward acima do máximo");
        }
        if self.fee_burn_bps > BPS {
            return Err("fee_burn_bps acima de 10 000");
        }
        Ok(())
    }

    /// Divide as taxas de um bloco: `(queimado, para o proponente)`.
    pub fn split_fees(&self, fees: u64) -> (u64, u64) {
        let burned =
            (u128::from(fees) * u128::from(self.fee_burn_bps.min(BPS)) / u128::from(BPS)) as u64;
        (burned, fees - burned)
    }
}

impl Encode for MonetaryParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.block_reward).u64(self.fee_burn_bps);
    }
}

impl Decode for MonetaryParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            block_reward: d.u64()?,
            fee_burn_bps: d.u64()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_bounds() {
        let m = MonetaryParams::TESTNET;
        assert_eq!(m.split_fees(0), (0, 0));
        assert_eq!(m.split_fees(7), (3, 4));
        assert_eq!(
            m.split_fees(u64::MAX).0 + m.split_fees(u64::MAX).1,
            u64::MAX
        );
        assert!(m.validate().is_ok());
        let all = MonetaryParams {
            block_reward: 0,
            fee_burn_bps: BPS,
        };
        assert_eq!(all.split_fees(9), (9, 0));
        assert!(MonetaryParams {
            block_reward: MAX_BLOCK_REWARD + 1,
            fee_burn_bps: 0
        }
        .validate()
        .is_err());
        assert!(MonetaryParams {
            block_reward: 0,
            fee_burn_bps: BPS + 1
        }
        .validate()
        .is_err());
    }
}
