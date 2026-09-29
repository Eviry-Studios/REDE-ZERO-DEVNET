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
use rz_crypto::{Address, Hash32, SecretKey};
use rz_p2p::{
    Client, ConnectOptions, Message, PeerAddr, MAX_KEY_IMAGES_PER_MSG, MAX_OUTPUTS_PER_MSG,
};
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

/// Como a Wallet alcança o Node (ADR-0011).
#[derive(Clone, Debug, Default)]
pub struct NetOptions {
    /// Proxy SOCKS5 (Tor/I2P). Com proxy, o Node vê o endereço de saída do
    /// proxy, não o IP do usuário.
    pub proxy: Option<SocketAddr>,
    /// Identidade esperada do Node (impede interceptação ativa).
    pub node_id: Option<rz_crypto::NodeId>,
    /// O usuário aceita expor seu IP a um Node remoto sem proxy.
    pub allow_direct: bool,
}

/// Mensagem exibida quando a política de privacidade bloqueia a conexão.
pub const DIRECT_REFUSED: &str = "conexão direta a um node remoto exporia seu endereço IP ao \
operador do node. Use um node local (127.0.0.1), conecte via Tor com --proxy 127.0.0.1:9050, \
ou aceite o risco explicitamente com --direct";

/// Conecta respeitando a política de privacidade por padrão (REQ-023):
/// sem proxy, só Nodes locais são aceitos, salvo consentimento explícito.
pub fn connect_to(node: &PeerAddr, genesis: &Genesis, net: &NetOptions) -> Result<Client, String> {
    if net.proxy.is_none() && !node.is_loopback() && !net.allow_direct {
        return Err(DIRECT_REFUSED.into());
    }
    let mut o = ConnectOptions::new(Duration::from_secs(30));
    o.proxy = net.proxy;
    o.expected_node = net.node_id;
    Client::connect_opts(node, &genesis.network_id, genesis.hash(), &o).map_err(|e| e.to_string())
}

