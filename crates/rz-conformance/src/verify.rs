//! Verificador de referência: executa cada vetor contra esta implementação
//! (`spec/CONFORMANCE.md §3`). Outra implementação faz o mesmo com o próprio
//! código; os resultados precisam coincidir.

use std::collections::BTreeMap;
use std::path::Path;

use rz_codec::{Decode, Encode};
use rz_core::bridge::{self, check_pow, Header, PowRules};
use rz_core::release::{file_hash, tree_hash, Release};
use rz_core::runtime::{execute, module_id, validate_module, Call};
use rz_core::{apply_block, Block, BlockId, Genesis, State, Transaction};
use rz_crypto::{hash, Address, Hash32, PublicKey, SecretKey, Signature};
use rz_p2p::Message;

use crate::format::{Case, Suite};
use crate::SUITES;

type R<T> = Result<T, String>;

/// Resultado de uma execução da suíte.
#[derive(Debug, Default)]
pub struct Report {
    /// `(arquivo, casos aprovados, casos no arquivo)`.
    pub suites: Vec<(String, usize, usize)>,
    pub failures: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.failures.is_empty() && !self.suites.is_empty()
    }
}

/// Executa todos os arquivos de `dir`. Um arquivo ausente é falha.
pub fn verify(dir: &Path) -> Report {
    let mut report = Report::default();
    for (file, _) in SUITES {
        let text = match std::fs::read_to_string(dir.join(file)) {
            Ok(t) => t,
            Err(e) => {
                report.failures.push(format!("{file}: {e}"));
                continue;
            }
        };
        let suite = match Suite::parse(&text) {
            Ok(s) => s,
            Err(e) => {
                report.failures.push(format!("{file}: {e}"));
                continue;
            }
        };
        let results = run(file, &suite.cases);
        let passed = results.iter().filter(|r| r.is_ok()).count();
        for (case, r) in suite.cases.iter().zip(&results) {
            if let Err(e) = r {
                report.failures.push(format!("{file} [{}]: {e}", case.name));
            }
        }
        if suite.cases.is_empty() {
            report.failures.push(format!("{file}: nenhum caso"));
        }
        report
            .suites
            .push((file.to_string(), passed, suite.cases.len()));
    }
    report
}

fn run(file: &str, cases: &[Case]) -> Vec<R<()>> {
    match file {
        "cadeia.txt" => chain(cases),
        "runtime.txt" => runtime(cases),
        _ => cases
            .iter()
            .map(|c| match file {
                "identidade.txt" => identity(c),
                "hash.txt" => hashes(c),
                "zero.txt" => zero(c),
                "codificacao.txt" => encoding(c),
                "transacoes.txt" => transaction(c),
                "p2p.txt" => p2p(c),
                "governanca.txt" => governance(c),
                "ponte.txt" => bridge_case(c),
                "versoes.txt" => release(c),
                other => Err(format!("conjunto desconhecido: {other}")),
            })
            .collect(),
    }
}

fn eq<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> R<()> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: obtido {got:?}, esperado {want:?}"))
    }
}

fn expect_ok(c: &Case) -> R<bool> {
    match c.get("resultado")? {
        "ok" => Ok(true),
        "rejeitado" => Ok(false),
        other => Err(format!("resultado desconhecido: {other}")),
    }
}

fn h32(c: &Case, key: &str) -> R<Hash32> {
    let b = c.bytes(key)?;
    <[u8; 32]>::try_from(b.as_slice())
        .map(Hash32)
        .map_err(|_| format!("'{key}' deve ter 32 bytes"))
}

// ----------------------------------------------------------- identidade

fn identity(c: &Case) -> R<()> {
    let pk_bytes = h32(c, "chave_publica")?.0;
    if c.opt("semente").is_some() {
        let sk = SecretKey::from_seed(h32(c, "semente")?.0);
        let pk = sk.public_key();
        eq("chave pública", *pk.as_bytes(), pk_bytes)?;
        eq("node_id", pk.node_id().0, h32(c, "node_id")?)?;
        eq("endereço", pk.address().0, h32(c, "endereco")?)?;
    }
    let pk = PublicKey::from_bytes(pk_bytes).map_err(|e| e.to_string())?;
    let sig = Signature(
        <[u8; 64]>::try_from(c.bytes("assinatura")?.as_slice())
            .map_err(|_| "assinatura deve ter 64 bytes")?,
    );
    let valid = pk
        .verify(
            c.get("contexto")?,
            c.get("rede")?,
            &c.bytes("mensagem")?,
            &sig,
        )
        .is_ok();
    eq("assinatura válida", valid, c.get("valida")? == "sim")
}

