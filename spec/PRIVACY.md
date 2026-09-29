# spec/PRIVACY.md — Privacidade transacional

**Versão:** 0.1.0 (DEVNET)
**Relacionamento:** ADR-0009, REQ-023..028, `SPECIFICATIONS.md §31–32`, THR-PRIV-001, THR-TX-002
**Implementação de referência:** `crates/rz-privacy`, `crates/rz-core/src/private.rs`, `crates/rz-core/src/state.rs`

---

## 1. Grupo e geradores

* Grupo: **Ristretto255**. Pontos codificados em 32 bytes; decodificação rejeita codificações não canônicas.
* Escalares: 32 bytes little-endian, **canônicos** (`< ℓ`); não canônicos são rejeitados.
* `G` = ponto base de Ristretto255.
* `H = Hp(PEDERSEN_H, ∅)` — gerador de valores com logaritmo discreto desconhecido.

## 2. Funções de hash

```text
XOF(ctx, p₁…pₙ) = BLAKE3-derive-key(ctx) sobre (u64(|p₁|) ‖ p₁ ‖ … ‖ u64(|pₙ|) ‖ pₙ), 64 bytes
Hs(ctx, …)      = escalar( XOF(ctx, …) mod ℓ )
Hp(ctx, …)      = Ristretto-from-uniform-bytes( XOF(ctx, …) )
H8(ctx, …)      = primeiros 8 bytes de XOF(ctx, …)
```

| Contexto | Uso |
| --- | --- |
| `rede-zero/pedersen-h/v1` | gerador `H` |
| `rede-zero/shielded-view-key/v1` | chave de visualização a partir da semente |
| `rede-zero/shielded-spend-key/v1` | chave de gasto a partir da semente |
| `rede-zero/output-key/v1` | escalar da chave de uso único |
| `rede-zero/output-blinding/v1` | fator de ocultação da saída |
| `rede-zero/output-amount/v1` | máscara do valor cifrado |
| `rede-zero/key-image-base/v1` | base da imagem de chave `Hp(P)` |
| `rede-zero/clsag-agg-p/v1`, `…-agg-c/v1`, `…-round/v1` | CLSAG |
| `rede-zero/excess-proof/v1` | prova de excesso |
| `rede-zero/range-proof/v1` | transcrição Merlin do Bulletproof |
| `rede-zero/private-tx-id/v1` | identificador de transação privada |
| `rede-zero/private-tx-message/v1` | mensagem assinada pelos anéis |
| `rede-zero/shield-message/v1` | mensagem da prova de excesso da blindagem |
| `rede-zero/output-accumulator/v1`, `…/key-image-accumulator/v1` | acumuladores do estado |

## 3. Chaves e endereços

```text
semente (32 bytes, a mesma da chave transparente)
a = Hs(VIEW_KEY, semente)      A = a·G
b = Hs(SPEND_KEY, semente)     B = b·G
endereço privado = "zs" ‖ hex(A ‖ B)
```

## 4. Saídas (notas)

Criação para `(A, B)` com valor `v`:

```text
r ← aleatório           R = r·G
s = r·A                 (destinatário calcula s = a·R)
k = Hs(OUTPUT_KEY, enc(s))
P = k·G + B             chave de uso único
x = Hs(OUTPUT_BLINDING, enc(s))
C = v·H + x·G           compromisso
e = u64_be(v) ⊕ H8(OUTPUT_AMOUNT, enc(s))
π = Bulletproof(C, v ∈ [0, 2⁶⁴))
```

Formato na cadeia:

```text
OutputData     { one_time_key: P, tx_pub: R, commitment: C, enc_amount: e }   (104 bytes)
ShieldedOutput { data: OutputData, range_proof: bytes (≤ 1024) }
```

Detecção pelo destinatário: `P − k·G == B` e `C == v·H + x·G` após decifrar `v`. Chave de gasto de uso único: `p = k + b`.

## 5. Imagem de chave

```text
I = p · Hp(KEY_IMAGE_BASE, P)
```

Determinística por nota; revelada ao gastar. Uma imagem de chave repetida significa gasto duplo.

## 6. CLSAG

Anel `(P₀, C₀) … (Pₙ₋₁, Cₙ₋₁)`, pseudo-saída `C'`, imagem `I`, `D = z·Hp(P_l)`, mensagem `m`:

```text
T   = P₀ ‖ C₀ ‖ … ‖ Pₙ₋₁ ‖ Cₙ₋₁ ‖ C' ‖ I ‖ D
μP  = Hs(CLSAG_AGG_P, T)        μC = Hs(CLSAG_AGG_C, T)
Wᵢ  = μP·Pᵢ + μC·(Cᵢ − C')      W̃ = μP·I + μC·D
Lᵢ  = sᵢ·G + cᵢ·Wᵢ              Rᵢ = sᵢ·Hp(Pᵢ) + cᵢ·W̃
cᵢ₊₁ = Hs(CLSAG_ROUND, T ‖ m, Lᵢ, Rᵢ)
```

Assinatura: `(c₀, s₀…sₙ₋₁, D)`. Válida se, partindo de `c₀`, após `n` rodadas se obtém `c₀`. `I` deve ser diferente da identidade.

## 7. Transações

