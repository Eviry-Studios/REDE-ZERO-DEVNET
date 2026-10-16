# spec/ENCODING.md — Codificação canônica

**Versão:** 0.1.0 (DEVNET)
**Relacionamento:** `SPECIFICATIONS.md §8`, ADR-0004, THR-TX-004, THR-TX-005, THR-P2P-002, THR-P2P-003
**Implementação de referência:** `crates/rz-codec`

---

## 1. Objetivo

Definir a única representação em bytes de todo objeto protocolar que participa de hashes, assinaturas, consenso ou mensagens P2P.

Regra fundamental:

> Para todo valor válido `v`, existe **exatamente uma** sequência de bytes `enc(v)`. Qualquer outra sequência é rejeitada na decodificação.

## 2. Tipos primitivos

| Tipo | Codificação | Tamanho |
| --- | --- | --- |
| `u8` | 1 byte | 1 |
| `u16` | big-endian | 2 |
| `u32` | big-endian | 4 |
| `u64` | big-endian | 8 |
| `bool` | `0x00` = falso, `0x01` = verdadeiro | 1 |
| `fixed[N]` | N bytes brutos, sem prefixo | N |
| `bytes` | `u32` comprimento ‖ bytes | 4 + n |
| `string` | igual a `bytes`, conteúdo UTF-8 válido | 4 + n |
| `list<T>` | `u32` quantidade ‖ enc(item₁) ‖ … ‖ enc(itemₙ) | variável |
| `option<T>` | `0x00` (ausente) ou `0x01` ‖ enc(valor) | 1 ou 1 + enc |

Não existem inteiros com sinal, ponto flutuante, mapas nem inteiros de tamanho variável.

## 3. Estruturas

Uma estrutura é a concatenação das codificações de seus campos, **na ordem definida pela especificação do objeto**. Não há nomes de campos, delimitadores nem preenchimento.

Enumerações (variantes) são codificadas como `u8` de tag seguido dos campos da variante. Tags não definidas são inválidas.

## 4. Regras de decodificação

Um decodificador conforme **deve** rejeitar:

| Caso | Erro |
| --- | --- |
| Entrada termina antes do fim do objeto | `UnexpectedEnd` |
| Sobram bytes após o objeto | `TrailingBytes` |
| `bool` diferente de `0x00`/`0x01` | `InvalidBool` |
| Tag de `option` ou variante desconhecida | `InvalidTag` |
| Comprimento declarado maior que o limite do campo | `LengthExceeded` |
| `string` com UTF-8 inválido | `InvalidValue` |
| Valor fora do domínio do tipo (ex.: chave pública inválida) | `InvalidValue` |

Requisitos adicionais:

* o limite de comprimento é verificado **antes** de ler ou alocar o corpo;
* o decodificador não pode pré-alocar memória proporcional a um comprimento declarado e não verificado;
* entrada malformada nunca pode provocar término anormal do processo.

## 5. Vetores de teste

| Valor | Bytes (hex) |
| --- | --- |
| `u8(1), u16(0x0203), u32(0x04050607), u64(0x08090a0b0c0d0e0f)` | `0102030405060708090a0b0c0d0e0f` |
| `bytes("abc")` | `00000003616263` |
| `bool(true)` | `01` |
| `option<u32>(None)` | `00` |
| `option<u32>(Some(7))` | `0100000007` |
| `list<u16>([1,2,3])` | `00000003000100020003` |

Vetores negativos:

| Entrada | Tipo esperado | Resultado |
| --- | --- | --- |
| `0000000100` | `u32` | `TrailingBytes(1)` |
| `000000` | `u64` | `UnexpectedEnd` |
| `02` | `bool` | `InvalidBool(2)` |
| `ffffffff` | `bytes` (máx. 1024) | `LengthExceeded` |
| `0000000309` | `list<u8>` | `UnexpectedEnd` |
