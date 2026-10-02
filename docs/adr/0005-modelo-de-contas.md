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
* **Taxas**: explícitas em cada transação, com valor mínimo definido no Genesis; na DEVNET são creditadas ao produtor do bloco.
* **Emissão**: somente o Genesis cria ZERO na DEVNET. Recompensa de bloco é zero até que a política monetária seja definida (`REQUIREMENTS.md §23`).

## Alternativas consideradas

* **UTXO** — melhor base para privacidade transacional, porém mais complexo para o primeiro protótipo.

## Consequências

* Transações são **transparentes** na DEVNET: remetente, destinatário e valor são públicos. Isso **não satisfaz** `REQ-024` e está registrado como aceitação temporária em `THREAT_MODEL.md §11`.
* O modelo de privacidade definitivo pode exigir migração para UTXO ou notas cifradas; a interface de transição de estado foi isolada para permitir essa troca.
