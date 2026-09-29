# ADR-0006 — Consenso da DEVNET: autoridade rotativa

**Estado:** Aceita
**Escopo:** **DEVNET apenas**
**Data:** 2026-09-29
**Relacionamento:** REQ-005, REQ-015, SPEC §19–22, THR-CON-002, THR-CON-004

## Contexto

O algoritmo de consenso é a decisão mais importante e mais aberta do projeto (`SPEC §71`). Não é responsável escolhê-lo sem pesquisa, simulação e revisão. Mesmo assim, a DEVNET precisa produzir blocos para testar o resto do sistema.

## Decisão

A DEVNET usa **autoridade rotativa (round-robin)** sobre um conjunto fixo de validadores declarado no Genesis:

* o tempo é dividido em *slots* de duração fixa definida no Genesis;
* o produtor do slot `s` é `validadores[s mod n]`;
* um bloco só é válido se assinado pelo produtor do seu slot, com slot estritamente crescente em relação ao pai e não no futuro além de uma tolerância;
* **escolha de fork**: a cadeia válida de maior altura; em empate, o menor Block ID;
* **finalidade na DEVNET**: um bloco é considerado finalizado quando possui `k` descendentes (parâmetro do Genesis). Reorganizações que revertam blocos finalizados são rejeitadas;
* dois blocos diferentes assinados pelo mesmo validador para o mesmo slot formam **evidência de equivocação** verificável (THR-CON-004).

O consenso foi isolado atrás de uma interface (`ConsensusEngine`), para ser substituído sem reescrever estado, blocos ou P2P.

## Por que isto NÃO serve para a MAINNET

* O conjunto de validadores é fixo e conhecido: viola `REQ-005` (ausência de autoridade absoluta) em escala de rede real.
* Não há tolerância a validadores bizantinos na finalidade.
* Não há mecanismo anti-Sybil para entrada de validadores.

## Condições de revisão

Esta ADR deve ser substituída antes de qualquer TESTNET pública por uma ADR que escolha o algoritmo definitivo com base em análise contra `SPEC §20`.
