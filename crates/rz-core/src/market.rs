//! Grande Mercado e Pool permanente (`SPEC §38–§42`, ADR-0014,
//! `spec/MARKET.md`).
//!
//! * **Ativos:** ZERO e representações de ativos externos, com identificador
//!   derivado da origem (`network ‖ asset_ref`), impossível de confundir com
//!   ZERO (`SPEC §41`, AT-POOL-004).
//! * **Grande Mercado:** livro de ordens com **leilão de preço uniforme por
//!   bloco**. Todas as ordens que cruzam num bloco executam ao mesmo preço, de
//!   modo que reordenar transações dentro do bloco não dá vantagem
//!   (THR-MKT-001). Todo par é cotado em ZERO.
//! * **Pool permanente:** reserva no estado da rede. Depósitos só entram:
//!   não existe operação que retire ativos do Pool (`SPEC §39–§40`,
//!   AT-POOL-002).
//!
//! Este módulo contém os tipos, a codificação e a matemática do leilão, sem
//! efeitos colaterais. A aplicação ao estado fica em `state.rs`.

use std::collections::{BTreeMap, BTreeSet};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Address, Hash32};

/// Escala de preço: `preço = unidades mínimas de ZERO por unidade mínima do
/// ativo × 10^8`. O valor em ZERO de `q` unidades do ativo a preço `p` é
/// `⌊q · p / PRICE_SCALE⌋`.
pub const PRICE_SCALE: u128 = 100_000_000;

/// Contexto do identificador de ativo externo.
pub const ASSET_ID: &str = "rede-zero/asset/v1";
/// Contexto da raiz da parte de mercado e Pool do estado.
pub const MARKET_ROOT: &str = "rede-zero/market-root/v1";

/// Limites de decodificação.
pub const MAX_ASSETS: usize = 64;
pub const MAX_ASSET_NETWORK_LEN: usize = 32;
pub const MAX_ASSET_REF_LEN: usize = 64;

/// Identificador de ativo. ZERO é o identificador nulo; ativos externos
/// usam `H(ASSET_ID, network ‖ asset_ref)`, que não pode ser nulo na
/// prática (pré-imagem de BLAKE3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(pub Hash32);

impl AssetId {
    pub const ZERO: AssetId = AssetId(Hash32::ZERO);

    pub fn external(network: &str, asset_ref: &str) -> AssetId {
        let mut e = Encoder::new();
        e.str(network).str(asset_ref);
        AssetId(hash(ASSET_ID, &e.into_bytes()))
    }

    pub fn is_zero(&self) -> bool {
        *self == AssetId::ZERO
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_zero() {
            write!(f, "ZERO")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl Encode for AssetId {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.0);
    }
}

impl Decode for AssetId {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(AssetId(d.get()?))
    }
}

/// Como a existência do ativo na rede de origem é comprovada (`SPEC §42`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    /// Ativo de teste declarado no Genesis de uma DEVNET. **Não** representa
    /// nada na rede de origem e é proibido fora da DEVNET. Pontes
    /// verificáveis (provas da rede de origem, sem custodiante único) estão
    /// **A DEFINIR** (ADR-0014).
    DevnetGenesis = 0,
    /// Representação de um ativo destruído na origem, comprovado por cliente
    /// leve e prova de Merkle (ADR-0019). Não há custodiante nem resgate.
    LightClientBurn = 1,
}

