//! Exonet Runtime (`spec/RUNTIME.md`, ADR-0018).
//!
//! Executa a lógica das Comunidades em WebAssembly, de forma determinística e
//! isolada (`SPEC §62`). Uma Comunidade só alcança o próprio armazenamento: o
//! módulo não tem como ler ou alterar saldos, consenso, governança, regras
//! monetárias, identidades de Nodes ou outras Comunidades (`SPEC §61`,
//! teste de isolamento §73). Essas operações simplesmente não existem na
//! interface oferecida ao módulo.
//!
//! Determinismo (THR-TX-005):
//!
//! * sem ponto flutuante: módulos com instruções de ponto flutuante são
//!   recusados na publicação;
//! * combustível medido por instrução, com limite por chamada e por bloco;
//! * limites fixos de memória, pilha e tabelas;
//! * a versão do interpretador é fixada (`Cargo.lock`) e faz parte do
//!   protocolo: trocá-la exige uma nova versão do software, aprovada pela
//!   governança.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use rz_codec::{Decode, DecodeError, Decoder, Encode, Encoder};
use rz_crypto::{hash, Address, Hash32};
use wasmi::{
    Caller, CompilationMode, Config, EnforcedLimits, Engine, Error, Extern, ExternType, Linker,
    Module, Store, StoreLimits, StoreLimitsBuilder, ValType,
};

/// Contexto do identificador de um módulo.
pub const MODULE_ID: &str = "rede-zero/module/v1";
/// Contexto da raiz da parte do Runtime no estado.
pub const RUNTIME_ROOT: &str = "rede-zero/runtime-root/v1";

pub const MAX_CODE: usize = 256 * 1024;
pub const MAX_METHOD: usize = 64;
pub const MAX_ARGS: usize = 16 * 1024;
pub const MAX_OUTPUT: usize = 16 * 1024;
pub const MAX_KEY: usize = 64;
pub const MAX_VALUE: usize = 4096;
/// Recibos guardados no estado (os mais recentes).
pub const MAX_RECEIPTS: usize = 4096;
/// Bytes da saída guardados no recibo.
pub const MAX_RECEIPT_OUTPUT: usize = 1024;
const MAX_ERROR: usize = 256;
/// Memória linear máxima de uma instância.
pub const MEMORY_LIMIT: usize = 4 << 20;
const MAX_TABLE_ELEMENTS: usize = 10_000;
const MAX_RECURSION: usize = 1024;

/// Custos de combustível das funções do host.
const FUEL_HOST_CALL: u64 = 500;
const FUEL_PER_BYTE_READ: u64 = 2;
const FUEL_STORAGE_WRITE: u64 = 2_000;
const FUEL_PER_BYTE_WRITTEN: u64 = 50;

/// Parâmetros do Runtime (no estado, alteráveis por governança constitucional).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeParams {
    /// Unidades de ZERO por milhão de combustível.
    pub fuel_price: u64,
    pub max_call_fuel: u64,
    pub max_block_fuel: u64,
    /// Unidades de ZERO por byte de módulo publicado (vão para o Pool).
    pub module_byte_fee: u64,
    /// Bytes de armazenamento por Comunidade (chaves + valores).
    pub storage_quota: u64,
}

impl Default for RuntimeParams {
    fn default() -> Self {
        Self {
            fuel_price: 100,
            max_call_fuel: 20_000_000,
            max_block_fuel: 200_000_000,
            module_byte_fee: 100,
            storage_quota: 1 << 20,
        }
    }
}

/// Teto absoluto de combustível por bloco, para que nenhum parâmetro torne
/// a validação de um bloco lenta demais.
pub const HARD_MAX_BLOCK_FUEL: u64 = 2_000_000_000;
pub const HARD_MAX_STORAGE: u64 = 16 << 20;

