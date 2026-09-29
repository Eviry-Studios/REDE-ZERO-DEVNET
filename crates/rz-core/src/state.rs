//! Estado da rede e função de transição (`SPEC §12–§14`, `§33–§36`,
//! `spec/STATE.md`, `spec/PRIVACY.md`).
//!
//! `S(n+1) = F(S(n), B(n+1))`. A única forma de alterar saldos é aplicar
//! transações válidas ou creditar taxas segundo a regra do bloco. Não existe
//! operação pública que permita `saldo += X` arbitrário (INV-003).
//!
//! O estado tem duas partes:
//!
//! * **transparente** — contas `endereço → (saldo, nonce)`;
//! * **privada** — lista de notas (saídas com valor oculto) e conjunto de
//!   imagens de chave já gastas, mais a **oferta privada** total, pública e
//!   rastreada por entradas e saídas públicas (defesa contra inflação oculta).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rz_codec::{Encode, Encoder};
use rz_crypto::{context, hash, Address, Hash32};
use rz_privacy::clsag::{self, RingMember};
use rz_privacy::note::OutputData;
use rz_privacy::{balance_holds, commitment, decode_point, excess};

use crate::genesis::{Genesis, GenesisError};
use crate::private::{
    accumulate, context as pctx, min_ring_size, shield_message, PrivateTx, ShieldedOutput,
    PRIVATE_TX_VERSION, RING_SIZE,
};
use crate::tx::{AccountTx, Transaction, TxError, TxKind};

/// Conta: saldo em unidades mínimas e contador de transações.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub balance: u64,
    pub nonce: u64,
}

/// Parâmetros de execução derivados do Genesis.
#[derive(Clone, Debug)]
pub struct ExecParams<'a> {
    pub network_id: &'a str,
    pub min_fee: u64,
}

impl<'a> ExecParams<'a> {
    pub fn from_genesis(g: &'a Genesis) -> Self {
        Self {
            network_id: &g.network_id,
            min_fee: g.min_fee,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateError {
    Genesis(GenesisError),
    /// A soma dos saldos divergiu da oferta total (violação de INV-003).
    SupplyMismatch {
        expected: u64,
        actual: u128,
    },
    Overflow,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Genesis(e) => write!(f, "genesis inválido: {e}"),
            Self::SupplyMismatch { expected, actual } => {
                write!(
                    f,
                    "oferta divergente: esperado {expected}, encontrado {actual}"
                )
            }
            Self::Overflow => write!(f, "overflow aritmético"),
        }
    }
}

impl std::error::Error for StateError {}

/// Estado completo. Estruturas ordenadas garantem determinismo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    accounts: BTreeMap<Address, Account>,
    total_supply: u64,
    shielded_supply: u64,
    outputs: Vec<OutputData>,
    output_acc: Hash32,
    key_images: BTreeSet<[u8; 32]>,
    key_image_log: Vec<[u8; 32]>,
    key_image_acc: Hash32,
}

/// Efeito calculado de uma transação, aplicado só após todas as verificações.
struct Effect {
    accounts: Vec<(Address, Account)>,
    new_outputs: Vec<OutputData>,
    spent: Vec<[u8; 32]>,
    shielded_supply: u64,
    fee: u64,
}

impl State {
    /// Estado inicial definido pelo Genesis.
    pub fn from_genesis(genesis: &Genesis) -> Result<Self, StateError> {
        genesis.validate().map_err(StateError::Genesis)?;
        let accounts = genesis
            .allocations
            .iter()
            .map(|a| {
                (
                    a.address,
                    Account {
                        balance: a.amount,
                        nonce: 0,
                    },
                )
            })
            .collect();
        let total_supply = genesis.total_supply().map_err(StateError::Genesis)?;
        Ok(Self {
            accounts,
            total_supply,
            shielded_supply: 0,
            outputs: Vec::new(),
            output_acc: Hash32::ZERO,
            key_images: BTreeSet::new(),
            key_image_log: Vec::new(),
            key_image_acc: Hash32::ZERO,
        })
    }

    pub fn account(&self, address: &Address) -> Account {
        self.accounts.get(address).copied().unwrap_or_default()
    }

    pub fn total_supply(&self) -> u64 {
        self.total_supply
    }

    /// Total de ZERO em notas privadas (público, sem revelar notas).
    pub fn shielded_supply(&self) -> u64 {
        self.shielded_supply
    }

    pub fn accounts(&self) -> impl Iterator<Item = (&Address, &Account)> {
        self.accounts.iter()
    }

    /// Todas as notas, por índice global.
    pub fn outputs(&self) -> &[OutputData] {
        &self.outputs
    }

    /// Imagens de chave gastas, na ordem em que foram registradas.
    pub fn key_image_log(&self) -> &[[u8; 32]] {
        &self.key_image_log
    }

