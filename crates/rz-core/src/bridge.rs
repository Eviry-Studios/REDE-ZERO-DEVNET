//! Ponte para ativos externos (`spec/BRIDGE.md`, ADR-0019).
//!
//! Duas peças, nenhuma com custodiante (`SPEC §42`, `ARCHITECTURE §30`):
//!
//! 1. **Entrada por prova de queima, verificada por cliente leve.** A Rede
//!    Zero mantém, na própria cadeia, os cabeçalhos de uma rede de prova de
//!    trabalho (Bitcoin e afins) e verifica as regras de dificuldade. Quem
//!    destrói um ativo na origem, com uma saída `OP_RETURN` que compromete a
//!    rede Zero de destino e o destinatário, prova a inclusão da transação
//!    (prova de Merkle) num bloco com confirmações suficientes e recebe a
//!    representação do ativo. É uma via de mão única: nada fica guardado por
//!    ninguém, logo não há resgate a pedir a ninguém.
//! 2. **Trocas atômicas (HTLC).** Bloqueio por hash SHA-256 e prazo em
//!    altura, compatível com contratos equivalentes nas redes de origem: duas
//!    pessoas trocam ZERO ou ativos por ativos de outra rede sem
//!    intermediário.
//!
//! Este módulo contém a verificação (cabeçalhos, dificuldade, Merkle,
//! transações) e os tipos. A aplicação ao estado fica em `state.rs`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Address, Hash32};
use sha2::{Digest, Sha256};

use crate::market::{AssetId, AssetInfo};

/// Contexto da etiqueta de rede gravada na saída de queima.
pub const BRIDGE_TAG: &str = "rede-zero/bridge-tag/v1";
/// Contexto da raiz da parte da ponte no estado.
pub const BRIDGE_ROOT: &str = "rede-zero/bridge-root/v1";

/// Prefixo do conteúdo da saída `OP_RETURN` de queima.
pub const BURN_MAGIC: &[u8; 3] = b"RZ1";
/// `RZ1 ‖ etiqueta (8) ‖ endereço (32)`.
pub const BURN_PAYLOAD_LEN: usize = 3 + 8 + 32;

pub const MAX_CHAINS: usize = 16;
pub const MAX_HEADERS_PER_TX: usize = 500;
pub const MAX_ORIGIN_TX: usize = 100_000;
pub const MAX_MERKLE_DEPTH: usize = 32;
pub const MAX_PREIMAGE: usize = 64;
/// Cabeçalhos guardados abaixo da ponta (reorganizações mais profundas são
/// impossíveis por construção).
pub const KEEP_HEADERS: u64 = 4_032;
/// Liquidações de HTLC lembradas (as mais recentes).
pub const MAX_SETTLED: usize = 4096;
pub const MAX_CHAIN_NAME: usize = 32;

// --------------------------------------------------------------- hashes

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub fn sha256d(data: &[u8]) -> [u8; 32] {
    sha256(&sha256(data))
}

/// Hash na ordem de exibição (invertida), como mostram as redes de origem.
pub fn display_hex(h: &Hash32) -> String {
    let mut b = h.0;
    b.reverse();
    rz_crypto::hex::encode(&b)
}

pub fn from_display_hex(s: &str) -> Option<Hash32> {
    let mut b: [u8; 32] = rz_crypto::hex::decode_array(s)?;
    b.reverse();
    Some(Hash32(b))
}

// ----------------------------------------------------------------- U256

/// Inteiro sem sinal de 256 bits (limbs em ordem crescente).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct U256([u64; 4]);

