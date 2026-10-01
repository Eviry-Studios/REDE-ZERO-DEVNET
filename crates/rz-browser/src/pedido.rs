//! Pedidos de assinatura (integração com a Wallet, `SPEC §58`).
//!
//! Uma publicação **nunca** recebe chaves. Ela pede uma operação; o pedido
//! aparece na interface do Navegador (outra origem, sem scripts), com o
//! conteúdo que será assinado, e só é executado se a pessoa aprovar ali
//! (THR-BRW-001).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rz_core::governance::Choice;
use rz_core::market::{AssetId, Side};
use rz_core::{format_zero, parse_zero, TxKind};
use rz_crypto::{Address, Hash32};
use rz_wallet::format_units;

use crate::html::json_str;
use crate::origin::Origin;

/// Pedidos pendentes por origem e no total.
pub const MAX_PER_ORIGIN: usize = 16;
pub const MAX_TOTAL: usize = 256;
/// Pedidos resolvidos ficam consultáveis por este tempo.
const KEEP_RESOLVED: Duration = Duration::from_secs(600);
pub const MAX_MESSAGE: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Transfer {
        to: Address,
        amount: u64,
    },
    Order {
        asset: AssetId,
        label: String,
        decimals: u8,
        side: Side,
        amount: u64,
        price: u64,
        blocks: u64,
    },
    Cancel {
        order: Hash32,
    },
    Vote {
        proposal: Hash32,
        choice: Choice,
    },
    /// Autenticação com a identidade própria deste site.
    Sign {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Pending,
    Approved {
        txid: Option<Hash32>,
        key: Option<String>,
        signature: Option<String>,
    },
    Refused,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Pedido {
    pub id: String,
    pub origin: Origin,
    pub op: Operation,
    pub state: State,
    pub created: Instant,
    pub resolved: Option<Instant>,
}

impl Operation {
    /// Linhas legíveis exibidas antes da aprovação.
    pub fn describe(&self) -> Vec<(String, String)> {
        let l = |a: &str, b: String| (a.to_string(), b);
        match self {
            Operation::Transfer { to, amount } => vec![
                l("Operação", "transferência transparente de ZERO".into()),
                l("Para", to.to_string()),
                l("Valor", format!("{} ZERO", format_zero(*amount))),
                l(
                    "Privacidade",
                    "remetente, destinatário e valor ficam públicos".into(),
                ),
            ],
            Operation::Order {
                asset,
                label,
                decimals,
                side,
                amount,
                price,
                blocks,
            } => {
                let per_unit = rz_wallet::zero_per_unit(*price, *decimals);
                vec![
                    l(
                        "Operação",
                        match side {
                            Side::Buy => "ordem de COMPRA no Grande Mercado".into(),
                            Side::Sell => "ordem de VENDA no Grande Mercado".into(),
                        },
                    ),
                    l("Ativo", format!("{label} ({asset})")),
                    l("Quantidade", format_units(*amount, *decimals)),
                    l(
                        "Preço limite",
                        format!(
                            "{} ZERO por unidade",
                            format_zero(u64::try_from(per_unit).unwrap_or(u64::MAX))
                        ),
                    ),
                    l("Validade", format!("{blocks} blocos")),
                    l("Privacidade", "ordens são públicas".into()),
                ]
            }
            Operation::Cancel { order } => vec![
                l("Operação", "cancelar ordem".into()),
                l("Ordem", order.to_string()),
            ],
            Operation::Vote { proposal, choice } => vec![
                l("Operação", "voto em proposta de governança".into()),
                l("Proposta", proposal.to_string()),
                l(
                    "Escolha",
                    match choice {
                        Choice::Yes => "sim",
                        Choice::No => "não",
                        Choice::Abstain => "abstenção",
                    }
                    .into(),
                ),
                l("Privacidade", "votos são públicos nesta versão".into()),
            ],
            Operation::Sign { message } => vec![
                l(
                    "Operação",
                    "autenticar com a identidade deste site (não revela sua Wallet)".into(),
                ),
                l("Mensagem", message.clone()),
            ],
        }
    }

    /// Transação a assinar (exceto autenticações). `height` é a altura atual.
    pub fn tx_kind(&self, height: u64) -> Option<TxKind> {
        Some(match self {
            Operation::Transfer { to, amount } => TxKind::Transfer {
                to: *to,
                amount: *amount,
            },
            Operation::Order {
                asset,
                side,
                amount,
                price,
                blocks,
                ..
            } => TxKind::PlaceOrder {
                asset: *asset,
                side: *side,
                amount: *amount,
                price: *price,
                expires_at: height.saturating_add(1).saturating_add(*blocks),
            },
            Operation::Cancel { order } => TxKind::CancelOrder { order: *order },
            Operation::Vote { proposal, choice } => TxKind::Vote {
                proposal: *proposal,
                choice: *choice,
            },
            Operation::Sign { .. } => return None,
        })
    }
}

/// Campos de formulário → operação. Ordens precisam dos ativos registrados
/// (casas decimais); `assets` os fornece sob demanda.
pub fn parse(
    f: &BTreeMap<String, String>,
    assets: impl FnOnce() -> Result<Vec<rz_core::market::AssetView>, String>,
) -> Result<Operation, String> {
    let get = |k: &str| {
        f.get(k)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or(format!("campo '{k}' ausente"))
    };
    let hash = |k: &str| -> Result<Hash32, String> {
        Hash32::from_hex(get(k)?).ok_or(format!("campo '{k}': identificador inválido"))
    };
    match get("tipo")? {
        "transferencia" => Ok(Operation::Transfer {
            to: Address::from_hex(get("para")?).ok_or("campo 'para': endereço inválido")?,
            amount: parse_zero(get("valor")?)
                .filter(|v| *v > 0)
                .ok_or("campo 'valor' inválido")?,
        }),
        "ordem" => {
            let side = match get("lado")? {
                "compra" => Side::Buy,
                "venda" => Side::Sell,
                _ => return Err("campo 'lado': use compra ou venda".into()),
            };
            let spec = get("ativo")?;
            let id = match spec.split_once(':') {
                Some((net, r)) => AssetId::external(net, r),
                None => AssetId(Hash32::from_hex(spec).ok_or("campo 'ativo' inválido")?),
            };
            let a = assets()?
                .into_iter()
                .find(|a| a.id == id)
                .ok_or("ativo não registrado")?;
            let d = a.info.decimals;
            let amount = rz_wallet::parse_units(get("quantidade")?, d)
                .filter(|v| *v > 0)
                .ok_or("campo 'quantidade' inválido")?;
            let price = rz_wallet::price_from_zero_per_unit(
                parse_zero(get("preco")?).ok_or("campo 'preco' inválido")?,
                d,
            )
            .ok_or("campo 'preco' inválido ou pequeno demais")?;
            let blocks = match f.get("blocos").map(|s| s.trim()).filter(|s| !s.is_empty()) {
                Some(b) => b.parse().map_err(|_| "campo 'blocos' inválido")?,
                None => 1_000,
            };
            Ok(Operation::Order {
                asset: id,
                label: format!("{}:{}", a.info.network, a.info.asset_ref),
                decimals: d,
                side,
                amount,
                price,
                blocks,
            })
        }
        "cancelar" => Ok(Operation::Cancel {
            order: hash("ordem")?,
        }),
        "voto" => Ok(Operation::Vote {
            proposal: hash("proposta")?,
            choice: Choice::parse(get("escolha")?)
                .ok_or("campo 'escolha': sim, nao ou abstencao")?,
        }),
        "assinatura" => {
            let message = get("mensagem")?.to_string();
            if message.len() > MAX_MESSAGE {
                return Err("mensagem longa demais".into());
            }
            Ok(Operation::Sign { message })
        }
        other => Err(format!("tipo de pedido desconhecido: {other}")),
    }
}

impl Pedido {
    pub fn json(&self) -> String {
        let mut fields = vec![format!("\"pedido\":{}", json_str(&self.id))];
        let (estado, extra): (&str, Vec<String>) = match &self.state {
            State::Pending => ("pendente", vec![]),
            State::Refused => ("recusado", vec![]),
            State::Failed(e) => ("erro", vec![format!("\"erro\":{}", json_str(e))]),
            State::Approved {
                txid,
                key,
                signature,
            } => (
                "aprovado",
                [
                    txid.map(|t| format!("\"txid\":{}", json_str(&t.to_hex()))),
                    key.as_ref().map(|k| format!("\"chave\":{}", json_str(k))),
                    signature
                        .as_ref()
                        .map(|s| format!("\"assinatura\":{}", json_str(s))),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
        };
        fields.push(format!("\"estado\":{}", json_str(estado)));
        fields.extend(extra);
        format!("{{{}}}", fields.join(","))
    }
}

#[derive(Default)]
pub struct Pedidos {
    pub items: BTreeMap<String, Pedido>,
}

impl Pedidos {
    fn expire(&mut self) {
        let now = Instant::now();
        self.items.retain(|_, p| match p.resolved {
            Some(t) => now.duration_since(t) < KEEP_RESOLVED,
            None => true,
        });
    }

    pub fn pending(&self) -> usize {
        self.items
            .values()
            .filter(|p| p.state == State::Pending)
            .count()
    }

    pub fn add(&mut self, id: String, origin: Origin, op: Operation) -> Result<(), String> {
        self.expire();
        let from_origin = self
            .items
            .values()
            .filter(|p| p.origin == origin && p.state == State::Pending)
            .count();
        if from_origin >= MAX_PER_ORIGIN || self.items.len() >= MAX_TOTAL {
            return Err("pedidos pendentes demais".into());
        }
        self.items.insert(
            id.clone(),
            Pedido {
                id,
                origin,
                op,
                state: State::Pending,
                created: Instant::now(),
                resolved: None,
            },
        );
        Ok(())
    }

    pub fn resolve(&mut self, id: &str, state: State) {
        if let Some(p) = self.items.get_mut(id) {
            p.state = state;
            p.resolved = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn parse_operations() {
        let to = Address(Hash32([1; 32]));
        let op = parse(
            &form(&[
                ("tipo", "transferencia"),
                ("para", &to.to_hex()),
                ("valor", "1.5"),
            ]),
            || unreachable!(),
        )
        .unwrap();
        assert_eq!(
            op,
            Operation::Transfer {
                to,
                amount: rz_core::UNITS_PER_ZERO * 3 / 2
            }
        );
        assert!(parse(&form(&[("tipo", "transferencia")]), || unreachable!()).is_err());
        assert!(parse(&form(&[("tipo", "apagar-tudo")]), || unreachable!()).is_err());
        let long = "x".repeat(MAX_MESSAGE + 1);
        assert!(parse(
            &form(&[("tipo", "assinatura"), ("mensagem", &long)]),
            || unreachable!()
        )
        .is_err());
    }

    #[test]
    fn limits_per_origin() {
        let mut p = Pedidos::default();
        let o = Origin::Id(Hash32([2; 32]));
        for i in 0..MAX_PER_ORIGIN {
            p.add(
                format!("{i}"),
                o.clone(),
                Operation::Sign {
                    message: "m".into(),
                },
            )
            .unwrap();
        }
        assert!(p
            .add(
                "x".into(),
                o,
                Operation::Sign {
                    message: "m".into()
                }
            )
            .is_err());
        assert!(p.items["0"].json().contains("\"estado\":\"pendente\""));
    }
}