// ----------------------------------------------------------------- hash

fn hashes(c: &Case) -> R<()> {
    let input = c.bytes("entrada")?;
    let got = match c.get("contexto")? {
        "sha256d" => bridge::sha256d(&input).to_vec(),
        ctx => hash(ctx, &input).0.to_vec(),
    };
    eq("hash", got, c.bytes("saida")?)
}

// ----------------------------------------------------------------- ZERO

fn zero(c: &Case) -> R<()> {
    match c.get("operacao")? {
        "ler" => {
            let got = rz_core::parse_zero(c.text("texto")?);
            match expect_ok(c)? {
                true => eq("unidades", got, Some(c.u64("unidades")?)),
                false => eq("rejeição", got, None),
            }
        }
        "escrever" => eq(
            "texto",
            rz_core::format_zero(c.u64("unidades")?),
            c.text("texto")?.to_string(),
        ),
        other => Err(format!("operação desconhecida: {other}")),
    }
}

// ------------------------------------------------------- codificação

fn decode_check<T: Decode + Encode>(bytes: &[u8]) -> R<T> {
    let v = T::from_canonical_bytes(bytes).map_err(|e| e.to_string())?;
    // Unicidade: reencodar devolve exatamente os mesmos bytes.
    if v.to_canonical_bytes() != bytes {
        return Err("codificação não canônica aceita".into());
    }
    Ok(v)
}

fn encoding(c: &Case) -> R<()> {
    let bytes = c.bytes("bytes")?;
    let id: R<Vec<u8>> = match c.get("tipo")? {
        "transacao" => decode_check::<Transaction>(&bytes).map(|t| t.id().0 .0.to_vec()),
        "genesis" => decode_check::<Genesis>(&bytes).and_then(|g| {
            g.validate().map_err(|e| e.to_string())?;
            Ok(g.hash().0.to_vec())
        }),
        "versao" => decode_check::<Release>(&bytes).map(|r| r.id().0.to_vec()),
        other => return Err(format!("tipo desconhecido: {other}")),
    };
    match (expect_ok(c)?, id) {
        (true, Ok(id)) => eq("id", id, c.bytes("id")?),
        (true, Err(e)) => Err(format!("rejeitado indevidamente: {e}")),
        (false, Ok(_)) => Err("aceito indevidamente".into()),
        (false, Err(_)) => Ok(()),
    }
}

// ----------------------------------------------------------- transações

fn transaction(c: &Case) -> R<()> {
    let tx = decode_check::<Transaction>(&c.bytes("bytes")?)?;
    eq("id", tx.id().0 .0.to_vec(), c.bytes("id")?)?;
    let (min_fee, net) = (c.u64("taxa_minima")?, c.get("rede")?);
    let check = match &tx {
        Transaction::Account(a) => a
            .check_stateless(min_fee)
            .and_then(|()| a.verify_signature(net))
            .map_err(|e| e.to_string()),
        Transaction::Private(_) => Err("vetor de transação privada fora deste conjunto".into()),
    };
    match (expect_ok(c)?, check) {
        (true, Err(e)) => Err(format!("rejeitada indevidamente: {e}")),
        (false, Ok(())) => Err("aceita indevidamente".into()),
        _ => Ok(()),
    }
}

// ----------------------------------------------------------------- cadeia

