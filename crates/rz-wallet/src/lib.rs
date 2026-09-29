//! Biblioteca da Wallet da DEVNET.
//!
//! A Wallet não depende do Node (`SPEC-WAL-001`): fala apenas o protocolo.
//! Chaves nunca saem do processo local (`SPEC-WAL-002`, `SPEC-WAL-003`).
//!
//! Privacidade por padrão (REQ-023): a Wallet baixa **todas** as saídas
//! privadas e **todas** as imagens de chave gastas e faz a varredura
//! localmente. O Node não aprende quais notas pertencem a quem.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::time::Duration;

use rz_core::private::{build_private_tx, build_shield, SpendableNote, Unshield};
use rz_core::{Genesis, Transaction, TxBody, TxId, TxKind};
use rz_crypto::{Address, SecretKey};
use rz_p2p::{Client, Message, MAX_KEY_IMAGES_PER_MSG, MAX_OUTPUTS_PER_MSG};
use rz_privacy::keys::{ShieldedAddress, ShieldedSecret};
use rz_privacy::note::{scan_output, OutputData};

/// Chaves da Wallet: uma única semente gera a chave transparente (Ed25519)
/// e as chaves privadas (visualização e gasto), em domínios separados.
pub struct Keys {
    pub transparent: SecretKey,
    pub shielded: ShieldedSecret,
}

impl Keys {
    pub fn from_secret(sk: SecretKey) -> Self {
        let shielded = ShieldedSecret::from_seed(&sk.seed());
        Self {
            transparent: sk,
            shielded,
        }
    }

    pub fn address(&self) -> Address {
        self.transparent.public_key().address()
    }

    pub fn shielded_address(&self) -> ShieldedAddress {
        self.shielded.address()
    }
}

/// Destino de um envio: privado (`zs…`) ou transparente (hex de 64 dígitos).
#[allow(clippy::large_enum_variant)] // valor de curta duração, criado uma vez por comando
pub enum Destination {
    Shielded(ShieldedAddress),
    Transparent(Address),
}

impl Destination {
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(a) = ShieldedAddress::decode(s) {
            return Some(Self::Shielded(a));
        }
        Address::from_hex(s).map(Self::Transparent)
    }
}

pub fn connect(node: SocketAddr, genesis: &Genesis) -> Result<Client, String> {
    Client::connect(
        node,
        &genesis.network_id,
        genesis.hash(),
        Duration::from_secs(20),
    )
    .map_err(|e| e.to_string())
}

/// `(saldo, nonce, altura)` de uma conta transparente.
pub fn account(client: &mut Client, address: Address) -> Result<(u64, u64, u64), String> {
    client
        .request(&Message::GetAccount(address), |m| match m {
            Message::Account {
                address: a,
                balance,
                nonce,
                height,
            } if a == address => Some((balance, nonce, height)),
            _ => None,
        })
        .map_err(|e| e.to_string())
}

pub fn fetch_outputs(client: &mut Client) -> Result<Vec<OutputData>, String> {
    let mut all = Vec::new();
    loop {
        let from = all.len() as u64;
        let batch = client
            .request(
                &Message::GetOutputs {
                    from,
                    max: MAX_OUTPUTS_PER_MSG as u32,
                },
                |m| match m {
                    Message::Outputs { start, outputs } if start == from => Some(outputs),
                    _ => None,
                },
            )
            .map_err(|e| e.to_string())?;
        let done = batch.len() < MAX_OUTPUTS_PER_MSG;
        all.extend(batch);
        if done {
            return Ok(all);
        }
    }
}

pub fn fetch_key_images(client: &mut Client) -> Result<HashSet<[u8; 32]>, String> {
    let mut all = HashSet::new();
    let mut from = 0u64;
    loop {
        let batch = client
            .request(
                &Message::GetKeyImages {
                    from,
                    max: MAX_KEY_IMAGES_PER_MSG as u32,
                },
                |m| match m {
                    Message::KeyImages { start, images } if start == from => Some(images),
                    _ => None,
                },
            )
            .map_err(|e| e.to_string())?;
        from += batch.len() as u64;
        let done = batch.len() < MAX_KEY_IMAGES_PER_MSG;
        all.extend(batch);
        if done {
            return Ok(all);
        }
    }
}

/// Notas não gastas pertencentes a `keys`.
pub fn scan(keys: &Keys, outputs: &[OutputData], spent: &HashSet<[u8; 32]>) -> Vec<SpendableNote> {
    outputs
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            scan_output(&keys.shielded, o).map(|note| SpendableNote {
                global_index: i as u64,
                data: o.clone(),
                note,
            })
        })
        .filter(|n| !spent.contains(&n.key_image()))
        .collect()
}

/// Estado privado atual da Wallet: todas as saídas e as notas próprias.
pub fn private_view(
    client: &mut Client,
    keys: &Keys,
) -> Result<(Vec<OutputData>, Vec<SpendableNote>), String> {
    let outputs = fetch_outputs(client)?;
    let spent = fetch_key_images(client)?;
    let notes = scan(keys, &outputs, &spent);
    Ok((outputs, notes))
}