### 7.1 Blindagem (`TxKind::Shield`, tag `0x01`, transação de conta)

```text
Shield { amount: u64, outputs: list<ShieldedOutput> (1..16), excess: { R: fixed[32], s: fixed[32] } }
```

Validação, além das regras de conta (assinatura, nonce, saldo ≥ `amount + fee`):

1. `amount > 0`;
2. cada saída: `P` e `R` válidos, prova de faixa válida;
3. prova de excesso: `E = Σ Cⱼ − amount·H`; `s·G == R + Hs(EXCESS, E, R, msg)·E`, com `msg = H(SHIELD_MSG, remetente ‖ amount ‖ outputs)`.

Efeito: debita a conta, adiciona as saídas, `oferta_privada += amount`.

### 7.2 Transação privada (tag de transação `0x01`)

```text
PrivateTx {
  version    : u16                      // = 1
  inputs     : list<RingInput> (1..16)
  outputs    : list<ShieldedOutput> (0..16)
  unshield   : option<{ to: Address, amount: u64 }>
  fee        : u64
  signatures : list<CLSAG> (uma por entrada)
}
RingInput { ring: list<u64> (índices globais), key_image: fixed[32], pseudo_out: fixed[32] }

prefixo    = enc(version, inputs, outputs, unshield, fee)
TxId       = H(PRIVATE_TX_ID, prefixo)
mensagem m = H(PRIVATE_TX_MSG, string(network_id) ‖ prefixo)
```

Validação (ordem: estrutura barata → balanço → assinaturas → provas):

| # | Regra | Erro |
| --- | --- | --- |
| 1 | `version == 1`, `fee ≥ min_fee` | `UnsupportedVersion`, `FeeTooLow` |
| 2 | ≥ 1 entrada; assinaturas = entradas; ≥ 1 saída ou retirada; retirada > 0 | `InvalidPrivate`, `ZeroAmount` |
| 3 | `min(11, total_saídas) ≤ |anel| ≤ 11` | `InvalidPrivate` |
| 4 | índices estritamente crescentes e existentes | `InvalidPrivate` |
| 5 | imagens de chave distintas e ainda não gastas | `KeyImageSpent` |
| 6 | `Σ pseudo_out == Σ C_saída + (fee + retirada)·H` | `Balance` |
| 7 | cada CLSAG válida sobre `m` | `RingSignature` |
| 8 | cada saída com pontos válidos e prova de faixa válida | `RangeProof` |
| 9 | `oferta_privada ≥ fee + retirada` | `ShieldedSupplyUnderflow` |

Efeito: registra imagens de chave, adiciona saídas, credita a retirada à conta indicada, `oferta_privada −= fee + retirada`. A taxa é distribuída ao produtor como nas transações de conta.

## 8. Estado

```text
outputs          : lista append-only de OutputData (índice global = posição)
output_acc       : acc' = H(OUTPUT_ACC, acc ‖ enc(OutputData))
key_images       : conjunto de imagens gastas
key_image_acc    : acc' = H(KEY_IMAGE_ACC, acc ‖ I)
shielded_supply  : u64
```

A raiz do estado inclui `shielded_supply`, a quantidade e o acumulador de saídas, e a quantidade e o acumulador de imagens de chave (`spec/STATE.md §3`).

Invariante monetária:

```text
Σ saldos_transparentes + shielded_supply == total_supply
```

## 9. Seleção de disfarces (Wallet)

* Metade dos disfarces é escolhida entre as 100 saídas mais recentes, e metade uniformemente entre todas, para aproximar o padrão real de gasto (que favorece saídas recentes) e dificultar a heurística "o membro mais novo é o real".
* A ordem das saídas é embaralhada; sempre há saída de troco.
* Parâmetros de seleção não são consenso e podem evoluir sem alterar o protocolo.

## 10. O que é público e o que é oculto

| Informação | Transação privada | Blindagem | Retirada |
| --- | --- | --- | --- |
| Remetente | oculto no anel de 11 | **público** (conta) | oculto |
| Destinatário | oculto | oculto | **público** (conta) |
| Valor | oculto | **público** | **público** (retirada) / oculto (troco) |
| Taxa | pública | pública | pública |
| Número de entradas e saídas | público | público | público |

## 11. Riscos residuais (REQ-028)

* Análise estatística de anéis pode reduzir o conjunto de anonimato efetivo.
* Correlação temporal entre blindagem e gasto, ou entre gasto e retirada.
* Valores muito característicos na blindagem ou retirada.
* Comprometimento do dispositivo expõe a chave de visualização (histórico recebido) e a de gasto.
* Implementação não auditada.

## 12. Testes

| Teste | Onde |
| --- | --- |
| CLSAG em todas as posições; rejeição de mensagem, resposta, anel, imagem e pseudo-saída adulterados | `rz-privacy/src/clsag.rs` |
| Detecção de notas apenas pelo destinatário; saídas não ligáveis | `rz-privacy/src/note.rs` |
| Blindagem, transferência privada, retirada, gasto duplo, inflação, adulterações, rede errada | `rz-core/src/private.rs` |
| Fluxo completo entre Nodes reais e ausência de endereços e valor nos bytes publicados | `rz-wallet/tests/private_devnet.rs` |