impl RuntimeParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_call_fuel == 0
            || self.max_call_fuel > self.max_block_fuel
            || self.max_block_fuel > HARD_MAX_BLOCK_FUEL
        {
            return Err("limites de combustível incoerentes");
        }
        if self.storage_quota == 0 || self.storage_quota > HARD_MAX_STORAGE {
            return Err("cota de armazenamento fora dos limites");
        }
        Ok(())
    }

    /// Taxa mínima, além de `min_fee`, para reservar `fuel` de combustível.
    pub fn fuel_fee(&self, fuel: u64) -> Option<u64> {
        let units = (fuel as u128).checked_mul(self.fuel_price as u128)?;
        u64::try_from(units.div_ceil(1_000_000)).ok()
    }
}

impl Encode for RuntimeParams {
    fn encode(&self, e: &mut Encoder) {
        e.u64(self.fuel_price)
            .u64(self.max_call_fuel)
            .u64(self.max_block_fuel)
            .u64(self.module_byte_fee)
            .u64(self.storage_quota);
    }
}

impl Decode for RuntimeParams {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            fuel_price: d.u64()?,
            max_call_fuel: d.u64()?,
            max_block_fuel: d.u64()?,
            module_byte_fee: d.u64()?,
            storage_quota: d.u64()?,
        })
    }
}

pub fn module_id(code: &[u8]) -> Hash32 {
    hash(MODULE_ID, code)
}

/// Nome de método: `[a-z0-9_]{1,64}`.
pub fn check_method(m: &str) -> Result<(), &'static str> {
    if m.is_empty()
        || m.len() > MAX_METHOD
        || !m
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
    {
        return Err("nome de método inválido");
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRecord {
    pub code: Vec<u8>,
    pub publisher: Address,
    pub published_at: u64,
}

/// Módulo vinculado a uma Comunidade. `seq` conta as vinculações e impede
/// reaproveitar aprovações antigas.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Binding {
    pub module: Option<Hash32>,
    pub seq: u32,
}

/// Resultado de uma chamada registrada no estado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub tx: Hash32,
    pub community: Hash32,
    pub method: String,
    pub ok: bool,
    pub fuel_used: u64,
    pub output: Vec<u8>,
    pub error: String,
    pub height: u64,
}

impl Encode for Receipt {
    fn encode(&self, e: &mut Encoder) {
        e.put(&self.tx)
            .put(&self.community)
            .str(&self.method)
            .bool(self.ok)
            .u64(self.fuel_used)
            .bytes(&self.output)
            .str(&self.error)
            .u64(self.height);
    }
}

impl Decode for Receipt {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            tx: d.get()?,
            community: d.get()?,
            method: d.str(MAX_METHOD)?,
            ok: d.bool()?,
            fuel_used: d.u64()?,
            output: d.bytes(MAX_RECEIPT_OUTPUT)?,
            error: d.str(MAX_ERROR)?,
            height: d.u64()?,
        })
    }
}

/// Assinado pelas chaves da Comunidade para vincular um módulo.
pub fn bind_payload(community: &Hash32, module: &Option<Hash32>, seq: u32) -> Vec<u8> {
    let mut e = Encoder::new();
    e.u8(3).put(community).option(module).u32(seq);
    e.into_bytes()
}

/// Parte do estado dedicada ao Runtime.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeState {
    pub modules: BTreeMap<Hash32, ModuleRecord>,
    pub bindings: BTreeMap<Hash32, Binding>,
    pub storage: BTreeMap<Hash32, BTreeMap<Vec<u8>, Vec<u8>>>,
    pub receipts: VecDeque<Receipt>,
}

impl RuntimeState {
    pub fn module_of(&self, community: &Hash32) -> Option<Hash32> {
        self.bindings.get(community).and_then(|b| b.module)
    }

    pub fn receipt(&self, tx: &Hash32) -> Option<&Receipt> {
        self.receipts.iter().rev().find(|r| r.tx == *tx)
    }

    pub fn usage(&self, community: &Hash32) -> u64 {
        self.storage
            .get(community)
            .map(|s| s.iter().map(|(k, v)| (k.len() + v.len()) as u64).sum())
            .unwrap_or(0)
    }

