# ADR-0004 — Codificação canônica binária própria

**Estado:** Aceita
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** SPEC §8, SPEC-001, THR-TX-004, THR-TX-005, THR-P2P-003

## Contexto

`SPEC §8` exige que dois Nodes nunca interpretem o mesmo objeto válido como sequências de bytes diferentes. Formatos genéricos (JSON, CBOR, Protobuf) admitem múltiplas codificações para o mesmo valor, a menos que se adote um subconjunto canônico estrito.

## Decisão

Codificação binária própria, mínima e totalmente especificada em `spec/ENCODING.md`:

* inteiros sem sinal de tamanho fixo em **big-endian**;
* sequências de bytes e listas com prefixo de comprimento `u32`;
* booleanos como `0x00`/`0x01` — qualquer outro valor é inválido;
* campos em ordem fixa definida pela especificação de cada objeto;
* nenhum mapa, nenhum campo opcional implícito;
* opcionais explícitos com tag `0x00` (ausente) / `0x01` (presente);
* decodificação **estrita**: bytes restantes, comprimentos excessivos ou valores fora do domínio são erros;
* limites máximos de comprimento verificados **antes** de alocar memória (THR-P2P-002).

## Alternativas consideradas

* **Serde + bincode** — conveniente, mas o formato depende da biblioteca e da versão, dificultando implementações independentes.
* **CBOR determinístico** — viável, porém mais complexo de implementar corretamente em todas as linguagens.

## Consequências

* Qualquer implementação pode reproduzir o formato a partir da especificação.
* O codificador é escrito manualmente, o que exige testes de ida e volta e fuzzing.