/// Os casos são aplicados em ordem: o primeiro é o Genesis; cada bloco
/// válido avança a cadeia; um bloco inválido é testado sobre a ponta atual e
/// descartado.
fn chain(cases: &[Case]) -> Vec<R<()>> {
    let mut out = Vec::with_capacity(cases.len());
    let Some((first, rest)) = cases.split_first() else {
        return out;
    };
    let start = (|| -> R<(Genesis, State)> {
        let g = decode_check::<Genesis>(&first.bytes("bytes")?)?;
        g.validate().map_err(|e| e.to_string())?;
        eq("id do genesis", g.hash().0.to_vec(), first.bytes("id")?)?;
        let s = State::from_genesis(&g).map_err(|e| e.to_string())?;
        eq("raiz do estado", s.root(), h32(first, "raiz_do_estado")?)?;
        Ok((g, s))
    })();
    let (g, mut state) = match start {
        Ok(v) => {
            out.push(Ok(()));
            v
        }
        Err(e) => {
            out.push(Err(e));
            out.extend(rest.iter().map(|_| Err("genesis inválido".to_string())));
            return out;
        }
    };
    let mut parent = BlockId(g.hash());
    let mut height = 0u64;
    for c in rest {
        let r = (|| -> R<Option<(BlockId, State)>> {
            let bytes = c.bytes("bytes")?;
            let applied = Block::from_canonical_bytes(&bytes)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    apply_block(&g, parent, height, &state, &b)
                        .map(|s| (b.id(), s))
                        .map_err(|e| e.to_string())
                });
            match (expect_ok(c)?, applied) {
                (true, Ok((id, s))) => {
                    eq("id do bloco", id.0, h32(c, "id")?)?;
                    eq("raiz do estado", s.root(), h32(c, "raiz_do_estado")?)?;
                    Ok(Some((id, s)))
                }
                (true, Err(e)) => Err(format!("bloco rejeitado indevidamente: {e}")),
                (false, Ok(_)) => Err("bloco inválido aceito".into()),
                (false, Err(_)) => Ok(None),
            }
        })();
        match r {
            Ok(Some((id, s))) => {
                parent = id;
                height += 1;
                state = s;
                out.push(Ok(()));
            }
            Ok(None) => out.push(Ok(())),
            Err(e) => {
                // Sem o estado deste bloco, os seguintes não podem ser
                // verificados.
                out.push(Err(e));
                let n = cases.len() - out.len();
                out.extend((0..n).map(|_| Err("bloco anterior divergiu".to_string())));
                return out;
            }
        }
    }
    out
}

// ------------------------------------------------------------------ P2P

fn p2p(c: &Case) -> R<()> {
    let r = decode_check::<Message>(&c.bytes("bytes")?);
    match (expect_ok(c)?, r) {
        (true, Ok(m)) => eq("nome", m.kind(), c.get("nome")?),
        (true, Err(e)) => Err(format!("rejeitada indevidamente: {e}")),
        (false, Ok(_)) => Err("aceita indevidamente".into()),
        (false, Err(_)) => Ok(()),
    }
}

// ------------------------------------------------------------ governança

fn governance(c: &Case) -> R<()> {
    match c.get("operacao")? {
        "decaimento" => eq(
            "decaimento",
            rz_core::governance::decay(c.u64("valor")?, c.u64("idade")?, c.u64("meia_vida")?),
            c.u64("resultado")?,
        ),
        other => Err(format!("operação desconhecida: {other}")),
    }
}

// --------------------------------------------------------------- Runtime

