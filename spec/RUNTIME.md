# spec/RUNTIME.md — Exonet Runtime

**Versão:** 0.1.0 (DEVNET)
**Decisão:** ADR-0018
**Relacionamento:** `SPECIFICATIONS.md §60–§62`, `ARCHITECTURE.md §24–§25`, `§57` (Comunidades: execução, armazenamento, atualização), REQ-068..074, AT-COM-003, `ACCEPTANCE_TESTS.md §73`, THR-COM-001, THR-TX-005, INV-007
**Implementação de referência:** `crates/rz-core/src/runtime.rs`, `crates/rz-core/src/state.rs`

---

## 1. Modelo

A lógica de uma Comunidade é um **módulo WebAssembly** publicado na cadeia e vinculado a ela pela sua regra de decisão. Chamadas são transações executadas por todos os validadores, de forma determinística, com combustível medido e pago. Cada Comunidade tem um armazenamento próprio de chave e valor; o módulo só alcança esse armazenamento.

```text
Navegador / Wallet ──CallModule──▶ bloco ──▶ módulo da Comunidade C ──▶ armazenamento de C
                    ──QUERY──────▶ Node  ──▶ (execução somente leitura, nada gravado)
```

## 2. Estado

```text
RuntimeState {
  modules  : mapa id → ModuleRecord { code: bytes (≤256 KiB), publisher: Address, published_at: u64 }
  bindings : mapa comunidade → Binding { module: option<fixed[32]>, seq: u32 }
  storage  : mapa comunidade → mapa chave (≤64 bytes) → valor (≤4096 bytes)
  receipts : lista<Receipt> (as 4096 mais recentes)
}
module_id = H("rede-zero/module/v1", code)
Receipt { tx, community, method, ok: bool, fuel_used: u64, output: bytes (≤1024), error: string (≤256), height }
runtime_root = H("rede-zero/runtime-root/v1", módulos (id, publisher, published_at) ‖ vínculos ‖ armazenamento ‖ recibos)
```

`runtime_root` entra na raiz do estado depois de `defense_root` (`spec/STATE.md`).

## 3. Operações

| Tag | Operação | Débito | Regras |
| --- | --- | --- | --- |
| `0x60` | `PublishModule { code }` | taxa + `len(code) · module_byte_fee` (vai para o Pool) | módulo ainda não publicado; validado (§4) |
| `0x61` | `BindModule { community, module?, approvals }` | taxa | Comunidade **reconhecida**; módulo publicado (ou `None` para desvincular); aprovações da regra vigente sobre `bind_payload` |
| `0x62` | `CallModule { community, method, args (≤16 KiB), fuel }` | taxa ≥ `min_fee + fuel_fee(fuel)` | `method` em `[a-z0-9_]{1,64}`; `0 < fuel ≤ max_call_fuel`; Comunidade com módulo vinculado |

```text
bind_payload = u8(3) ‖ community ‖ option(module) ‖ u32(seq)       // assinado com ctx "rede-zero/community/v1"
fuel_fee(f)  = ⌈f · fuel_price / 1 000 000⌉
```

`seq` aumenta a cada vinculação: uma aprovação antiga nunca é reaproveitada.

**Execução de uma chamada** (na aplicação do bloco):

1. A taxa e o nonce são cobrados, e a transação é sempre incluída.
2. O módulo vinculado roda o método exportado `method` (assinatura `() → ()`) com o combustível reservado.
3. Sucesso: as escritas no armazenamento da Comunidade são aplicadas.
4. Qualquer falha (aborto, armadilha, combustível esgotado, cota excedida, método inexistente) **não grava nada**.

Em todos os casos é registrado um recibo. Quem esgota o combustível consome todo o reservado. Uma falha dentro do módulo nunca torna o bloco inválido.

**Por bloco:** a soma de `fuel` das chamadas não passa de `max_block_fuel` (`BlockError::FuelExceeded`).

## 4. Validação de módulos

* WebAssembly válido, até 256 KiB;
* **sem ponto flutuante** (não determinístico entre plataformas) e sem SIMD, memória de 64 bits, múltiplas memórias ou chamadas de cauda;
* sem função de início;
* importações só do módulo `zero` (§5), com a assinatura exata;
* exporta sua memória como `memory`;
* limites de estrutura estritos do interpretador (`EnforcedLimits::strict`).

