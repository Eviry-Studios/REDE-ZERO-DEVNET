# ADR-0005 — Modelo de contas com nonce e unidade mínima do ZERO

**Estado:** Aceita — complementada pela ADR-0009 (camada privada)
**Escopo:** DEVNET
**Data:** 2026-09-29
**Relacionamento:** SPEC §6, §11, §33, §34, §36, THR-ID-004, THR-TX-001, THR-TX-003, THR-PRIV-001

## Contexto

É preciso escolher entre modelo de contas e modelo UTXO, e definir `N` em `1 ZERO = N unidades mínimas` (`SPEC §33`).

## Decisão

* **Modelo de contas**: o estado mapeia `endereço → (saldo, nonce)`.
* Cada transação carrega o `nonce` esperado da conta remetente; ele é incrementado exatamente em 1 a cada transação aplicada. Isso impede replay (THR-ID-004) e double spend (THR-TX-001) de forma determinística.
* **Unidade mínima**: `1 ZERO = 100 000 000` unidades (10⁸), representadas como `u64`. Toda aritmética monetária é verificada (THR-TX-003).
* **Taxas**: explícitas em cada transação, com valor mínimo definido no Genesis; creditadas ao produtor do bloco, menos a parte queimada (`fee_burn_bps`, ADR-0025).
* **Emissão**: além do Genesis, só a recompensa fixa por bloco cria ZERO (ADR-0025, `spec/STATE.md §5`).

## Alternativas consideradas

* **UTXO** — melhor base para privacidade transacional, porém mais complexo para o primeiro protótipo.

## Consequências

* Transações são **transparentes** na DEVNET: remetente, destinatário e valor são públicos. Isso **não satisfaz** `REQ-024` e está registrado como aceitação temporária em `THREAT_MODEL.md §11`.
* O modelo de privacidade definitivo pode exigir migração para UTXO ou notas cifradas; a interface de transição de estado foi isolada para permitir essa troca.