impl Ord for U256 {
    fn cmp(&self, o: &Self) -> Ordering {
        for i in (0..4).rev() {
            match self.0[i].cmp(&o.0[i]) {
                Ordering::Equal => {}
                x => return x,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl U256 {
    pub const ZERO: U256 = U256([0; 4]);
    pub const MAX: U256 = U256([u64::MAX; 4]);

    pub fn from_u64(v: u64) -> Self {
        U256([v, 0, 0, 0])
    }

    /// Número em little-endian (como o hash de um bloco é comparado).
    pub fn from_le(b: &[u8; 32]) -> Self {
        let mut l = [0u64; 4];
        for (i, x) in l.iter_mut().enumerate() {
            let mut w = [0u8; 8];
            w.copy_from_slice(&b[i * 8..i * 8 + 8]);
            *x = u64::from_le_bytes(w);
        }
        U256(l)
    }

    pub fn from_be(b: &[u8; 32]) -> Self {
        let mut r = *b;
        r.reverse();
        Self::from_le(&r)
    }

    pub fn to_be(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..4 {
            out[i * 8..i * 8 + 8].copy_from_slice(&self.0[i].to_le_bytes());
        }
        out.reverse();
        out
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }

    pub fn bits(&self) -> u32 {
        for i in (0..4).rev() {
            if self.0[i] != 0 {
                return 64 * i as u32 + (64 - self.0[i].leading_zeros());
            }
        }
        0
    }

    pub fn shl_bits(self, n: u32) -> Self {
        let mut r = [0u64; 4];
        let (w, b) = ((n / 64) as usize, n % 64);
        for i in (0..4).rev() {
            if i < w {
                continue;
            }
            let mut v = self.0[i - w] << b;
            if b > 0 && i > w {
                v |= self.0[i - w - 1] >> (64 - b);
            }
            r[i] = v;
        }
        U256(r)
    }

    pub fn shr_bits(self, n: u32) -> Self {
        let mut r = [0u64; 4];
        let (w, b) = ((n / 64) as usize, n % 64);
        for (i, slot) in r.iter_mut().enumerate() {
            if i + w >= 4 {
                break;
            }
            let mut v = self.0[i + w] >> b;
            if b > 0 && i + w + 1 < 4 {
                v |= self.0[i + w + 1] << (64 - b);
            }
            *slot = v;
        }
        U256(r)
    }

    pub fn checked_add(self, o: Self) -> Option<Self> {
        let mut r = [0u64; 4];
        let mut carry = 0u128;
        for (i, slot) in r.iter_mut().enumerate() {
            let s = self.0[i] as u128 + o.0[i] as u128 + carry;
            *slot = s as u64;
            carry = s >> 64;
        }
        (carry == 0).then_some(U256(r))
    }

    fn sub(self, o: Self) -> Self {
        let mut r = [0u64; 4];
        let mut borrow = 0i128;
        for (i, slot) in r.iter_mut().enumerate() {
            let d = self.0[i] as i128 - o.0[i] as i128 - borrow;
            *slot = d as u64;
            borrow = i128::from(d < 0);
        }
        U256(r)
    }

    pub fn checked_mul_u64(self, m: u64) -> Option<Self> {
        let mut r = [0u64; 4];
        let mut carry = 0u128;
        for (i, slot) in r.iter_mut().enumerate() {
            let p = self.0[i] as u128 * m as u128 + carry;
            *slot = p as u64;
            carry = p >> 64;
        }
        (carry == 0).then_some(U256(r))
    }

    pub fn div_u64(self, d: u64) -> Self {
        let mut r = [0u64; 4];
        let mut rem = 0u128;
        for i in (0..4).rev() {
            let cur = (rem << 64) | self.0[i] as u128;
            r[i] = (cur / d as u128) as u64;
            rem = cur % d as u128;
        }
        U256(r)
    }

    /// Divisão longa binária (256 iterações).
    pub fn div_u256(self, d: Self) -> Self {
        if d.is_zero() {
            return U256::MAX;
        }
        let mut q = U256::ZERO;
        let mut r = U256::ZERO;
        for i in (0..256).rev() {
            r = r.shl_bits(1);
            if (self.0[i / 64] >> (i % 64)) & 1 == 1 {
                r.0[0] |= 1;
            }
            if r >= d {
                r = r.sub(d);
                q.0[i / 64] |= 1 << (i % 64);
            }
        }
        q
    }

    fn low_u64(&self) -> u64 {
        self.0[0]
    }

    /// Alvo a partir da forma compacta (`nBits`). `None` se negativo, com
    /// estouro ou nulo.
    pub fn from_compact(bits: u32) -> Option<Self> {
        let size = bits >> 24;
        let word = bits & 0x007f_ffff;
        if word != 0 && bits & 0x0080_0000 != 0 {
            return None;
        }
        if word != 0 && (size > 34 || (word > 0xff && size > 33) || (word > 0xffff && size > 32)) {
            return None;
        }
        let t = if size <= 3 {
            U256::from_u64((word >> (8 * (3 - size))) as u64)
        } else {
            U256::from_u64(word as u64).shl_bits(8 * (size - 3))
        };
        (!t.is_zero()).then_some(t)
    }

    pub fn to_compact(self) -> u32 {
        let mut size = self.bits().div_ceil(8);
        let mut compact = if size <= 3 {
            (self.low_u64() << (8 * (3 - size))) as u32
        } else {
            self.shr_bits(8 * (size - 3)).low_u64() as u32
        };
        if compact & 0x0080_0000 != 0 {
            compact >>= 8;
            size += 1;
        }
        compact | (size << 24)
    }

    /// Trabalho esperado para um alvo: `2^256 / (alvo + 1)`.
    pub fn work(target: Self) -> Self {
        // (~alvo / (alvo + 1)) + 1, sem estourar 256 bits.
        let inv = U256([!target.0[0], !target.0[1], !target.0[2], !target.0[3]]);
        let plus1 = target.checked_add(U256::from_u64(1)).unwrap_or(U256::MAX);
        inv.div_u256(plus1)
            .checked_add(U256::from_u64(1))
            .unwrap_or(U256::MAX)
    }
}

// ------------------------------------------------------------ cabeçalhos

/// Cabeçalho de 80 bytes de uma rede de prova de trabalho.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: i32,
    pub prev: Hash32,
    pub merkle_root: Hash32,
    pub time: u32,
    pub bits: u32,
    pub nonce: u32,
}

impl Header {
    pub fn parse(b: &[u8; 80]) -> Self {
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let h32 = |i: usize| {
            let mut x = [0u8; 32];
            x.copy_from_slice(&b[i..i + 32]);
            Hash32(x)
        };
        Header {
            version: u32_at(0) as i32,
            prev: h32(4),
            merkle_root: h32(36),
            time: u32_at(68),
            bits: u32_at(72),
            nonce: u32_at(76),
        }
    }

    pub fn serialize(&self) -> [u8; 80] {
        let mut b = [0u8; 80];
        b[0..4].copy_from_slice(&self.version.to_le_bytes());
        b[4..36].copy_from_slice(&self.prev.0);
        b[36..68].copy_from_slice(&self.merkle_root.0);
        b[68..72].copy_from_slice(&self.time.to_le_bytes());
        b[72..76].copy_from_slice(&self.bits.to_le_bytes());
        b[76..80].copy_from_slice(&self.nonce.to_le_bytes());
        b
    }

    /// Hash do bloco (ordem interna).
    pub fn hash(&self) -> Hash32 {
        Hash32(sha256d(&self.serialize()))
    }
}

/// Regras de prova de trabalho da rede de origem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowRules {
    /// Alvo máximo (big-endian).
    pub pow_limit: Hash32,
    /// `false` = sem reajuste (regtest): a dificuldade nunca muda.
    pub retarget: bool,
    /// Blocos por período de reajuste (Bitcoin: 2016).
    pub interval: u32,
    /// Duração alvo do período em segundos (Bitcoin: 1 209 600).
    pub target_timespan: u32,
}

impl PowRules {
    pub fn limit(&self) -> U256 {
        U256::from_be(&self.pow_limit.0)
    }

    /// Bitcoin: `CalculateNextWorkRequired`.
    pub fn next_bits(&self, last_bits: u32, first_time: u32, last_time: u32) -> Option<u32> {
        let t = self.target_timespan as i64;
        let actual = (last_time as i64 - first_time as i64).clamp(t / 4, t * 4);
        let new = U256::from_compact(last_bits)?
            .checked_mul_u64(actual as u64)?
            .div_u64(t as u64);
        Some(new.min(self.limit()).to_compact())
    }
}

impl Encode for PowRules {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.pow_limit)
            .bool(self.retarget)
            .u32(self.interval)
            .u32(self.target_timespan);
    }
}