    pub fn is_spent(&self, key_image: &[u8; 32]) -> bool {
        self.key_images.contains(key_image)
    }

    /// Compromisso criptográfico do estado (`spec/STATE.md §3`).
    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.total_supply).u64(self.shielded_supply);
        e.u32(u32::try_from(self.accounts.len()).unwrap_or(u32::MAX));
        for (addr, acc) in &self.accounts {
            e.put(addr).u64(acc.balance).u64(acc.nonce);
        }
        e.u64(self.outputs.len() as u64)
            .put(&self.output_acc)
            .u64(self.key_image_log.len() as u64)
            .put(&self.key_image_acc);
        hash(context::STATE_ROOT, &e.into_bytes())
    }

    /// Invariante monetária: `Σ saldos + oferta_privada = oferta_total`.
    pub fn check_supply(&self) -> Result<(), StateError> {
        let actual: u128 = self
            .accounts
            .values()
            .map(|a| a.balance as u128)
            .sum::<u128>()
            + self.shielded_supply as u128;
        if actual != self.total_supply as u128 {
            return Err(StateError::SupplyMismatch {
                expected: self.total_supply,
                actual,
            });
        }
        Ok(())
    }

    /// Valida uma transação contra este estado **sem alterá-lo**.
    pub fn check_transaction(&self, tx: &Transaction, p: &ExecParams<'_>) -> Result<(), TxError> {
        self.plan(tx, p).map(|_| ())
    }

    /// Aplica uma transação válida e retorna a taxa a ser distribuída.
    ///
    /// A operação é atômica: em caso de erro, o estado não é modificado.
    pub fn apply_transaction(
        &mut self,
        tx: &Transaction,
        p: &ExecParams<'_>,
    ) -> Result<u64, TxError> {
        let effect = self.plan(tx, p)?;
        for (addr, acc) in effect.accounts {
            self.accounts.insert(addr, acc);
        }
        for out in effect.new_outputs {
            self.output_acc = accumulate(
                pctx::OUTPUT_ACC,
                &self.output_acc,
                &out.to_canonical_bytes(),
            );
            self.outputs.push(out);
        }
        for ki in effect.spent {
            self.key_image_acc = accumulate(pctx::KEY_IMAGE_ACC, &self.key_image_acc, &ki);
            self.key_images.insert(ki);
            self.key_image_log.push(ki);
        }
        self.shielded_supply = effect.shielded_supply;
        Ok(effect.fee)
    }

    /// Credita as taxas coletadas no bloco ao produtor (ADR-0005).
    ///
    /// Taxas já foram debitadas dos remetentes, portanto a oferta total não muda.
    pub(crate) fn credit_fees(&mut self, to: Address, fees: u64) -> Result<(), StateError> {
        if fees == 0 {
            return Ok(());
        }
        let acc = self.accounts.entry(to).or_default();
        acc.balance = acc.balance.checked_add(fees).ok_or(StateError::Overflow)?;
        Ok(())
    }

    fn plan(&self, tx: &Transaction, p: &ExecParams<'_>) -> Result<Effect, TxError> {
        match tx {
            Transaction::Account(a) => self.plan_account(a, p),
            Transaction::Private(t) => self.plan_private(t, p),
        }
    }

    fn plan_account(&self, tx: &AccountTx, p: &ExecParams<'_>) -> Result<Effect, TxError> {
        tx.check_stateless(p.min_fee)?;
        tx.verify_signature(p.network_id)?;

        let sender = tx.body.sender_address();
        let mut sender_acc = self.account(&sender);
        if tx.body.nonce != sender_acc.nonce {
            return Err(TxError::BadNonce {
                expected: sender_acc.nonce,
                got: tx.body.nonce,
            });
        }
        let amount = match &tx.body.kind {
            TxKind::Transfer { amount, .. } | TxKind::Shield { amount, .. } => *amount,
        };
        let required = amount.checked_add(tx.body.fee).ok_or(TxError::Overflow)?;
        if sender_acc.balance < required {
            return Err(TxError::InsufficientBalance {
                balance: sender_acc.balance,
                required,
            });
        }
        sender_acc.balance -= required;
        sender_acc.nonce = sender_acc.nonce.checked_add(1).ok_or(TxError::Overflow)?;

        let mut effect = Effect {
            accounts: Vec::new(),
            new_outputs: Vec::new(),
            spent: Vec::new(),
            shielded_supply: self.shielded_supply,
            fee: tx.body.fee,
        };

        match &tx.body.kind {
            TxKind::Transfer { to, amount } => {
                if *to == sender {
                    sender_acc.balance = sender_acc
                        .balance
                        .checked_add(*amount)
                        .ok_or(TxError::Overflow)?;
                    effect.accounts.push((sender, sender_acc));
                } else {
                    let mut to_acc = self.account(to);
                    to_acc.balance = to_acc
                        .balance
                        .checked_add(*amount)
                        .ok_or(TxError::Overflow)?;
                    effect.accounts.push((sender, sender_acc));
                    effect.accounts.push((*to, to_acc));
                }
            }
            TxKind::Shield {
                amount,
                outputs,
                excess: proof,
            } => {
                verify_outputs(outputs)?;
                let commitments: Vec<[u8; 32]> =
                    outputs.iter().map(|o| o.data.commitment).collect();
                let msg = shield_message(tx.body.sender.as_bytes(), *amount, outputs);
                if !excess::verify(&commitments, *amount, &msg, proof) {
                    return Err(TxError::Excess);
                }
                effect.shielded_supply = self
                    .shielded_supply
                    .checked_add(*amount)
                    .ok_or(TxError::Overflow)?;
                effect.new_outputs = outputs.iter().map(|o| o.data.clone()).collect();
                effect.accounts.push((sender, sender_acc));
            }
        }
        Ok(effect)
    }

    fn plan_private(&self, tx: &PrivateTx, p: &ExecParams<'_>) -> Result<Effect, TxError> {
        // Verificações estruturais baratas primeiro (THR-P2P-002).
        if tx.version != PRIVATE_TX_VERSION {
            return Err(TxError::UnsupportedVersion(tx.version));
        }
        if tx.fee < p.min_fee {
            return Err(TxError::FeeTooLow {
                fee: tx.fee,
                min: p.min_fee,
            });
        }
        if tx.inputs.is_empty() {
            return Err(TxError::InvalidPrivate("sem entradas"));
        }
        if tx.signatures.len() != tx.inputs.len() {
            return Err(TxError::InvalidPrivate("número de assinaturas"));
        }
        if tx.outputs.is_empty() && tx.unshield.is_none() {
            return Err(TxError::InvalidPrivate("sem saídas"));
        }
        if tx.unshield.as_ref().is_some_and(|u| u.amount == 0) {
            return Err(TxError::ZeroAmount);
        }
        let public = tx.public_amount().ok_or(TxError::Overflow)?;

        let total = self.outputs.len() as u64;
        let ring_min = min_ring_size(self.outputs.len()).max(1);
        let mut seen = BTreeSet::new();
        for input in &tx.inputs {
            let n = input.ring.len();
            if n < ring_min || n > RING_SIZE {
                return Err(TxError::InvalidPrivate("tamanho do anel"));
            }
            if !input.ring.windows(2).all(|w| w[0] < w[1]) {
                return Err(TxError::InvalidPrivate("anel fora de ordem"));
            }
            if input.ring.last().is_some_and(|i| *i >= total) {
                return Err(TxError::InvalidPrivate("membro inexistente"));
            }
            if self.key_images.contains(&input.key_image) || !seen.insert(input.key_image) {
                return Err(TxError::KeyImageSpent);
            }
        }

        let pseudo: Vec<[u8; 32]> = tx.inputs.iter().map(|i| i.pseudo_out).collect();
        let commitments: Vec<[u8; 32]> = tx.outputs.iter().map(|o| o.data.commitment).collect();
        if !balance_holds(&pseudo, &commitments, public) {
            return Err(TxError::Balance);
        }

        let msg = tx.message(p.network_id);
        for (input, sig) in tx.inputs.iter().zip(&tx.signatures) {
            let ring: Vec<RingMember> = input
                .ring
                .iter()
                .map(|i| {
                    let o = &self.outputs[*i as usize];
                    RingMember {
                        one_time_key: o.one_time_key,
                        commitment: o.commitment,
                    }
                })
                .collect();
            if !clsag::verify(&ring, &input.pseudo_out, &input.key_image, &msg, sig) {
                return Err(TxError::RingSignature);
            }
        }
        verify_outputs(&tx.outputs)?;

        let shielded_supply = self
            .shielded_supply
            .checked_sub(public)
            .ok_or(TxError::ShieldedSupplyUnderflow)?;

        let mut accounts = Vec::new();
        if let Some(u) = &tx.unshield {
            let mut acc = self.account(&u.to);
            acc.balance = acc.balance.checked_add(u.amount).ok_or(TxError::Overflow)?;
            accounts.push((u.to, acc));
        }
        Ok(Effect {
            accounts,
            new_outputs: tx.outputs.iter().map(|o| o.data.clone()).collect(),
            spent: tx.inputs.iter().map(|i| i.key_image).collect(),
            shielded_supply,
            fee: tx.fee,
        })
    }
}