Na execução: memória até 4 MiB (crescer além disso devolve `-1`), uma instância, uma tabela de até 10 000 elementos, recursão até 1 024 níveis. Cada chamada roda numa thread com pilha nativa fixa, independente de quem chama.

## 5. Interface do host (módulo `zero`)

| Função | Assinatura | Efeito | Combustível extra |
| --- | --- | --- | --- |
| `input_len` | `() → i32` | tamanho dos argumentos | — |
| `input_read` | `(ptr)` | copia os argumentos para a memória | 500 + 2/byte |
| `caller` | `(ptr)` | 32 bytes do endereço de quem assinou a chamada (zeros numa consulta sem remetente) | 500 |
| `height` | `() → i64` | altura do bloco | — |
| `storage_get` | `(kptr, klen, vptr, vcap) → i32` | `-1` se ausente; senão o tamanho do valor, copiando até `vcap` bytes | 500 + 2/byte |
| `storage_set` | `(kptr, klen, vptr, vlen)` | grava (chave não vazia ≤ 64, valor ≤ 4096); respeita `storage_quota` | 2 000 + 50/byte |
| `storage_remove` | `(kptr, klen)` | remove | 500 |
| `output` | `(ptr, len)` | define a saída (≤ 16 KiB) | 500 + 2/byte |
| `abort` | `(ptr, len)` | encerra com a mensagem; nada é gravado | — |

**Não existe** função para ler ou mover saldos, criar ZERO, votar, alterar parâmetros, validadores, identidades de Nodes ou outra Comunidade (`SPEC §61`). Um módulo que tente importar algo assim é recusado na publicação (teste §73).

## 6. Parâmetros (`ProtocolParams.runtime`)

| Campo | Padrão | Governança (constitucional) |
| --- | --- | --- |
| `fuel_price` | 100 unidades por milhão de combustível | `runtime_fuel_price` (tag 27) |
| `max_call_fuel` | 20 000 000 | `runtime_max_call_fuel` (tag 28) |
| `max_block_fuel` | 200 000 000 (teto absoluto 2 000 000 000) | `runtime_max_block_fuel` (tag 29) |
| `module_byte_fee` | 100 unidades por byte | `runtime_module_byte_fee` (tag 30) |
| `storage_quota` | 1 MiB por Comunidade (teto 16 MiB) | `runtime_storage_quota` (tag 31) |

## 7. Consultas e recibos (P2P)

| Mensagem | Uso |
| --- | --- |
| `QUERY {community, method, args, caller?}` → `QUERY_RESULT {height, ok, fuel_used, output, error}` | execução somente leitura sobre o estado atual, até 10 000 000 de combustível; balde de 16, recarga 4/s por conexão |
| `GET_RECEIPT tx` → `RECEIPT {height, receipt?}` | resultado de uma chamada |
| `GET_MODULE_INFO community` → `MODULE_INFO {height, community, module?, seq, usage}` | vínculo e uso de armazenamento (o `seq` é necessário para aprovar uma vinculação) |

No Navegador Zero (`spec/BROWSER.md §5`), uma publicação usa:

* `POST /.zero/consulta` (`metodo`, `args` ou `args_hex`, `comunidade`);
* `GET /.zero/recibo/TX`;
* pedidos `tipo=chamada` (`metodo`, `args`/`args_hex`, `combustivel`, `comunidade`), aprovados na interface.

Numa publicação `NOME.comunidade`, `comunidade` é ela mesma por padrão.

## 8. Determinismo e versão

O resultado depende só do estado, da transação e da versão do interpretador:

* `wasmi` 2.0.0, fixado com `=` e no `Cargo.lock`;
* recursos `deterministic` e `portable-dispatch`, que não dependem de otimização do compilador.

Trocar o interpretador ou a tabela de combustível muda o protocolo e exige uma versão de software aprovada pela governança (`spec/GOVERNANCE.md`). O cache de módulos compilados é local e não afeta resultados.

## 9. Testes

* `runtime::tests`: execução, aborto sem escrita, combustível, laço longo sem estouro de pilha, cota, validação de publicação, parâmetros.
* `governance_tests`:
  * `runtime_publish_bind_call_query`, `runtime_failed_call_pays_fee_and_writes_nothing`, `runtime_binding_rules`;
  * `runtime_fees_and_fuel_limits`, `test_73_malicious_community_module_isolated`, `runtime_block_replay_is_deterministic`.
* `runtime_devnet`: Nodes reais, Wallet, Navegador e o exemplo `examples/runtime/contador.wat`.
* `robustness.rs`.