impl Decode for PowRules {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            pow_limit: d.get()?,
            retarget: d.bool()?,
            interval: d.u32()?,
            target_timespan: d.u32()?,
        })
    }
}

/// Rede de origem conectada por cliente leve (definida no Genesis).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainConfig {
    /// Nome da rede (origem do ativo, ex.: `bitcoin`).
    pub name: String,
    /// Ativo nativo dela (ex.: `BTC`).
    pub asset_ref: String,
    pub decimals: u8,
    pub rules: PowRules,
    /// Cabeçalho de partida, aceito por configuração (como um ponto de
    /// verificação), numa fronteira de reajuste.
    pub checkpoint: [u8; 80],
    pub checkpoint_height: u64,
    /// Confirmações exigidas para aceitar uma prova de queima.
    pub confirmations: u32,
}

impl ChainConfig {
    pub fn asset(&self) -> AssetId {
        AssetId::external(&self.name, &self.asset_ref)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.len() > MAX_CHAIN_NAME {
            return Err("nome de rede de origem longo demais");
        }
        AssetInfo::check_names(&self.name, &self.asset_ref)?;
        if self.decimals > 18 {
            return Err("casas decimais demais");
        }
        let r = &self.rules;
        if r.interval == 0 || r.target_timespan == 0 || r.limit().is_zero() {
            return Err("regras de prova de trabalho inválidas");
        }
        if r.retarget && !self.checkpoint_height.is_multiple_of(r.interval as u64) {
            return Err("o ponto de partida deve estar numa fronteira de reajuste");
        }
        if self.confirmations == 0 || self.confirmations > 1_000 {
            return Err("confirmações fora dos limites");
        }
        let h = Header::parse(&self.checkpoint);
        check_pow(&h, r).map_err(|_| "ponto de partida sem prova de trabalho válida")
    }
}

impl Encode for ChainConfig {
    fn encode(&self, e: &mut Encoder) {
        e.str(&self.name)
            .str(&self.asset_ref)
            .u8(self.decimals)
            .put(&self.rules)
            .fixed(&self.checkpoint)
            .u64(self.checkpoint_height)
            .u32(self.confirmations);
    }
}

impl Decode for ChainConfig {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            name: d.str(MAX_CHAIN_NAME)?,
            asset_ref: d.str(crate::market::MAX_ASSET_REF_LEN)?,
            decimals: d.u8()?,
            rules: d.get()?,
            checkpoint: d.fixed()?,
            checkpoint_height: d.u64()?,
            confirmations: d.u32()?,
        })
    }
}

/// Prova de trabalho de um cabeçalho contra o próprio `bits` e o limite.
pub fn check_pow(h: &Header, rules: &PowRules) -> Result<(), &'static str> {
    let target = U256::from_compact(h.bits).ok_or("dificuldade inválida")?;
    if target > rules.limit() {
        return Err("dificuldade abaixo do mínimo da rede");
    }
    if U256::from_le(&h.hash().0) > target {
        return Err("prova de trabalho insuficiente");
    }
    Ok(())
}