/// Pontos válidos e prova de faixa válida para cada saída.
fn verify_outputs(outputs: &[ShieldedOutput]) -> Result<(), TxError> {
    for o in outputs {
        if decode_point(&o.data.one_time_key).is_none() || decode_point(&o.data.tx_pub).is_none() {
            return Err(TxError::InvalidPrivate("ponto inválido"));
        }
        if !commitment::verify_range(&o.data.commitment, &o.range_proof) {
            return Err(TxError::RangeProof);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::tests::sample;
    use crate::tx::{TxBody, TX_VERSION};
    use rz_crypto::SecretKey;

    fn rich() -> SecretKey {
        SecretKey::from_seed([2; 32])
    }

    fn transfer(from: &SecretKey, nonce: u64, to: Address, amount: u64, fee: u64) -> Transaction {
        TxBody {
            version: TX_VERSION,
            sender: from.public_key(),
            nonce,
            fee,
            kind: TxKind::Transfer { to, amount },
        }
        .sign(from, "rede-zero-devnet-test")
        .unwrap()
    }

    fn bob() -> Address {
        SecretKey::from_seed([50; 32]).public_key().address()
    }

    // AT-TX-001 / AT-ZERO-001 / AT-STATE-001 — transição válida
    #[test]
    fn valid_transfer() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let before = s.account(&rich().public_key().address()).balance;
        let fee = s
            .apply_transaction(&transfer(&rich(), 0, bob(), 100, 5), &p)
            .unwrap();
        assert_eq!(fee, 5);
        let a = s.account(&rich().public_key().address());
        assert_eq!(a.balance, before - 105);
        assert_eq!(a.nonce, 1);
        assert_eq!(s.account(&bob()).balance, 100);
    }

    // AT-TX-006 / AT-MONEY-003 — saldo insuficiente não produz saldo negativo
    #[test]
    fn insufficient_balance() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let snapshot = s.clone();
        let err = s
            .apply_transaction(&transfer(&rich(), 0, bob(), 2_000_000, 1), &p)
            .unwrap_err();
        assert!(matches!(err, TxError::InsufficientBalance { .. }));
        assert_eq!(s, snapshot, "estado não pode mudar após erro");
    }

    // AT-TX-007 / AT-DS-001 — replay e double spend
    #[test]
    fn replay_and_double_spend() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let tx1 = transfer(&rich(), 0, bob(), 900_000, 1);
        let tx2 = transfer(
            &rich(),
            0,
            SecretKey::from_seed([51; 32]).public_key().address(),
            900_000,
            1,
        );
        s.apply_transaction(&tx1, &p).unwrap();
        assert_eq!(
            s.apply_transaction(&tx1, &p),
            Err(TxError::BadNonce {
                expected: 1,
                got: 0
            })
        );
        assert!(s.apply_transaction(&tx2, &p).is_err());
    }

    // AT-MONEY-002 / AT-ZERO-002 — soma monetária preservada
    #[test]
    fn supply_preserved() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let supply = s.total_supply();
        let fee = s
            .apply_transaction(&transfer(&rich(), 0, bob(), 10, 3), &p)
            .unwrap();
        // Antes do crédito da taxa, a soma é menor exatamente pela taxa.
        assert!(s.check_supply().is_err());
        s.credit_fees(bob(), fee).unwrap();
        s.check_supply().unwrap();
        assert_eq!(s.total_supply(), supply);
    }

    // AT-STATE-003 / AT-DET-001 — determinismo
    #[test]
    fn deterministic_root() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let run = || {
            let mut s = State::from_genesis(&g).unwrap();
            s.apply_transaction(&transfer(&rich(), 0, bob(), 10, 3), &p)
                .unwrap();
            s.root()
        };
        assert_eq!(run(), run());
        assert_ne!(run(), State::from_genesis(&g).unwrap().root());
    }

    #[test]
    fn self_transfer_only_costs_fee() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let mut s = State::from_genesis(&g).unwrap();
        let me = rich().public_key().address();
        let before = s.account(&me).balance;
        s.apply_transaction(&transfer(&rich(), 0, me, 10, 3), &p)
            .unwrap();
        assert_eq!(s.account(&me).balance, before - 3);
    }

    #[test]
    fn amount_plus_fee_overflow() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        assert_eq!(
            s.check_transaction(&transfer(&rich(), 0, bob(), u64::MAX, 1), &p),
            Err(TxError::Overflow)
        );
    }

    #[test]
    fn unknown_account_cannot_spend() {
        let g = sample();
        let p = ExecParams::from_genesis(&g);
        let s = State::from_genesis(&g).unwrap();
        let nobody = SecretKey::from_seed([77; 32]);
        assert!(matches!(
            s.check_transaction(&transfer(&nobody, 0, bob(), 1, 1), &p),
            Err(TxError::InsufficientBalance { .. })
        ));
    }
}
