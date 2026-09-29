//! Estado da rede e função de transição (`SPEC §12–§14`, `§33–§36`).
//!
//! `S(n+1) = F(S(n), B(n+1))`. A única forma de alterar saldos é aplicar
//! transações válidas ou creditar taxas segundo a regra do bloco. Não existe
//! operação pública que permita `saldo += X` arbitrário (INV-003).

use std::collections::BTreeMap;
use std::fmt;

use rz_codec::Encoder;
use rz_crypto::{context, hash, Address, Hash32};

use crate::genesis::{Genesis, GenesisError};
use crate::tx::{Transaction, TxError, TxKind};

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

/// Estado completo. `BTreeMap` garante ordem de iteração determinística.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    accounts: BTreeMap<Address, Account>,
    total_supply: u64,
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
        })
    }

    pub fn account(&self, address: &Address) -> Account {
        self.accounts.get(address).copied().unwrap_or_default()
    }

    pub fn total_supply(&self) -> u64 {
        self.total_supply
    }

    pub fn accounts(&self) -> impl Iterator<Item = (&Address, &Account)> {
        self.accounts.iter()
    }

    /// Compromisso criptográfico do estado:
    /// `H(STATE_ROOT, u64(total_supply) ‖ list<(address, balance, nonce)>)`.
    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.total_supply);
        e.u32(u32::try_from(self.accounts.len()).unwrap_or(u32::MAX));
        for (addr, acc) in &self.accounts {
            e.put(addr).u64(acc.balance).u64(acc.nonce);
        }
        hash(context::STATE_ROOT, &e.into_bytes())
    }

    /// Verifica a invariante monetária: soma dos saldos = oferta total.
    pub fn check_supply(&self) -> Result<(), StateError> {
        let actual: u128 = self.accounts.values().map(|a| a.balance as u128).sum();
        if actual != self.total_supply as u128 {
            return Err(StateError::SupplyMismatch {
                expected: self.total_supply,
                actual,
            });
        }
        Ok(())
    }

    /// Valida uma transação contra este estado **sem alterá-lo**.
    ///
    /// Ordem (`SPEC §10`): formato/taxa (barato) → assinatura → estado.
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
        let plan = self.plan(tx, p)?;
        self.accounts.insert(plan.sender, plan.sender_after);
        if let Some((to, acc)) = plan.recipient_after {
            self.accounts.insert(to, acc);
        }
        Ok(tx.body.fee)
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

    fn plan(&self, tx: &Transaction, p: &ExecParams<'_>) -> Result<Plan, TxError> {
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

        match &tx.body.kind {
            TxKind::Transfer { to, amount } => {
                let required = amount.checked_add(tx.body.fee).ok_or(TxError::Overflow)?;
                if sender_acc.balance < required {
                    return Err(TxError::InsufficientBalance {
                        balance: sender_acc.balance,
                        required,
                    });
                }
                sender_acc.balance -= required;
                sender_acc.nonce = sender_acc.nonce.checked_add(1).ok_or(TxError::Overflow)?;

                if *to == sender {
                    sender_acc.balance = sender_acc
                        .balance
                        .checked_add(*amount)
                        .ok_or(TxError::Overflow)?;
                    return Ok(Plan {
                        sender,
                        sender_after: sender_acc,
                        recipient_after: None,
                    });
                }
                let mut to_acc = self.account(to);
                to_acc.balance = to_acc
                    .balance
                    .checked_add(*amount)
                    .ok_or(TxError::Overflow)?;
                Ok(Plan {
                    sender,
                    sender_after: sender_acc,
                    recipient_after: Some((*to, to_acc)),
                })
            }
        }
    }
}

struct Plan {
    sender: Address,
    sender_after: Account,
    recipient_after: Option<(Address, Account)>,
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