/// Cabeçalho aceito.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderRec {
    pub height: u64,
    pub prev: Hash32,
    pub time: u32,
    pub bits: u32,
    pub merkle_root: Hash32,
    /// Trabalho acumulado desde o ponto de partida (big-endian).
    pub work: Hash32,
}

impl Encode for HeaderRec {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.height)
            .put(&self.prev)
            .u32(self.time)
            .u32(self.bits)
            .put(&self.merkle_root)
            .put(&self.work);
    }
}

/// Cabeçalhos novos e a nova ponta, se mudar.
pub type HeaderUpdate = (Vec<(Hash32, HeaderRec)>, Option<Hash32>);

/// Cliente leve de uma rede de origem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainState {
    pub config: ChainConfig,
    pub headers: BTreeMap<Hash32, HeaderRec>,
    pub best: Hash32,
    /// Cadeia de maior trabalho: altura → bloco (janela recente).
    pub by_height: BTreeMap<u64, Hash32>,
    /// Transações de origem já usadas como prova (contra cunhagem dupla).
    pub claimed: BTreeSet<Hash32>,
}

impl ChainState {
    pub fn new(config: ChainConfig) -> Self {
        let h = Header::parse(&config.checkpoint);
        let id = h.hash();
        let rec = HeaderRec {
            height: config.checkpoint_height,
            prev: h.prev,
            time: h.time,
            bits: h.bits,
            merkle_root: h.merkle_root,
            work: Hash32(U256::ZERO.to_be()),
        };
        let mut headers = BTreeMap::new();
        headers.insert(id, rec);
        let mut by_height = BTreeMap::new();
        by_height.insert(config.checkpoint_height, id);
        Self {
            config,
            headers,
            best: id,
            by_height,
            claimed: BTreeSet::new(),
        }
    }

    pub fn best_height(&self) -> u64 {
        self.headers.get(&self.best).map_or(0, |h| h.height)
    }

    fn work_of(&self, id: &Hash32, extra: &BTreeMap<Hash32, HeaderRec>) -> Option<U256> {
        extra
            .get(id)
            .or_else(|| self.headers.get(id))
            .map(|r| U256::from_be(&r.work.0))
    }

    /// Valida uma sequência de cabeçalhos. Retorna os novos registros e a
    /// nova ponta, se mudar. Não altera o estado.
    pub fn check_headers(&self, raw: &[[u8; 80]]) -> Result<HeaderUpdate, &'static str> {
        if raw.is_empty() || raw.len() > MAX_HEADERS_PER_TX {
            return Err("número de cabeçalhos fora dos limites");
        }
        let rules = &self.config.rules;
        let mut extra: BTreeMap<Hash32, HeaderRec> = BTreeMap::new();
        let mut out = Vec::new();
        let mut best = self.best;
        let mut best_work = self.work_of(&self.best, &extra).unwrap_or(U256::ZERO);
        let get = |extra: &BTreeMap<Hash32, HeaderRec>, id: &Hash32| {
            extra.get(id).or_else(|| self.headers.get(id)).cloned()
        };
        for b in raw {
            let h = Header::parse(b);
            let id = h.hash();
            if self.headers.contains_key(&id) || extra.contains_key(&id) {
                return Err("cabeçalho repetido");
            }
            let parent = get(&extra, &h.prev).ok_or("cabeçalho não se conecta a um conhecido")?;
            let height = parent.height + 1;
            // Dificuldade esperada.
            let expected = if rules.retarget && height.is_multiple_of(rules.interval as u64) {
                // Primeiro bloco do período anterior.
                let mut first = parent.clone();
                for _ in 1..rules.interval {
                    first =
                        get(&extra, &first.prev).ok_or("histórico insuficiente para o reajuste")?;
                }
                rules
                    .next_bits(parent.bits, first.time, parent.time)
                    .ok_or("reajuste de dificuldade inválido")?
            } else {
                parent.bits
            };
            if h.bits != expected {
                return Err("dificuldade diferente da exigida pelas regras da rede");
            }
            check_pow(&h, rules)?;
            // Tempo acima da mediana dos 11 anteriores (os disponíveis).
            let mut times = vec![parent.time];
            let mut cur = parent.clone();
            while times.len() < 11 {
                match get(&extra, &cur.prev) {
                    Some(p) => {
                        times.push(p.time);
                        cur = p;
                    }
                    None => break,
                }
            }
            times.sort_unstable();
            if h.time <= times[times.len() / 2] {
                return Err("horário abaixo da mediana dos blocos anteriores");
            }
            let target = U256::from_compact(h.bits).ok_or("dificuldade inválida")?;
            let work = U256::from_be(&parent.work.0)
                .checked_add(U256::work(target))
                .ok_or("trabalho acumulado estourou")?;
            let rec = HeaderRec {
                height,
                prev: h.prev,
                time: h.time,
                bits: h.bits,
                merkle_root: h.merkle_root,
                work: Hash32(work.to_be()),
            };
            if work > best_work {
                best_work = work;
                best = id;
            }
            extra.insert(id, rec.clone());
            out.push((id, rec));
        }
        Ok((out, (best != self.best).then_some(best)))
    }

    /// Aplica cabeçalhos validados por [`check_headers`](Self::check_headers).
    pub fn add_headers(&mut self, recs: Vec<(Hash32, HeaderRec)>, best: Option<Hash32>) {
        for (id, r) in recs {
            self.headers.insert(id, r);
        }
        if let Some(b) = best {
            self.best = b;
            // Reconstrói o índice da cadeia de maior trabalho até o ponto em
            // que coincide com o anterior.
            let mut cur = b;
            while let Some(r) = self.headers.get(&cur) {
                if self.by_height.get(&r.height) == Some(&cur) {
                    break;
                }
                self.by_height.insert(r.height, cur);
                cur = r.prev;
            }
            let top = self.best_height();
            self.by_height.retain(|h, _| *h <= top);
            // Poda abaixo da janela.
            let floor = top.saturating_sub(KEEP_HEADERS);
            self.by_height.retain(|h, _| *h >= floor);
            self.headers.retain(|_, r| r.height >= floor);
        }
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.put(&self.config).put(&self.best);
        e.u64(self.headers.len() as u64);
        for (id, r) in &self.headers {
            e.put(id).put(r);
        }
        e.u64(self.claimed.len() as u64);
        for c in &self.claimed {
            e.put(c);
        }
        hash(BRIDGE_ROOT, &e.into_bytes())
    }
}