impl Encode for Verification {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for Verification {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(Verification::DevnetGenesis),
            1 => Ok(Verification::LightClientBurn),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Ativo externo registrado (`SPEC §41`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetInfo {
    /// Rede de origem (ex.: `bitcoin`).
    pub network: String,
    /// Identificador do ativo na origem (ex.: `BTC`, endereço de contrato).
    pub asset_ref: String,
    pub decimals: u8,
    pub verification: Verification,
    /// Quantidade representada na Rede Zero.
    pub supply: u64,
}

impl AssetInfo {
    pub fn id(&self) -> AssetId {
        AssetId::external(&self.network, &self.asset_ref)
    }

    /// Regras de nome: impossível registrar ZERO ou a própria Rede Zero como
    /// origem de um ativo externo (AT-POOL-004).
    pub fn check_names(network: &str, asset_ref: &str) -> Result<(), &'static str> {
        let valid = |s: &str, max: usize| {
            !s.is_empty()
                && s.len() <= max
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
        };
        if !valid(network, MAX_ASSET_NETWORK_LEN) || !valid(asset_ref, MAX_ASSET_REF_LEN) {
            return Err("nome de ativo inválido");
        }
        if network.to_ascii_lowercase().starts_with("rede-zero") {
            return Err("a Rede Zero não é origem de ativo externo");
        }
        if asset_ref.eq_ignore_ascii_case("zero") {
            return Err("ativo externo não pode se chamar ZERO");
        }
        Ok(())
    }
}

impl Encode for AssetInfo {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.network)
            .str(&self.asset_ref)
            .u8(self.decimals)
            .put(&self.verification)
            .u64(self.supply);
    }
}

impl Decode for AssetInfo {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            network: d.str(MAX_ASSET_NETWORK_LEN)?,
            asset_ref: d.str(MAX_ASSET_REF_LEN)?,
            decimals: d.u8()?,
            verification: d.get()?,
            supply: d.u64()?,
        })
    }
}

/// Lado da ordem, sempre em relação ao ativo-base (cotado em ZERO).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    /// Compra o ativo pagando ZERO.
    Buy = 0,
    /// Vende o ativo recebendo ZERO.
    Sell = 1,
}

impl Encode for Side {
    fn encode(&self, e: &mut Encoder) {
        e.u8(*self as u8);
    }
}

impl Decode for Side {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        match d.u8()? {
            0 => Ok(Side::Buy),
            1 => Ok(Side::Sell),
            t => Err(DecodeError::InvalidTag(t)),
        }
    }
}

/// Ordem aberta no livro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    /// Identificador: o `TxId` da transação que a criou.
    pub id: Hash32,
    pub owner: Address,
    pub asset: AssetId,
    pub side: Side,
    /// Preço-limite (ver [`PRICE_SCALE`]).
    pub price: u64,
    /// Quantidade restante do ativo-base.
    pub remaining: u64,
    /// Valor reservado: ZERO numa compra, ativo-base numa venda.
    pub escrow: u64,
    pub placed_at: u64,
    pub expires_at: u64,
}

impl Encode for Order {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id)
            .put(&self.owner)
            .put(&self.asset)
            .put(&self.side)
            .u64(self.price)
            .u64(self.remaining)
            .u64(self.escrow)
            .u64(self.placed_at)
            .u64(self.expires_at);
    }
}

impl Decode for Order {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: d.get()?,
            owner: d.get()?,
            asset: d.get()?,
            side: d.get()?,
            price: d.u64()?,
            remaining: d.u64()?,
            escrow: d.u64()?,
            placed_at: d.u64()?,
            expires_at: d.u64()?,
        })
    }
}

/// Ordem cujo restante vale menos de 1 unidade mínima de ZERO ao próprio
/// limite: nunca executaria e sai do livro com reembolso.
pub fn is_dust(o: &Order) -> bool {
    quote_floor(o.remaining, o.price).is_none_or(|v| v == 0)
}

/// Valor em ZERO de `qty` unidades a preço `price`, arredondado para baixo.
pub fn quote_floor(qty: u64, price: u64) -> Option<u64> {
    u64::try_from(qty as u128 * price as u128 / PRICE_SCALE).ok()
}

/// Valor em ZERO arredondado para cima (reserva de uma compra).
pub fn quote_ceil(qty: u64, price: u64) -> Option<u64> {
    let v = qty as u128 * price as u128;
    u64::try_from(v.div_ceil(PRICE_SCALE)).ok()
}