/// Envia a transação e aguarda a confirmação de aceitação pelo Node.
pub fn submit(client: &mut Client, tx: Transaction) -> Result<TxId, String> {
    let id = tx.id();
    let (accepted, reason) = client
        .request(&Message::Transaction(tx), |m| match m {
            Message::TxResult {
                id: rid,
                accepted,
                reason,
            } if rid == id => Some((accepted, reason)),
            _ => None,
        })
        .map_err(|e| e.to_string())?;
    if accepted {
        Ok(id)
    } else {
        Err(format!("rejeitada: {reason}"))
    }
}

/// Transferência transparente.
pub fn send_transparent(
    client: &mut Client,
    genesis: &Genesis,
    keys: &Keys,
    to: Address,
    amount: u64,
    fee: u64,
) -> Result<TxId, String> {
    let (_, nonce, _) = account(client, keys.address())?;
    let tx = TxBody {
        version: rz_core::tx::TX_VERSION,
        sender: keys.transparent.public_key(),
        nonce,
        fee,
        kind: TxKind::Transfer { to, amount },
    }
    .sign(&keys.transparent, &genesis.network_id)
    .map_err(|e| e.to_string())?;
    submit(client, tx)
}

/// Blindagem: saldo transparente → nota privada.
pub fn shield(
    client: &mut Client,
    genesis: &Genesis,
    keys: &Keys,
    to: &ShieldedAddress,
    amount: u64,
    fee: u64,
) -> Result<TxId, String> {
    let (_, nonce, _) = account(client, keys.address())?;
    let tx = build_shield(
        &keys.transparent,
        &genesis.network_id,
        nonce,
        fee,
        to,
        amount,
    )
    .map_err(|e| e.to_string())?;
    submit(client, tx)
}

/// Seleciona notas suficientes para `required` (maiores primeiro).
fn select_notes(
    mut notes: Vec<SpendableNote>,
    required: u64,
) -> Result<Vec<SpendableNote>, String> {
    notes.sort_by(|a, b| b.note.amount.cmp(&a.note.amount));
    let mut chosen = Vec::new();
    let mut total = 0u64;
    for n in notes {
        if total >= required {
            break;
        }
        total = total.saturating_add(n.note.amount);
        chosen.push(n);
    }
    if total < required {
        return Err(format!("saldo privado insuficiente: {total} < {required}"));
    }
    Ok(chosen)
}

/// Envio privado (para `zs…`) ou retirada (`unshield`) para conta transparente.
pub fn send_private(
    client: &mut Client,
    genesis: &Genesis,
    keys: &Keys,
    to: Destination,
    amount: u64,
    fee: u64,
) -> Result<TxId, String> {
    let (outputs, notes) = private_view(client, keys)?;
    let required = amount.checked_add(fee).ok_or("overflow")?;
    let chosen = select_notes(notes, required)?;
    let change = keys.shielded_address();
    let (recipients, unshield) = match to {
        Destination::Shielded(a) => (vec![(a, amount)], None),
        Destination::Transparent(a) => (vec![], Some(Unshield { to: a, amount })),
    };
    let tx = build_private_tx(
        &genesis.network_id,
        &chosen,
        &outputs,
        &recipients,
        &change,
        unshield,
        fee,
    )
    .map_err(|e| e.to_string())?;
    submit(client, Transaction::Private(tx))
}

// ------------------------------------------------------------- governança

/// Visão de governança obtida de um Node.
pub struct GovernanceView {
    pub height: u64,
    pub params: rz_core::ProtocolParams,
    pub proposals: Vec<rz_core::governance::Proposal>,
    pub locks: Vec<rz_core::governance::LockEntry>,
}

/// Contexto do hash do texto de uma proposta.
pub const PROPOSAL_CONTENT: &str = "rede-zero/proposal-content/v1";

pub fn governance(client: &mut Client, address: Option<Address>) -> Result<GovernanceView, String> {
    client
        .request(&Message::GetGovernance { address }, |m| match m {
            Message::Governance {
                height,
                params,
                proposals,
                locks,
            } => Some(GovernanceView {
                height,
                params,
                proposals: proposals.into_iter().map(|p| p.0).collect(),
                locks,
            }),
            _ => None,
        })
        .map_err(|e| e.to_string())
}

/// Monta, assina localmente e envia uma transação de conta.
pub fn send_account(
    client: &mut Client,
    genesis: &Genesis,
    keys: &Keys,
    kind: TxKind,
    fee: u64,
) -> Result<TxId, String> {
    let (_, nonce, _) = account(client, keys.address())?;
    let tx = TxBody {
        version: rz_core::tx::TX_VERSION,
        sender: keys.transparent.public_key(),
        nonce,
        fee,
        kind,
    }
    .sign(&keys.transparent, &genesis.network_id)
    .map_err(|e| e.to_string())?;
    submit(client, tx)
}