// ------------------------------------------------------- provas de queima

/// Etiqueta de 8 bytes da rede Zero de destino, gravada na saída de queima:
/// a mesma queima não vale em duas redes.
pub fn network_tag(network_id: &str) -> [u8; 8] {
    let h = hash(BRIDGE_TAG, network_id.as_bytes());
    let mut t = [0u8; 8];
    t.copy_from_slice(&h.0[..8]);
    t
}

/// Script `OP_RETURN` que queima o valor da saída para `recipient`.
pub fn burn_script(network_id: &str, recipient: &Address) -> Vec<u8> {
    let mut s = vec![0x6a, BURN_PAYLOAD_LEN as u8];
    s.extend_from_slice(BURN_MAGIC);
    s.extend_from_slice(&network_tag(network_id));
    s.extend_from_slice(&recipient.0 .0);
    s
}

/// Raiz de Merkle a partir da folha, do caminho e da posição (regras de
/// Bitcoin: SHA-256 duplo dos pares concatenados).
pub fn merkle_root(leaf: &Hash32, path: &[Hash32], index: u32) -> Option<Hash32> {
    if path.len() > MAX_MERKLE_DEPTH || (path.len() < 32 && (index as u64) >> path.len() != 0) {
        return None;
    }
    let mut h = leaf.0;
    for (i, sib) in path.iter().enumerate() {
        let mut buf = [0u8; 64];
        if (index >> i) & 1 == 1 {
            buf[..32].copy_from_slice(&sib.0);
            buf[32..].copy_from_slice(&h);
        } else {
            buf[..32].copy_from_slice(&h);
            buf[32..].copy_from_slice(&sib.0);
        }
        h = sha256d(&buf);
    }
    Some(Hash32(h))
}

/// Saída de uma transação de origem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxOut {
    pub value: u64,
    pub script: Vec<u8>,
}

fn varint(b: &[u8], p: &mut usize) -> Option<u64> {
    let first = *b.get(*p)?;
    *p += 1;
    let n = match first {
        0xfd => 2,
        0xfe => 4,
        0xff => 8,
        v => return Some(v as u64),
    };
    let bytes = b.get(*p..*p + n)?;
    *p += n;
    let mut v = [0u8; 8];
    v[..n].copy_from_slice(bytes);
    Some(u64::from_le_bytes(v))
}

fn take<'a>(b: &'a [u8], p: &mut usize, n: usize) -> Option<&'a [u8]> {
    let s = b.get(*p..p.checked_add(n)?)?;
    *p += n;
    Some(s)
}

/// Saídas de uma transação na serialização **sem testemunhas** (a que define
/// o identificador). A serialização com testemunhas é recusada.
pub fn parse_outputs(tx: &[u8]) -> Result<Vec<TxOut>, &'static str> {
    let bad = "transação de origem malformada";
    let mut p = 4usize;
    if tx.len() < 10 {
        return Err(bad);
    }
    if tx[4] == 0x00 {
        return Err("forneça a transação sem testemunhas (a serialização do identificador)");
    }
    let n_in = varint(tx, &mut p).ok_or(bad)?;
    if n_in == 0 || n_in > 10_000 {
        return Err(bad);
    }
    for _ in 0..n_in {
        take(tx, &mut p, 36).ok_or(bad)?;
        let l = varint(tx, &mut p).ok_or(bad)? as usize;
        take(tx, &mut p, l).ok_or(bad)?;
        take(tx, &mut p, 4).ok_or(bad)?;
    }
    let n_out = varint(tx, &mut p).ok_or(bad)?;
    if n_out == 0 || n_out > 10_000 {
        return Err(bad);
    }
    let mut outs = Vec::new();
    for _ in 0..n_out {
        let v = take(tx, &mut p, 8).ok_or(bad)?;
        let mut w = [0u8; 8];
        w.copy_from_slice(v);
        let l = varint(tx, &mut p).ok_or(bad)? as usize;
        let script = take(tx, &mut p, l).ok_or(bad)?.to_vec();
        outs.push(TxOut {
            value: u64::from_le_bytes(w),
            script,
        });
    }
    take(tx, &mut p, 4).ok_or(bad)?;
    if p != tx.len() {
        return Err(bad);
    }
    Ok(outs)
}