/// Parâmetros do mercado (no estado, alteráveis por governança).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketParams {
    /// Tarifa do mercado sobre o ZERO recebido pelo vendedor, em pontos-base,
    /// destinada ao Pool permanente. Padrão 0.
    pub fee_bps: u32,
    /// Validade máxima de uma ordem, em blocos.
    pub order_lifetime_blocks: u64,
    /// Máximo de ordens abertas no livro inteiro.
    pub max_open_orders: u32,
    /// Máximo de ordens abertas por conta.
    pub max_orders_per_account: u32,
}

/// Limite da tarifa: 10%.
pub const MAX_FEE_BPS: u32 = 1_000;

impl Default for MarketParams {
    fn default() -> Self {
        Self {
            fee_bps: 0,
            // ~30 dias com blocos de 2 s.
            order_lifetime_blocks: 1_296_000,
            max_open_orders: 100_000,
            max_orders_per_account: 256,
        }
    }
}

impl MarketParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.fee_bps > MAX_FEE_BPS {
            return Err("tarifa do mercado acima de 10%");
        }
        if self.order_lifetime_blocks == 0 || self.max_open_orders == 0 {
            return Err("parâmetros do mercado devem ser positivos");
        }
        if self.max_orders_per_account == 0 {
            return Err("parâmetros do mercado devem ser positivos");
        }
        Ok(())
    }
}

impl Encode for MarketParams {
    fn encode(&self, e: &mut Encoder) {
        e.u32(self.fee_bps)
            .u64(self.order_lifetime_blocks)
            .u32(self.max_open_orders)
            .u32(self.max_orders_per_account);
    }
}

impl Decode for MarketParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            fee_bps: d.u32()?,
            order_lifetime_blocks: d.u64()?,
            max_open_orders: d.u32()?,
            max_orders_per_account: d.u32()?,
        })
    }
}

// ---------------------------------------------------------------- consultas

/// Ativo como exibido a interfaces (resposta de `GET_ASSETS`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetView {
    pub id: AssetId,
    pub info: AssetInfo,
    /// Quantidade no Pool permanente.
    pub pool: u64,
    /// Saldo da conta consultada, se houver.
    pub balance: Option<u64>,
    pub last_price: Option<u64>,
}

impl Encode for AssetView {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id)
            .put(&self.info)
            .u64(self.pool)
            .option(&self.balance)
            .option(&self.last_price);
    }
}

impl Decode for AssetView {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: d.get()?,
            info: d.get()?,
            pool: d.u64()?,
            balance: d.option()?,
            last_price: d.option()?,
        })
    }
}

/// Nível agregado do livro: quantidade total a um preço.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookLevel {
    pub price: u64,
    pub amount: u64,
}

impl Encode for BookLevel {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.price).u64(self.amount);
    }
}

impl Decode for BookLevel {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            price: d.u64()?,
            amount: d.u64()?,
        })
    }
}

impl MarketState {
    /// Livro agregado de um ativo: compras do maior preço para o menor e
    /// vendas do menor para o maior, até `max` níveis de cada lado.
    pub fn book(&self, asset: &AssetId, max: usize) -> (Vec<BookLevel>, Vec<BookLevel>) {
        let mut bids: BTreeMap<u64, u64> = BTreeMap::new();
        let mut asks: BTreeMap<u64, u64> = BTreeMap::new();
        for o in self.orders.values().filter(|o| o.asset == *asset) {
            let side = match o.side {
                Side::Buy => &mut bids,
                Side::Sell => &mut asks,
            };
            let v = side.entry(o.price).or_default();
            *v = v.saturating_add(o.remaining);
        }
        let level = |(price, amount): (&u64, &u64)| BookLevel {
            price: *price,
            amount: *amount,
        };
        (
            bids.iter().rev().take(max).map(level).collect(),
            asks.iter().take(max).map(level).collect(),
        )
    }
}

// ------------------------------------------------------------------ leilão

/// Execução entre uma compra e uma venda, ao preço de equilíbrio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fill {
    pub buy: Hash32,
    pub sell: Hash32,
    /// Quantidade do ativo-base.
    pub base: u64,
    /// ZERO pago pelo comprador e recebido pelo vendedor (antes da tarifa).
    pub quote: u64,
}