    pub fn push_receipt(&mut self, r: Receipt) {
        if self.receipts.len() >= MAX_RECEIPTS {
            self.receipts.pop_front();
        }
        self.receipts.push_back(r);
    }

    /// Aplica as escritas de uma chamada bem-sucedida.
    pub fn commit(&mut self, community: Hash32, writes: BTreeMap<Vec<u8>, Option<Vec<u8>>>) {
        let s = self.storage.entry(community).or_default();
        for (k, v) in writes {
            match v {
                Some(v) => {
                    s.insert(k, v);
                }
                None => {
                    s.remove(&k);
                }
            }
        }
        if s.is_empty() {
            self.storage.remove(&community);
        }
    }

    pub fn root(&self) -> Hash32 {
        let mut e = Encoder::new();
        e.u64(self.modules.len() as u64);
        for (id, m) in &self.modules {
            e.put(id).put(&m.publisher).u64(m.published_at);
        }
        e.u64(self.bindings.len() as u64);
        for (c, b) in &self.bindings {
            e.put(c).option(&b.module).u32(b.seq);
        }
        e.u64(self.storage.len() as u64);
        for (c, s) in &self.storage {
            e.put(c).u64(s.len() as u64);
            for (k, v) in s {
                e.bytes(k).bytes(v);
            }
        }
        e.u64(self.receipts.len() as u64);
        for r in &self.receipts {
            e.put(r);
        }
        hash(RUNTIME_ROOT, &e.into_bytes())
    }
}

// ------------------------------------------------------------- interpretador

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut c = Config::default();
        c.consume_fuel(true)
            .floats(false)
            .compilation_mode(CompilationMode::Eager)
            .enforced_limits(EnforcedLimits::strict())
            .set_max_recursion_depth(MAX_RECURSION)
            .allow_start_fn(false)
            .wasm_mutable_global(true)
            .wasm_sign_extension(true)
            .wasm_saturating_float_to_int(false)
            .wasm_multi_value(true)
            .wasm_bulk_memory(true)
            .wasm_reference_types(true)
            .wasm_extended_const(true)
            .wasm_multi_memory(false)
            .wasm_tail_call(false)
            .wasm_custom_page_sizes(false)
            .wasm_wide_arithmetic(false);
        Engine::new(&c)
    })
}

/// Funções que o host oferece (módulo `zero`) e suas assinaturas.
fn host_signature(name: &str) -> Option<(&'static [ValType], &'static [ValType])> {
    use ValType::{I32, I64};
    Some(match name {
        "input_len" => (&[], &[I32]),
        "input_read" => (&[I32], &[]),
        "caller" => (&[I32], &[]),
        "height" => (&[], &[I64]),
        "storage_get" => (&[I32, I32, I32, I32], &[I32]),
        "storage_set" => (&[I32, I32, I32, I32], &[]),
        "storage_remove" => (&[I32, I32], &[]),
        "output" => (&[I32, I32], &[]),
        "abort" => (&[I32, I32], &[]),
        _ => return None,
    })
}

/// Compila e verifica um módulo para publicação: sem ponto flutuante, só
/// importações do host conhecidas e com a assinatura exata, uma memória
/// exportada como `memory`.
pub fn validate_module(code: &[u8]) -> Result<(), String> {
    if code.is_empty() || code.len() > MAX_CODE {
        return Err("tamanho de módulo fora dos limites".into());
    }
    let m = Module::new(engine(), code).map_err(|e| format!("módulo inválido: {e}"))?;
    for imp in m.imports() {
        let ExternType::Func(f) = imp.ty() else {
            return Err("o módulo só pode importar funções do host".into());
        };
        let Some((params, results)) = (imp.module() == "zero")
            .then(|| host_signature(imp.name()))
            .flatten()
        else {
            return Err(format!(
                "importação desconhecida: {}.{}",
                imp.module(),
                imp.name()
            ));
        };
        if f.params() != params || f.results() != results {
            return Err(format!("assinatura incorreta de zero.{}", imp.name()));
        }
    }
    match m.get_export("memory") {
        Some(ExternType::Memory(_)) => Ok(()),
        _ => Err("o módulo deve exportar sua memória como 'memory'".into()),
    }
}