/// Destinatário de uma saída de queima válida para esta rede.
pub fn burn_recipient(script: &[u8], tag: &[u8; 8]) -> Option<Address> {
    let data = match script {
        [0x6a, n, rest @ ..] if *n as usize == BURN_PAYLOAD_LEN => rest,
        [0x6a, 0x4c, n, rest @ ..] if *n as usize == BURN_PAYLOAD_LEN => rest,
        _ => return None,
    };
    if data.len() != BURN_PAYLOAD_LEN || &data[..3] != BURN_MAGIC || &data[3..11] != tag {
        return None;
    }
    let mut a = [0u8; 32];
    a.copy_from_slice(&data[11..]);
    Some(Address(Hash32(a)))
}

/// Resultado de uma prova de queima aceita.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Burn {
    pub txid: Hash32,
    pub recipient: Address,
    pub amount: u64,
}

impl ChainState {
    /// Verifica uma prova de queima contra o cliente leve.
    pub fn check_burn(
        &self,
        network_id: &str,
        block: &Hash32,
        tx: &[u8],
        path: &[Hash32],
        index: u32,
    ) -> Result<Burn, &'static str> {
        let rec = self.headers.get(block).ok_or("bloco desconhecido")?;
        if self.by_height.get(&rec.height) != Some(block) {
            return Err("bloco fora da cadeia de maior trabalho");
        }
        let confirmations = self.best_height() - rec.height + 1;
        if confirmations < self.config.confirmations as u64 {
            return Err("confirmações insuficientes");
        }
        // Uma transação de 64 bytes poderia se passar por um nó interno da
        // árvore de Merkle (CVE-2017-12842).
        if tx.len() == 64 || tx.len() > MAX_ORIGIN_TX {
            return Err("tamanho de transação de origem inválido");
        }
        let txid = Hash32(sha256d(tx));
        if self.claimed.contains(&txid) {
            return Err("queima já usada");
        }
        if merkle_root(&txid, path, index) != Some(rec.merkle_root) {
            return Err("prova de Merkle inválida");
        }
        let tag = network_tag(network_id);
        let burns: Vec<(Address, u64)> = parse_outputs(tx)?
            .into_iter()
            .filter_map(|o| burn_recipient(&o.script, &tag).map(|a| (a, o.value)))
            .collect();
        match burns.as_slice() {
            [(recipient, amount)] if *amount > 0 => Ok(Burn {
                txid,
                recipient: *recipient,
                amount: *amount,
            }),
            [] => Err("nenhuma saída de queima para esta rede"),
            _ => Err("saída de queima ausente, nula ou repetida"),
        }
    }
}

/// Extrai `(cabeçalho, caminho, posição)` de uma prova `merkleblock` (a saída
/// de `gettxoutproof` de um nó da rede de origem) para a transação `txid`.
pub fn path_from_merkleblock(
    bytes: &[u8],
    txid: &Hash32,
) -> Result<([u8; 80], Vec<Hash32>, u32), &'static str> {
    let bad = "merkleblock malformado";
    let header: [u8; 80] = bytes.get(..80).ok_or(bad)?.try_into().map_err(|_| bad)?;
    let mut p = 80usize;
    let total = u32::from_le_bytes(
        take(bytes, &mut p, 4)
            .ok_or(bad)?
            .try_into()
            .map_err(|_| bad)?,
    );
    if total == 0 {
        return Err(bad);
    }
    let nh = varint(bytes, &mut p).ok_or(bad)? as usize;
    if nh > 100_000 {
        return Err(bad);
    }
    let mut hashes = Vec::with_capacity(nh);
    for _ in 0..nh {
        let h: [u8; 32] = take(bytes, &mut p, 32)
            .ok_or(bad)?
            .try_into()
            .map_err(|_| bad)?;
        hashes.push(Hash32(h));
    }
    let nf = varint(bytes, &mut p).ok_or(bad)? as usize;
    let flags = take(bytes, &mut p, nf).ok_or(bad)?.to_vec();
    let mut height = 0u32;
    while (1u64 << height) < total as u64 {
        height += 1;
    }
    let width = |h: u32| ((total as u64) + (1u64 << h) - 1) >> h;
    struct Walk<'a> {
        hashes: &'a [Hash32],
        flags: &'a [u8],
        hi: usize,
        fi: usize,
        target: Hash32,
        found: Option<(u32, Vec<Hash32>)>,
    }
    fn walk(w: &mut Walk<'_>, h: u32, pos: u64, width: &dyn Fn(u32) -> u64) -> Option<Hash32> {
        let flag = (w.flags.get(w.fi / 8)? >> (w.fi % 8)) & 1;
        w.fi += 1;
        if h == 0 || flag == 0 {
            let x = *w.hashes.get(w.hi)?;
            w.hi += 1;
            if h == 0 && flag == 1 && x == w.target {
                w.found = Some((pos as u32, Vec::new()));
            }
            return Some(x);
        }
        let left = walk(w, h - 1, pos * 2, width)?;
        let right = if pos * 2 + 1 < width(h - 1) {
            walk(w, h - 1, pos * 2 + 1, width)?
        } else {
            left
        };
        // Registra o irmão no caminho, se a transação estiver nesta subárvore.
        if let Some((idx, path)) = &mut w.found {
            if path.len() == (h - 1) as usize && (*idx as u64 >> (h - 1)) == pos {
                let sibling = if (*idx >> (h - 1)) & 1 == 0 {
                    right
                } else {
                    left
                };
                path.push(sibling);
            }
        }
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(&left.0);
        buf[32..].copy_from_slice(&right.0);
        Some(Hash32(sha256d(&buf)))
    }
    let mut w = Walk {
        hashes: &hashes,
        flags: &flags,
        hi: 0,
        fi: 0,
        target: *txid,
        found: None,
    };
    let root = walk(&mut w, height, 0, &width).ok_or(bad)?;
    if root != Header::parse(&header).merkle_root {
        return Err("merkleblock não confere com o cabeçalho");
    }
    let (index, path) = w.found.ok_or("a transação não está no merkleblock")?;
    Ok((header, path, index))
}