/// Resultado do leilão de um ativo num bloco.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Auction {
    pub price: Option<u64>,
    pub fills: Vec<Fill>,
}

/// Leilão de preço uniforme sobre as ordens de **um** ativo.
///
/// 1. Candidatos: todos os preços-limite presentes.
/// 2. Para cada candidato `p`: demanda `D(p)` = compras com limite `≥ p`;
///    oferta `S(p)` = vendas com limite `≤ p`; volume `min(D, S)`.
/// 3. Escolhe o volume máximo; empate → menor desequilíbrio `|D − S|`;
///    empate → a mediana (inferior) dos candidatos restantes.
/// 4. Preenche por prioridade de preço, depois altura de colocação, depois
///    identificador; os pares são formados em ordem, e cada par paga
///    `⌊q · p / PRICE_SCALE⌋`. Se esse valor for 0, o comprador paga 1
///    unidade mínima quando isso cabe no seu limite; senão o par é pulado.
///
/// Determinístico: depende só do conjunto de ordens, não da ordem das
/// transações no bloco.
pub fn clear<'a>(orders: impl IntoIterator<Item = &'a Order>) -> Auction {
    let mut bids: Vec<&Order> = Vec::new();
    let mut asks: Vec<&Order> = Vec::new();
    for o in orders {
        if o.remaining == 0 {
            continue;
        }
        match o.side {
            Side::Buy => bids.push(o),
            Side::Sell => asks.push(o),
        }
    }
    if bids.is_empty() || asks.is_empty() {
        return Auction::default();
    }
    let candidates: BTreeSet<u64> = bids.iter().chain(&asks).map(|o| o.price).collect();
    let mut best: Vec<(u64, u128, u128)> = Vec::new(); // (preço, volume, desequilíbrio)
    for &p in &candidates {
        let d: u128 = bids
            .iter()
            .filter(|o| o.price >= p)
            .map(|o| o.remaining as u128)
            .sum();
        let s: u128 = asks
            .iter()
            .filter(|o| o.price <= p)
            .map(|o| o.remaining as u128)
            .sum();
        let vol = d.min(s);
        if vol == 0 {
            continue;
        }
        let imbalance = d.abs_diff(s);
        match best.first() {
            Some(&(_, bv, bi)) if vol < bv || (vol == bv && imbalance > bi) => {}
            Some(&(_, bv, bi)) if vol == bv && imbalance == bi => best.push((p, vol, imbalance)),
            _ => best = vec![(p, vol, imbalance)],
        }
    }
    let Some(&(price, volume, _)) = best.get((best.len().saturating_sub(1)) / 2) else {
        return Auction::default();
    };

    bids.retain(|o| o.price >= price);
    asks.retain(|o| o.price <= price);
    bids.sort_by(|a, b| {
        b.price
            .cmp(&a.price)
            .then(a.placed_at.cmp(&b.placed_at))
            .then(a.id.cmp(&b.id))
    });
    asks.sort_by(|a, b| {
        a.price
            .cmp(&b.price)
            .then(a.placed_at.cmp(&b.placed_at))
            .then(a.id.cmp(&b.id))
    });

    let mut fills = Vec::new();
    let mut left = volume;
    let (mut bi, mut si) = (0usize, 0usize);
    let mut bid_rem: Vec<u64> = bids.iter().map(|o| o.remaining).collect();
    let mut ask_rem: Vec<u64> = asks.iter().map(|o| o.remaining).collect();
    while left > 0 && bi < bids.len() && si < asks.len() {
        let m = bid_rem[bi]
            .min(ask_rem[si])
            .min(u64::try_from(left).unwrap_or(u64::MAX));
        // Valor que arredonda para 0: o comprador paga 1 unidade mínima se
        // isso couber no seu limite (o vendedor recebe acima do seu).
        let q = match quote_floor(m, price) {
            Some(0) if quote_floor(m, bids[bi].price).is_some_and(|v| v >= 1) => Some(1),
            other => other,
        };
        match q {
            Some(q) if q > 0 => {
                fills.push(Fill {
                    buy: bids[bi].id,
                    sell: asks[si].id,
                    base: m,
                    quote: q,
                });
                bid_rem[bi] -= m;
                ask_rem[si] -= m;
                left -= m as u128;
            }
            // Valor nulo (poeira) ou fora de u64: não executa este par.
            _ => {
                if bid_rem[bi] <= ask_rem[si] {
                    bid_rem[bi] = 0;
                } else {
                    ask_rem[si] = 0;
                }
            }
        }
        if bid_rem[bi] == 0 {
            bi += 1;
        }
        if si < ask_rem.len() && ask_rem[si] == 0 {
            si += 1;
        }
    }
    Auction {
        price: (!fills.is_empty()).then_some(price),
        fills,
    }
}