/// Módulos compilados. Cache local: não afeta o resultado, só evita
/// recompilar (a compilação é determinística e foi validada na publicação).
fn compiled(id: &Hash32, code: &[u8]) -> Result<Module, String> {
    static CACHE: OnceLock<Mutex<BTreeMap<Hash32, Module>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(m) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
        return Ok(m.clone());
    }
    let m = Module::new(engine(), code).map_err(|e| format!("módulo inválido: {e}"))?;
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if c.len() >= 32 {
        if let Some(k) = c.keys().next().copied() {
            c.remove(&k);
        }
    }
    c.insert(*id, m.clone());
    Ok(m)
}

/// Contexto de uma chamada.
pub struct Call<'a> {
    pub community: Hash32,
    pub caller: Address,
    pub height: u64,
    pub method: &'a str,
    pub input: &'a [u8],
    pub fuel: u64,
    pub quota: u64,
}

/// Resultado de uma execução. As escritas só valem se `ok`.
#[derive(Debug, Default)]
pub struct Outcome {
    pub ok: bool,
    pub fuel_used: u64,
    pub output: Vec<u8>,
    pub error: String,
    pub writes: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
}

struct Host {
    caller: Address,
    height: u64,
    input: Vec<u8>,
    /// Armazenamento da Comunidade antes da chamada.
    base: BTreeMap<Vec<u8>, Vec<u8>>,
    writes: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    usage: u64,
    quota: u64,
    output: Vec<u8>,
    abort: Option<String>,
    limits: StoreLimits,
}

impl Host {
    fn get(&self, k: &[u8]) -> Option<&Vec<u8>> {
        match self.writes.get(k) {
            Some(v) => v.as_ref(),
            None => self.base.get(k),
        }
    }
}

fn charge(c: &mut Caller<'_, Host>, fuel: u64) -> Result<(), Error> {
    let left = c.get_fuel()?;
    if left < fuel {
        c.set_fuel(0)?;
        return Err(Error::new("combustível esgotado"));
    }
    c.set_fuel(left - fuel)
}

fn memory(c: &Caller<'_, Host>) -> Result<wasmi::Memory, Error> {
    match c.get_export("memory") {
        Some(Extern::Memory(m)) => Ok(m),
        _ => Err(Error::new("memória ausente")),
    }
}

fn range(ptr: i32, len: i32, size: usize) -> Result<std::ops::Range<usize>, Error> {
    let (p, l) = (ptr as u32 as usize, len as u32 as usize);
    let end = p
        .checked_add(l)
        .filter(|e| *e <= size)
        .ok_or_else(|| Error::new("acesso fora da memória"))?;
    Ok(p..end)
}

fn read(c: &Caller<'_, Host>, ptr: i32, len: i32, max: usize) -> Result<Vec<u8>, Error> {
    if len as u32 as usize > max {
        return Err(Error::new("dado longo demais"));
    }
    let mem = memory(c)?;
    let data = mem.data(c);
    Ok(data[range(ptr, len, data.len())?].to_vec())
}

fn write(c: &mut Caller<'_, Host>, ptr: i32, bytes: &[u8]) -> Result<(), Error> {
    let mem = memory(c)?;
    let data = mem.data_mut(&mut *c);
    let r = range(ptr, bytes.len() as i32, data.len())?;
    data[r].copy_from_slice(bytes);
    Ok(())
}