// ------------------------------------------------------------------ HTLC

/// Contrato de troca atômica: `amount` de `asset` para `recipient` se a
/// pré-imagem de `hashlock` (SHA-256) for revelada antes de `timeout`; senão,
/// volta para `sender`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Htlc {
    pub id: Hash32,
    pub sender: Address,
    pub recipient: Address,
    pub asset: AssetId,
    pub amount: u64,
    pub hashlock: Hash32,
    pub timeout: u64,
}

impl Encode for Htlc {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id)
            .put(&self.sender)
            .put(&self.recipient)
            .put(&self.asset)
            .u64(self.amount)
            .put(&self.hashlock)
            .u64(self.timeout);
    }
}

impl Decode for Htlc {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: d.get()?,
            sender: d.get()?,
            recipient: d.get()?,
            asset: d.get()?,
            amount: d.u64()?,
            hashlock: d.get()?,
            timeout: d.u64()?,
        })
    }
}

/// Liquidação registrada: pré-imagem revelada (resgate) ou devolução.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settled {
    pub id: Hash32,
    pub preimage: Option<Vec<u8>>,
    pub height: u64,
}

impl Encode for Settled {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.id);
        match &self.preimage {
            Some(p) => e.u8(1).bytes(p),
            None => e.u8(0),
        };
        e.u64(self.height);
    }
}

impl Decode for Settled {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let id = d.get()?;
        let preimage = match d.u8()? {
            0 => None,
            1 => Some(d.bytes(MAX_PREIMAGE)?),
            t => return Err(DecodeError::InvalidTag(t)),
        };
        Ok(Self {
            id,
            preimage,
            height: d.u64()?,
        })
    }
}

/// Parte do estado dedicada à ponte.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BridgeState {
    pub chains: BTreeMap<String, ChainState>,
    pub htlcs: BTreeMap<Hash32, Htlc>,
    pub settled: VecDeque<Settled>,
}

impl BridgeState {
    /// ZERO retido em contratos de troca.
    pub fn zero_escrow(&self) -> u128 {
        self.htlcs
            .values()
            .filter(|h| h.asset.is_zero())
            .map(|h| h.amount as u128)
            .sum()
    }

    /// Ativos externos retidos em contratos de troca.
    pub fn asset_escrow(&self) -> BTreeMap<AssetId, u128> {
        let mut m = BTreeMap::new();
        for h in self.htlcs.values().filter(|h| !h.asset.is_zero()) {
            *m.entry(h.asset).or_default() += h.amount as u128;
        }
        m
    }

    pub fn settled(&self, id: &Hash32) -> Option<&Settled> {
        self.settled.iter().rev().find(|s| s.id == *id)
    }

    pub fn push_settled(&mut self, s: Settled) {
        if self.settled.len() >= MAX_SETTLED {
            self.settled.pop_front();
        }
        self.settled.push_back(s);
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.chains.len() as u64);
        for (n, c) in &self.chains {
            e.str(n).put(&c.root());
        }
        e.u64(self.htlcs.len() as u64);
        for h in self.htlcs.values() {
            e.put(h);
        }
        e.u64(self.settled.len() as u64);
        for s in &self.settled {
            e.put(s);
        }
        hash(BRIDGE_ROOT, &e.into_bytes())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Bloco gênese do Bitcoin.
    const BTC_GENESIS: &str = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c";

    pub fn mainnet_rules() -> PowRules {
        let mut limit = [0xffu8; 32];
        limit[..4].copy_from_slice(&[0, 0, 0, 0]);
        PowRules {
            pow_limit: Hash32(limit),
            retarget: true,
            interval: 2016,
            target_timespan: 14 * 24 * 60 * 60,
        }
    }