// ------------------------------------------------------------------- estado

/// Parte do estado dedicada ao Grande Mercado e ao Pool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketState {
    /// Ativos externos registrados.
    pub assets: BTreeMap<AssetId, AssetInfo>,
    /// Saldos de ativos externos (ZERO fica em `Account::balance`).
    pub balances: BTreeMap<(Address, AssetId), u64>,
    /// Pool permanente: só cresce (INV-POOL).
    pub pool: BTreeMap<AssetId, u64>,
    /// Livro de ordens abertas.
    pub orders: BTreeMap<Hash32, Order>,
    /// Último preço de equilíbrio por ativo.
    pub last_price: BTreeMap<AssetId, u64>,
    /// Ativos cujo livro mudou no bloco corrente (não entra na raiz: é
    /// sempre vazio entre blocos).
    pub dirty: BTreeSet<AssetId>,
}

impl MarketState {
    pub fn balance(&self, owner: &Address, asset: &AssetId) -> u64 {
        self.balances.get(&(*owner, *asset)).copied().unwrap_or(0)
    }

    pub fn pool_balance(&self, asset: &AssetId) -> u64 {
        self.pool.get(asset).copied().unwrap_or(0)
    }

    pub fn orders_of(&self, owner: &Address) -> usize {
        self.orders.values().filter(|o| o.owner == *owner).count()
    }

    /// ZERO reservado por compras abertas.
    pub fn zero_escrow(&self) -> u128 {
        self.orders
            .values()
            .filter(|o| o.side == Side::Buy)
            .map(|o| o.escrow as u128)
            .sum()
    }