fn linker() -> Result<Linker<Host>, Error> {
    let mut l = Linker::<Host>::new(engine());
    l.func_wrap("zero", "input_len", |c: Caller<'_, Host>| -> i32 {
        c.data().input.len() as i32
    })?;
    l.func_wrap(
        "zero",
        "input_read",
        |mut c: Caller<'_, Host>, ptr: i32| -> Result<(), Error> {
            let input = c.data().input.clone();
            charge(
                &mut c,
                FUEL_HOST_CALL + FUEL_PER_BYTE_READ * input.len() as u64,
            )?;
            write(&mut c, ptr, &input)
        },
    )?;
    l.func_wrap(
        "zero",
        "caller",
        |mut c: Caller<'_, Host>, ptr: i32| -> Result<(), Error> {
            charge(&mut c, FUEL_HOST_CALL)?;
            let a = c.data().caller;
            write(&mut c, ptr, &a.0 .0)
        },
    )?;
    l.func_wrap("zero", "height", |c: Caller<'_, Host>| -> i64 {
        c.data().height as i64
    })?;
    l.func_wrap(
        "zero",
        "storage_get",
        |mut c: Caller<'_, Host>, kp: i32, kl: i32, vp: i32, vcap: i32| -> Result<i32, Error> {
            let k = read(&c, kp, kl, MAX_KEY)?;
            let v = c.data().get(&k).cloned();
            match v {
                None => {
                    charge(&mut c, FUEL_HOST_CALL)?;
                    Ok(-1)
                }
                Some(v) => {
                    charge(&mut c, FUEL_HOST_CALL + FUEL_PER_BYTE_READ * v.len() as u64)?;
                    let n = v.len().min(vcap.max(0) as usize);
                    write(&mut c, vp, &v[..n])?;
                    Ok(v.len() as i32)
                }
            }
        },
    )?;
    l.func_wrap(
        "zero",
        "storage_set",
        |mut c: Caller<'_, Host>, kp: i32, kl: i32, vp: i32, vl: i32| -> Result<(), Error> {
            let k = read(&c, kp, kl, MAX_KEY)?;
            let v = read(&c, vp, vl, MAX_VALUE)?;
            if k.is_empty() {
                return Err(Error::new("chave vazia"));
            }
            charge(
                &mut c,
                FUEL_STORAGE_WRITE + FUEL_PER_BYTE_WRITTEN * (k.len() + v.len()) as u64,
            )?;
            let h = c.data_mut();
            let old = h.get(&k).map_or(0, |o| (k.len() + o.len()) as u64);
            let usage = h.usage - old + (k.len() + v.len()) as u64;
            if usage > h.quota {
                return Err(Error::new("cota de armazenamento da Comunidade excedida"));
            }
            h.usage = usage;
            h.writes.insert(k, Some(v));
            Ok(())
        },
    )?;
    l.func_wrap(
        "zero",
        "storage_remove",
        |mut c: Caller<'_, Host>, kp: i32, kl: i32| -> Result<(), Error> {
            let k = read(&c, kp, kl, MAX_KEY)?;
            charge(&mut c, FUEL_HOST_CALL)?;
            let h = c.data_mut();
            if let Some(o) = h.get(&k) {
                h.usage -= (k.len() + o.len()) as u64;
            }
            h.writes.insert(k, None);
            Ok(())
        },
    )?;
    l.func_wrap(
        "zero",
        "output",
        |mut c: Caller<'_, Host>, ptr: i32, len: i32| -> Result<(), Error> {
            let out = read(&c, ptr, len, MAX_OUTPUT)?;
            charge(
                &mut c,
                FUEL_HOST_CALL + FUEL_PER_BYTE_READ * out.len() as u64,
            )?;
            c.data_mut().output = out;
            Ok(())
        },
    )?;
    l.func_wrap(
        "zero",
        "abort",
        |mut c: Caller<'_, Host>, ptr: i32, len: i32| -> Result<(), Error> {
            let msg = read(&c, ptr, len.min(MAX_ERROR as i32), MAX_ERROR)?;
            let msg = String::from_utf8_lossy(&msg).into_owned();
            c.data_mut().abort = Some(msg.clone());
            Err(Error::new(msg))
        },
    )?;
    Ok(l)
}

/// Pilha nativa da thread que executa um módulo: fixa, para que o resultado
/// não dependa da thread chamadora (o Node, a validação, os testes).
const EXEC_STACK: usize = 16 << 20;