    pub fn regtest_rules() -> PowRules {
        let mut limit = [0xffu8; 32];
        limit[0] = 0x7f;
        PowRules {
            pow_limit: Hash32(limit),
            retarget: false,
            interval: 2016,
            target_timespan: 14 * 24 * 60 * 60,
        }
    }

    #[test]
    fn bitcoin_genesis_pow() {
        let b: [u8; 80] = rz_crypto::hex::decode_array(BTC_GENESIS).unwrap();
        let h = Header::parse(&b);
        assert_eq!(h.serialize(), b);
        assert_eq!(
            display_hex(&h.hash()),
            "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
        );
        assert!(check_pow(&h, &mainnet_rules()).is_ok());
        // Um nonce diferente quebra a prova.
        let mut bad = h;
        bad.nonce ^= 1;
        assert!(check_pow(&bad, &mainnet_rules()).is_err());
    }

    /// Vetores de `pow_tests.cpp` do Bitcoin Core.
    #[test]
    fn bitcoin_retarget_vectors() {
        let r = mainnet_rules();
        // get_next_work
        assert_eq!(
            r.next_bits(0x1d00ffff, 1261130161, 1262152739),
            Some(0x1d00d86a)
        );
        // get_next_work_pow_limit
        assert_eq!(
            r.next_bits(0x1d00ffff, 1231006505, 1233061996),
            Some(0x1d00ffff)
        );
        // get_next_work_lower_limit_actual
        assert_eq!(
            r.next_bits(0x1c05a3f4, 1279008237, 1279297671),
            Some(0x1c0168fd)
        );
        // get_next_work_upper_limit_actual
        assert_eq!(
            r.next_bits(0x1c387f6f, 1263163443, 1269211443),
            Some(0x1d00e1fd)
        );
    }

    #[test]
    fn compact_roundtrip_and_rejections() {
        for bits in [
            0x1d00ffffu32,
            0x1c0168fd,
            0x207fffff,
            0x1b0404cb,
            0x03123456,
        ] {
            assert_eq!(U256::from_compact(bits).unwrap().to_compact(), bits);
        }
        assert!(U256::from_compact(0x04923456).is_none(), "negativo");
        assert!(U256::from_compact(0xff123456).is_none(), "estouro");
        assert!(U256::from_compact(0).is_none());
        // Regtest: alvo máximo representável em 0x207fffff.
        let r = regtest_rules();
        assert!(U256::from_compact(0x207fffff).unwrap() <= r.limit());
        assert!(!r.retarget);
        let w = U256::work(U256::from_compact(0x1d00ffff).unwrap());
        assert_eq!(w, U256::from_u64(0x0001_0001_0001));
    }

    fn tx_with_outputs(outs: &[(u64, Vec<u8>)]) -> Vec<u8> {
        let mut t = vec![1, 0, 0, 0, 1];
        t.extend([0x11; 36]);
        t.extend([0, 0xff, 0xff, 0xff, 0xff]);
        t.push(outs.len() as u8);
        for (v, s) in outs {
            t.extend(v.to_le_bytes());
            t.push(s.len() as u8);
            t.extend(s);
        }
        t.extend([0, 0, 0, 0]);
        t
    }

    #[test]
    fn burn_outputs_and_merkle() {
        let a = Address(Hash32([7; 32]));
        let script = burn_script("rede-x", &a);
        let tx = tx_with_outputs(&[(5_000, vec![0x76, 0xa9]), (1_234, script.clone())]);
        let outs = parse_outputs(&tx).unwrap();
        assert_eq!(outs[1].value, 1_234);
        let tag = network_tag("rede-x");
        assert_eq!(burn_recipient(&outs[1].script, &tag), Some(a));
        assert_eq!(burn_recipient(&outs[1].script, &network_tag("outra")), None);
        assert_eq!(burn_recipient(&outs[0].script, &tag), None);
        // Serialização com testemunhas recusada; lixo recusado.
        let mut segwit = tx.clone();
        segwit.insert(4, 0x00);
        segwit.insert(5, 0x01);
        assert!(parse_outputs(&segwit).is_err());
        assert!(parse_outputs(&tx[..tx.len() - 1]).is_err());
        // Merkle com quatro folhas.
        let leaves: Vec<Hash32> = (0..4u8).map(|i| Hash32(sha256d(&[i]))).collect();
        let pair = |x: &Hash32, y: &Hash32| {
            let mut b = [0u8; 64];
            b[..32].copy_from_slice(&x.0);
            b[32..].copy_from_slice(&y.0);
            Hash32(sha256d(&b))
        };
        let l = pair(&leaves[0], &leaves[1]);
        let r = pair(&leaves[2], &leaves[3]);
        let root = pair(&l, &r);
        assert_eq!(merkle_root(&leaves[2], &[leaves[3], l], 2), Some(root));
        assert_ne!(merkle_root(&leaves[2], &[leaves[3], l], 3), Some(root));
        assert_eq!(merkle_root(&leaves[2], &[leaves[3], l], 4), None);
    }
}