fn runtime(cases: &[Case]) -> Vec<R<()>> {
    let mut modules: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    cases
        .iter()
        .map(|c| match c.get("operacao")? {
            "validar" => {
                let m = c.bytes("modulo")?;
                eq("id do módulo", module_id(&m), h32(c, "id")?)?;
                let ok = validate_module(&m).is_ok();
                modules.insert(c.name.clone(), m);
                eq("módulo aceito", ok, expect_ok(c)?)
            }
            "executar" => {
                let name = c.get("modulo")?;
                let code = modules
                    .get(name)
                    .ok_or_else(|| format!("módulo '{name}' não definido antes"))?;
                let mut storage = BTreeMap::new();
                for kv in c.all("armazenamento") {
                    let (k, v) = kv.split_once(':').ok_or("armazenamento: chave:valor")?;
                    storage.insert(
                        rz_crypto::hex::decode(k).ok_or("hex")?,
                        rz_crypto::hex::decode(v).ok_or("hex")?,
                    );
                }
                let input = c.bytes("entrada")?;
                let call = Call {
                    community: h32(c, "comunidade")?,
                    caller: Address(h32(c, "chamador")?),
                    height: c.u64("altura")?,
                    method: c.get("metodo")?,
                    input: &input,
                    fuel: c.u64("combustivel")?,
                    quota: c.u64("cota")?,
                };
                let base = (!storage.is_empty()).then_some(&storage);
                let o = execute(&module_id(code), code, base, &call);
                eq("sucesso", o.ok, c.get("resultado")? == "ok")?;
                eq(
                    "combustível usado",
                    o.fuel_used,
                    c.u64("combustivel_usado")?,
                )?;
                eq("saída", o.output, c.bytes("saida")?)?;
                if !o.ok {
                    eq("erro", o.error.as_str(), c.text("erro")?)?;
                }
                let mut writes = Vec::new();
                for (k, v) in &o.writes {
                    writes.push(format!(
                        "{}:{}",
                        rz_crypto::hex::encode(k),
                        v.as_ref().map_or("-".into(), |v| rz_crypto::hex::encode(v))
                    ));
                }
                eq(
                    "escritas",
                    writes,
                    c.all("escrita").iter().map(|s| s.to_string()).collect(),
                )
            }
            other => Err(format!("operação desconhecida: {other}")),
        })
        .collect()
}

// ------------------------------------------------------------------ ponte

fn bridge_case(c: &Case) -> R<()> {
    match c.get("operacao")? {
        "compacto" => {
            let bits = u32::from_str_radix(c.get("bits")?, 16).map_err(|_| "bits")?;
            match (expect_ok(c)?, bridge::U256::from_compact(bits)) {
                (true, Some(t)) => eq("alvo", t.to_be().to_vec(), c.bytes("alvo")?),
                (false, None) => Ok(()),
                (want, got) => Err(format!("esperado ok={want}, obtido {got:?}")),
            }
        }
        "cabecalho" => {
            let raw = <[u8; 80]>::try_from(c.bytes("bytes")?.as_slice())
                .map_err(|_| "cabeçalho deve ter 80 bytes")?;
            let h = Header::parse(&raw);
            eq("hash", h.hash(), h32(c, "hash")?)?;
            let rules = PowRules {
                pow_limit: h32(c, "limite")?,
                retarget: false,
                interval: 2016,
                target_timespan: 1_209_600,
            };
            eq(
                "trabalho válido",
                check_pow(&h, &rules).is_ok(),
                expect_ok(c)?,
            )
        }
        "queima" => eq(
            "script",
            bridge::burn_script(c.get("rede")?, &Address(h32(c, "destinatario")?)),
            c.bytes("script")?,
        ),
        "merkle" => {
            let path = c
                .all("caminho")
                .iter()
                .map(|p| {
                    rz_crypto::hex::decode_array::<32>(p)
                        .map(Hash32)
                        .ok_or_else(|| "caminho".to_string())
                })
                .collect::<R<Vec<_>>>()?;
            let idx = u32::try_from(c.u64("indice")?).map_err(|_| "índice")?;
            eq(
                "raiz",
                bridge::merkle_root(&h32(c, "folha")?, &path, idx),
                Some(h32(c, "raiz")?),
            )
        }
        other => Err(format!("operação desconhecida: {other}")),
    }
}

// ---------------------------------------------------------------- versões

fn release(c: &Case) -> R<()> {
    match c.get("operacao")? {
        "manifesto" => {
            let r = decode_check::<Release>(&c.bytes("bytes")?)?;
            eq("id", r.id(), h32(c, "id")?)
        }
        "arquivo" => eq("hash", file_hash(&c.bytes("bytes")?), h32(c, "hash")?),
        "arvore" => {
            let mut files = BTreeMap::new();
            for f in c.all("arquivo") {
                let (p, b) = f.rsplit_once(':').ok_or("arquivo: caminho:hex")?;
                files.insert(p.to_string(), rz_crypto::hex::decode(b).ok_or("hex")?);
            }
            eq("hash", tree_hash(&files), h32(c, "hash")?)
        }
        other => Err(format!("operação desconhecida: {other}")),
    }
}