/// Executa `call.method` do módulo. Nunca entra em pânico: qualquer falha
/// vira um resultado `ok = false`, sem escritas.
pub fn execute(
    module: &Hash32,
    code: &[u8],
    storage: Option<&BTreeMap<Vec<u8>, Vec<u8>>>,
    call: &Call<'_>,
) -> Outcome {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("exonet-runtime".into())
            .stack_size(EXEC_STACK)
            .spawn_scoped(scope, || execute_here(module, code, storage, call))
            .ok()
            .and_then(|h| h.join().ok())
            .unwrap_or_else(|| Outcome {
                ok: false,
                error: "falha interna do executor".into(),
                ..Outcome::default()
            })
    })
}

fn execute_here(
    module: &Hash32,
    code: &[u8],
    storage: Option<&BTreeMap<Vec<u8>, Vec<u8>>>,
    call: &Call<'_>,
) -> Outcome {
    let fail = |error: String, fuel_used: u64| Outcome {
        ok: false,
        fuel_used,
        error: truncate(error),
        ..Outcome::default()
    };
    let m = match compiled(module, code) {
        Ok(m) => m,
        Err(e) => return fail(e, 0),
    };
    let base = storage.cloned().unwrap_or_default();
    let usage = base.iter().map(|(k, v)| (k.len() + v.len()) as u64).sum();
    let host = Host {
        caller: call.caller,
        height: call.height,
        input: call.input.to_vec(),
        base,
        writes: BTreeMap::new(),
        usage,
        quota: call.quota,
        output: Vec::new(),
        abort: None,
        limits: StoreLimitsBuilder::new()
            .memory_size(MEMORY_LIMIT)
            .table_elements(MAX_TABLE_ELEMENTS)
            .instances(1)
            .tables(1)
            .memories(1)
            .trap_on_grow_failure(false)
            .build(),
    };
    let mut store = Store::new(engine(), host);
    store.limiter(|h| &mut h.limits);
    if store.set_fuel(call.fuel).is_err() {
        return fail("combustível indisponível".into(), 0);
    }
    let used = |s: &Store<Host>| call.fuel.saturating_sub(s.get_fuel().unwrap_or(0));
    let linker = match linker() {
        Ok(l) => l,
        Err(e) => return fail(e.to_string(), 0),
    };
    let instance = match linker.instantiate_and_start(&mut store, &m) {
        Ok(i) => i,
        Err(e) => {
            let u = used(&store);
            return fail(format!("instanciação falhou: {}", stable_error(&e)), u);
        }
    };
    let func = match instance.get_typed_func::<(), ()>(&store, call.method) {
        Ok(f) => f,
        Err(_) => return fail(format!("método inexistente: {}", call.method), used(&store)),
    };
    let result = func.call(&mut store, ());
    let fuel_used = used(&store);
    let host = store.into_data();
    match result {
        Ok(()) => Outcome {
            ok: true,
            fuel_used,
            output: host.output,
            error: String::new(),
            writes: host.writes,
        },
        Err(e) => {
            let out_of_fuel = e.as_trap_code() == Some(wasmi::TrapCode::OutOfFuel)
                || e.to_string().contains("combustível esgotado");
            let msg = match host.abort {
                Some(m) => format!("abortado: {m}"),
                None if out_of_fuel => "combustível esgotado".into(),
                None => stable_error(&e),
            };
            // Quem esgota o combustível consome todo o reservado.
            fail(msg, if out_of_fuel { call.fuel } else { fuel_used })
        }
    }
}

/// Mensagem de erro estável: os recibos ficam no estado, então o texto não
/// pode depender de detalhes internos do interpretador.
fn stable_error(e: &Error) -> String {
    if let Some(code) = e.as_trap_code() {
        return format!("armadilha: {code:?}");
    }
    let msg = e.to_string();
    const OWN: [&str; 6] = [
        "acesso fora da memória",
        "dado longo demais",
        "chave vazia",
        "cota de armazenamento da Comunidade excedida",
        "memória ausente",
        "combustível esgotado",
    ];
    match OWN.iter().find(|m| msg.contains(**m)) {
        Some(m) => (*m).to_string(),
        None => "erro de execução".into(),
    }
}