    /// Conservação de cada ativo externo:
    /// `Σ saldos + reservas de venda + Pool = supply`.
    /// `escrow`: ativos externos retidos fora do mercado (trocas atômicas).
    pub fn check_assets(&self, escrow: &BTreeMap<AssetId, u128>) -> Result<(), AssetId> {
        let mut sums: BTreeMap<AssetId, u128> = escrow.clone();
        for ((_, a), v) in &self.balances {
            *sums.entry(*a).or_default() += *v as u128;
        }
        for o in self.orders.values().filter(|o| o.side == Side::Sell) {
            *sums.entry(o.asset).or_default() += o.escrow as u128;
        }
        for (a, v) in &self.pool {
            if !a.is_zero() {
                *sums.entry(*a).or_default() += *v as u128;
            }
        }
        for (id, info) in &self.assets {
            if sums.remove(id).unwrap_or(0) != info.supply as u128 {
                return Err(*id);
            }
        }
        // Saldo de ativo não registrado é impossível.
        match sums.into_iter().find(|(_, v)| *v > 0) {
            Some((id, _)) => Err(id),
            None => Ok(()),
        }
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.assets.len() as u64);
        for (id, info) in &self.assets {
            e.put(id).put(info);
        }
        e.u64(self.balances.len() as u64);
        for ((addr, asset), v) in &self.balances {
            e.put(addr).put(asset).u64(*v);
        }
        e.u64(self.pool.len() as u64);
        for (asset, v) in &self.pool {
            e.put(asset).u64(*v);
        }
        e.u64(self.orders.len() as u64);
        for o in self.orders.values() {
            e.put(o);
        }
        e.u64(self.last_price.len() as u64);
        for (asset, p) in &self.last_price {
            e.put(asset).u64(*p);
        }
        hash(MARKET_ROOT, &e.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(n: u8, side: Side, price: u64, qty: u64, height: u64) -> Order {
        Order {
            id: Hash32([n; 32]),
            owner: Address(Hash32([n; 32])),
            asset: AssetId::external("teste", "ATV"),
            side,
            price,
            remaining: qty,
            escrow: 0,
            placed_at: height,
            expires_at: u64::MAX,
        }
    }

    const P: u64 = PRICE_SCALE as u64; // preço 1:1

    #[test]
    fn asset_ids_never_zero_and_names_checked() {
        let a = AssetId::external("bitcoin", "BTC");
        assert!(!a.is_zero());
        assert_ne!(a, AssetId::external("bitcoin", "BTC2"));
        // AT-POOL-004 — ativo externo registrado como ZERO.
        assert!(AssetInfo::check_names("bitcoin", "ZERO").is_err());
        assert!(AssetInfo::check_names("bitcoin", "zero").is_err());
        assert!(AssetInfo::check_names("rede-zero-devnet-1", "X").is_err());
        assert!(AssetInfo::check_names("", "X").is_err());
        assert!(AssetInfo::check_names("bitcoin", "BTC").is_ok());
    }

    #[test]
    fn no_cross_no_trade() {
        let o = [
            order(1, Side::Buy, P, 10, 1),
            order(2, Side::Sell, 2 * P, 10, 1),
        ];
        assert_eq!(clear(&o), Auction::default());
    }

    #[test]
    fn uniform_price_for_all_fills() {
        // Compras a 3 e 2; vendas a 1 e 2 (em unidades de P).
        let o = [
            order(1, Side::Buy, 3 * P, 100, 1),
            order(2, Side::Buy, 2 * P, 100, 1),
            order(3, Side::Sell, P, 100, 1),
            order(4, Side::Sell, 2 * P, 100, 1),
        ];
        let a = clear(&o);
        // A 2: demanda 200, oferta 200 → volume máximo e equilíbrio.
        assert_eq!(a.price, Some(2 * P));
        let base: u64 = a.fills.iter().map(|f| f.base).sum();
        assert_eq!(base, 200);
        // Todos pagam o mesmo preço, inclusive quem aceitava pagar 3.
        assert!(a.fills.iter().all(|f| f.quote == 2 * f.base));
    }

    #[test]
    fn result_independent_of_input_order() {
        let mut o = vec![
            order(1, Side::Buy, 5 * P, 30, 2),
            order(2, Side::Buy, 4 * P, 50, 1),
            order(3, Side::Sell, 3 * P, 40, 1),
            order(4, Side::Sell, 4 * P, 60, 3),
            order(5, Side::Buy, 4 * P, 20, 1),
        ];
        let a = clear(&o);
        o.reverse();
        assert_eq!(clear(&o), a);
        o.swap(0, 3);
        assert_eq!(clear(&o), a);
        assert!(a.price.is_some());
    }

    #[test]
    fn price_then_time_priority() {
        // Duas vendas no mesmo preço: a mais antiga executa primeiro.
        let o = [
            order(1, Side::Buy, P, 10, 5),
            order(2, Side::Sell, P, 10, 4),
            order(3, Side::Sell, P, 10, 2),
        ];
        let a = clear(&o);
        assert_eq!(a.fills.len(), 1);
        assert_eq!(a.fills[0].sell, Hash32([3; 32]));
    }

    #[test]
    fn dust_pairs_skipped() {
        // Preço muito baixo: 1 unidade vale menos de 1 unidade de ZERO.
        let o = [order(1, Side::Buy, 1, 1, 1), order(2, Side::Sell, 1, 1, 1)];
        assert_eq!(clear(&o), Auction::default());
    }

    #[test]
    fn quote_rounding() {
        assert_eq!(quote_floor(3, P / 2), Some(1));
        assert_eq!(quote_ceil(3, P / 2), Some(2));
        assert_eq!(quote_floor(u64::MAX, u64::MAX), None);
    }
}