/// Conexão direta a um Node local (atalho para testes e uso local).
pub fn connect(node: SocketAddr, genesis: &Genesis) -> Result<Client, String> {
    connect_to(&PeerAddr::Ip(node), genesis, &NetOptions::default())
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
    notes.sort_by_key(|n| std::cmp::Reverse(n.note.amount));
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

// ------------------------------------------------------ Grande Mercado e Pool

/// Ativos externos, Pool e (opcionalmente) saldos de uma conta.
pub struct AssetsView {
    pub height: u64,
    pub pool_zero: u64,
    pub assets: Vec<rz_core::market::AssetView>,
}

pub fn assets(client: &mut Client, address: Option<Address>) -> Result<AssetsView, String> {
    client
        .request(&Message::GetAssets { address }, |m| match m {
            Message::Assets {
                height,
                pool_zero,
                assets,
            } => Some(AssetsView {
                height,
                pool_zero,
                assets,
            }),
            _ => None,
        })
        .map_err(|e| e.to_string())
}

/// Livro agregado de um ativo e as ordens de `owner`.
pub struct MarketView {
    pub height: u64,
    pub last_price: Option<u64>,
    pub bids: Vec<rz_core::market::BookLevel>,
    pub asks: Vec<rz_core::market::BookLevel>,
    pub own: Vec<rz_core::market::Order>,
}

pub fn market(
    client: &mut Client,
    asset: rz_core::market::AssetId,
    owner: Option<Address>,
) -> Result<MarketView, String> {
    client
        .request(&Message::GetMarket { asset, owner }, |m| match m {
            Message::Market {
                height,
                asset: a,
                last_price,
                bids,
                asks,
                own,
            } if a == asset => Some(MarketView {
                height,
                last_price,
                bids,
                asks,
                own,
            }),
            _ => None,
        })
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------ Comunidades e nomes

/// Comunidade pelo nome (reconhecida ou pendente).
pub fn community(
    client: &mut Client,
    name: &str,
) -> Result<(u64, Option<rz_core::community::Community>), String> {
    client
        .request(&Message::GetCommunity { name: name.into() }, |m| match m {
            Message::Community { height, community } => Some((height, community.map(|c| *c))),
            _ => None,
        })
        .map_err(|e| e.to_string())
}

/// Resolve `zero://nome.tipo`: `(alvo, dono)`.
pub fn resolve(
    client: &mut Client,
    name: &str,
    kind: rz_core::community::NameKind,
) -> Result<(Option<Hash32>, Option<Address>), String> {
    client
        .request(
            &Message::Resolve {
                name: name.into(),
                kind,
            },
            |m| match m {
                Message::Resolved {
                    name: n,
                    kind: k,
                    target,
                    owner,
                    ..
                } if n == name && k == kind => Some((target, owner)),
                _ => None,
            },
        )
        .map_err(|e| e.to_string())
}

/// Interpreta uma quantidade decimal de um ativo com `decimals` casas.
pub fn parse_units(s: &str, decimals: u8) -> Option<u64> {
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    if int.is_empty() && frac.is_empty() || frac.len() > decimals as usize {
        return None;
    }
    if !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let scale = 10u128.checked_pow(decimals as u32)?;
    let int: u128 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let frac_scaled: u128 = if frac.is_empty() {
        0
    } else {
        frac.parse::<u128>().ok()? * 10u128.pow((decimals as usize - frac.len()) as u32)
    };
    u64::try_from(int.checked_mul(scale)?.checked_add(frac_scaled)?).ok()
}

/// Formata uma quantidade de um ativo com `decimals` casas.
pub fn format_units(v: u64, decimals: u8) -> String {
    if decimals == 0 {
        return v.to_string();
    }
    let scale = 10u128.pow(decimals as u32);
    let v = v as u128;
    format!(
        "{}.{:0width$}",
        v / scale,
        v % scale,
        width = decimals as usize
    )
}

/// Converte "ZERO por unidade inteira do ativo" (unidades mínimas de ZERO)
/// para o preço do protocolo (`market::PRICE_SCALE`).
pub fn price_from_zero_per_unit(zero_units: u64, decimals: u8) -> Option<u64> {
    let scale = 10u128.checked_pow(decimals as u32)?;
    let raw = zero_units as u128 * rz_core::market::PRICE_SCALE / scale;
    u64::try_from(raw).ok().filter(|p| *p > 0)
}

/// Preço do protocolo em ZERO (unidades mínimas) por unidade inteira.
pub fn zero_per_unit(price: u64, decimals: u8) -> u128 {
    price as u128 * 10u128.pow(decimals as u32) / rz_core::market::PRICE_SCALE
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

#[cfg(test)]
mod market_units_tests {
    use super::*;

    #[test]
    fn units_roundtrip() {
        assert_eq!(parse_units("1.5", 8), Some(150_000_000));
        assert_eq!(parse_units("0.00000001", 8), Some(1));
        assert_eq!(parse_units("1.000000001", 8), None);
        assert_eq!(parse_units("abc", 8), None);
        assert_eq!(parse_units("7", 0), Some(7));
        assert_eq!(format_units(150_000_000, 8), "1.50000000");
        assert_eq!(format_units(7, 0), "7");
    }

    #[test]
    fn prices() {
        // 2 ZERO por unidade inteira de um ativo com 8 casas.
        let p = price_from_zero_per_unit(2 * rz_core::UNITS_PER_ZERO, 8).unwrap();
        assert_eq!(zero_per_unit(p, 8), 2 * rz_core::UNITS_PER_ZERO as u128);
        // 1 unidade inteira (10^8) custa 2 ZERO.
        assert_eq!(
            rz_core::market::quote_floor(100_000_000, p),
            Some(2 * rz_core::UNITS_PER_ZERO)
        );
        assert_eq!(price_from_zero_per_unit(0, 8), None);
    }
}