fn truncate(mut s: String) -> String {
    if s.len() > MAX_ERROR {
        let mut cut = MAX_ERROR;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Módulo de exemplo usado nos testes do Runtime e do estado.
    pub const CONTADOR: &str = r#"
(module
  (import "zero" "storage_get" (func $get (param i32 i32 i32 i32) (result i32)))
  (import "zero" "storage_set" (func $set (param i32 i32 i32 i32)))
  (import "zero" "output" (func $out (param i32 i32)))
  (import "zero" "abort" (func $abort (param i32 i32)))
  (import "zero" "caller" (func $caller (param i32)))
  (import "zero" "input_len" (func $ilen (result i32)))
  (import "zero" "input_read" (func $iread (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "n")
  (data (i32.const 16) "falhou de proposito")
  (func (export "incrementar")
    (if (i32.eq (call $get (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8)) (i32.const -1))
      (then (i64.store (i32.const 8) (i64.const 0))))
    (i64.store (i32.const 8) (i64.add (i64.load (i32.const 8)) (i64.const 1)))
    (call $set (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8))
    (call $out (i32.const 8) (i32.const 8)))
  (func (export "ler")
    (drop (call $get (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 8)))
    (call $out (i32.const 8) (i32.const 8)))
  (func (export "incrementar_e_abortar")
    (call $set (i32.const 0) (i32.const 1) (i32.const 16) (i32.const 8))
    (call $abort (i32.const 16) (i32.const 19)))
  (func (export "girar") (loop $l (br $l)))
  (func (export "quem") (call $caller (i32.const 64)) (call $out (i32.const 64) (i32.const 32)))
  (func (export "eco") (call $iread (i32.const 128)) (call $out (i32.const 128) (call $ilen)))
  (func (export "encher") (local $i i32)
    (loop $l
      (i32.store16 (i32.const 1000) (local.get $i))
      (call $set (i32.const 1000) (i32.const 2) (i32.const 2048) (i32.const 4096))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 300)))))
  (func (export "fora") (call $out (i32.const 65530) (i32.const 100)))
  (func (export "crescer") (result) (drop (memory.grow (i32.const 1000))))
)"#;

    pub fn contador() -> Vec<u8> {
        wat::parse_str(CONTADOR).unwrap()
    }

    fn call<'a>(method: &'a str, input: &'a [u8], fuel: u64) -> Call<'a> {
        Call {
            community: Hash32([1; 32]),
            caller: Address(Hash32([0xca; 32])),
            height: 7,
            method,
            input,
            fuel,
            quota: 1 << 20,
        }
    }

    #[test]
    fn counter_runs_and_commits() {
        let code = contador();
        validate_module(&code).unwrap();
        let id = module_id(&code);
        let mut st = RuntimeState::default();
        for expected in 1u64..=3 {
            let o = execute(
                &id,
                &code,
                st.storage.get(&Hash32([1; 32])),
                &call("incrementar", &[], 1_000_000),
            );
            assert!(o.ok, "{}", o.error);
            assert_eq!(o.output, expected.to_le_bytes());
            assert!(o.fuel_used > 0);
            st.commit(Hash32([1; 32]), o.writes);
        }
        assert_eq!(st.usage(&Hash32([1; 32])), 9);
        let o = execute(
            &id,
            &code,
            st.storage.get(&Hash32([1; 32])),
            &call("quem", &[], 1_000_000),
        );
        assert_eq!(o.output, [0xca; 32]);
        let o = execute(&id, &code, None, &call("eco", b"ola", 1_000_000));
        assert_eq!(o.output, b"ola");
    }

    #[test]
    fn failures_do_not_write_and_are_deterministic() {
        let code = contador();
        let id = module_id(&code);
        let a = execute(
            &id,
            &code,
            None,
            &call("incrementar_e_abortar", &[], 1_000_000),
        );
        assert!(!a.ok);
        assert!(a.writes.is_empty());
        assert!(a.error.contains("falhou de proposito"), "{}", a.error);
        let b = execute(
            &id,
            &code,
            None,
            &call("incrementar_e_abortar", &[], 1_000_000),
        );
        assert_eq!((a.fuel_used, a.error.clone()), (b.fuel_used, b.error));
        // Laço infinito: termina por combustível, sempre no mesmo ponto.
        let g = execute(&id, &code, None, &call("girar", &[], 50_000));
        assert!(!g.ok);
        assert_eq!(g.fuel_used, 50_000);
        assert!(g.error.contains("combustível"), "{}", g.error);
        // Método inexistente, acesso fora da memória.
        assert!(!execute(&id, &code, None, &call("nada", &[], 10_000)).ok);
        assert!(!execute(&id, &code, None, &call("fora", &[], 10_000)).ok);
        // Crescer além do limite de memória falha de forma determinística (sem
        // pânico): memory.grow devolve -1.
        assert!(execute(&id, &code, None, &call("crescer", &[], 10_000)).ok);
    }

    /// Regressão: o despacho do interpretador não pode depender de
    /// otimização de chamada de cauda (estouraria a pilha nativa em laços
    /// longos em compilações sem otimização).
    #[test]
    fn long_loop_ends_by_fuel_without_native_overflow() {
        let code = contador();
        let id = module_id(&code);
        let o = execute(&id, &code, None, &call("girar", &[], 20_000_000));
        assert!(!o.ok);
        assert_eq!(o.fuel_used, 20_000_000);
        let e = execute(&id, &code, None, &call("encher", &[], 1_000_000));
        assert!(e.error.contains("combustível"), "{}", e.error);
    }

    #[test]
    fn storage_quota_enforced() {
        let code = contador();
        let id = module_id(&code);
        let mut c = call("encher", &[], 1_000_000_000);
        c.quota = 50_000;
        let o = execute(&id, &code, None, &c);
        assert!(!o.ok);
        assert!(o.error.contains("cota"), "{}", o.error);
    }

    #[test]
    fn modules_validated_on_publish() {
        let bad = [
            // Ponto flutuante: não determinístico entre plataformas.
            r#"(module (memory (export "memory") 1)
                (func (export "f") (drop (f32.add (f32.const 1) (f32.const 2)))))"#,
            // Importação fora do host: não existe acesso a saldos.
            r#"(module (import "env" "saldo" (func)) (memory (export "memory") 1))"#,
            r#"(module (import "zero" "transferir" (func)) (memory (export "memory") 1))"#,
            // Assinatura errada.
            r#"(module (import "zero" "height" (func (result i32))) (memory (export "memory") 1))"#,
            // Sem memória exportada.
            r#"(module (func (export "f")))"#,
            // Função de início.
            r#"(module (memory (export "memory") 1) (func $s) (start $s))"#,
        ];
        for w in bad {
            assert!(validate_module(&wat::parse_str(w).unwrap()).is_err(), "{w}");
        }
        assert!(validate_module(b"lixo").is_err());
        assert!(validate_module(&[]).is_err());
    }

    #[test]
    fn params_and_payloads() {
        let p = RuntimeParams::default();
        p.validate().unwrap();
        assert_eq!(p.fuel_fee(1_000_000), Some(100));
        assert_eq!(p.fuel_fee(1), Some(1));
        let mut q = p.clone();
        q.max_call_fuel = q.max_block_fuel + 1;
        assert!(q.validate().is_err());
        assert_ne!(
            bind_payload(&Hash32([1; 32]), &None, 0),
            bind_payload(&Hash32([1; 32]), &None, 1)
        );
        assert!(check_method("incrementar").is_ok());
        assert!(check_method("Incrementar").is_err());
        assert!(check_method("").is_err());
    }
}
